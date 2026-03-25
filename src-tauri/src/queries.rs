use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

// ---------------------------------------------------------------------------
// Return types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageRow {
    pub id: i64,
    pub file_name: String,
    pub file_path: String,
    pub format: String,
    pub image_type: Option<String>,
    pub object_name: Option<String>,
    pub filter_name: Option<String>,
    pub exposure_time: Option<f64>,
    pub gain: Option<f64>,
    pub date_obs: Option<String>,
    pub instrument: Option<String>,
    pub telescope: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub ccd_temp: Option<f64>,
    pub software: Option<String>,
    pub fwhm: Option<f64>,
    pub eccentricity: Option<f64>,
    pub star_count: Option<i64>,
    pub snr: Option<f64>,
    pub quality_rejected: bool,
    pub indexed_at: String,
    pub parse_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub id: i64,
    pub path: String,
    pub added_at: String,
    pub last_scanned_at: Option<String>,
    pub image_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageDetail {
    pub row: ImageRow,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub iso: Option<i64>,
    pub offset: Option<i64>,
    pub binning_x: Option<i64>,
    pub binning_y: Option<i64>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub site_lat: Option<f64>,
    pub site_lon: Option<f64>,
    pub airmass: Option<f64>,
    pub bit_depth: Option<i64>,
    pub file_size: Option<i64>,
    pub file_hash: Option<String>,
    pub sky_background: Option<f64>,
    pub raw_headers: Vec<(String, String)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryStats {
    pub total_images: i64,
    pub light_frames: i64,
    pub total_exposure_hours: f64,
    pub unique_objects: i64,
    pub unique_filters: i64,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_images(
    search: Option<String>,
    image_type: Option<String>,
    filter_name: Option<String>,
    state: State<AppState>,
) -> Result<Vec<ImageRow>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    // Build query dynamically based on active filters
    let mut conditions: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref s) = search {
        if !s.is_empty() {
            conditions.push(
                "(object_name LIKE ?  OR file_name LIKE ?  OR instrument LIKE ?)".into(),
            );
            let pattern = format!("%{s}%");
            values.push(Box::new(pattern.clone()));
            values.push(Box::new(pattern.clone()));
            values.push(Box::new(pattern));
        }
    }
    if let Some(ref t) = image_type {
        if !t.is_empty() {
            conditions.push("LOWER(image_type) = LOWER(?)".into());
            values.push(Box::new(t.clone()));
        }
    }
    if let Some(ref f) = filter_name {
        if !f.is_empty() {
            conditions.push("filter_name = ?".into());
            values.push(Box::new(f.clone()));
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, file_name, file_path, format, image_type, object_name,
                filter_name, exposure_time, gain, date_obs, instrument, telescope,
                width, height, ccd_temp, software, fwhm, eccentricity, star_count,
                snr, quality_rejected, indexed_at, parse_error
         FROM images
         {where_clause}
         ORDER BY date_obs DESC, file_name ASC"
    );

    let params_refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_refs.as_slice(), |r| {
            Ok(ImageRow {
                id: r.get(0)?,
                file_name: r.get(1)?,
                file_path: r.get(2)?,
                format: r.get(3)?,
                image_type: r.get(4)?,
                object_name: r.get(5)?,
                filter_name: r.get(6)?,
                exposure_time: r.get(7)?,
                gain: r.get(8)?,
                date_obs: r.get(9)?,
                instrument: r.get(10)?,
                telescope: r.get(11)?,
                width: r.get(12)?,
                height: r.get(13)?,
                ccd_temp: r.get(14)?,
                software: r.get(15)?,
                fwhm: r.get(16)?,
                eccentricity: r.get(17)?,
                star_count: r.get(18)?,
                snr: r.get(19)?,
                quality_rejected: r.get::<_, i64>(20)? != 0,
                indexed_at: r.get(21)?,
                parse_error: r.get(22)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

#[tauri::command]
pub fn get_image_detail(id: i64, state: State<AppState>) -> Result<ImageDetail, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let row: ImageRow = conn
        .query_row(
            "SELECT id, file_name, file_path, format, image_type, object_name,
                    filter_name, exposure_time, gain, date_obs, instrument, telescope,
                    width, height, ccd_temp, software, fwhm, eccentricity, star_count,
                    snr, quality_rejected, indexed_at, parse_error
             FROM images WHERE id = ?1",
            params![id],
            |r| {
                Ok(ImageRow {
                    id: r.get(0)?,
                    file_name: r.get(1)?,
                    file_path: r.get(2)?,
                    format: r.get(3)?,
                    image_type: r.get(4)?,
                    object_name: r.get(5)?,
                    filter_name: r.get(6)?,
                    exposure_time: r.get(7)?,
                    gain: r.get(8)?,
                    date_obs: r.get(9)?,
                    instrument: r.get(10)?,
                    telescope: r.get(11)?,
                    width: r.get(12)?,
                    height: r.get(13)?,
                    ccd_temp: r.get(14)?,
                    software: r.get(15)?,
                    fwhm: r.get(16)?,
                    eccentricity: r.get(17)?,
                    star_count: r.get(18)?,
                    snr: r.get(19)?,
                    quality_rejected: r.get::<_, i64>(20)? != 0,
                    indexed_at: r.get(21)?,
                    parse_error: r.get(22)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let (ra, dec, iso, offset, binning_x, binning_y, focal_length, aperture,
         site_lat, site_lon, airmass, bit_depth, file_size, file_hash, sky_background) = conn
        .query_row(
            "SELECT ra, dec, iso, offset, binning_x, binning_y, focal_length, aperture,
                    site_lat, site_lon, airmass, bit_depth, file_size, file_hash, sky_background
             FROM images WHERE id = ?1",
            params![id],
            |r| Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?,
                r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?,
                r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?,
            )),
        )
        .map_err(|e| e.to_string())?;

    let raw_headers: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT key, value FROM raw_headers WHERE image_id = ?1 ORDER BY key")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    Ok(ImageDetail {
        row,
        ra,
        dec,
        iso,
        offset,
        binning_x,
        binning_y,
        focal_length,
        aperture,
        site_lat,
        site_lon,
        airmass,
        bit_depth,
        file_size,
        file_hash,
        sky_background,
        raw_headers,
    })
}

#[tauri::command]
pub fn list_directories(state: State<AppState>) -> Result<Vec<DirectoryEntry>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT d.id, d.path, d.added_at, d.last_scanned_at,
                    COUNT(i.id) as image_count
             FROM scan_directories d
             LEFT JOIN images i ON i.file_path LIKE d.path || '%'
             GROUP BY d.id
             ORDER BY d.added_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<DirectoryEntry> = stmt
        .query_map([], |r| {
            Ok(DirectoryEntry {
                id: r.get(0)?,
                path: r.get(1)?,
                added_at: r.get(2)?,
                last_scanned_at: r.get(3)?,
                image_count: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

#[tauri::command]
pub fn remove_directory(path: String, state: State<AppState>) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // Delete images in this directory (raw_headers cascade)
    conn.execute(
        "DELETE FROM images WHERE file_path LIKE ?1",
        params![format!("{path}%")],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM scan_directories WHERE path = ?1",
        params![path],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_library_stats(state: State<AppState>) -> Result<LibraryStats, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let stats = conn
        .query_row(
            "SELECT
                COUNT(*) as total,
                SUM(CASE WHEN LOWER(image_type) LIKE '%light%' THEN 1 ELSE 0 END),
                COALESCE(SUM(CASE WHEN LOWER(image_type) LIKE '%light%'
                              THEN exposure_time ELSE 0 END), 0) / 3600.0,
                COUNT(DISTINCT object_name),
                COUNT(DISTINCT filter_name)
             FROM images",
            [],
            |r| {
                Ok(LibraryStats {
                    total_images: r.get(0)?,
                    light_frames: r.get(1)?,
                    total_exposure_hours: r.get(2)?,
                    unique_objects: r.get(3)?,
                    unique_filters: r.get(4)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(stats)
}

#[tauri::command]
pub fn get_filter_options(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT filter_name FROM images
             WHERE filter_name IS NOT NULL
             ORDER BY filter_name",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}
