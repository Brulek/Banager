use crate::model::ArtifactKey;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    System,
    En,
    ZhCn,
}

/// One update the user chose to skip with the Updates page's "Skip this
/// version": `key`'s update to `version`, the `UpdateCandidate.target` the
/// source offered when they did. It hides that update only while the source
/// still offers `version`; once it offers another, the row is listed again.
/// So the page offers the button, and lets a stored skip hide a row, only
/// where the target names one release (`canSkipVersion` in
/// src/lib/updateState.ts): not on a row Canager could not check, whose
/// target is its installed version, and not on a Homebrew cask declared
/// `version :latest`, every release of which is offered as "latest", so
/// that a skip of it would never end.
/// For an Ollama model `version` is a registry manifest's config digest (an
/// `UpdateChannel::Digest` candidate's target), which the front end never
/// shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedVersion {
    pub key: ArtifactKey,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub show_technical_details: bool,
    /// The packages the user asked, with "Never remind me", never to be
    /// reminded about: every update of each is hidden, whatever its version.
    pub ignored_updates: Vec<ArtifactKey>,
    /// The single versions the user skipped with "Skip this version"
    /// (`SkippedVersion`). `#[serde(default)]` so a settings.json written
    /// before this field existed still loads with its other fields, instead
    /// of `load()` falling back to `Settings::default()`.
    ///
    /// Neither this nor `ignored_updates` is read anywhere in Rust, and
    /// neither needs to be: both decide only what the Updates page lists
    /// (`hidingRule` in src/lib/updateState.ts, which the Installed page's
    /// badge reads too). `refresh` keeps reporting every candidate, which
    /// is what lets the Installed page's badge say that an update was
    /// skipped or its reminders turned off; and `Session::issue_plan` does
    /// not refuse to upgrade a hidden package, because hiding a reminder is
    /// not a refusal to update (a pin is one, carried by
    /// `UpdateCandidate.blocked`) and no page offers the button for one.
    #[serde(default)]
    pub skipped_versions: Vec<SkippedVersion>,
    /// Feeds CheckOptions.include_self_updating. Default false: most people
    /// do not want Chrome and Docker listed as updatable when those apps
    /// update themselves. `#[serde(default)]` so a settings.json written by
    /// an older Canager version (or a front end not yet sending this field)
    /// still deserializes instead of losing every other field to
    /// `Settings::default()` in `load()`.
    #[serde(default)]
    pub include_self_updating: bool,
    /// The daily check, Settings → Updates' 「每天自动检查」: whether
    /// Canager, while it runs, refreshes by itself once a day -- the same
    /// refresh as Check again, which installs nothing. Read at every tick
    /// of the shell's task (`check_automatically` in
    /// src-tauri/src/auto_check.rs), which hands it to `auto_check::tick`.
    /// Off by default. `#[serde(default)]` so a settings.json written
    /// before this field existed still loads with its other fields,
    /// instead of `load()` falling back to `Settings::default()`.
    #[serde(default)]
    pub auto_check: bool,
    /// Settings → Updates' 「有可更新时通知我」, under the daily check: the
    /// Settings page offers it only while `auto_check` is on, and turning
    /// the daily check off turns this off with it. Read, with `auto_check`,
    /// each time the page reports the updates it offers
    /// (`notify_updates::notifications_on`), which decides whether a round
    /// of the daily check posts a notification. Off by default, and
    /// `#[serde(default)]` for the same reason as `auto_check`.
    #[serde(default)]
    pub notify_updates: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            language: Language::System,
            show_technical_details: false,
            ignored_updates: Vec::new(),
            skipped_versions: Vec::new(),
            include_self_updating: false,
            auto_check: false,
            notify_updates: false,
        }
    }
}

