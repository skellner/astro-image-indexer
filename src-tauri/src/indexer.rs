use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use walkdir::WalkDir;

use crate::fits;
use crate::metadata::ImageMetadata;
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
// Internal types
// ---------------------------------------------------------------------------

struct ExistingRecord {
    file_size: i64,
    file_modified_at: Option<String>,
}

/// Parsed data for a single file, ready to be written to the DB.
struct ParsedFile {
    path: PathBuf,
    file_size: i64,
    file_modified: Option<String>,
    meta: ImageMetadata,
    raw: HashMap<String, String>,
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
    let is_scanning = state.is_scanning.clone();
    cancel.store(false, Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        is_scanning.store(true, Ordering::Relaxed);
        let result = (|| {
            let path = PathBuf::from(&dir);
            if !path.is_dir() {
                return Err(format!("Not a directory: {dir}"));
            }
            {
                let conn = conn.lock().map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT OR IGNORE INTO scan_directories (path, added_at) VALUES (?1, ?2)",
                    params![dir, Utc::now().to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
            }
            scan_dir(&dir, &app, &conn, &cancel)
        })();
        is_scanning.store(false, Ordering::Relaxed);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Cancel an in-progress scan.
#[tauri::command]
pub fn cancel_scan(state: State<AppState>) {
    state.cancel_flag.store(true, Ordering::Relaxed);
}

/// Re-scan all previously added directories.
#[tauri::command]
pub async fn rescan_all(app: AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let cancel = state.cancel_flag.clone();
    let conn = state.conn.clone();
    let is_scanning = state.is_scanning.clone();
    cancel.store(false, Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        is_scanning.store(true, Ordering::Relaxed);
        let result = (|| {
            let dirs: Vec<String> = {
                let conn = conn.lock().map_err(|e| e.to_string())?;
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
        })();
        is_scanning.store(false, Ordering::Relaxed);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Scanner — releases the mutex between batches so UI stays responsive
// ---------------------------------------------------------------------------

/// How many files to write in one transaction before releasing the mutex.
const BATCH: usize = 50;

fn scan_dir(
    dir: &str,
    app: &AppHandle,
    conn: &Arc<Mutex<Connection>>,
    cancel: &AtomicBool,
) -> Result<ScanResult, String> {
    let files = collect_image_files(&PathBuf::from(dir));
    let total = files.len();

    // Pre-fetch existing records (brief lock).
    let existing = {
        let conn = conn.lock().map_err(|e| e.to_string())?;
        prefetch_existing(&conn)?
    };

    let mut result = ScanResult {
        indexed: 0,
        skipped: 0,
        errors: 0,
        error_details: Vec::new(),
    };

    let throttle = Duration::from_millis(100);
    let mut last_emit = Instant::now()
        .checked_sub(throttle)
        .unwrap_or_else(Instant::now);

    // Process files in batches. Each batch:
    //   1. Parse headers without holding the lock (only reads file headers, no DB).
    //   2. Acquire the lock, write the batch in a transaction, release.
    // This lets UI queries go through between batches.
    let mut batch_parsed: Vec<ParsedFile> = Vec::with_capacity(BATCH);

    for (i, file_path) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        // Throttled progress event.
        let now = Instant::now();
        if now.duration_since(last_emit) >= throttle {
            let file_name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let _ = app.emit(
                "indexer://progress",
                ScanProgress {
                    current: i + 1,
                    total,
                    file_name,
                },
            );
            last_emit = now;
        }

        // Parse (no lock needed — only reads the file header).
        match parse_file(file_path, &existing) {
            Ok(Some(parsed)) => {
                batch_parsed.push(parsed);
                result.indexed += 1;
            }
            Ok(None) => result.skipped += 1,
            Err(e) => {
                result.errors += 1;
                result.error_details
                    .push(format!("{}: {e}", file_path.display()));
            }
        }

        // Flush batch to DB when full.
        if batch_parsed.len() >= BATCH {
            let conn = conn.lock().map_err(|e| e.to_string())?;
            write_batch(&conn, &mut batch_parsed)?;
        }
    }

    // Flush any remaining parsed files (including partial batches after a cancel).
    if !batch_parsed.is_empty() {
        let conn = conn.lock().map_err(|e| e.to_string())?;
        write_batch(&conn, &mut batch_parsed)?;
    }

    // Update last_scanned_at.
    {
        let conn = conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE scan_directories SET last_scanned_at = ?1 WHERE path = ?2",
            params![Utc::now().to_rfc3339(), dir],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(result)
}

/// Parse a file's metadata without touching the DB. Returns None if the file
/// should be skipped (unchanged).
fn parse_file(
    path: &Path,
    existing: &HashMap<String, ExistingRecord>,
) -> Result<Option<ParsedFile>, String> {
    let (file_size, file_modified) = file_stat(path)?;
    let fp = path.to_string_lossy();

    // Fast path: mtime + size match → skip.
    if let Some(ex) = existing.get(fp.as_ref()) {
        if ex.file_size == file_size
            && ex.file_modified_at.as_deref() == file_modified.as_deref()
        {
            return Ok(None);
        }
    }

    let ext = extension_lower(path);
    let (meta, raw) = match ext.as_str() {
        "fits" | "fit" => fits::parse(path).map_err(|e| e.to_string())?,
        "xisf" => xisf::parse(path).map_err(|e| e.to_string())?,
        _ => return Err(format!("Unsupported extension: {ext}")),
    };

    Ok(Some(ParsedFile {
        path: path.to_path_buf(),
        file_size,
        file_modified,
        meta,
        raw,
    }))
}

/// Write a batch of parsed files to the DB in a single transaction, then clear
/// the batch buffer.
fn write_batch(
    conn: &Connection,
    batch: &mut Vec<ParsedFile>,
) -> Result<(), String> {
    conn.execute_batch("BEGIN DEFERRED")
        .map_err(|e| e.to_string())?;

    for pf in batch.iter() {
        if let Err(e) = upsert_and_headers(
            &pf.path, conn, pf.file_size, pf.file_modified.clone(), &pf.meta, &pf.raw,
        ) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    if let Err(e) = conn.execute_batch("COMMIT") {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(format!("DB commit failed: {e}"));
    }

    batch.clear();
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn prefetch_existing(conn: &Connection) -> Result<HashMap<String, ExistingRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT file_path, file_size, file_modified_at FROM images",
        )
        .map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                ExistingRecord {
                    file_size: r.get(1)?,
                    file_modified_at: r.get(2)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let (path, record) = row.map_err(|e| e.to_string())?;
        map.insert(path, record);
    }
    Ok(map)
}

fn file_stat(path: &Path) -> Result<(i64, Option<String>), String> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    let size = m.len() as i64;
    let modified = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0)
                .unwrap_or_default()
                .to_rfc3339()
        });
    Ok((size, modified))
}

fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
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
// Database writes
// ---------------------------------------------------------------------------

fn upsert_and_headers(
    path: &Path,
    conn: &Connection,
    file_size: i64,
    file_modified: Option<String>,
    meta: &ImageMetadata,
    raw: &HashMap<String, String>,
) -> Result<(), String> {
    let file_path = path.to_string_lossy().to_string();
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let now = Utc::now().to_rfc3339();
    let hash = ""; // No longer computed — mtime+size used for change detection.

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
            fwhm            = excluded.fwhm,
            star_count      = excluded.star_count,
            eccentricity    = excluded.eccentricity,
            snr             = excluded.snr,
            sky_background  = excluded.sky_background,
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

    // Replace raw headers for this image.
    let image_id: i64 = conn
        .query_row(
            "SELECT id FROM images WHERE file_path = ?1",
            params![file_path],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

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
