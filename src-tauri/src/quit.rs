//! Quitting while an operation is under way. Closing the window only hides
//! it (window.rs), and whatever runs carries on. Quitting through the
//! question below -- 「退出」, or a question the page never showed --
//! first cancels every operation that can be cancelled, as the operation
//! bar's 「全部取消」 does (`Session::cancel`): one still queued never
//! starts, and a running command is stopped partway, which can leave the
//! tool it was updating or uninstalling half done. Banager waits for those
//! commands to stop, `STOP_WITHIN` at the most, then quits (`quit_now`).
//! A running operation that cannot be cancelled -- rustup's self update or
//! self uninstall (`operations.noCancelHint`) -- is not stopped: its command
//! runs on without Banager (`quit_now` says what becomes of it).
//!
//! So on a Mac every way of quitting -- Quit Banager (⌘Q) in the menu bar,
//! Quit in the Dock icon's menu, logging out, restarting or shutting down
//! -- first asks `should_quit`. While every operation is `Done`, it
//! answers yes and Banager quits as it always has. While one is not, the
//! quit is called off, the window comes back, and the page asks
//! (src/components/QuitQuestion.tsx): 「还有N个操作未完成」, with
//! 「取消」, which leaves Banager running, and 「退出」, which quits
//! (`quit_anyway`).
//!
//! A quit is called off only while the page is there to ask: one that
//! nobody asks about would never happen, and nothing but Force Quit would
//! end Banager. So nothing asks until the page has said it listens for the
//! question (`ask_before_quit`), and the page takes that back as it goes
//! -- taken down by an error in drawing it, say (src/lib/quit.ts). A page
//! can also go without a word: reloaded, or its web content crashed. So
//! once asked, the page has `SHOW_WITHIN`, 2 seconds, to say that the
//! question is on screen (`quit_question_shown`); without that, Banager
//! quits, as it does on 「退出」 (`quit_unless_shown`). 「取消」, or
//! Escape, says so too (`quit_kept_waiting`), which stops that wait from
//! quitting should the first word not have got through; and a quit
//! repeated while the question is pending asks it again without a second
//! wait (`QuitGuard::ask`).
//!
//! How every quit reaches `should_quit`: each ends in AppKit's
//! `terminate:` -- the menu bar's Quit is macOS's own item
//! (`menu::MacItem::Quit`), and the Dock's Quit and a logout, restart or
//! shutdown send the quit Apple event, which AppKit answers with
//! `terminate:` -- and `terminate:` first asks the application delegate's
//! `applicationShouldTerminate:`, when the delegate has one. tao's
//! delegate has none, so AppKit goes on to quit, and tauri hears of it
//! only as `RunEvent::Exit`, too late to stop; its `RunEvent::ExitRequested`
//! comes only from tauri itself -- `AppHandle::exit`, or the last window
//! closing, which on a Mac only hides it. So `guard_quitting` adds the
//! method to the class of tao's delegate, as Banager starts.
//!
//! Logging out, restarting or shutting down: the method answers at once,
//! so macOS is never kept waiting on Banager. While an operation is under
//! way the answer is no, which calls the logout off, and the window asks
//! as it does for ⌘Q; after 「退出」 Banager quits, and the logout has
//! to be started again. Holding the logout until the user answers would
//! take AppKit's `NSTerminateLater`, then `replyToApplicationShouldTerminate:`,
//! which tauri does not offer: until the reply, AppKit runs the main run
//! loop in its modal-panel mode, and the reply would come from the page,
//! through the web view and tauri's IPC, which no test here can run -- a
//! Mac logging out would be left waiting on it. Force Quit, or a process
//! killed from outside, ends Banager with no question at all.
//!
//! Off a Mac nothing asks: closing the window closes it (window.rs), and
//! Banager quits with its last window.

use crate::state::AppState;
use crate::window;
use banager_core::model::{CancelPolicy, OpStatus};
use banager_core::ops::OpSummary;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, State};

/// The event `should_quit` tells the window, once it is back on screen,
/// when a quit waits on the page's question: src/lib/api.ts's
/// `QUIT_REQUESTED_EVENT` spells the same. Its payload is the question's
/// number (`QuitGuard::ask`), which the page hands back once the question
/// is on screen (`quit_question_shown`).
pub const QUIT_REQUESTED_EVENT: &str = "quit://requested";

/// How long the page has, once asked, to say that the question is on
/// screen (`quit_question_shown`): time enough to ask the backend for the
/// operations and draw the sheet, which takes it a moment. Past that,
/// nobody is there to answer -- the page was reloaded, its web content
/// crashed, or it could not draw the sheet -- and Banager quits
/// (`quit_unless_shown`) rather than be left unable to.
pub const SHOW_WITHIN: Duration = Duration::from_secs(2);

/// How long a quit waits, once it has cancelled what can be cancelled, for
/// those commands to stop (`waits_for`) before Banager quits all the same.
/// Longer than the runner's grace after SIGTERM (`STOP_GRACE`, 5 seconds),
/// at the end of which it SIGKILLs whatever of a command is left: so every
/// command a quit cancels has had that SIGKILL sent by the time Banager
/// quits. A command that honours SIGTERM ends in milliseconds, and Banager
/// quits as soon as every one has.
pub const STOP_WITHIN: Duration = Duration::from_secs(7);

/// How often a quit that waits for commands to stop looks at them again.
const STOP_POLL: Duration = Duration::from_millis(20);

