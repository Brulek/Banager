//! One writer for the app-data directory; no lock file is created.
use banager_core::settings::lock_directory as acquire;

/// The held directory, managed for the app's whole life: dropping it, as
/// the process ends, is what lets the next launch take it.
struct DirectoryOwner<T> {
    _held: T,
}

/// What a launch does with the lock it tried to take.
#[derive(Debug)]
enum Launch<T> {
    /// This instance owns the app-data directory until it quits: `T` is
    /// the open directory `acquire` locked.
    Own(T),
    /// Another instance owns it: have macOS open that one again, which
    /// brings its window back (`reopen_existing`), and quit.
    Defer,
    /// Not taken for another reason -- a file system without locks, a
    /// directory that cannot be made. Banager started then before the lock
    /// existed, so it still starts, without it, rather than not at all.
    Unlocked(std::io::Error),
}

fn launch<T>(result: std::io::Result<T>) -> Launch<T> {
    match result {
        Ok(lock) => Launch::Own(lock),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Launch::Defer,
        Err(error) => Launch::Unlocked(error),
    }
}

/// Register before plugins that read or save app data. Keep the descriptor
/// for the entire app lifetime, including history flush and window-state save.
pub fn guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    use tauri::Manager;
    tauri::plugin::Builder::new("data-directory-owner")
        .setup(|app, _| {
            let directory = app.path().app_data_dir()?;
            match launch(acquire(&directory)) {
                Launch::Own(lock) => {
                    app.manage(DirectoryOwner { _held: lock });
                }
                Launch::Defer => {
                    reopen_existing(&app.config().identifier);
                    // No state/plugin writer has started in this process.
                    std::process::exit(0);
                }
                Launch::Unlocked(error) => {
                    eprintln!(
                        "[banager] could not lock the app-data folder, starting without it: {error}"
                    );
                }
            }
            Ok(())
        })
        .build()
}

/// Brings the Banager already running forward the way Finder does when
/// Banager is opened while it runs: has macOS open that copy's own bundle
/// (`NSWorkspace openURL:` with its `bundleURL`, the app LaunchServices
/// lists as running under Banager's bundle identifier -- no address from
/// anywhere else), which LaunchServices answers by sending the running
/// copy the reopen event a click on its Dock icon sends: tauri's
/// `RunEvent::Reopen`, on which that copy brings its window back, closed
/// or not (`window::on_run_event`, `on_reopen`). This launch may be a copy
/// at another path -- the disk image's, then the one in Applications -- or
/// at the same one (`open -n`). Asking macOS only to activate the running
/// copy (`activateWithOptions:`) sent no reopen event, so a closed window
/// stayed closed, and macOS may refuse that request from an app not yet in
/// front. A running copy without a bundle -- a bare binary -- is asked to
/// activate as before. No command runs and nothing is written; nothing
/// asks for a permission.
#[cfg(target_os = "macos")]
fn reopen_existing(identifier: &str) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
    use objc2_foundation::NSString;
    let current = NSRunningApplication::currentApplication();
    let applications = NSRunningApplication::runningApplicationsWithBundleIdentifier(
        &NSString::from_str(identifier),
    );
    let workspace = NSWorkspace::sharedWorkspace();
    // `==` is `isEqual:`, which Apple names for telling two running
    // applications apart.
    for application in applications.iter().filter(|app| **app != *current) {
        let reopened = application
            .bundleURL()
            .is_some_and(|bundle| workspace.openURL(&bundle));
        if !reopened {
            application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn reopen_existing(_identifier: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn directory_lock_child() {
        let Some(path) = std::env::var_os("BANAGER_TEST_LOCK_DIRECTORY") else {
            return;
        };
        // Held by the parent process: this launch defers to it.
        assert!(matches!(launch(acquire(Path::new(&path))), Launch::Defer));
    }

    #[test]
    fn a_deferring_launch_reopens_the_running_copy_rather_than_only_activating_it() {
        // r38 S5: with the running copy's window closed, activating that
        // copy showed nothing; opening its bundle has macOS send it the
        // reopen event, which brings the window back. What AppKit does is
        // not run here (no app is launched in a test): this pins the call.
        let production = include_str!("instance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(production.contains("reopen_existing(&app.config().identifier)"));
        assert!(production.contains(".bundleURL()"));
        assert!(production.contains("workspace.openURL(&bundle)"));
    }

    #[test]
    fn a_lock_that_cannot_be_taken_for_another_reason_still_starts() {
        // A data folder that cannot be made (here: under a regular file)
        // fails with something other than WouldBlock. Banager started in
        // that case before the lock existed, with default settings, and
        // must still start rather than fail its setup.
        let file = std::env::temp_dir().join(format!(
            "banager-instance-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&file, b"not a folder").unwrap();
        let outcome = launch(acquire(&file.join("com.brulek.banager")));
        assert!(
            matches!(&outcome, Launch::Unlocked(error) if error.kind() != std::io::ErrorKind::WouldBlock),
            "{outcome:?}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"not a folder");
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn one_writer_across_processes_without_an_extra_file() {
        let directory = std::env::temp_dir().join(format!(
            "banager-instance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("history.json"),
            br#"{"format":1,"records":[]}"#,
        )
        .unwrap();
        std::fs::write(
            directory.join("settings.json"),
            br#"{"language":"ZhHant","welcome_seen":true}"#,
        )
        .unwrap();
        let guard = acquire(&directory).unwrap();
        assert!(matches!(launch(acquire(&directory)), Launch::Defer));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "instance::tests::directory_lock_child"])
            .env("BANAGER_TEST_LOCK_DIRECTORY", &directory)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        drop(guard);
        let next = acquire(&directory).unwrap();
        assert_eq!(
            std::fs::read(directory.join("history.json")).unwrap(),
            br#"{"format":1,"records":[]}"#
        );
        drop(next);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