/// Missing file, unreadable file or malformed JSON all yield
/// `Settings::default()` — settings are a convenience, never a reason to
/// fail startup.
pub fn load(path: &Path) -> Settings {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Per-process counter for `save()`'s staging file name. A fixed `<path>.tmp`
/// would let two concurrent `save()` calls to the same path clobber each
/// other's staging file (one call's `write` landing in the middle of
/// another's, or one `rename` picking up the wrong writer's bytes); suffixing
/// each call's staging file with its own counter value makes that
/// impossible, regardless of how many callers race.
static SAVE_TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes to a `<path>.tmp.<n>` staging file, `n` unique to this call within
/// this process, then renames it over `path`, so a crash mid-write can never
/// leave a half-written, corrupt settings file in `path`'s place, and two
/// concurrent calls can never collide on the same staging file.
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let seq = SAVE_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(format!(".tmp.{seq}"));
    let tmp_path = std::path::PathBuf::from(tmp_os);
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use std::path::PathBuf;

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-settings-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn key(name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: "brew:/opt/homebrew".to_string(),
            kind: ArtifactKind::Formula,
            name: name.to_string(),
        }
    }

    #[test]
    fn test_default_settings_are_the_documented_safe_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.language, Language::System);
        assert!(!settings.show_technical_details);
        assert!(settings.ignored_updates.is_empty());
        assert!(settings.skipped_versions.is_empty());
        assert!(!settings.auto_check, "the daily check is off by default");
        assert!(!settings.notify_updates, "and so are its notifications");
    }

    #[test]
    fn test_default_settings_serialize_with_no_renames() {
        // Guards the JSON wire-format contract the TypeScript mirror in
        // docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md depends on:
        // plain snake_case field names, bare-string unit variants. A
        // `#[serde(rename_all = ...)]` added later would still round-trip
        // inside Rust but would silently break the front end.
        let json = serde_json::to_string(&Settings::default()).expect("serialize");
        assert!(json.contains("\"language\":\"System\""));
        assert!(json.contains("\"show_technical_details\":false"));
        assert!(json.contains("\"ignored_updates\":[]"));
        assert!(json.contains("\"skipped_versions\":[]"));
        assert!(json.contains("\"include_self_updating\":false"));
        assert!(json.contains("\"auto_check\":false"));
        assert!(json.contains("\"notify_updates\":false"));
    }

    #[test]
    fn test_default_settings_wire_shape_matches_the_hand_written_ts_mirror() {
        // `Settings` in src/lib/types.ts; the shape test in
        // src/lib/types.test.ts expects exactly this string, every field in
        // this order, the daily check's two last.
        assert_eq!(
            serde_json::to_string(&Settings::default()).expect("serialize"),
            r#"{"language":"System","show_technical_details":false,"ignored_updates":[],"skipped_versions":[],"include_self_updating":false,"auto_check":false,"notify_updates":false}"#
        );
    }

    #[test]
    fn test_skipped_versions_wire_shape_matches_the_hand_written_ts_mirror() {
        // `SkippedVersion` in src/lib/types.ts; the shape test in
        // src/lib/types.test.ts expects exactly this string: snake_case
        // fields, the key as the same object `ignored_updates` holds, and
        // the skipped version as a bare string.
        let skipped = vec![SkippedVersion {
            key: key("glib"),
            version: "2.90.0".to_string(),
        }];
        assert_eq!(
            serde_json::to_string(&skipped).expect("serialize"),
            r#"[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"glib"},"version":"2.90.0"}]"#
        );
    }

    #[test]
    fn test_load_of_json_missing_skipped_versions_defaults_it_to_empty_and_keeps_the_rest() {
        // Every settings.json written before Skip this version existed
        // lacks this field. Without `#[serde(default)]` on it, `load` would
        // fall back to Settings::default() and silently drop the user's
        // language, the updates they asked never to be reminded about, and
        // include_self_updating.
        let path = temp_settings_path("no-skipped-versions");
        std::fs::write(
            &path,
            br#"{"language":"ZhCn","show_technical_details":true,"ignored_updates":[{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"jq"}],"include_self_updating":true}"#,
        )
        .expect("write settings.json without skipped_versions");
        let loaded = load(&path);
        assert_eq!(loaded.language, Language::ZhCn);
        assert!(loaded.show_technical_details);
        assert_eq!(loaded.ignored_updates, vec![key("jq")]);
        assert!(loaded.include_self_updating);
        assert!(loaded.skipped_versions.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_of_a_missing_file_returns_defaults() {
        let path = temp_settings_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn test_load_of_malformed_json_returns_defaults() {
        let path = temp_settings_path("malformed");
        std::fs::write(&path, b"{ not json").expect("write garbage");
        assert_eq!(load(&path), Settings::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_of_json_missing_include_self_updating_defaults_it_to_false_and_keeps_the_rest() {
        // A settings.json written before this field existed (or sent by a
        // not-yet-updated front end) must still load its other fields
        // rather than falling back to Settings::default() entirely — that
        // would silently discard a user's language and ignored_updates.
        let path = temp_settings_path("no-include-self-updating");
        std::fs::write(
            &path,
            br#"{"language":"ZhCn","show_technical_details":true,"ignored_updates":[]}"#,
        )
        .expect("write settings.json without include_self_updating");
        let loaded = load(&path);
        assert_eq!(loaded.language, Language::ZhCn);
        assert!(loaded.show_technical_details);
        assert!(!loaded.include_self_updating);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_of_json_written_before_the_daily_check_turns_it_off_and_keeps_the_rest() {
        // Every settings.json written before the daily check existed: all
        // of today's other fields, set, and neither `auto_check` nor
        // `notify_updates`. Without `#[serde(default)]` on both `load`
        // would fall back to Settings::default() and drop every one of
        // them.
        let path = temp_settings_path("no-auto-check");
        std::fs::write(
            &path,
            br#"{"language":"ZhCn","show_technical_details":true,"ignored_updates":[{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"jq"}],"skipped_versions":[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"glib"},"version":"2.90.0"}],"include_self_updating":true}"#,
        )
        .expect("write settings.json without auto_check");
        let loaded = load(&path);
        assert_eq!(
            loaded,
            Settings {
                language: Language::ZhCn,
                show_technical_details: true,
                ignored_updates: vec![key("jq")],
                skipped_versions: vec![SkippedVersion {
                    key: key("glib"),
                    version: "2.90.0".to_string(),
                }],
                include_self_updating: true,
                auto_check: false,
                notify_updates: false,
            }
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_of_json_with_the_daily_check_but_no_notify_updates_keeps_the_daily_check() {
        // Written by a Canager with the daily check and nothing after it:
        // `notify_updates` alone is missing, and alone defaults.
        let path = temp_settings_path("no-notify-updates");
        std::fs::write(
            &path,
            br#"{"language":"En","show_technical_details":false,"ignored_updates":[],"skipped_versions":[],"include_self_updating":false,"auto_check":true}"#,
        )
        .expect("write settings.json without notify_updates");
        let loaded = load(&path);
        assert_eq!(loaded.language, Language::En);
        assert!(loaded.auto_check);
        assert!(!loaded.notify_updates);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_then_load_round_trips_a_non_default_settings() {
        let path = temp_settings_path("roundtrip");
        let settings = Settings {
            language: Language::ZhCn,
            show_technical_details: true,
            ignored_updates: vec![key("jq")],
            skipped_versions: vec![SkippedVersion {
                key: key("glib"),
                version: "2.90.0".to_string(),
            }],
            include_self_updating: true,
            auto_check: true,
            notify_updates: true,
        };
        save(&path, &settings).expect("save");
        let loaded = load(&path);
        assert_eq!(loaded, settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_writes_atomically_and_leaves_no_tmp_file_behind() {
        let path = temp_settings_path("atomic");
        save(&path, &Settings::default()).expect("save");
        // The staging file is named `<path>.tmp.<n>` (`n` a process-local
        // counter, so concurrent saves never collide on one fixed name) —
        // scan for any leftover `<file-name>.tmp.*` sibling rather than
        // checking one fixed `.tmp` path.
        let dir = path.parent().expect("path has a parent");
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let leftover = std::fs::read_dir(dir)
            .expect("read temp dir")
            .filter_map(|e| e.ok())
            .any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&format!("{file_name}.tmp."))
            });
        assert!(
            !leftover,
            "no <path>.tmp.<n> staging file may be left behind"
        );
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_creates_missing_parent_directory() {
        let dir = temp_settings_path("parent-dir");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        save(&path, &Settings::default()).expect("save should create the parent dir");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
