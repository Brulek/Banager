mod auto_check;
pub mod events;
mod ipc;
mod menu;
mod notify;
// `pub` (deviation from the brief's literal `mod state;`, recorded in the
// task report): `AppState::new` is now called for real below, but its
// `get_settings`/`set_settings` methods are only exercised by this module's
// own `#[cfg(test)]` tests until Task 8's IPC commands call them, and a
// private `mod state;` would make clippy's dead-code lint flag those methods
// as unused in the meantime (`-D warnings` is clean otherwise). Task 6 hit
// and fixed the identical issue for `events` (see 26cb50c) by making that
// module pub for the same reason.
pub mod state;
mod window;

use state::AppState;
use tauri::Manager;
use tauri_plugin_window_state::StateFlags;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if fix_path_env::fix().is_err() {
        eprintln!("[canager] failed to fix PATH; falling back to the process's default PATH");
    }
    let host_env = canager_core::runner::HostEnv::discover();
    println!("[canager] discovered PATH dirs: {:?}", host_env.path_dirs);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The update notification's plugin (notify.rs): asked for
        // permission to post, and, off a Mac, posting through. The page is
        // given one of its commands, the one the plugin's own script calls
        // as the page loads (capabilities/default.json); Canager's
        // commands in notify.rs do the rest.
        .plugin(tauri_plugin_notification::init())
        // The window opens as big as it was when Canager last quit, and
        // where it was if a display is still there -- zoomed or in full
        // screen, if it was -- as a Mac app's does; with nothing saved yet,
        // at tauri.conf.json's size, centred. Saved as the app quits, to
        // `.window-state.json` in the app's config folder, on macOS the
        // folder `settings.json` is in (docs/what-we-run.md, "Files
        // Canager writes"). A window closed before then was only hidden
        // (window.rs), so it is still there to be saved, with the size and
        // position it had. Not whether the window is shown, or its title
        // bar: it always opens shown, with the title bar tauri.conf.json
        // gives it. All in Rust -- restored as the window is created, saved
        // on quit -- so the page is given none of the plugin's commands:
        // capabilities/default.json has no `window-state:` permission.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    StateFlags::SIZE
                        | StateFlags::POSITION
                        | StateFlags::MAXIMIZED
                        | StateFlags::FULLSCREEN,
                )
                .build(),
        )
        // The cask icons `ipc::artifact_icon` hands the window, drawn by
        // macOS once per app folder and kept in memory until Canager quits.
        // Managed beside `AppState`, not in it: nothing but that command
        // reads it.
        .manage(std::sync::Arc::new(canager_core::icon::AppIcons::real()))
        .setup(|app| {
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let channel_sink = events::ChannelSink::new();
            app.manage(AppState::new(settings_path, channel_sink));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                ipc::refresh_on_background_change(&handle.state::<AppState>()).await
            });
            // The daily check (auto_check.rs): checks nothing while
            // `Settings::auto_check` is off, which it is by default.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auto_check::check_automatically(&handle.state::<AppState>()).await
            });
            // The menu bar (menu.rs), up before the window has loaded, in
            // the language the page is about to choose; the page then says
            // which it did (`menu::set_menu_language`).
            let language = menu::initial_language(
                app.state::<AppState>().get_settings().language,
                &menu::preferred_languages(),
            );
            menu::show(app.handle(), language)?;
            Ok(())
        })
        // Canager's own menu bar in place of tauri's default, which `setup`
        // above would only replace. Its state is managed here, on the
        // builder, so that it is there before the page can name a language.
        .enable_macos_default_menu(false)
        .manage(menu::MenuBar::default())
        // Its items that act in the page bring the window back and tell it;
        // macOS carries out the rest itself.
        .on_menu_event(|app, event| menu::forward_to_page(app, event.id().as_ref()))
        // Closing the window hides it, and Canager keeps running until it
        // quits (window.rs).
        .on_window_event(window::on_window_event)
        .invoke_handler(tauri::generate_handler![
            ipc::get_snapshot,
            ipc::refresh,
            ipc::plan_operation,
            ipc::submit_operation,
            ipc::cancel_operation,
            ipc::list_operations,
            ipc::get_settings,
            ipc::set_settings,
            ipc::subscribe_events,
            ipc::open_ollama_app,
            ipc::scan_unknown,
            ipc::artifact_icon,
            menu::set_menu_language,
            notify::report_update_set,
            notify::request_notification_permission,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // A click on Canager's icon in the Dock brings a closed window back
        // (window.rs).
        .run(window::on_run_event);
}