/// What a request to quit does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnQuit {
    /// Banager quits, as it always has.
    Quit,
    /// The quit is called off, and the page asks whether to quit anyway.
    Ask,
    /// The quit is called off, and nothing asks: a quit is under way
    /// already, stopping the operations (`quit_now`), and ends Banager
    /// within `STOP_WITHIN` -- quitting at once would cut that short.
    Stopping,
}

/// What a request to quit does, with `unfinished` operations not `Done`:
/// it asks while at least one is not and the page listens for the
/// question (`page_asks`), and quits otherwise -- and always once the user
/// has chosen 「退出」 (`confirmed`).
pub fn on_quit(unfinished: usize, page_asks: bool, confirmed: bool) -> OnQuit {
    if unfinished > 0 && page_asks && !confirmed {
        OnQuit::Ask
    } else {
        OnQuit::Quit
    }
}

/// What becomes of a quit the page was asked about, once `SHOW_WITHIN`
/// has passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnceAsked {
    /// The question is on screen: Banager waits for the user's answer.
    Wait,
    /// The page never said the question was on screen: nobody is there to
    /// answer it, and Banager quits.
    Quit,
}

/// What becomes of a quit the page was asked about, `SHOW_WITHIN` after
/// asking: it waits for the user once the page has said the question is
/// on screen (`shown`), and there is nothing left to do once the user has
/// answered 「退出」 (`confirmed`), which quits already; with neither,
/// Banager quits.
pub fn once_asked(shown: bool, confirmed: bool) -> OnceAsked {
    if shown || confirmed {
        OnceAsked::Wait
    } else {
        OnceAsked::Quit
    }
}

/// How many of `operations` are not `Done`: queued, running, being
/// cancelled or checking their result. A refresh under way is none of
/// them, and does not hold a quit: it only reads, and the next launch
/// checks again (`Session::busy` counts refreshes too, which is why it is
/// not what this asks).
pub fn unfinished(operations: &[OpSummary]) -> usize {
    operations
        .iter()
        .filter(|op| op.status != OpStatus::Done)
        .count()
}

/// Whether a quit cancels `op`, as 「全部取消」 does (`Session::cancel`):
/// every one queued, and every one running that can be cancelled. A
/// running `NoCancel` one -- rustup's self update or self uninstall --
/// `Session::cancel` refuses, so it is not asked.
pub fn cancels(op: &OpSummary) -> bool {
    match op.status {
        OpStatus::Queued => true,
        OpStatus::Running => op.cancel_policy != CancelPolicy::NoCancel,
        OpStatus::CancelRequested | OpStatus::Cancelling | OpStatus::Verifying | OpStatus::Done => {
            false
        }
    }
}

/// Whether a quit waits for `op` before Banager quits: while it may still
/// start a command (queued), or its command is still being stopped (a
/// cancel requested, or running and cancellable). `Cancelling`, `Verifying`
/// and `Done` come once its command has ended (`run_operation`); a running
/// `NoCancel` one does not end on Banager's account, and is not waited for.
pub fn waits_for(op: &OpSummary) -> bool {
    match op.status {
        OpStatus::Queued | OpStatus::CancelRequested => true,
        OpStatus::Running => op.cancel_policy != CancelPolicy::NoCancel,
        OpStatus::Cancelling | OpStatus::Verifying | OpStatus::Done => false,
    }
}

/// A quit's order: cancel every operation it `cancels`, wait until none is
/// left that it `waits_for` -- `within` at the most -- then `quit`. It
/// cancels again each time it looks, so one queued in the meantime does not
/// start either. `quit_now` runs it on the session; the tests, on lists of
/// their own.
async fn stop_then_quit(
    operations: impl Fn() -> Vec<OpSummary>,
    cancel: impl Fn(u64),
    within: Duration,
    quit: impl FnOnce(),
) {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        for op in operations().iter().filter(|op| cancels(op)) {
            cancel(op.id);
        }
        let left = operations().iter().filter(|op| waits_for(op)).count();
        if left == 0 {
            break;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            eprintln!(
                "[banager] {left} operation(s) had not stopped within {within:?} of quitting; Banager quits"
            );
            break;
        }
        tokio::time::sleep_until(deadline.min(now + STOP_POLL)).await;
    }
    quit();
}

/// A question as `QuitGuard::ask` hands it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Question {
    /// Its number, which the page hands back (`QuitGuard::shown`,
    /// `QuitGuard::kept_waiting`).
    pub number: u64,
    /// Whether it is a new question, whose fallback is to start
    /// (`quit_unless_shown`); `false` for the one still pending, asked
    /// again, whose fallback already runs.
    pub new: bool,
}

/// Whether a quit asks first -- whether the page listens for the question,
/// and whether the user has already answered 「退出」 -- and which
/// questions are settled: on screen, or answered 「取消」. Managed on
/// the builder in `run()`; in memory only, for this run.
#[derive(Debug, Default)]
pub struct QuitGuard {
    page_asks: AtomicBool,
    confirmed: AtomicBool,
    /// A quit is under way, stopping the operations (`quit_now`).
    stopping: AtomicBool,
    /// The newest question's number, counting from 1; 0 before the first.
    asked: AtomicU64,
    /// The newest question the page has said is on screen, or the user
    /// has answered 「取消」 to; 0 before any. A question newer than
    /// this is pending: its fallback still runs.
    settled: AtomicU64,
}

