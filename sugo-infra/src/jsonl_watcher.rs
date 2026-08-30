//! On-demand stall detector for Claude Code project jsonl files.

use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, PartialEq)]
pub struct StallInfo {
    pub is_stalled: bool,
    pub secs_since_last_modified: Option<u64>,
}

/// Check whether the Claude Code session for `project_path` appears stalled.
/// Scans all *.jsonl files under ~/.claude/projects/ whose records have
/// "cwd": project_path. If the most recently modified such file was written
/// more than timeout_secs ago, is_stalled is true.
pub fn check_stall(project_path: &str, timeout_secs: u64) -> StallInfo {
    let home = std::env::var("HOME").unwrap_or_default();
    let projects_dir = Path::new(&home).join(".claude").join("projects");

    let mut min_secs: Option<u64> = None;

    let Ok(entries) = std::fs::read_dir(&projects_dir) else {
        return StallInfo {
            is_stalled: false,
            secs_since_last_modified: None,
        };
    };

    for entry in entries.flatten() {
        let subdir = entry.path();
        if !subdir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&subdir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let matches_cwd = contents.lines().any(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| {
                        v.get("cwd")
                            .and_then(|c| c.as_str())
                            .map(|c| c == project_path)
                    })
                    .unwrap_or(false)
            });
            if !matches_cwd {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(&path)
                && let Ok(modified) = meta.modified()
                && let Ok(dur) = SystemTime::now().duration_since(modified)
            {
                let secs = dur.as_secs();
                if min_secs.map(|m| secs < m).unwrap_or(true) {
                    min_secs = Some(secs);
                }
            }
        }
    }

    match min_secs {
        None => StallInfo {
            is_stalled: false,
            secs_since_last_modified: None,
        },
        Some(secs) => StallInfo {
            is_stalled: secs > timeout_secs,
            secs_since_last_modified: Some(secs),
        },
    }
}

/// Return true if any jsonl file whose records have "cwd": project_path contains
/// an "assistant" entry with a timestamp strictly after `since_iso` (ISO 8601, UTC).
/// ISO 8601 UTC strings are lexicographically ordered, so plain string comparison works.
pub fn has_assistant_entry_since(project_path: &str, since_iso: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let projects_dir = Path::new(&home).join(".claude").join("projects");

    let Ok(entries) = std::fs::read_dir(&projects_dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let subdir = entry.path();
        if !subdir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&subdir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let lines: Vec<_> = contents.lines().collect();
            let matches_cwd = lines.iter().any(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| {
                        v.get("cwd")
                            .and_then(|c| c.as_str())
                            .map(|c| c == project_path)
                    })
                    .unwrap_or(false)
            });
            if !matches_cwd {
                continue;
            }
            let found = lines.iter().any(|line| {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    return false;
                };
                if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                    return false;
                }
                v.get("timestamp")
                    .and_then(|t| t.as_str())
                    .map(|ts| ts > since_iso)
                    .unwrap_or(false)
            });
            if found {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_not_stalled_for_unknown_path() {
        let info = check_stall("/nonexistent/project/path/that/cannot/exist/12345", 60);
        assert!(!info.is_stalled);
        assert!(info.secs_since_last_modified.is_none());
    }

    #[test]
    fn jsonl_record_cwd_matching_logic() {
        // Verify the CWD-matching logic in isolation using a temp file
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let jsonl_path = dir.path().join("session.jsonl");
        let mut f = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(f, r#"{{"type":"message","cwd":"/my/special/project"}}"#).unwrap();
        writeln!(f, r#"{{"type":"result","cwd":"/my/special/project"}}"#).unwrap();
        drop(f);

        let contents = std::fs::read_to_string(&jsonl_path).unwrap();
        let has_cwd = contents.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| {
                    v.get("cwd")
                        .and_then(|c| c.as_str())
                        .map(|c| c == "/my/special/project")
                })
                .unwrap_or(false)
        });
        assert!(has_cwd, "jsonl record with matching cwd should be found");

        let no_cwd = contents.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| {
                    v.get("cwd")
                        .and_then(|c| c.as_str())
                        .map(|c| c == "/other/project")
                })
                .unwrap_or(false)
        });
        assert!(!no_cwd, "different cwd should not match");
    }
}
