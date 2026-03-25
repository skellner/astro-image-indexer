use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, UNIX_EPOCH};

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use walkdir::WalkDir;

use crate::fits;
use crate::metadata::ImageMetadata;
use crate::preview;
use crate::quality;
use crate::xisf;
use crate::AppState;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub error_details: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Add a directory and immediately scan it.
#[tauri::command]
pub async fn index_directory(
    dir: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanResult, String> {
    let cancel = state.cancel_flag.clone();
    let conn = state.conn.clone();
    cancel.store(false, Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&dir);
        if !path.is_dir() {
            return Err(format!("Not a directory: {dir}"));
        }
        let conn = conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO scan_directories (path, added_at) VALUES (?1, ?2)",
            params![dir, Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        scan_dir(&dir, &app, &conn, &cancel)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Cancel an in-progress scan.
#[tauri::command]
pub fn cancel_scan(state: State<AppState>) {
    state.cancel_flag.store(true, Ordering::Relaxed);
}

/// Shared scanning logic — works on an already-locked connection.
fn scan_dir(
    dir: &str,
    app: &AppHandle,
    conn: &Connection,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<ScanResult, String> {
    let path = PathBuf::from(dir);
    let files = collect_image_files(&path);
    let total = files.len();

    let mut result = ScanResult {
        indexed: 0,
        skipped: 0,
        errors: 0,
        error_details: Vec::new(),
    };

    // Throttle progress events — emit at most once every 100 ms to avoid
    // flooding the WebView2 message queue.
    let throttle = Duration::from_millis(100);
    let mut last_emit = Instant::now().checked_sub(throttle).unwrap_or_else(Instant::now);

    for (i, file_path) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let now = Instant::now();
        if now.duration_since(last_emit) >= throttle {
            let file_name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let _ = app.emit(
                "indexer://progress",
                ScanProgress { current: i + 1, total, file_name },
            );
            last_emit = now;
        }

        match process_file(file_path, conn) {
            Ok(true) => result.indexed += 1,
            Ok(false) => result.skipped += 1,
            Err(e) => {
                result.errors += 1;
                result.error_details.push(format!("{}: {e}", file_path.display()));
            }
        }
    }

    conn.execute(
        "UPDATE scan_directories SET last_scanned_at = ?1 WHERE path = ?2",
        params![Utc::now().to_rfc3339(), dir],
    )
    .map_err(|e| e.to_string())?;

    Ok(result)
}

/// Re-scan all previously added directories.
#[tauri::command]
pub async fn rescan_all(app: AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let cancel = state.cancel_flag.clone();
    let conn = state.conn.clone();
    cancel.store(false, Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn.lock().map_err(|e| e.to_string())?;

        let dirs: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT path FROM scan_directories")
                .map_err(|e| e.to_string())?;
            let rows: Vec<String> = stmt
                .query_map([], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut combined = ScanResult {
            indexed: 0,
            skipped: 0,
            errors: 0,
            error_details: Vec::new(),
        };

        for dir in dirs {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match scan_dir(&dir, &app, &conn, &cancel) {
                Ok(r) => {
                    combined.indexed += r.indexed;
                    combined.skipped += r.skipped;
                    combined.errors += r.errors;
                    combined.error_details.extend(r.error_details);
                }
                Err(e) => {
                    combined.errors += 1;
                    combined.error_details.push(e);
                }
            }
        }

        Ok(combined)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// File collection
// ---------------------------------------------------------------------------

fn collect_image_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_image_file(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("fits") | Some("fit") | Some("xisf")
    )
}

// ---------------------------------------------------------------------------
// Per-file processing
// ---------------------------------------------------------------------------

/// Returns Ok(true) if indexed, Ok(false) if skipped (unchanged), Err on failure.
fn process_file(path: &Path, conn: &Connection) -> Result<bool, String> {
    let fs_meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let file_size = fs_meta.len() as i64;
    let file_modified = fs_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0)
                .unwrap_or_default()
                .to_rfc3339()
        });

    let hash = sha256_file(path).map_err(|e| e.to_string())?;

    // Skip if already indexed with the same hash
    let existing_hash: Option<String> = conn
        .query_row(
            "SELECT file_hash FROM images WHERE file_path = ?1",
            params![path.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .ok();

    if existing_hash.as_deref() == Some(&hash) {
        return Ok(false);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    let (mut meta, raw) = match ext.as_str() {
        "fits" | "fit" => fits::parse(path).map_err(|e| e.to_string())?,
        "xisf" => xisf::parse(path).map_err(|e| e.to_string())?,
        _ => return Err(format!("Unsupported extension: {ext}")),
    };

    // For light frames, read pixel data and compute FWHM + star count.
    // Errors are silently ignored — the file is still indexed without quality data.
    if meta.image_type.as_deref() == Some("Light") {
        let pixel_result = match ext.as_str() {
            "fits" | "fit" => preview::load_fits_pixels(path),
            _ => preview::load_xisf_pixels(path),
        };
        if let Ok(buf) = pixel_result {
            if let Some((fwhm, count)) = quality::analyse_stars(&buf) {
                meta.fwhm = Some(fwhm);
                meta.star_count = Some(count);
            }
        }
    }

    upsert_image(conn, path, file_size, file_modified, hash, &meta)?;
    upsert_raw_headers(conn, path, &raw)?;

    Ok(true)
}

// ---------------------------------------------------------------------------
// Database writes
// ---------------------------------------------------------------------------

fn upsert_image(
    conn: &Connection,
    path: &Path,
    file_size: i64,
    file_modified: Option<String>,
    hash: String,
    meta: &ImageMetadata,
) -> Result<(), String> {
    let file_path = path.to_string_lossy().to_string();
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO images (
            file_path, file_name, file_size, file_modified_at, file_hash, format,
            image_type, object_name, ra, dec, date_obs, exposure_time,
            gain, offset, iso, filter_name, binning_x, binning_y,
            telescope, instrument, focal_length, aperture,
            ccd_temp, site_lat, site_lon, airmass,
            width, height, bit_depth, software,
            fwhm, eccentricity, star_count, snr, sky_background,
            indexed_at, parse_error
        ) VALUES (
            ?1,  ?2,  ?3,  ?4,  ?5,  ?6,
            ?7,  ?8,  ?9,  ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25, ?26,
            ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34, ?35,
            ?36, NULL
        )
        ON CONFLICT(file_path) DO UPDATE SET
            file_size       = excluded.file_size,
            file_modified_at= excluded.file_modified_at,
            file_hash       = excluded.file_hash,
            format          = excluded.format,
            image_type      = excluded.image_type,
            object_name     = excluded.object_name,
            ra              = excluded.ra,
            dec             = excluded.dec,
            date_obs        = excluded.date_obs,
            exposure_time   = excluded.exposure_time,
            gain            = excluded.gain,
            offset          = excluded.offset,
            iso             = excluded.iso,
            filter_name     = excluded.filter_name,
            binning_x       = excluded.binning_x,
            binning_y       = excluded.binning_y,
            telescope       = excluded.telescope,
            instrument      = excluded.instrument,
            focal_length    = excluded.focal_length,
            aperture        = excluded.aperture,
            ccd_temp        = excluded.ccd_temp,
            site_lat        = excluded.site_lat,
            site_lon        = excluded.site_lon,
            airmass         = excluded.airmass,
            width           = excluded.width,
            height          = excluded.height,
            bit_depth       = excluded.bit_depth,
            software        = excluded.software,
            indexed_at      = excluded.indexed_at,
            parse_error     = NULL",
        params![
            file_path, file_name, file_size, file_modified, hash, meta.format,
            meta.image_type, meta.object_name, meta.ra, meta.dec, meta.date_obs,
            meta.exposure_time, meta.gain, meta.offset, meta.iso, meta.filter_name,
            meta.binning_x, meta.binning_y, meta.telescope, meta.instrument,
            meta.focal_length, meta.aperture, meta.ccd_temp, meta.site_lat,
            meta.site_lon, meta.airmass, meta.width, meta.height, meta.bit_depth,
            meta.software, meta.fwhm, meta.eccentricity, meta.star_count,
            meta.snr, meta.sky_background, now,
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn upsert_raw_headers(
    conn: &Connection,
    path: &Path,
    raw: &HashMap<String, String>,
) -> Result<(), String> {
    let image_id: i64 = conn
        .query_row(
            "SELECT id FROM images WHERE file_path = ?1",
            params![path.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    // Replace all headers for this image
    conn.execute(
        "DELETE FROM raw_headers WHERE image_id = ?1",
        params![image_id],
    )
    .map_err(|e| e.to_string())?;

    for (key, value) in raw {
        conn.execute(
            "INSERT INTO raw_headers (image_id, key, value) VALUES (?1, ?2, ?3)",
            params![image_id, key, value],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
