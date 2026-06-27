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
            let db_dir = app.path().app_data_dir().expect("app_data_dir");
            std::fs::create_dir_all(&db_dir).expect("create db dir");
            let db_path = db_dir.join("sugo.db");
            let state = AppState::new(db_path.to_str().unwrap()).expect("init db");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_harnesses, get_harness, add_cell,])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
