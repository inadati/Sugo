//! Filesystem paths shared by every Sugo process.
//!
//! The SQLite DB is the single source of truth coordinating the GUI and the MCP
//! server, so both must open the *same* file. They resolve it through
//! [`default_db_path`], which lives under `~/.sugo/` regardless of each
//! process's working directory or platform-specific app-data location.

use std::path::PathBuf;

/// Directory holding all Sugo runtime state: `~/.sugo`.
///
/// Falls back to the current directory's `.sugo` if `$HOME` is unset (e.g. in a
/// minimal CI shell), so the path is always usable.
pub fn sugo_home() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".sugo"),
        None => PathBuf::from(".sugo"),
    }
}

/// Absolute path to the shared SQLite DB: `~/.sugo/sugo.db`.
///
/// Ensures the parent directory exists before returning.
pub fn default_db_path() -> std::io::Result<PathBuf> {
    let dir = sugo_home();
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("sugo.db"))
}
