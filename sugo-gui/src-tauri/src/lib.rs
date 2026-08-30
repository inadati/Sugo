mod commands;
mod dto;
mod state;

use commands::{
    add_cell, add_edge, create_folder, create_harness, delete_cell, delete_edge, delete_folder,
    get_active_runs, get_harness, list_folders, list_harnesses, list_trash,
    move_harness_to_folder, purge_harness, rename_cell, rename_folder, rename_harness,
    restore_harness, set_cell_memo, trash_harness, update_edge,
};
use state::AppState;
use sugo_core::ports::repository::HarnessRepository;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Embedded W3C WebDriver server (WKWebView-native on macOS), used only by
    // the `wdio-webdriver` E2E suite (see sugo-gui/e2e-wdio/). Gated behind
    // the `webdriver` Cargo feature (opt-in via `--features webdriver`) so it
    // is never linked into a normal release build: it exposes an automation
    // HTTP endpoint on localhost and must not ship to users.
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(|app| {
            // `SUGO_DB` lets tooling (E2E tests, sugo-mcp) point the app at an
            // isolated DB file instead of the shared `~/.sugo/sugo.db`. Mirrors
            // the same override in sugo-mcp/src/main.rs so the two processes
            // agree on how to find a non-default DB during testing.
            let db_path = match std::env::var("SUGO_DB") {
                Ok(p) => std::path::PathBuf::from(p),
                Err(_) => sugo_infra::paths::default_db_path().expect("resolve db path"),
            };
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
            create_harness,
            get_harness,
            add_cell,
            rename_cell,
            set_cell_memo,
            delete_cell,
            add_edge,
            delete_edge,
            update_edge,
            get_active_runs,
            trash_harness,
            restore_harness,
            purge_harness,
            list_trash,
            list_folders,
            create_folder,
            rename_folder,
            rename_harness,
            delete_folder,
            move_harness_to_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
