use rusqlite::{Connection, Result};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Database path error: {0}")]
    Path(String),
}

pub fn open(db_path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );
    ")?;

    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch(MIGRATION_1)?;
        conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
    }

    Ok(())
}

const MIGRATION_1: &str = "
    -- Root directories the user has added for scanning
    CREATE TABLE IF NOT EXISTS scan_directories (
        id           INTEGER PRIMARY KEY,
        path         TEXT    NOT NULL UNIQUE,
        added_at     TEXT    NOT NULL,
        last_scanned_at TEXT
    );

    -- One row per image file
    CREATE TABLE IF NOT EXISTS images (
        id               INTEGER PRIMARY KEY,

        -- File identity
        file_path        TEXT    NOT NULL UNIQUE,
        file_name        TEXT    NOT NULL,
        file_size        INTEGER,
        file_modified_at TEXT,
        file_hash        TEXT,           -- SHA-256, for dedup detection
        format           TEXT    NOT NULL CHECK(format IN ('FITS','XISF')),

        -- Image classification
        image_type       TEXT,           -- Light, Dark, Flat, Bias, MasterDark, MasterFlat, …

        -- Target
        object_name      TEXT,
        ra               REAL,           -- degrees J2000
        dec              REAL,           -- degrees J2000

        -- Capture settings
        date_obs         TEXT,           -- ISO 8601 UTC
        exposure_time    REAL,           -- seconds
        gain             REAL,
        offset           INTEGER,
        iso              INTEGER,
        filter_name      TEXT,
        binning_x        INTEGER,
        binning_y        INTEGER,

        -- Equipment
        telescope        TEXT,
        instrument       TEXT,           -- camera model
        focal_length     REAL,           -- mm
        aperture         REAL,           -- mm (diameter)

        -- Conditions
        ccd_temp         REAL,           -- °C
        site_lat         REAL,           -- degrees
        site_lon         REAL,           -- degrees
        airmass          REAL,

        -- Image dimensions
        width            INTEGER,
        height           INTEGER,
        bit_depth        INTEGER,

        -- Capture software
        software         TEXT,           -- e.g. N.I.N.A., SGP, APT, KStars

        -- Sub-quality flags (populated by external tools or plate solver)
        fwhm             REAL,           -- arcseconds
        eccentricity     REAL,           -- 0.0–1.0
        star_count       INTEGER,
        snr              REAL,
        sky_background   REAL,
        quality_rejected INTEGER NOT NULL DEFAULT 0 CHECK(quality_rejected IN (0,1)),

        -- Indexing bookkeeping
        indexed_at       TEXT    NOT NULL,
        parse_error      TEXT            -- NULL if parsed successfully
    );

    -- Full key/value dump of every header card (FITS) or XML property (XISF)
    -- Lets us query fields we don't explicitly model without re-parsing files
    CREATE TABLE IF NOT EXISTS raw_headers (
        image_id  INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
        key       TEXT    NOT NULL,
        value     TEXT,
        PRIMARY KEY (image_id, key)
    );

    -- Indexes for common query patterns
    CREATE INDEX IF NOT EXISTS idx_images_object_name  ON images(object_name);
    CREATE INDEX IF NOT EXISTS idx_images_date_obs     ON images(date_obs);
    CREATE INDEX IF NOT EXISTS idx_images_filter_name  ON images(filter_name);
    CREATE INDEX IF NOT EXISTS idx_images_image_type   ON images(image_type);
    CREATE INDEX IF NOT EXISTS idx_images_instrument   ON images(instrument);
    CREATE INDEX IF NOT EXISTS idx_images_software     ON images(software);
";
