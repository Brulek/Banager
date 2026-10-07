mod auto_check;
pub mod events;
mod history;
mod homepage;
mod ipc;
mod menu;
mod navigation;
mod notify;
mod notify_ops;
mod quit;
mod reveal;
mod shared_run;
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
    tauri::Builder::default()
        // The window never leaves Banager's own page (navigation.rs).
        .plugin(navigation::stay_on_the_page())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The update notification's plugin (notify.rs): asked for
        // permission to post, and, off a Mac, posting through. The page is
        // given one of its commands, the one the plugin's own script calls
        // as the page loads (capabilities/default.json); Banager's
        // commands in notify.rs do the rest.
        .plugin(tauri_plugin_notification::init())
        // The window opens as big as it was when Banager last quit, and
        // where it was if a display is still there -- zoomed or in full
        // screen, if it was -- as a Mac app's does; with nothing saved yet,
        // at tauri.conf.json's size, centred. Saved as the app quits, to
        // `.window-state.json` in the app's config folder, on macOS the
        // folder `settings.json` is in (docs/what-we-run.md, "Files
        // Banager writes"). A window closed before then was only hidden
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
        // macOS once per app folder and kept in memory until Banager quits.
        // Managed beside `AppState`, not in it: nothing but that command
        // reads it.
        .manage(std::sync::Arc::new(banager_core::icon::AppIcons::real()))
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let settings_path = data_dir.join("settings.json");
            let channel_sink = events::ChannelSink::new();
            app.manage(AppState::new(settings_path, channel_sink));
            // 「最近的更新记录」 after a restart: `history.json` beside
            // `settings.json` (history.rs; docs/what-we-run.md, "Files
            // Banager writes").
            history::attach(&app.state::<AppState>(), &data_dir);
            // The login shell's `PATH` and proxy and mirror settings
            // (runner::login_path), read in the background from now on --
            // `$SHELL` (`/bin/zsh` when unset) from the home folder, within
            // `login_path::TIMEOUT` -- so a slow shell never holds the
            // window back. Every refresh waits for it, and reads again when
            // it failed (`read_login_path`). The log names the settings
            // read, never their values: a proxy's can hold a password.
            let shell = std::env::var_os("SHELL")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "/bin/zsh".into());
            let probe = std::sync::Arc::new(banager_core::runner::login_path::LoginPath::new(
                std::sync::Arc::new(banager_core::runner::RealRunner::new()),
                shell,
                banager_core::runner::HostEnv::discover().home,
                banager_core::runner::login_path::TIMEOUT,
                |found: &banager_core::runner::login_path::LoginEnv| {
                    banager_core::runner::login_path::accept(found);
                    println!("[banager] read the login shell's PATH: {}", found.path);
                    if !found.imported.is_empty() {
                        println!(
                            "[banager] and its proxy and mirror settings: {}",
                            found.imported_names().join(", ")
                        );
                    }
                },
            ));
            let _ = app.state::<AppState>().login_path.set(probe);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                state.read_login_path().await;
                if !state.session.login_path_restored() {
                    eprintln!(
                        "[banager] could not read the login shell's PATH; using the process's own until a check reads it"
                    );
                }
            });
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
            // Banager coming to the front -- a click on the update
            // notification brings it there -- with its window closed while
            // a notification waits on the window brings the window back on
            // the Updates page (window.rs).
            window::observe_activation(app.handle());
            // Every way of quitting -- ⌘Q, the Dock's Quit, a logout --
            // asks first while an operation is not done, and the page
            // asks the user (quit.rs). After `AppState`, which it reads.
            quit::guard_quitting(app.handle());
            Ok(())
        })
        // Banager's own menu bar in place of tauri's default, which `setup`
        // above would only replace. Its state is managed here, on the
        // builder, so that it is there before the page can name a language.
        .enable_macos_default_menu(false)
        .manage(menu::MenuBar::default())
        // Whether an update notification waits on the window (window.rs):
        // set as one is handed off, cleared as the window comes back.
        .manage(window::NotificationPending::default())
        // The runs of operations the page has reported finished, for the
        // notification when operations finish (notify_ops.rs).
        .manage(notify_ops::OperationRuns::default())
        // What Show in Finder may show: the programs the newest scan of
        // Other Programs found (reveal.rs).
        .manage(reveal::Revealable::default())
        // Whether a quit asks first (quit.rs): while the page listens for
        // the question, until the user answers 「退出」; and which
        // questions the page has said are on screen.
        .manage(quit::QuitGuard::default())
        // Its items that act in the page bring the window back and tell it;
        // macOS carries out the rest itself.
        .on_menu_event(|app, event| menu::forward_to_page(app, event.id().as_ref()))
        // Closing the window hides it, and Banager keeps running until it
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
            reveal::reveal_in_finder,
            homepage::open_homepage,
            ipc::artifact_icon,
            ipc::get_sizes,
            ipc::get_system_facts,
            history::get_history,
            history::clear_history,
            menu::set_menu_language,
            notify::report_update_set,
            notify::request_notification_permission,
            notify_ops::report_finished_run,
            quit::ask_before_quit,
            quit::quit_question_shown,
            quit::quit_kept_waiting,
            quit::quit_anyway,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // A click on Banager's icon in the Dock brings a closed window back,
        // on the Updates page while a notification waits on it (window.rs).
        // At exit, the history's last records reach `history.json` first.
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                history::flush_on_exit(&app.state::<AppState>());
            }
            window::on_run_event(app, event);
        });
}

/// What the window is allowed to ask of Tauri and of the page's own address,
/// pinned: a change here is a change of what a page that ran someone else's
/// script could do, so it must come with a change of this test, read by a
/// person (docs/what-we-run.md, Network; the security review of round 5).
#[cfg(test)]
mod window_rights {
    fn permissions() -> Vec<String> {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p.as_str()
                    .or_else(|| p["identifier"].as_str())
                    .expect("a permission is a string or has an identifier")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn test_the_window_is_given_exactly_these_permissions() {
        // `core:default` is Tauri's own set of getters, events, menus and
        // paths; `core:image:deny-from-path` takes from it the one command
        // that reads a file by the path it is given, which Tauri leaves out
        // only while no `image-png` or `image-ico` feature is on.
        assert_eq!(
            permissions(),
            [
                "core:default",
                "core:image:deny-from-path",
                "core:window:allow-start-dragging",
                "core:window:allow-set-badge-count",
                "notification:allow-is-permission-granted",
            ]
        );
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert!(
            capability.get("remote").is_none(),
            "no page on another address may be given a command"
        );
    }

    #[test]
    fn test_the_content_security_policy_lets_the_page_load_and_connect_to_itself_only() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            config["app"]["security"]["csp"],
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: asset: https://asset.localhost; connect-src 'self'"
        );
    }
}
