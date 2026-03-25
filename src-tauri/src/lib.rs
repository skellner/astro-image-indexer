mod db;
mod fits;
mod indexer;
mod metadata;
mod queries;
mod xisf;

use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub cancel_flag: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join("index.db");
            let conn = db::open(&db_path).expect("failed to open database");

            app.manage(AppState {
                conn: Arc::new(Mutex::new(conn)),
                cancel_flag: Arc::new(AtomicBool::new(false)),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            indexer::index_directory,
            indexer::rescan_all,
            indexer::cancel_scan,
            queries::list_images,
            queries::get_image_detail,
            queries::list_directories,
            queries::remove_directory,
            queries::get_library_stats,
            queries::get_filter_options,
            queries::get_object_options,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
