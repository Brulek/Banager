//! The window stays on Banager's own page: any navigation to another
//! address is refused, whoever asks for it.
//!
//! The page's content security policy (`src-tauri/tauri.conf.json`,
//! `connect-src 'self'`) stops its requests, not a navigation: a page that
//! ran a script of someone else's could set `location` to an address of the
//! sender's, with what it had read -- the snapshot, the folders on `PATH` --
//! in the address, and Tauri allows that unless a hook says no. This is
//! that hook (`Builder::on_navigation`). A page on another address would
//! be given none of Banager's commands (Tauri checks the origin of every
//! call, and `capabilities/default.json` names none but Banager's own), so
//! this closes the way out, not a way in. Nothing in Banager's own page
//! links away from it: no homepage link ships (docs/what-we-run.md,
//! Network).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, Url};

/// Whether the window may load `url`: Banager's own page, which a built app
/// serves as `tauri://localhost/...`, and nothing else. A development
/// build (`dev`: `pnpm tauri dev` and `pnpm tauri:mock`) also loads the
/// page from Vite's server on this Mac, which is `localhost`. On Windows
/// (on the roadmap) Tauri serves the built page as `http(s)://tauri.localhost`
/// instead (`tauri_protocol_url` in the tauri crate), so that address is
/// Banager's own page there and no other system's.
pub(crate) fn page_may_load(url: &Url, dev: bool) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => match url.host_str() {
            Some("localhost" | "127.0.0.1") => dev,
            Some("tauri.localhost") => cfg!(windows),
            _ => false,
        },
        "about" => url.as_str() == "about:blank",
        _ => false,
    }
}

/// The plugin that carries the hook; `run()` registers it.
pub fn stay_on_the_page<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("banager-navigation")
        .on_navigation(|_webview, url| page_may_load(url, tauri::is_dev()))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn may(url: &str, dev: bool) -> bool {
        page_may_load(&Url::parse(url).unwrap(), dev)
    }

    #[test]
    fn test_a_built_app_loads_its_own_page_and_no_other_address() {
        for dev in [false, true] {
            assert!(may("tauri://localhost/", dev));
            assert!(may("tauri://localhost/index.html", dev));
            assert!(may("about:blank", dev));
            for elsewhere in [
                "https://example.com/",
                "http://example.com/?paths=%2FUsers%2Fsomeone",
                "https://localhost.example.com/",
                "http://localhost.evil.test/",
                "tauri://example.com/",
                "ftp://example.com/",
                "file:///etc/hosts",
                "data:text/html,<script>1</script>",
                "javascript:alert(1)",
                "about:srcdoc",
                "blob:https://example.com/1",
            ] {
                assert!(!may(elsewhere, dev), "{elsewhere}");
            }
        }
    }

    #[test]
    fn test_only_a_development_build_loads_the_page_from_this_macs_vite_server() {
        for local in [
            "http://localhost:1420/",
            "http://localhost:1440/",
            "http://127.0.0.1:1420/",
            "https://localhost:1420/",
        ] {
            assert!(may(local, true), "{local}");
            assert!(!may(local, false), "{local}");
        }
        // Windows' address for the built page is no other system's.
        for dev in [false, true] {
            assert_eq!(may("http://tauri.localhost/", dev), cfg!(windows));
        }
        // Not even a development build follows a navigation to another host.
        assert!(!may("http://192.168.1.2:1420/", true));
        assert!(!may("http://localhost@example.com/", true));
    }

    #[test]
    fn test_run_registers_the_hook() {
        // The hook does nothing unless `run()` registers its plugin.
        let lib = include_str!("lib.rs");
        assert!(
            lib.contains(".plugin(navigation::stay_on_the_page())"),
            "src-tauri/src/lib.rs does not register navigation::stay_on_the_page"
        );
    }
}
