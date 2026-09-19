use crate::model::ArtifactKey;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    System,
    En,
    ZhCn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub show_technical_details: bool,
    pub ignored_updates: Vec<ArtifactKey>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            language: Language::System,
            show_technical_details: false,
            ignored_updates: Vec::new(),
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
    let seq = SAVE_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

    #[test]
    fn test_default_settings_are_the_documented_safe_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.language, Language::System);
        assert!(!settings.show_technical_details);
        assert!(settings.ignored_updates.is_empty());
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
    fn test_save_then_load_round_trips_a_non_default_settings() {
        let path = temp_settings_path("roundtrip");
        let settings = Settings {
            language: Language::ZhCn,
            show_technical_details: true,
            ignored_updates: vec![ArtifactKey {
                instance_id: "brew:/opt/homebrew".to_string(),
                kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            }],
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
