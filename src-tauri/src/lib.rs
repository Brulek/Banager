// `pub` because this crate has a library target (`canager_lib`): items of a
// public module are reachable API, so the dead-code lint does not fire in the
// window between this module existing and Task 7 wiring it into `AppState`.
pub mod events;
// `pub` for the same reason as `events` above: `AppState`, its fields and
// its `get_settings`/`set_settings` methods are only exercised by this
// module's own `#[cfg(test)]` tests until Task 8's IPC commands call them
// from `run()`'s wiring, and a private `mod state;` would make clippy's
// dead-code lint flag the struct/fields/methods as unused in the meantime
// (see 26cb50c, which did the same for `events` in Task 6).
pub mod state;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

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
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