impl QuitGuard {
    /// Whether the page listens for `QUIT_REQUESTED_EVENT` and answers it:
    /// from `true` on, a quit asks first while an operation is not `Done`;
    /// from `false` on -- the page has gone -- it quits at once again.
    pub fn page_asks(&self, asks: bool) {
        self.page_asks.store(asks, Ordering::SeqCst);
    }

    /// The user answered 「退出」: every quit from now on quits.
    pub fn confirm(&self) {
        self.confirmed.store(true, Ordering::SeqCst);
    }

    /// A quit is under way, stopping the operations before Banager quits
    /// (`quit_now`): until it does, another quit is called off
    /// (`OnQuit::Stopping`), so as not to cut it short. Its own
    /// `AppHandle::exit` asks AppKit nothing -- tao ends with `stop:`, not
    /// `terminate:` -- so is not called off.
    pub fn stop(&self) {
        self.confirm();
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// `on_quit` with what this guard knows, and `OnQuit::Stopping` while a
    /// quit is under way (`stop`).
    pub fn decide(&self, unfinished: usize) -> OnQuit {
        if self.stopping.load(Ordering::SeqCst) {
            return OnQuit::Stopping;
        }
        on_quit(
            unfinished,
            self.page_asks.load(Ordering::SeqCst),
            self.confirmed.load(Ordering::SeqCst),
        )
    }

    /// A question is about to go to the page. While the newest one is
    /// pending -- neither on screen nor answered yet -- that one again, with
    /// no fallback of its own: a quit repeated while the page has not yet
    /// answered starts no second one. Otherwise a new one, with the next
    /// number. Asked on the main thread only (`should_quit`).
    pub fn ask(&self) -> Question {
        let asked = self.asked.load(Ordering::SeqCst);
        if asked > self.settled.load(Ordering::SeqCst) {
            return Question {
                number: asked,
                new: false,
            };
        }
        Question {
            number: self.asked.fetch_add(1, Ordering::SeqCst) + 1,
            new: true,
        }
    }

    /// The page has question `question` on screen, and with it every one
    /// asked before: the one sheet answers every quit so far. A number
    /// that no question has had yet is not taken.
    pub fn shown(&self, question: u64) {
        self.settle(question);
    }

    /// The user answered question `question` 「取消」 -- or the sheet
    /// went by itself, everything having finished: its fallback, should it
    /// still run, does not quit, whether or not the page's word that the
    /// question was on screen got through. A number that no question has
    /// had yet is not taken.
    pub fn kept_waiting(&self, question: u64) {
        self.settle(question);
    }

    fn settle(&self, question: u64) {
        if question <= self.asked.load(Ordering::SeqCst) {
            self.settled.fetch_max(question, Ordering::SeqCst);
        }
    }

    /// `once_asked` for question `question`, with what this guard knows.
    pub fn once_asked(&self, question: u64) -> OnceAsked {
        once_asked(
            self.settled.load(Ordering::SeqCst) >= question,
            self.confirmed.load(Ordering::SeqCst),
        )
    }
}

/// Whether Banager quits now: what the method `guard_quitting` adds to
/// AppKit's delegate asks at every quit, on the main thread. When the
/// page is to ask (`OnQuit::Ask`), the window comes back and is told,
/// with the question's number (`window::show_and_send`), and the quit is
/// called off -- and Banager quits after all should the page not say
/// within `SHOW_WITHIN` that the question is on screen
/// (`quit_unless_shown`). A quit repeated while that question is pending
/// asks the same question again, and starts no second wait
/// (`QuitGuard::ask`). Should the page be out of reach, Banager quits
/// at once, as it always has, rather than not quitting and asking nobody.
/// So does anything missing here.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn should_quit<R: Runtime>(app: &AppHandle<R>) -> bool {
    let (Some(state), Some(guard)) = (app.try_state::<AppState>(), app.try_state::<QuitGuard>())
    else {
        return true;
    };
    match guard.decide(unfinished(&state.session.operations())) {
        OnQuit::Quit => true,
        OnQuit::Stopping => false,
        OnQuit::Ask => {
            let question = guard.ask();
            match window::show_and_send(app, QUIT_REQUESTED_EVENT, question.number) {
                Ok(()) => {
                    if question.new {
                        quit_unless_shown(app, question.number);
                    }
                    false
                }
                Err(e) => {
                    eprintln!(
                        "[banager] could not ask the window before quitting, so Banager quits: {e}"
                    );
                    true
                }
            }
        }
    }
}

/// Banager quits `SHOW_WITHIN` from now, unless the page has said by then
/// that question `question` is on screen (`wait_for_the_page`) -- on
/// tauri's async runtime, so that AppKit has its answer, no, at once.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn quit_unless_shown<R: Runtime>(app: &AppHandle<R>, question: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let quits = wait_for_the_page(&app.state::<QuitGuard>(), question, SHOW_WITHIN).await;
        if quits {
            eprintln!(
                "[banager] the window did not show the question within {SHOW_WITHIN:?}, so Banager quits"
            );
            quit_now(&app).await;
        }
    });
}

/// Waits `within`, then answers whether Banager quits: yes unless the page
/// has said by then that question `question` is on screen
/// (`QuitGuard::once_asked`). `quit_unless_shown`'s wait, which the tests
/// run with a wait of their own.
async fn wait_for_the_page(guard: &QuitGuard, question: u64, within: Duration) -> bool {
    tokio::time::sleep(within).await;
    guard.once_asked(question) == OnceAsked::Quit
}

