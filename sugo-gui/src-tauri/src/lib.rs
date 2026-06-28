mod commands;
mod dto;
mod state;

use commands::{add_cell, get_harness, list_harnesses};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Share the single source-of-truth DB at ~/.sugo/sugo.db with the
            // MCP server, rather than the GUI's private app-data directory.
            let db_path = sugo_infra::paths::default_db_path().expect("resolve db path");
            let state = AppState::new(db_path.to_str().unwrap()).expect("init db");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_harnesses, get_harness, add_cell,])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
