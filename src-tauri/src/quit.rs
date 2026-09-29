//! Quitting while an operation is under way. Closing the window only hides
//! it (window.rs), and whatever runs carries on; quitting ends Canager and
//! what it is doing with it: an operation still queued never starts, and
//! one whose command is running loses Canager partway, so the tool it was
//! updating or uninstalling can be left half done -- what the update and
//! uninstall confirmations warn of for rustup, which cannot be cancelled
//! (`operations.noCancelHint`).
//!
//! So on a Mac every way of quitting -- Quit Canager (⌘Q) in the menu bar,
//! Quit in the Dock icon's menu, logging out, restarting or shutting down
//! -- first asks `should_quit`. While every operation is `Done`, it
//! answers yes and Canager quits as it always has. While one is not, the
//! quit is called off, the window comes back, and the page asks
//! (src/components/QuitQuestion.tsx): 「还有 N 个操作没完成」, with
//! 「继续等待」, which leaves Canager running, and 「仍然退出」, which quits
//! (`quit_anyway`). Nothing asks until the page has said it listens for
//! the question (`ask_before_quit`): a page that never loaded, or could not
//! listen, would leave a quit that nobody asks about and that never
//! happens.
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
//! method to the class of tao's delegate, as Canager starts.
//!
//! Logging out, restarting or shutting down: the method answers at once,
//! so macOS is never kept waiting on Canager. While an operation is under
//! way the answer is no, which calls the logout off, and the window asks
//! as it does for ⌘Q; after 「仍然退出」 Canager quits, and the logout has
//! to be started again. Holding the logout until the user answers would
//! take AppKit's `NSTerminateLater`, then `replyToApplicationShouldTerminate:`,
//! which tauri does not offer: until the reply, AppKit runs the main run
//! loop in its modal-panel mode, and the reply would come from the page,
//! through the web view and tauri's IPC, which no test here can run -- a
//! Mac logging out would be left waiting on it. Force Quit, or a process
//! killed from outside, ends Canager with no question at all.
//!
//! Off a Mac nothing asks: closing the window closes it (window.rs), and
//! Canager quits with its last window.

use crate::state::AppState;
use crate::window;
use canager_core::model::OpStatus;
use canager_core::ops::OpSummary;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, Runtime, State};

/// The event `should_quit` tells the window, once it is back on screen,
/// when a quit waits on the page's question: src/lib/api.ts's
/// `QUIT_REQUESTED_EVENT` spells the same.
pub const QUIT_REQUESTED_EVENT: &str = "quit://requested";

/// What a request to quit does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnQuit {
    /// Canager quits, as it always has.
    Quit,
    /// The quit is called off, and the page asks whether to quit anyway.
    Ask,
}

