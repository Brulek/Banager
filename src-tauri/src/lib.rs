pub mod events;
mod ipc;
// `pub` (deviation from the brief's literal `mod state;`, recorded in the
// task report): `AppState::new` is now called for real below, but its
// `get_settings`/`set_settings` methods are only exercised by this module's
// own `#[cfg(test)]` tests until Task 8's IPC commands call them, and a
// private `mod state;` would make clippy's dead-code lint flag those methods
// as unused in the meantime (`-D warnings` is clean otherwise). Task 6 hit
// and fixed the identical issue for `events` (see 26cb50c) by making that
// module pub for the same reason.
pub mod state;

use state::AppState;
use tauri::Manager;

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
        .setup(|app| {
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let channel_sink = events::ChannelSink::new();
            app.manage(AppState::new(settings_path, channel_sink));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                ipc::refresh_on_background_change(&handle.state::<AppState>()).await
            });
            Ok(())
        })
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
