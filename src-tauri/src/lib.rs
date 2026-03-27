mod db;
mod fits;
mod indexer;
mod metadata;
mod preview;
mod quality;
mod queries;
mod xisf;

use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::Manager;

/// Open a file with its associated application (equivalent to double-clicking in Explorer).
#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // `cmd /c start "" "<path>"` launches the file with its default handler.
        // The empty string is a required title argument when the path may contain spaces.
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Open File Explorer with the given file selected/highlighted.
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let parent = std::path::Path::new(&path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(&path)
            .to_string();
        std::process::Command::new("xdg-open")
            .arg(&parent)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub struct AppState {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub cancel_flag: Arc<AtomicBool>,
    /// True while a scan is in progress — background quality worker pauses.
    pub is_scanning: Arc<AtomicBool>,
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

            let conn = Arc::new(Mutex::new(conn));
            let is_scanning = Arc::new(AtomicBool::new(false));

            app.manage(AppState {
                conn: conn.clone(),
                cancel_flag: Arc::new(AtomicBool::new(false)),
                is_scanning: is_scanning.clone(),
            });

            // Spawn background quality worker.
            let handle = app.handle().clone();
            quality::spawn_backfill_worker(conn, is_scanning, handle);

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
            queries::compute_quality,
            queries::get_quality_progress,
            preview::get_image_preview,
            open_file,
            reveal_in_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