/// Banager quits: 「退出」 (`quit_anyway`), and a question the page never
/// showed (`quit_unless_shown`). A quit that comes while this runs is called
/// off, and this goes on (`QuitGuard::stop`). First every operation that can
/// be cancelled is, as
/// 「全部取消」 does, and Banager waits for those commands to stop,
/// `STOP_WITHIN` at the most (`stop_then_quit`); then it quits through
/// tauri's own `AppHandle::exit`, which asks AppKit nothing and ends in
/// `RunEvent::Exit` as a quit from the menu does, the window's size and
/// place saved with it.
///
/// That exit is tao's `process::exit`: nothing still running in Banager
/// gets to clean up, and the runner's `GroupedChild`, which SIGKILLs its
/// command's process group when dropped, is never dropped. So a command
/// still running then -- one that cannot be cancelled, or one that outlived
/// `STOP_WITHIN` -- is sent no signal by Banager, and runs in a process
/// group of its own (`process_group(0)`), which nothing sent to Banager
/// reaches: it runs on without Banager. Its output went to pipes that only
/// Banager read, and their reading ends close as Banager exits, so a write
/// to its standard output or error after that fails with a broken pipe
/// (EPIPE, or SIGPIPE, which ends a program that does not ignore it).
async fn quit_now<R: Runtime>(app: &AppHandle<R>) {
    app.state::<QuitGuard>().stop();
    if let Some(state) = app.try_state::<AppState>() {
        let session = &state.session;
        stop_then_quit(
            || session.operations(),
            |id| {
                // Refused only for a running `NoCancel` one, which `cancels`
                // does not pick, or one that has just ended: nothing to do.
                let _ = session.cancel(id);
            },
            STOP_WITHIN,
            || (),
        )
        .await;
    }
    app.exit(0);
}

/// Whether the page listens for `QUIT_REQUESTED_EVENT` and answers it
/// (src/lib/quit.ts): `true` once it listens, at every load, and a quit
/// asks first from then on while an operation is not `Done`; `false` as
/// it stops listening -- the page taken down, which an error in drawing
/// it that nothing catches does -- and a quit quits at once again.
#[tauri::command]
pub fn ask_before_quit(guard: State<'_, QuitGuard>, ask: bool) {
    guard.page_asks(ask);
}

/// The page has question `question` (`QUIT_REQUESTED_EVENT`'s payload) on
/// screen, and waits for the user's answer: Banager does not quit for
/// want of one (`quit_unless_shown`).
#[tauri::command]
pub fn quit_question_shown(guard: State<'_, QuitGuard>, question: u64) {
    guard.shown(question);
}

/// The user answered question `question` 「取消」 (or Escape), or the
/// sheet went by itself, everything having finished: Banager does not quit
/// for want of word from the page (`QuitGuard::kept_waiting`) -- which
/// matters when that word, `quit_question_shown`, did not get through.
#[tauri::command]
pub fn quit_kept_waiting(guard: State<'_, QuitGuard>, question: u64) {
    guard.kept_waiting(question);
}

/// 「退出」: Banager cancels what can be cancelled, waits for it to stop,
/// and quits (`quit_now`); the answer comes only if Banager is still there
/// to give it. Also the page's answer when a quit it was asked about finds
/// nothing left to wait for.
#[tauri::command]
pub async fn quit_anyway(app: AppHandle) {
    quit_now(&app).await;
}

/// What the method added to AppKit's delegate asks at every quit
/// (`should_quit`, handed over by `guard_quitting`), or nothing yet: then
/// it answers that Banager quits. Behind a lock only so that it can be
/// set as Banager starts, and by the tests; taken out before it is
/// called, so that nothing it does can wait on this lock.
#[cfg(target_os = "macos")]
type Decide = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

#[cfg(target_os = "macos")]
static DECIDE: std::sync::Mutex<Option<Decide>> = std::sync::Mutex::new(None);

/// Makes every quit ask `should_quit` first: adds
/// `applicationShouldTerminate:` to the class of AppKit's application
/// delegate -- tao's, which has none -- answering `NSTerminateNow` when
/// `should_quit` says Banager quits and `NSTerminateCancel` when it does
/// not. AppKit looks for the method at each `terminate:`, not only as
/// the delegate is set, so one added after launch is asked -- as it was
/// on macOS 27, for `terminate:` and for the quit Apple event with and
/// without a logout's reason, in a program of AppKit's alone, outside
/// the tests. Called once, from `run()`'s setup, on the main thread;
/// should the method not go in -- no delegate, or one whose class answers
/// the method already, itself or through a superclass, which is left as it
/// is (`add_should_terminate`) -- quitting stays as it was, and the reason
/// is logged, once.
#[cfg(target_os = "macos")]
pub fn guard_quitting<R: Runtime>(app: &AppHandle<R>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("[banager] quitting asks nothing: not set up on the main thread");
        return;
    };
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
        eprintln!("[banager] quitting asks nothing: AppKit has no application delegate");
        return;
    };
    let app = app.clone();
    *DECIDE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(std::sync::Arc::new(move || should_quit(&app)));
    let delegate = AsRef::<objc2::runtime::AnyObject>::as_ref(&*delegate);
    if let Err(e) = add_should_terminate(delegate.class()) {
        eprintln!("[banager] quitting asks nothing: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn guard_quitting<R: Runtime>(_app: &AppHandle<R>) {}

/// The Rust type of `application_should_terminate`: the receiver, the
/// selector, AppKit's `NSApplication` asking.
#[cfg(target_os = "macos")]
type ShouldTerminate = unsafe extern "C-unwind" fn(
    *mut objc2::runtime::AnyObject,
    objc2::runtime::Sel,
    *mut objc2::runtime::AnyObject,
) -> objc2_app_kit::NSApplicationTerminateReply;

/// `applicationShouldTerminate:`, as `guard_quitting` adds it: `DECIDE`'s
/// answer, and Banager quits when there is none to ask or it panics --
/// nothing unwinds into AppKit.
#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn application_should_terminate(
    _this: *mut objc2::runtime::AnyObject,
    _cmd: objc2::runtime::Sel,
    _sender: *mut objc2::runtime::AnyObject,
) -> objc2_app_kit::NSApplicationTerminateReply {
    use objc2_app_kit::NSApplicationTerminateReply;

    let decide = DECIDE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let quit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decide.is_none_or(|decide| (*decide)())
    }))
    .unwrap_or(true);
    if quit {
        NSApplicationTerminateReply::TerminateNow
    } else {
        NSApplicationTerminateReply::TerminateCancel
    }
}

