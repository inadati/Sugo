mod commands;
mod dto;
mod state;

use commands::{
    add_cell, add_edge, delete_cell, delete_edge, get_active_runs, get_harness, list_harnesses,
    list_trash, purge_harness, rename_cell, restore_harness, trash_harness,
};
use state::AppState;
use sugo_core::ports::repository::HarnessRepository;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db_path =
                sugo_infra::paths::default_db_path().expect("resolve db path");
            let state = AppState::new(db_path.to_str().unwrap()).expect("init db");

            // 180日を超えてゴミ箱に入ったハーネスを起動時に自動パージ
            let before = (chrono::Local::now() - chrono::Duration::days(180))
                .to_rfc3339();
            let repo = state.repo.clone();
            tauri::async_runtime::block_on(async move {
                let _ = repo.purge_trash_before(&before).await;
            });

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_harnesses,
            get_harness,
            add_cell,
            rename_cell,
            delete_cell,
            add_edge,
            delete_edge,
            get_active_runs,
            trash_harness,
            restore_harness,
            purge_harness,
            list_trash,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
