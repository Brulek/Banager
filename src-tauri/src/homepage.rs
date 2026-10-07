//! The details panel's homepage (`open_homepage`): a click on a tool's
//! homepage opens it in the default browser -- a homepage a source listed
//! for a tool in the current snapshot, and nothing else.
//!
//! The window sends the address it shows, and is refused any other: the
//! address must be, exactly, the homepage some tool in the snapshot this
//! side holds lists (`InstalledArtifact::homepage`, trimmed as the page
//! trims it), and an `http` or `https` address with a host. So a page
//! that ran someone else's script could have the browser open only a page
//! Banager itself listed -- never an address of the script's own, carrying
//! what the page has read -- and never a `file:`, an app's own scheme, or
//! anything else macOS would hand to another application.
//!
//! The browser is asked through AppKit (`NSWorkspace openURL:`, as
//! `reveal.rs` asks Finder): no command runs, and the opener plugin, which
//! opens any URL or path it is given, stays out of the app (decision S4).
//! Banager itself connects to nothing: the browser loads the page. The
//! window never leaves Banager's page either (`navigation.rs`).

use banager_core::session::Snapshot;
use tauri::{State, Url};

/// The refusal of an address that is not, exactly, a homepage a tool in
/// the current snapshot lists, in the `{"kind": ...}` envelope every
/// command's refusals use.
fn not_listed_json() -> String {
    serde_json::json!({ "kind": "not_listed" }).to_string()
}

/// The refusal of a homepage a tool lists that is not an `http` or `https`
/// address with a host -- what a tap's formula may say -- which the
/// default browser is not asked to open.
fn not_web_json() -> String {
    serde_json::json!({ "kind": "not_web" }).to_string()
}

/// `address`, as the default browser may be asked to open it: exactly --
/// no case folded, no slash added -- the homepage, trimmed, of a tool in
/// `snapshot`, and an `http` or `https` address with a host, parsed. An
/// empty address names no homepage. Refused as `not_listed` otherwise,
/// before anything is parsed, and as `not_web` when the homepage listed is
/// another kind of address.
pub(crate) fn listed_homepage(snapshot: &Snapshot, address: &str) -> Result<Url, String> {
    let listed = !address.is_empty()
        && snapshot
            .artifacts
            .iter()
            .filter_map(|artifact| artifact.homepage.as_deref())
            .any(|homepage| homepage.trim() == address);
    if !listed {
        return Err(not_listed_json());
    }
    let url = Url::parse(address).map_err(|_| not_web_json())?;
    let has_host = url.host_str().is_some_and(|host| !host.is_empty());
    if !matches!(url.scheme(), "http" | "https") || !has_host {
        return Err(not_web_json());
    }
    Ok(url)
}

/// `open_homepage`'s whole effect: `open` asked of `address` when it is a
/// homepage the current snapshot lists (`listed_homepage`), and nothing at
/// all otherwise. `open` is a parameter so a test never opens a browser.
pub(crate) fn open_homepage_impl(
    snapshot: &Snapshot,
    address: &str,
    open: impl FnOnce(&Url) -> Result<(), String>,
) -> Result<(), String> {
    let url = listed_homepage(snapshot, address)?;
    open(&url).map_err(|detail| {
        serde_json::json!({ "kind": "open_failed", "detail": detail }).to_string()
    })
}

/// The default browser, on `url`: one call to AppKit, `NSWorkspace
/// openURL:`, which hands an `http` or `https` URL to whichever browser
/// the Mac has as its default (System Settings → Desktop & Dock → Default
/// web browser). The URL as `listed_homepage` parsed it -- so the scheme
/// macOS reads is the one checked -- and nothing runs. In a pool of its
/// own (`autoreleasepool`), drained when the call returns, as
/// `reveal::show_in_finder`'s is: the command runs on one of the async
/// runtime's threads, which has none.
#[cfg(target_os = "macos")]
fn open_in_browser(url: &Url) -> Result<(), String> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};
    autoreleasepool(|_| {
        let url = NSURL::URLWithString(&NSString::from_str(url.as_str()))
            .ok_or_else(|| "macOS could not read the address".to_string())?;
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            Ok(())
        } else {
            Err("macOS did not open the address".to_string())
        }
    })
}

