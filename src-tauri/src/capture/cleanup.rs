//! Orphan call-capture temp directories.
//!
//! Call at app startup, BEFORE creating any new sessions. Removes every
//! directory under `<app_data>/tmp/call-captures/` whose name is not in
//! `active_ids`. At boot time `active_ids` is empty, so every directory
//! gets removed — any temp state that survived a crash is garbage.

use std::collections::HashSet;
use std::path::Path;

pub fn cleanup_orphaned_call_temps(
    captures_tmp_dir: &Path,
    active_ids: &HashSet<String>,
) -> Result<usize, String> {
    let entries = match std::fs::read_dir(captures_tmp_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !active_ids.contains(&name) {
            if std::fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn deletes_inactive_session_dirs_keeps_active() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("keep-me")).unwrap();
        std::fs::create_dir(dir.path().join("drop-1")).unwrap();
        std::fs::create_dir(dir.path().join("drop-2")).unwrap();

        let mut active = HashSet::new();
        active.insert("keep-me".to_string());

        let n = cleanup_orphaned_call_temps(dir.path(), &active).unwrap();
        assert_eq!(n, 2);
        assert!(dir.path().join("keep-me").exists());
        assert!(!dir.path().join("drop-1").exists());
        assert!(!dir.path().join("drop-2").exists());
    }

    #[test]
    fn handles_missing_dir_gracefully() {
        let bogus = std::path::PathBuf::from("/definitely/not/a/real/xyz");
        let active = HashSet::new();
        let n = cleanup_orphaned_call_temps(&bogus, &active).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn ignores_non_directory_entries() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("not-a-dir.txt"), b"x").unwrap();
        let active = HashSet::new();
        let n = cleanup_orphaned_call_temps(dir.path(), &active).unwrap();
        assert_eq!(n, 0);
        assert!(dir.path().join("not-a-dir.txt").exists());
    }
}
