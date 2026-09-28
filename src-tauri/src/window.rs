//! The window in a running app, as a Mac app with one window has it
//! (Music, CleanMyMac): closing it -- its red button, or Close Window (⌘W)
//! -- takes it off the screen and leaves Canager running. An operation
//! under way carries on, and so does the page, hidden with the window: it
//! still hears how the operation went, checks again once it has finished
//! and keeps the Dock icon's badge up to date. The Dock icon brings the
//! window back as it was left. Canager quits only when asked to: Quit
//! Canager (⌘Q), or Quit in the Dock icon's menu.
//!
//! What each of those does is decided by `on_close` and `on_reopen`, plain
//! functions a test can call without a running app; `on_window_event` and
//! `on_run_event` hand them what tauri reports and carry out their answer.
//!
//! On a Mac only. Elsewhere no Dock icon would bring a hidden window back,
//! so closing the window closes it, and Canager quits with its last
//! window, as tauri has it.

use tauri::{AppHandle, Manager, RunEvent, Runtime, Window, WindowEvent};

/// The window's label: tauri.conf.json's one window, which gives none, so
/// Tauri's default -- the label capabilities/default.json names too.
pub const MAIN_WINDOW: &str = "main";

/// What closing the window does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// The window leaves the screen, and Canager stays the app in front,
    /// its menu bar up, as a Mac app does once its window is closed.
    HideWindow,
    /// In full screen, Canager hides, as Hide Canager (⌘H) hides it, and
    /// macOS moves on from the window's full-screen space. A window taken
    /// off the screen inside that space would leave the space behind,
    /// empty; hidden with Canager, the window stays in it, and comes back
    /// there.
    HideApp,
}

/// What closing the window does: in full screen, Canager hides
/// (`Close::HideApp`); otherwise the window alone leaves the screen
/// (`Close::HideWindow`).
pub fn on_close(full_screen: bool) -> Close {
    if full_screen {
        Close::HideApp
    } else {
        Close::HideWindow
    }
}

/// Whether a click on Canager's icon in the Dock -- or opening Canager in
/// Finder while it runs -- brings the window back: when macOS finds none
/// of Canager's windows on screen (`has_visible_windows`), as once the
/// window is closed or minimized into the Dock, tao has told macOS to
/// leave things as they are, so this brings the window back. With the
/// window on screen, the click only brings Canager to the front, the
/// window with it, and nothing is left to do.
pub fn on_reopen(has_visible_windows: bool) -> bool {
    !has_visible_windows
}

/// Closing the window, by its red button or Close Window: `on_close`'s
/// answer instead of tauri's closing it, which would end the page and,
/// with the last window, Canager. Registered for every window; there is
/// one.
#[cfg(target_os = "macos")]
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != MAIN_WINDOW {
        return;
    }
    api.prevent_close();
    let hidden = window
        .is_fullscreen()
        .and_then(|full_screen| match on_close(full_screen) {
            Close::HideWindow => window.hide(),
            Close::HideApp => window.app_handle().hide(),
        });
    if let Err(e) = hidden {
        eprintln!("[canager] could not hide the window: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn on_window_event<R: Runtime>(_window: &Window<R>, _event: &WindowEvent) {}

/// A click on Canager's icon in the Dock (`RunEvent::Reopen`): the window
/// back, when `on_reopen` says so. No other event of the app's needs
/// anything from here.
#[cfg(target_os = "macos")]
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    if let RunEvent::Reopen {
        has_visible_windows,
        ..
    } = event
    {
        if on_reopen(has_visible_windows) {
            show(app);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn on_run_event<R: Runtime>(_app: &AppHandle<R>, _event: RunEvent) {}

/// Brings the window back, in front and focused: out of the Dock, where
/// Minimize put it, and onto the screen, which closing it took it off --
/// with the page as it was left, since closing only hid it. A window on
/// screen already only comes to the front. For the Dock icon
/// (`on_run_event`) and the menu bar's items that act in the page
/// (`menu::forward_to_page`).
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(e) = shown {
        eprintln!("[canager] could not show the window: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closing_the_window_hides_it_and_in_full_screen_hides_canager() {
        assert_eq!(on_close(false), Close::HideWindow);
        assert_eq!(on_close(true), Close::HideApp);
    }

    #[test]
    fn test_the_dock_icon_brings_the_window_back_only_when_none_is_on_screen() {
        assert!(on_reopen(false));
        assert!(!on_reopen(true));
    }

    /// A window by another label would be closed, not hidden -- and with
    /// it, Canager would quit.
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