#[cfg(not(target_os = "macos"))]
fn open_in_browser(_url: &Url) -> Result<(), String> {
    Err("opening a homepage is only on a Mac".to_string())
}

/// Has the default browser open `address` when it is, exactly, a homepage
/// a tool in the current snapshot lists, and an `http` or `https` address
/// (`listed_homepage`), through AppKit (`open_in_browser`); runs nothing.
/// Any other address is refused, as `not_listed` or `not_web`; one macOS
/// did not open comes back as `open_failed`.
#[tauri::command]
pub async fn open_homepage(
    state: State<'_, crate::state::AppState>,
    address: String,
) -> Result<(), String> {
    open_homepage_impl(&state.session.snapshot(), &address, open_in_browser)
}

#[cfg(test)]
mod tests {
    use super::*;
    use banager_core::model::ArtifactKind;
    use banager_core::session::DetectOutcome;
    use banager_core::testing::installed_artifact;
    use std::cell::RefCell;

    const NOT_LISTED: &str = r#"{"kind":"not_listed"}"#;
    const NOT_WEB: &str = r#"{"kind":"not_web"}"#;

    /// A snapshot listing one tool for each of `homepages`, as the sources
    /// reported them.
    fn snapshot(homepages: &[Option<&str>]) -> Snapshot {
        let artifacts = homepages
            .iter()
            .enumerate()
            .map(|(index, homepage)| {
                let mut artifact = installed_artifact(
                    "brew:/opt/homebrew",
                    ArtifactKind::Formula,
                    &format!("tool-{index}"),
                );
                artifact.homepage = homepage.map(str::to_string);
                artifact
            })
            .collect();
        Snapshot {
            generation: 1,
            round: 1,
            detect: DetectOutcome::Found,
            instances: Vec::new(),
            artifacts,
            updates: Vec::new(),
            refreshed_at: Some(1_790_586_000),
            stale: false,
            errors: Vec::new(),
            next_auto_check_at: None,
        }
    }

    /// `open_homepage_impl` over `snapshot` for each of `addresses`, with
    /// what it would have opened.
    fn opened(snapshot: &Snapshot, addresses: &[&str]) -> (Vec<Result<(), String>>, Vec<String>) {
        let asked = RefCell::new(Vec::new());
        let results = addresses
            .iter()
            .map(|address| {
                open_homepage_impl(snapshot, address, |url| {
                    asked.borrow_mut().push(url.as_str().to_string());
                    Ok(())
                })
            })
            .collect();
        (results, asked.into_inner())
    }

    #[test]
    fn test_a_homepage_a_tool_lists_is_opened_and_nothing_else() {
        let listed = snapshot(&[
            Some("https://jqlang.org"),
            Some("https://code.claude.com/docs/en/setup"),
            None,
        ]);
        let (results, asked) = opened(
            &listed,
            &[
                "https://jqlang.org",
                "https://code.claude.com/docs/en/setup",
            ],
        );
        assert_eq!(results, [Ok(()), Ok(())]);
        assert_eq!(
            asked,
            [
                "https://jqlang.org/",
                "https://code.claude.com/docs/en/setup"
            ]
        );

        // Any address the snapshot does not list, however close to one it
        // does: refused before anything is parsed, and nothing opened.
        let (results, asked) = opened(
            &listed,
            &[
                "https://example.com/?paths=%2FUsers%2Fsomeone",
                "https://jqlang.org/",
                "HTTPS://JQLANG.ORG",
                "http://jqlang.org",
                "https://jqlang.org.example.com",
                "https://jqlang.org#x",
                " https://jqlang.org",
                "https://code.claude.com/docs/en",
                "",
            ],
        );
        assert!(
            results.iter().all(|r| *r == Err(NOT_LISTED.to_string())),
            "{results:?}"
        );
        assert_eq!(asked, Vec::<String>::new());
    }

