//! The window in a running app, as a Mac app with one window has it
//! (Music, CleanMyMac): closing it -- its red button, or Close Window (⌘W)
//! -- takes it off the screen and leaves Banager running. An operation
//! under way carries on, and so does the page, hidden with the window: it
//! still hears how the operation went, checks again once it has finished
//! and keeps the Dock icon's badge up to date. The Dock icon brings the
//! window back as it was left. Banager quits only when asked to: Quit
//! Banager (⌘Q), or Quit in the Dock icon's menu -- and while an operation
//! is under way, only once the window has asked and the user has answered
//! 「退出」, or the window could not ask (quit.rs).
//!
//! The update notification (notify.rs) is the one exception to "as it was
//! left". Banager hears no click on it, but a click brings Banager to the
//! front, and Banager coming to the front with its window closed or in the
//! Dock while a notification waits on the window (`NotificationPending`)
//! brings the window back on the Updates page -- whatever brought Banager
//! to the front, since Banager cannot tell: the click, ⌘-Tab or the Dock
//! icon.
//!
//! What each of those does is decided by `on_close`, `on_reopen` and
//! `on_activate`, plain functions a test can call without a running app;
//! `on_window_event`, `on_run_event` and `observe_activation` hand them
//! what tauri and AppKit report and carry out their answer.
//!
//! On a Mac only. Elsewhere no Dock icon would bring a hidden window back,
//! so closing the window closes it, and Banager quits with its last
//! window, as tauri has it.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, Window, WindowEvent};

/// The window's label: tauri.conf.json's one window, which gives none, so
/// Tauri's default -- the label capabilities/default.json names too.
pub const MAIN_WINDOW: &str = "main";

/// What closing the window does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// The window leaves the screen, and Banager stays the app in front,
    /// its menu bar up, as a Mac app does once its window is closed.
    HideWindow,
    /// In full screen, Banager hides, as Hide Banager (⌘H) hides it, and
    /// macOS moves on from the window's full-screen space. A window taken
    /// off the screen inside that space would leave the space behind,
    /// empty; hidden with Banager, the window stays in it, and comes back
    /// there.
    HideApp,
}

/// What closing the window does: in full screen, Banager hides
/// (`Close::HideApp`); otherwise the window alone leaves the screen
/// (`Close::HideWindow`).
pub fn on_close(full_screen: bool) -> Close {
    if full_screen {
        Close::HideApp
    } else {
        Close::HideWindow
    }
}

/// Whether a click on Banager's icon in the Dock -- or opening Banager in
/// Finder while it runs -- brings the window back: when macOS finds none
/// of Banager's windows on screen (`has_visible_windows`), as once the
/// window is closed or minimized into the Dock, tao has told macOS to
/// leave things as they are, so this brings the window back. With the
/// window on screen, the click only brings Banager to the front, the
/// window with it, and nothing is left to do.
pub fn on_reopen(has_visible_windows: bool) -> bool {
    !has_visible_windows
}

/// Whether an update notification waits on the window: one has been
/// handed off in this run (`notify::report_update_set`), and the window
/// has not been in front since. Cleared whenever it is: brought back by
/// Banager (`show`, for the Dock icon, the menu bar's items that act in
/// the page and the notification), or given the focus in any other way
/// macOS has of bringing it forward -- out of the Dock, with Banager when
/// it is on screen, a click on it (`on_window_event`). What `on_activate`
/// goes by. Managed on the builder in `run()`, so it is there before
/// anything can read it; in memory only.
#[derive(Debug, Default)]
pub struct NotificationPending(AtomicBool);

