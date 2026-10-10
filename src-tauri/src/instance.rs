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
/// front. Only a `.app` folder is opened (`is_app_bundle`). A copy run as
/// a bare binary (`tauri dev`, `pnpm tauri:mock`) has no bundle
/// identifier, so macOS does not list it here at all, and nothing is done
/// for it; were one listed, its `bundleURL` would be the executable
/// itself, which `openURL:` would hand to whatever opens such a file --
/// Terminal, which runs it -- so it is asked only to activate, as before,
/// as is a copy with no `bundleURL`, or one `openURL:` refuses. No command
/// runs and nothing is written; nothing asks for a permission.
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
        let reopened = application.bundleURL().is_some_and(|bundle| {
            let extension = bundle.pathExtension().map(|ext| ext.to_string());
            is_app_bundle(extension.as_deref()) && workspace.openURL(&bundle)
        });
        if !reopened {
            application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn reopen_existing(_identifier: &str) {}

/// Whether a running copy's `bundleURL`, by its path extension
/// (`extension`, `pathExtension`), is an app bundle -- a `.app` folder,
/// which `openURL:` hands to LaunchServices to reopen -- rather than a bare
/// executable, whose extension is empty.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_app_bundle(extension: Option<&str>) -> bool {
    extension.is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Takes the lock just released on `directory`. Other tests in this
    /// binary spawn processes at the same time, and a spawned process holds
    /// a copy of every open descriptor, the `O_CLOEXEC` one too, until macOS
    /// has run its exec; a flock lasts while any copy is open, so a release
    /// can be seen a moment late. Only that answer, `WouldBlock`, is retried,
    /// and for at most 5 s: a lock never released still fails.
    fn acquire_after_release(directory: &Path) -> std::fs::File {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match acquire(directory) {
                Ok(lock) => return lock,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("the released lock was not taken again: {error:?}"),
            }
        }
    }

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
    fn only_an_app_bundle_is_handed_to_open_url() {
        // r38 skeptic 3: a running copy built as a bare binary (`tauri
        // dev`, `pnpm tauri:mock`) has as its `bundleURL` the executable
        // itself, and `openURL:` would hand that to whatever opens such a
        // file -- Terminal, which runs it. Only a `.app` folder is opened;
        // anything else is asked to activate, as before r38 S5.
        assert!(is_app_bundle(Some("app")));
        assert!(is_app_bundle(Some("APP")));
        assert!(!is_app_bundle(Some("")));
        assert!(!is_app_bundle(None));
        assert!(!is_app_bundle(Some("command")));
        let production = include_str!("instance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(production.contains("bundle.pathExtension()"));
        // `&&`: the bundle is opened only once it is a `.app` folder.
        assert!(production
            .contains("is_app_bundle(extension.as_deref()) && workspace.openURL(&bundle)"));
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
        let next = acquire_after_release(&directory);
        assert_eq!(
            std::fs::read(directory.join("history.json")).unwrap(),
            br#"{"format":1,"records":[]}"#
        );
        drop(next);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_released_lock_is_taken_again_while_other_threads_spawn() {
        // r16: the test above once failed in a full run, its lock not taken
        // again right after `drop(guard)`. Other tests in this binary spawn
        // processes at the same time (`/bin/echo`, `/usr/bin/false`, a
        // probe script), and each spawned process holds a copy of every
        // open descriptor, the `O_CLOEXEC` one too, until macOS has run its
        // exec; a flock lasts while any copy is open. This repeats that,
        // and `acquire_after_release` waits that moment out.
        let directory = std::env::temp_dir().join(format!(
            "banager-instance-spawn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = std::process::Command::new("/usr/bin/true")
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                })
            })
            .collect();
        for _ in 0..300 {
            drop(acquire_after_release(&directory));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for spawner in spawners {
            spawner.join().unwrap();
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