    #[test]
    fn test_a_homepage_is_compared_trimmed_as_the_page_shows_it() {
        // The page trims what a source reported (`homepageFact`) and sends
        // that; the same trimmed homepage is what it is compared with.
        let listed = snapshot(&[Some("  https://jqlang.org\n")]);
        let (results, asked) = opened(&listed, &["https://jqlang.org"]);
        assert_eq!(results, [Ok(())]);
        assert_eq!(asked, ["https://jqlang.org/"]);
        // A blank homepage lists nothing, not even an empty address.
        let blank = snapshot(&[Some("   "), Some("")]);
        let (results, asked) = opened(&blank, &["", " "]);
        assert!(
            results.iter().all(|r| *r == Err(NOT_LISTED.to_string())),
            "{results:?}"
        );
        assert!(asked.is_empty());
    }

    #[test]
    fn test_a_listed_homepage_that_is_no_web_address_is_not_opened() {
        // A tap's formula can say anything in its homepage; only an http or
        // https address with a host goes to the browser -- no file, no
        // app's own scheme, nothing macOS would hand another application.
        let odd = [
            "file:///etc/hosts",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ftp://ftp.gnu.org/gnu/wget/",
            "mailto:someone@example.com",
            "x-apple.systempreferences:com.apple.preference.security",
            "vscode://file/Users/someone/.ssh",
            "https://",
            "not an address",
            "/usr/local/bin",
        ];
        let listed = snapshot(&odd.iter().map(|a| Some(*a)).collect::<Vec<_>>());
        let (results, asked) = opened(&listed, &odd);
        assert!(
            results.iter().all(|r| *r == Err(NOT_WEB.to_string())),
            "{results:?}"
        );
        assert_eq!(asked, Vec::<String>::new());
        // Plain http is a web address: some projects' homepages still are.
        let http = snapshot(&[Some("http://www.lua.org/")]);
        let (results, asked) = opened(&http, &["http://www.lua.org/"]);
        assert_eq!(results, [Ok(())]);
        assert_eq!(asked, ["http://www.lua.org/"]);
    }

    #[test]
    fn test_only_the_current_snapshot_counts() {
        // A homepage the window still shows from an older snapshot, gone
        // from the newest one: refused.
        let before = snapshot(&[Some("https://jqlang.org")]);
        let after = snapshot(&[Some("https://www.gnu.org/software/wget/")]);
        assert_eq!(opened(&before, &["https://jqlang.org"]).0, [Ok(())]);
        assert_eq!(
            opened(&after, &["https://jqlang.org"]).0,
            [Err(NOT_LISTED.to_string())]
        );
        // No snapshot yet: nothing at all.
        assert_eq!(
            opened(&snapshot(&[]), &["https://jqlang.org"]).0,
            [Err(NOT_LISTED.to_string())]
        );
    }

    #[test]
    fn test_a_browser_failure_comes_back_in_the_envelope() {
        let listed = snapshot(&[Some("https://jqlang.org")]);
        let err = open_homepage_impl(&listed, "https://jqlang.org", |_| {
            Err("macOS did not open the address".to_string())
        })
        .unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(v["kind"], "open_failed");
        assert_eq!(v["detail"], "macOS did not open the address");
    }

    #[test]
    fn test_run_registers_the_command_and_the_window_gets_no_new_permission() {
        // Banager's own commands are not behind a permission of their own
        // (no app manifest in build.rs), so the window reaches this one with
        // the permissions it had: `lib.rs`'s `window_rights` pins them, and
        // this one adds no opener permission.
        let lib = include_str!("lib.rs");
        assert!(
            lib.contains("homepage::open_homepage,"),
            "src-tauri/src/lib.rs does not register homepage::open_homepage"
        );
        assert_eq!(
            include_str!("../build.rs").trim(),
            "fn main() {\n    tauri_build::build()\n}"
        );
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
            .collect();
        assert!(
            !permissions
                .iter()
                .any(|p| p.starts_with("opener:") || p.starts_with("shell:")),
            "{permissions:?}"
        );
    }
}