impl NotificationPending {
    /// A notification has been handed off.
    pub fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// The window is in front: nothing waits on it any more.
    pub fn clear(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    /// Whether a notification waits on the window.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Whether Banager coming to the front brings the window back on the
/// Updates page: when a notification waits on the window
/// (`NotificationPending`) and the window is off the screen -- closed, or
/// minimized into the Dock. A click on the update notification brings
/// Banager to the front and tells it nothing else (`notify::post`), so
/// this is what the click does; nor can Banager tell the click from ⌘-Tab
/// or its Dock icon, which do the same while a notification waits. With
/// the window on screen, macOS brings it forward with Banager, as it was;
/// with no notification waiting, Banager comes to the front as it always
/// has, and `window_on_screen` is not even called: nothing asks about the
/// window.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn on_activate(notification_pending: bool, window_on_screen: impl FnOnce() -> bool) -> bool {
    notification_pending && !window_on_screen()
}

/// Closing the window, by its red button or Close Window: `on_close`'s
/// answer instead of tauri's closing it, which would end the page and,
/// with the last window, Banager. And the window taking the focus, which
/// puts it in front however it got there: no notification waits on it any
/// more (`NotificationPending`). Registered for every window; there is
/// one.
#[cfg(target_os = "macos")]
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW {
        return;
    }
    if let WindowEvent::Focused(true) = event {
        window.state::<NotificationPending>().clear();
        return;
    }
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    api.prevent_close();
    // The close is already prevented, so a failure here must still end in
    // something hidden: a window that ignores its red button can only be
    // quit. Not knowing whether it is full screen counts as not; a window
    // that will not hide hides the app instead.
    let full_screen = window.is_fullscreen().unwrap_or_else(|e| {
        eprintln!("[banager] could not ask whether the window is full screen: {e}");
        false
    });
    let hidden = match on_close(full_screen) {
        Close::HideWindow => window.hide().or_else(|_| window.app_handle().hide()),
        Close::HideApp => window.app_handle().hide(),
    };
    if let Err(e) = hidden {
        eprintln!("[banager] could not hide the window: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn on_window_event<R: Runtime>(_window: &Window<R>, _event: &WindowEvent) {}

/// A click on Banager's icon in the Dock (`RunEvent::Reopen`): the window
/// back, when `on_reopen` says so -- on the Updates page when a
/// notification waits on it (`on_activate`), as when Banager comes to the
/// front (`observe_activation`), which a click on the Dock icon also
/// brings it to. Whichever of the two macOS reports first, the window
/// comes back the same, and only once: bringing it back clears what
/// waits on it (`show`). No other event of the app's needs anything from
/// here.
#[cfg(target_os = "macos")]
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    if let RunEvent::Reopen {
        has_visible_windows,
        ..
    } = event
    {
        let pending = app.state::<NotificationPending>().get();
        if on_activate(pending, || has_visible_windows) {
            crate::notify::open_updates(app);
        } else if on_reopen(has_visible_windows) {
            show(app);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn on_run_event<R: Runtime>(_app: &AppHandle<R>, _event: RunEvent) {}

/// Watches, from launch until Banager quits, for Banager coming to the
/// front: AppKit's `NSApplicationDidBecomeActiveNotification`, which it
/// posts on the main thread whatever brought Banager there -- a click on
/// its notification, ⌘-Tab, its Dock icon -- and which `on_activated`
/// handles. Called once, from `run()`'s setup.
#[cfg(target_os = "macos")]
pub fn observe_activation<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    // SAFETY: an extern static of AppKit's, a constant string that lives
    // as long as the process.
    let name = unsafe { objc2_app_kit::NSApplicationDidBecomeActiveNotification };
    observe(name, move || on_activated(&app));
}

#[cfg(not(target_os = "macos"))]
pub fn observe_activation<R: Runtime>(_app: &AppHandle<R>) {}

/// Runs `on_notice` each time `name` is posted to Foundation's default
/// notification center, from any object, on the thread that posts it and
/// before the post returns, for as long as the process runs: the observer
/// is never removed.
#[cfg(target_os = "macos")]
fn observe(
    name: &objc2_foundation::NSNotificationName,
    on_notice: impl Fn() + Send + Sync + 'static,
) {
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use std::ptr::NonNull;

    let block = block2::RcBlock::new(move |_: NonNull<NSNotification>| on_notice());
    // SAFETY: no object is given, so none can be of the wrong type; with
    // no queue, the block runs on the thread that posts, whichever that
    // is, which `on_notice` being `Send` and `Sync` allows.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(name),
            None,
            None,
            &block,
        )
    };
    // The center keeps the observer, and its own copy of the block, until
    // the observer is removed, which it never is.
    drop(observer);
}

/// Banager has come to the front: the window back on the Updates page
/// when `on_activate` says so. On the main thread, where AppKit posts the
/// notification. Only while a notification waits is the window asked
/// about; every other time Banager comes to the front, nothing here
/// touches the window.
#[cfg(target_os = "macos")]
fn on_activated<R: Runtime>(app: &AppHandle<R>) {
    let pending = app.state::<NotificationPending>().get();
    if on_activate(pending, || window_on_screen(app)) {
        crate::notify::open_updates(app);
    }
}

/// Whether the window is on screen: shown, and not minimized into the
/// Dock; behind another app's windows counts. Not knowing counts as on
/// screen, which leaves the window as it is.
#[cfg(target_os = "macos")]
fn window_on_screen<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return true;
    };
    match (window.is_visible(), window.is_minimized()) {
        (Ok(visible), Ok(minimized)) => visible && !minimized,
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("[banager] could not ask whether the window is on screen: {e}");
            true
        }
    }
}

