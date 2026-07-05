use std::path::Path;

/// True if `cmd` resolves on `$PATH` (checked directly, without spawning a shell).
pub fn command_exists_on_path(cmd: &str) -> bool {
    let Some(path_env) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    for dir in std::env::split_paths(&path_env) {
        for ext in exts {
            let candidate = dir.join(format!("{cmd}{ext}"));
            if is_executable(&candidate) {
                return true;
            }
        }
    }
    false
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// True if a file exists and is readable, e.g. a provider's local config/state file.
pub fn file_exists(path: &Path) -> bool {
    path.exists()
}

/// Reads and JSON-parses a file, returning `None` if it's missing or malformed.
pub fn read_json_file_if_exists<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_a_command_known_to_exist() {
        // `sh` is guaranteed present on every CI/dev machine this runs on (unix); on
        // Windows this crate's implementation checks executability by extension instead.
        if cfg!(unix) {
            assert!(command_exists_on_path("sh"));
        }
    }

    #[test]
    fn rejects_a_made_up_command() {
        assert!(!command_exists_on_path("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn file_exists_reports_correctly() {
        let dir = std::env::temp_dir().join(format!("agent-status-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state.json");
        assert!(!file_exists(&file));
        std::fs::write(&file, "{}").unwrap();
        assert!(file_exists(&file));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_valid_json_and_gives_up_gracefully_on_garbage() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Doc {
            ok: bool,
        }
        let dir = std::env::temp_dir().join(format!("agent-status-test-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.json");
        let mut f = std::fs::File::create(&good).unwrap();
        f.write_all(br#"{"ok":true}"#).unwrap();
        assert_eq!(read_json_file_if_exists::<Doc>(&good), Some(Doc { ok: true }));

        let missing = dir.join("missing.json");
        assert_eq!(read_json_file_if_exists::<Doc>(&missing), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