/// Adds `application_should_terminate` to `class` as its
/// `applicationShouldTerminate:`. Refused, and `class` left as it is, when
/// the class answers `applicationShouldTerminate:` already, with a method
/// of its own or one it inherits: `class_addMethod` refuses only the
/// first, and would put this one in front of an inherited one, which then
/// never runs. Whatever answers there already decides quitting, and the
/// guard stays off (`guard_quitting` logs why). tests/tao_delegate.rs
/// checks that tao's delegate class answers nothing there, so that a tao
/// that starts to is looked at.
#[cfg(target_os = "macos")]
fn add_should_terminate(class: &objc2::runtime::AnyClass) -> Result<(), String> {
    use objc2::encode::Encode;
    use objc2::runtime::{AnyClass, Imp};
    use objc2_app_kit::NSApplicationTerminateReply;

    // `class_getInstanceMethod`, which looks through the superclasses too.
    if class
        .instance_method(objc2::sel!(applicationShouldTerminate:))
        .is_some()
    {
        return Err(format!(
            "{:?} already answers applicationShouldTerminate:, itself or through a superclass, and that answer is left to decide",
            class.name()
        ));
    }

    // The method's types, as the Objective-C runtime spells them: the
    // reply (an `NSUInteger`), the receiver, the selector, the sender.
    let types = std::ffi::CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))
        .map_err(|e| e.to_string())?;
    // SAFETY: an `Imp` is any method's function pointer, called by the
    // runtime with the arguments `types` names, which are the ones
    // `ShouldTerminate` takes.
    let imp = unsafe { std::mem::transmute::<ShouldTerminate, Imp>(application_should_terminate) };
    // SAFETY: `class` is a class the runtime has registered, and so may
    // be given a method; `types` is a NUL-terminated string that outlives
    // the call, which copies it.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            objc2::sel!(applicationShouldTerminate:),
            imp,
            types.as_ptr(),
        )
    };
    if added.as_bool() {
        Ok(())
    } else {
        Err(format!(
            "{:?} already answers applicationShouldTerminate:",
            class.name()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banager_core::model::{ArtifactKind, OpKind};

    fn op(id: u64, status: OpStatus) -> OpSummary {
        op_with(id, status, CancelPolicy::KillThenReconcile)
    }

    fn op_with(id: u64, status: OpStatus, cancel_policy: CancelPolicy) -> OpSummary {
        OpSummary {
            id,
            kind: OpKind::Upgrade,
            instance_id: "brew:/opt/homebrew".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: format!("tool-{id}"),
            status,
            outcome: None,
            argv_preview: Vec::new(),
            env_preview: Vec::new(),
            cancel_policy,
            already_updated: None,
            follow_up_warnings: Vec::new(),
        }
    }

    #[test]
    fn test_a_quit_cancels_what_is_queued_or_running_and_can_be_cancelled() {
        use CancelPolicy::{KillThenReconcile as Kill, NoCancel};
        let cancels_it = |status, policy| cancels(&op_with(1, status, policy));
        assert!(cancels_it(OpStatus::Queued, Kill));
        assert!(
            cancels_it(OpStatus::Queued, NoCancel),
            "queued, it has started nothing"
        );
        assert!(cancels_it(OpStatus::Running, Kill));
        assert!(
            !cancels_it(OpStatus::Running, NoCancel),
            "rustup's self update, once started"
        );
        for status in [
            OpStatus::CancelRequested,
            OpStatus::Cancelling,
            OpStatus::Verifying,
            OpStatus::Done,
        ] {
            assert!(!cancels_it(status, Kill), "{status:?}");
        }
    }

    #[test]
    fn test_a_quit_waits_for_what_may_still_start_or_is_still_being_stopped() {
        use CancelPolicy::{KillThenReconcile as Kill, NoCancel};
        let waits = |status, policy| waits_for(&op_with(1, status, policy));
        assert!(waits(OpStatus::Queued, Kill));
        assert!(waits(OpStatus::CancelRequested, Kill));
        assert!(
            waits(OpStatus::CancelRequested, NoCancel),
            "cancelled queued"
        );
        assert!(waits(OpStatus::Running, Kill));
        assert!(
            !waits(OpStatus::Running, NoCancel),
            "it does not end on Banager's account"
        );
        for status in [OpStatus::Cancelling, OpStatus::Verifying, OpStatus::Done] {
            assert!(!waits(status, Kill), "{status:?}: its command has ended");
        }
    }

    #[test]
    fn test_a_quit_waits_longer_than_the_runner_takes_to_sigkill_a_command() {
        assert!(STOP_WITHIN > banager_core::runner::real::STOP_GRACE);
    }

    /// A fake session for `stop_then_quit`: its operations, a cancel that
    /// marks one `CancelRequested` as `Session::cancel` does, and what was
    /// done in which order.
    struct Fake {
        ops: std::cell::RefCell<Vec<OpSummary>>,
        done: std::cell::RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(ops: Vec<OpSummary>) -> Self {
            Fake {
                ops: std::cell::RefCell::new(ops),
                done: std::cell::RefCell::new(Vec::new()),
            }
        }
        fn list(&self) -> Vec<OpSummary> {
            self.ops.borrow().clone()
        }
        fn cancel(&self, id: u64) {
            self.done.borrow_mut().push(format!("cancel {id}"));
            for op in self.ops.borrow_mut().iter_mut() {
                if op.id == id {
                    op.status = OpStatus::CancelRequested;
                }
            }
        }
        fn set(&self, id: u64, status: OpStatus) {
            for op in self.ops.borrow_mut().iter_mut() {
                if op.id == id {
                    op.status = status;
                }
            }
        }
    }

    #[tokio::test]
    async fn test_a_quit_cancels_first_waits_for_the_commands_to_stop_then_quits() {
        use CancelPolicy::NoCancel;
        let fake = Fake::new(vec![
            op(1, OpStatus::Running),
            op(2, OpStatus::Queued),
            op_with(3, OpStatus::Running, NoCancel),
            op(4, OpStatus::Done),
        ]);
        let started = std::time::Instant::now();
        let mut quitting = std::pin::pin!(stop_then_quit(
            || fake.list(),
            |id| fake.cancel(id),
            Duration::from_secs(10),
            || fake.done.borrow_mut().push("quit".to_string()),
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(60), quitting.as_mut())
                .await
                .is_err(),
            "it waits while the cancelled commands have not stopped"
        );
        assert_eq!(*fake.done.borrow(), ["cancel 1", "cancel 2"]);
        // The runner stops 1; 2 ends without starting.
        fake.set(1, OpStatus::Verifying);
        fake.set(2, OpStatus::Done);
        quitting.await;
        assert_eq!(
            *fake.done.borrow(),
            ["cancel 1", "cancel 2", "quit"],
            "rustup's self update (3) is neither cancelled nor waited for"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_a_quit_with_nothing_to_stop_quits_at_once() {
        let fake = Fake::new(vec![
            op(1, OpStatus::Done),
            op(2, OpStatus::Verifying),
            op_with(3, OpStatus::Running, CancelPolicy::NoCancel),
        ]);
        let quit = std::cell::Cell::new(false);
        tokio::time::timeout(
            Duration::from_millis(500),
            stop_then_quit(
                || fake.list(),
                |id| fake.cancel(id),
                Duration::from_secs(10),
                || quit.set(true),
            ),
        )
        .await
        .expect("no wait");
        assert!(quit.get());
        assert!(fake.done.borrow().is_empty(), "nothing cancelled");
    }

    #[tokio::test]
    async fn test_a_quit_quits_once_the_wait_is_over_though_a_command_has_not_stopped() {
        let fake = Fake::new(vec![op(1, OpStatus::Running)]);
        let within = Duration::from_millis(50);
        let started = std::time::Instant::now();
        let quit = std::cell::Cell::new(false);
        stop_then_quit(
            || fake.list(),
            |id| fake.cancel(id),
            within,
            || quit.set(true),
        )
        .await;
        assert!(quit.get());
        assert!(started.elapsed() >= within, "not before the wait was over");
        assert_eq!(*fake.done.borrow(), ["cancel 1"]);
    }

    #[tokio::test]
    async fn test_a_quit_cancels_one_queued_while_it_waits() {
        let fake = Fake::new(vec![op(1, OpStatus::Running)]);
        let mut quitting = std::pin::pin!(stop_then_quit(
            || fake.list(),
            |id| fake.cancel(id),
            Duration::from_secs(10),
            || fake.done.borrow_mut().push("quit".to_string()),
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(40), quitting.as_mut())
                .await
                .is_err()
        );
        fake.ops.borrow_mut().push(op(2, OpStatus::Queued));
        assert!(
            tokio::time::timeout(Duration::from_millis(40), quitting.as_mut())
                .await
                .is_err()
        );
        fake.set(1, OpStatus::Done);
        fake.set(2, OpStatus::Done);
        quitting.await;
        assert_eq!(*fake.done.borrow(), ["cancel 1", "cancel 2", "quit"]);
    }

    #[test]
    fn test_a_quit_asks_only_while_something_is_unfinished_the_page_asks_and_nobody_confirmed() {
        assert_eq!(on_quit(0, true, false), OnQuit::Quit, "nothing running");
        assert_eq!(on_quit(1, true, false), OnQuit::Ask);
        assert_eq!(on_quit(3, true, false), OnQuit::Ask);
        assert_eq!(
            on_quit(1, false, false),
            OnQuit::Quit,
            "no page listens for the question: a quit nobody asks about would never happen"
        );
        assert_eq!(on_quit(1, true, true), OnQuit::Quit, "「退出」");
        assert_eq!(on_quit(0, false, true), OnQuit::Quit);
    }

    #[test]
    fn test_every_operation_but_a_done_one_is_unfinished() {
        assert_eq!(unfinished(&[]), 0);
        let each = [
            OpStatus::Queued,
            OpStatus::Running,
            OpStatus::CancelRequested,
            OpStatus::Cancelling,
            OpStatus::Verifying,
        ];
        for status in each {
            assert_eq!(unfinished(&[op(1, status)]), 1, "{status:?}");
        }
        assert_eq!(unfinished(&[op(1, OpStatus::Done)]), 0);
        assert_eq!(
            unfinished(&[
                op(1, OpStatus::Done),
                op(2, OpStatus::Running),
                op(3, OpStatus::Queued),
                op(4, OpStatus::Done),
            ]),
            2
        );
    }

    #[test]
    fn test_the_guard_asks_once_the_page_listens_and_quits_once_the_user_confirmed() {
        let guard = QuitGuard::default();
        assert_eq!(
            guard.decide(2),
            OnQuit::Quit,
            "before the page has said it listens, quitting stays as it was"
        );
        guard.page_asks(true);
        assert_eq!(guard.decide(2), OnQuit::Ask);
        assert_eq!(guard.decide(0), OnQuit::Quit);
        assert_eq!(
            guard.decide(2),
            OnQuit::Ask,
            "「取消」 changes nothing here"
        );
        guard.confirm();
        assert_eq!(
            guard.decide(2),
            OnQuit::Quit,
            "「退出」, and every quit after it"
        );
    }

    #[test]
    fn test_a_quit_while_one_is_stopping_the_operations_is_called_off() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        guard.stop();
        assert_eq!(
            guard.decide(2),
            OnQuit::Stopping,
            "it would cut the stop short"
        );
        assert_eq!(guard.decide(0), OnQuit::Stopping);
        let question = guard.ask().number;
        assert_eq!(
            guard.once_asked(question),
            OnceAsked::Wait,
            "a question asked before stopping does not quit on its own"
        );
    }

    #[test]
    fn test_a_page_that_stops_listening_leaves_every_quit_quitting_at_once() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        assert_eq!(guard.decide(1), OnQuit::Ask);
        guard.page_asks(false);
        assert_eq!(
            guard.decide(1),
            OnQuit::Quit,
            "the page has gone: nobody is there to ask, and the quit happens"
        );
        guard.page_asks(true);
        assert_eq!(
            guard.decide(1),
            OnQuit::Ask,
            "a page loaded again asks again"
        );
    }

    #[test]
    fn test_a_quit_asked_about_waits_for_the_user_only_once_the_question_is_on_screen() {
        assert_eq!(once_asked(true, false), OnceAsked::Wait, "on screen");
        assert_eq!(
            once_asked(false, false),
            OnceAsked::Quit,
            "never said to be on screen: nobody is there to answer"
        );
        assert_eq!(
            once_asked(false, true),
            OnceAsked::Wait,
            "「退出」 was chosen, and Banager quits already"
        );
        assert_eq!(once_asked(true, true), OnceAsked::Wait);
    }

    #[test]
    fn test_the_guard_numbers_each_question_and_waits_only_for_one_the_page_has_on_screen() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        let first = guard.ask().number;
        assert_eq!(first, 1);
        assert_eq!(
            guard.once_asked(first),
            OnceAsked::Quit,
            "not on screen yet"
        );
        guard.shown(first);
        assert_eq!(guard.once_asked(first), OnceAsked::Wait);

        let second = guard.ask().number;
        assert_eq!(second, 2);
        assert_eq!(
            guard.once_asked(second),
            OnceAsked::Quit,
            "the page had the first question on screen, and has said nothing of this one"
        );
        guard.shown(second);
        assert_eq!(guard.once_asked(second), OnceAsked::Wait);

        // Two quits in a row, the first not yet on screen: the second asks
        // the same question again, and starts no second wait.
        let third = guard.ask();
        assert!(third.new);
        let again = guard.ask();
        assert_eq!(
            again,
            Question {
                number: third.number,
                new: false
            }
        );
        guard.shown(third.number);
        assert_eq!(guard.once_asked(third.number), OnceAsked::Wait);
        // Once it is on screen, a quit asks a new question, with a wait of
        // its own: the page may have gone since.
        let fourth = guard.ask();
        assert_eq!(fourth.number, third.number + 1);
        assert!(fourth.new);
        guard.shown(fourth.number);
        // An answer to the older one after the newer one's changes nothing.
        guard.shown(third.number);
        assert_eq!(guard.once_asked(fourth.number), OnceAsked::Wait);

        // A number no question has had yet is not taken: the question that
        // gets it later still has to be on screen.
        guard.shown(fourth.number + 1);
        guard.kept_waiting(fourth.number + 1);
        let fifth = guard.ask().number;
        assert_eq!(fifth, fourth.number + 1);
        assert_eq!(guard.once_asked(fifth), OnceAsked::Quit);

        guard.confirm();
        assert_eq!(
            guard.once_asked(fifth),
            OnceAsked::Wait,
            "「退出」: Banager quits already"
        );
    }

    /// `quit_unless_shown`'s wait, the page saying while it lasts that the
    /// question is on screen: Banager goes on waiting for the user.
    #[tokio::test]
    async fn test_a_question_the_page_has_on_screen_in_time_waits_for_the_user() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        let question = guard.ask().number;
        let mut waiting = std::pin::pin!(wait_for_the_page(
            &guard,
            question,
            Duration::from_millis(20),
        ));
        assert!(
            tokio::time::timeout(Duration::ZERO, waiting.as_mut())
                .await
                .is_err(),
            "Banager waits"
        );
        guard.shown(question);
        assert!(!waiting.await, "Banager does not quit");
    }

    /// `quit_unless_shown`'s wait, with no word from the page: Banager
    /// quits once the wait is over, and not before.
    #[tokio::test]
    async fn test_a_question_nobody_says_is_on_screen_quits_once_the_wait_is_over() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        let question = guard.ask().number;
        let within = Duration::from_millis(20);
        let started = std::time::Instant::now();
        assert!(wait_for_the_page(&guard, question, within).await);
        assert!(started.elapsed() >= within, "not before the wait was over");

        // An earlier question on screen does not stand for a later quit's.
        guard.shown(question);
        let next = guard.ask().number;
        assert!(wait_for_the_page(&guard, next, within).await);
    }

    /// 「取消」 while the page's word that the question is on screen has
    /// not got through: Banager does not quit.
    #[tokio::test]
    async fn test_keep_waiting_stops_the_wait_from_quitting_without_word_that_it_was_shown() {
        let guard = QuitGuard::default();
        guard.page_asks(true);
        let question = guard.ask();
        assert!(question.new);
        let mut waiting = std::pin::pin!(wait_for_the_page(
            &guard,
            question.number,
            Duration::from_millis(20),
        ));
        assert!(tokio::time::timeout(Duration::ZERO, waiting.as_mut())
            .await
            .is_err());
        guard.kept_waiting(question.number);
        assert!(!waiting.await, "Banager does not quit");
        assert_eq!(guard.once_asked(question.number), OnceAsked::Wait);
        // Answered, it is not pending: the next quit asks anew, with a wait.
        let next = guard.ask();
        assert_eq!(next.number, question.number + 1);
        assert!(next.new);
    }

    #[test]
    fn test_the_page_has_two_seconds_to_show_the_question() {
        assert_eq!(SHOW_WITHIN, Duration::from_secs(2));
    }

    #[test]
    fn test_the_question_is_the_event_the_page_listens_for() {
        // `QUIT_REQUESTED_EVENT` in src/lib/api.ts, which api.test.ts pins
        // to the same string.
        assert_eq!(QUIT_REQUESTED_EVENT, "quit://requested");
    }

    /// `guard_quitting`'s method, added to a class of the test's own in
    /// place of tao's delegate's and sent `applicationShouldTerminate:` as
    /// AppKit sends it: the Objective-C runtime only, no app and no main
    /// thread. What it answers is `DECIDE`'s, which only this test sets.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_the_added_method_answers_appkit_with_the_decision_and_quits_when_it_cannot_ask() {
        use objc2::rc::Retained;
        use objc2::runtime::{AnyObject, ClassBuilder, NSObject};
        use objc2::{msg_send, ClassType};
        use objc2_app_kit::NSApplicationTerminateReply;
        use std::sync::Arc;

        let name = std::ffi::CString::new(format!(
            "BanagerQuitGuardTestDelegate{}",
            std::process::id()
        ))
        .unwrap();
        let class = ClassBuilder::new(&name, NSObject::class())
            .expect("a class of this name is not registered yet")
            .register();
        let delegate: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let ask = |decide: Option<Decide>| -> NSApplicationTerminateReply {
            *DECIDE.lock().unwrap() = decide;
            // SAFETY: the method takes the sender AppKit passes, an
            // object, here none, and returns the reply `types` names.
            unsafe { msg_send![&*delegate, applicationShouldTerminate: None::<&AnyObject>] }
        };

        add_should_terminate(class).expect("added");
        assert!(
            add_should_terminate(class).is_err(),
            "one already there is left as it is"
        );
        // A subclass inherits it: refused too, and the superclass's method
        // is still what answers.
        let sub_name = std::ffi::CString::new(format!(
            "BanagerQuitGuardTestDelegateSub{}",
            std::process::id()
        ))
        .unwrap();
        let subclass = ClassBuilder::new(&sub_name, class)
            .expect("a class of this name is not registered yet")
            .register();
        let refused = add_should_terminate(subclass).expect_err("inherited: left as it is");
        assert!(refused.contains("already answers"), "{refused}");
        let method = |of: &objc2::runtime::AnyClass| {
            of.instance_method(objc2::sel!(applicationShouldTerminate:))
                .map(|method| method as *const objc2::runtime::Method)
        };
        assert_eq!(
            method(subclass),
            method(class),
            "the subclass still answers with the superclass's method"
        );
        assert_eq!(
            ask(Some(Arc::new(|| false) as Decide)),
            NSApplicationTerminateReply::TerminateCancel
        );
        assert_eq!(
            ask(Some(Arc::new(|| true) as Decide)),
            NSApplicationTerminateReply::TerminateNow
        );
        assert_eq!(
            ask(None),
            NSApplicationTerminateReply::TerminateNow,
            "nothing to ask yet: Banager quits"
        );
        assert_eq!(
            ask(Some(
                Arc::new(|| -> bool { panic!("the page could not be asked") }) as Decide
            )),
            NSApplicationTerminateReply::TerminateNow,
            "a panic quits, and does not unwind into AppKit"
        );
        *DECIDE.lock().unwrap() = None;
    }
}