/// Brings the window back, in front and focused: out of the Dock, where
/// Minimize put it, and onto the screen, which closing it took it off --
/// with the page as it was left, since closing only hid it. A window on
/// screen already only comes to the front. No notification waits on it
/// any more (`NotificationPending`). For the Dock icon (`on_run_event`),
/// and through `show_and_tell` for the menu bar's items that act in the
/// page and the update notification.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    app.state::<NotificationPending>().clear();
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(e) = shown {
        eprintln!("[banager] could not show the window: {e}");
    }
}

/// Brings the window back (`show`), then tells the page `event`, to the
/// window alone: closed, or in the Dock, it would show nothing of what the
/// page does for it. What the menu bar's items that act in the page do
/// (`menu::forward_to_page`), and Banager coming to the front while the
/// update notification waits on the window (`notify::open_updates`). An
/// error is the event's: the window is shown whatever becomes of it.
pub fn show_and_tell<R: Runtime>(app: &AppHandle<R>, event: &str) -> tauri::Result<()> {
    show_and_send(app, event, ())
}

/// `show_and_tell`, with `payload` for the page to read along with
/// `event`: the question a quit asks, by its number (`quit::should_quit`).
pub fn show_and_send<R: Runtime, S: serde::Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
) -> tauri::Result<()> {
    show(app);
    app.emit_to(MAIN_WINDOW, event, payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closing_the_window_hides_it_and_in_full_screen_hides_banager() {
        assert_eq!(on_close(false), Close::HideWindow);
        assert_eq!(on_close(true), Close::HideApp);
    }

    #[test]
    fn test_the_dock_icon_brings_the_window_back_only_when_none_is_on_screen() {
        assert!(on_reopen(false));
        assert!(!on_reopen(true));
    }

    #[test]
    fn test_coming_to_the_front_opens_updates_only_while_a_notification_waits_on_a_window_off_screen(
    ) {
        assert!(on_activate(true, || false));
        assert!(
            !on_activate(true, || true),
            "on screen, the window comes forward with Banager, as it was"
        );
        assert!(
            !on_activate(false, || -> bool {
                panic!("the window was asked about with no notification waiting")
            }),
            "⌘-Tab to Banager with its window closed and nothing waiting: as before"
        );
    }

    /// `observe_activation`'s registration, with a name of the test's own
    /// in place of AppKit's: Foundation only, no app and no window.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_an_observer_runs_at_each_post_of_its_name_on_the_posting_thread_before_it_returns() {
        use objc2_foundation::{NSNotificationCenter, NSString};
        use std::sync::{Arc, Mutex};

        let name = NSString::from_str(&format!(
            "BanagerTestNotice-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let heard = Arc::new(Mutex::new(Vec::new()));
        observe(&name, {
            let heard = heard.clone();
            move || heard.lock().unwrap().push(std::thread::current().id())
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: no object is given, so none can be of the wrong type.
        unsafe { center.postNotificationName_object(&name, None) };
        assert_eq!(
            *heard.lock().unwrap(),
            [std::thread::current().id()],
            "heard once, on this thread, before the post returned"
        );
        // SAFETY: as above.
        unsafe { center.postNotificationName_object(&name, None) };
        assert_eq!(heard.lock().unwrap().len(), 2, "and at each post after");
        // Another name is not heard.
        let other = NSString::from_str("BanagerTestNoticeNobodyWatches");
        // SAFETY: as above.
        unsafe { center.postNotificationName_object(&other, None) };
        assert_eq!(heard.lock().unwrap().len(), 2);
    }

    #[test]
    fn test_a_notification_waits_on_the_window_from_its_hand_off_until_the_window_is_in_front() {
        let pending = NotificationPending::default();
        assert!(!pending.get(), "nothing waits as Banager opens");
        pending.set();
        pending.set();
        assert!(pending.get());
        pending.clear();
        assert!(
            !pending.get(),
            "once in front answers every notification handed off before"
        );
    }

    /// A window by another label would be closed, not hidden -- and with
    /// it, Banager would quit.
    #[test]
    fn test_main_window_is_the_label_of_the_apps_one_window() {
        let config: tauri::utils::config::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let labels: Vec<&str> = config
            .app
            .windows
            .iter()
            .map(|window| window.label.as_str())
            .collect();
        assert_eq!(labels, [MAIN_WINDOW]);
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!([MAIN_WINDOW]));
    }
}