/// What a request to quit does, with `unfinished` operations not `Done`:
/// it asks while at least one is not and the page listens for the
/// question (`page_asks`), and quits otherwise -- and always once the user
/// has chosen 「仍然退出」 (`confirmed`).
pub fn on_quit(unfinished: usize, page_asks: bool, confirmed: bool) -> OnQuit {
    if unfinished > 0 && page_asks && !confirmed {
        OnQuit::Ask
    } else {
        OnQuit::Quit
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

/// Whether a quit asks first: whether the page listens for the question,
/// and whether the user has already answered 「仍然退出」. Managed on the
/// builder in `run()`; in memory only, for this run.
#[derive(Debug, Default)]
pub struct QuitGuard {
    page_asks: AtomicBool,
    confirmed: AtomicBool,
}

impl QuitGuard {
    /// The page listens for `QUIT_REQUESTED_EVENT` and answers it; from
    /// now on a quit asks first while an operation is not `Done`.
    pub fn page_asks(&self) {
        self.page_asks.store(true, Ordering::SeqCst);
    }

    /// The user answered 「仍然退出」: every quit from now on quits.
    pub fn confirm(&self) {
        self.confirmed.store(true, Ordering::SeqCst);
    }

    /// `on_quit` with what this guard knows.
    pub fn decide(&self, unfinished: usize) -> OnQuit {
        on_quit(
            unfinished,
            self.page_asks.load(Ordering::SeqCst),
            self.confirmed.load(Ordering::SeqCst),
        )
    }
}

/// Whether Canager quits now: what the method `guard_quitting` adds to
/// AppKit's delegate asks at every quit, on the main thread. When the
/// page is to ask (`OnQuit::Ask`), the window comes back and is told
/// (`window::show_and_tell`), and the quit is called off; should the page
/// be out of reach, Canager quits as it always has, rather than not
/// quitting and asking nobody. So does anything missing here.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn should_quit<R: Runtime>(app: &AppHandle<R>) -> bool {
    let (Some(state), Some(guard)) = (app.try_state::<AppState>(), app.try_state::<QuitGuard>())
    else {
        return true;
    };
    match guard.decide(unfinished(&state.session.operations())) {
        OnQuit::Quit => true,
        OnQuit::Ask => match window::show_and_tell(app, QUIT_REQUESTED_EVENT) {
            Ok(()) => false,
            Err(e) => {
                eprintln!(
                    "[canager] could not ask the window before quitting, so Canager quits: {e}"
                );
                true
            }
        },
    }
}

/// The page listens for `QUIT_REQUESTED_EVENT` from now on
/// (src/lib/quit.ts), and answers it: a quit asks first while an
/// operation is not `Done`. Sent once the page listens, at every load.
#[tauri::command]
pub fn ask_before_quit(guard: State<'_, QuitGuard>) {
    guard.page_asks();
}

/// 「仍然退出」: Canager quits now, whatever is under way, through tauri's
/// own `AppHandle::exit`, which asks AppKit nothing and ends in
/// `RunEvent::Exit` as a quit from the menu does, the window's size and
/// place saved with it. Also the page's answer when a quit it was asked
/// about finds nothing left to wait for. A quit that comes before this one
/// has ended quits too (`confirm`).
#[tauri::command]
pub fn quit_anyway(app: AppHandle) {
    app.state::<QuitGuard>().confirm();
    app.exit(0);
}

/// What the method added to AppKit's delegate asks at every quit
/// (`should_quit`, handed over by `guard_quitting`), or nothing yet: then
/// it answers that Canager quits. Behind a lock only so that it can be
/// set as Canager starts, and by the tests; taken out before it is
/// called, so that nothing it does can wait on this lock.
#[cfg(target_os = "macos")]
type Decide = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

#[cfg(target_os = "macos")]
static DECIDE: std::sync::Mutex<Option<Decide>> = std::sync::Mutex::new(None);

/// Makes every quit ask `should_quit` first: adds
/// `applicationShouldTerminate:` to the class of AppKit's application
/// delegate -- tao's, which has none -- answering `NSTerminateNow` when
/// `should_quit` says Canager quits and `NSTerminateCancel` when it does
/// not. AppKit looks for the method at each `terminate:`, not only as
/// the delegate is set, so one added after launch is asked -- as it was
/// on macOS 27, for `terminate:` and for the quit Apple event with and
/// without a logout's reason, in a program of AppKit's alone, outside
/// the tests. Called once, from `run()`'s setup, on the main thread;
/// should the method not go in -- no delegate, or one that already has the
/// method, which is left as it is -- quitting stays as it was, and the
/// reason is logged.
#[cfg(target_os = "macos")]
pub fn guard_quitting<R: Runtime>(app: &AppHandle<R>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("[canager] quitting asks nothing: not set up on the main thread");
        return;
    };
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
        eprintln!("[canager] quitting asks nothing: AppKit has no application delegate");
        return;
    };
    let app = app.clone();
    *DECIDE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(std::sync::Arc::new(move || should_quit(&app)));
    let delegate = AsRef::<objc2::runtime::AnyObject>::as_ref(&*delegate);
    if let Err(e) = add_should_terminate(delegate.class()) {
        eprintln!("[canager] quitting asks nothing: {e}");
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
/// answer, and Canager quits when there is none to ask or it panics --
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
/// the class already has one of its own.
#[cfg(target_os = "macos")]
fn add_should_terminate(class: &objc2::runtime::AnyClass) -> Result<(), String> {
    use objc2::encode::Encode;
    use objc2::runtime::{AnyClass, Imp};
    use objc2_app_kit::NSApplicationTerminateReply;

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
    use canager_core::model::{ArtifactKind, CancelPolicy, OpKind};

    fn op(id: u64, status: OpStatus) -> OpSummary {
        OpSummary {
            id,
            kind: OpKind::Upgrade,
            instance_id: "brew:/opt/homebrew".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: format!("tool-{id}"),
            status,
            outcome: None,
            argv_preview: Vec::new(),
            cancel_policy: CancelPolicy::KillThenReconcile,
        }
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
        assert_eq!(on_quit(1, true, true), OnQuit::Quit, "「仍然退出」");
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
        guard.page_asks();
        assert_eq!(guard.decide(2), OnQuit::Ask);
        assert_eq!(guard.decide(0), OnQuit::Quit);
        assert_eq!(
            guard.decide(2),
            OnQuit::Ask,
            "「继续等待」 changes nothing here"
        );
        guard.confirm();
        assert_eq!(
            guard.decide(2),
            OnQuit::Quit,
            "「仍然退出」, and every quit after it"
        );
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
            "CanagerQuitGuardTestDelegate{}",
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
            "nothing to ask yet: Canager quits"
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
