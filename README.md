# Astrophotography Image Indexer

A Tauri 2 desktop application for Windows that scans local directories, parses metadata from FITS and XISF astronomical image files, and stores an indexed catalog in a local SQLite database.

## Features

- Recursive directory scanning for `.fits`, `.fit`, and `.xisf` files
- Metadata extraction from 40+ fields (target, exposure, gain, filter, telescope, RA/Dec, temperature, etc.)
- Fast change detection via mtime + file size — unchanged files are skipped on rescan
- Full-text search and filtering by object name, image type, and filter
- Sortable image table with FWHM and star-count columns for light frames
- **Calendar view** — monthly grid showing which objects were imaged each day, with moon phase icons and month navigation
- **Image preview** — auto-stretched grayscale thumbnail in the detail panel (FITS and XISF, including LZ4/LZ4+sh-compressed files)
- **Background quality analysis** — FWHM and star count computed automatically for light frames after scanning; results stream into the table in real time with a progress bar in the title area
- **On-demand quality** — clicking a light frame in the detail panel triggers immediate quality analysis if not yet computed
- Click a directory in the sidebar to open it in Windows Explorer
- Double-click a table row to open the file with its default application
- Raw header/property storage for arbitrary ad-hoc queries
- Library statistics (total images, unique objects/filters, total exposure hours)
- Non-blocking async scanning with a cancellable progress popup; cancelling preserves all already-indexed files
- Real-time scan progress events (throttled to 100 ms) with elapsed time and smoothed ETA

## Requirements

- Windows (WebView2 runtime — included in Windows 10/11)
- [Rust toolchain](https://rustup.rs/) (for building from source)
- Node.js 18+ (for building from source)

## Getting Started

```bash
# Install frontend dependencies
npm install

# Run in development (Vite + Rust hot-reload)
npm run tauri dev

# Production build
npm run tauri build
```

The compiled installer will be in `src-tauri/target/release/bundle/`.

## Development Commands

```bash
# Frontend only (no Tauri shell, browser at http://localhost:1420)
npm run dev

# Type-check frontend
npx tsc --noEmit

# Lint Rust
cargo clippy --manifest-path src-tauri/Cargo.toml

# Run Rust tests
cargo test --manifest-path src-tauri/Cargo.toml
```

> If `cargo` is not on PATH, prepend: `export PATH="$PATH:/c/Users/stefan/.cargo/bin" &&`

## Architecture

### Two-Process Model

This app uses Tauri's two-process architecture:

- **Frontend** (`src/`) — React + TypeScript + Tailwind CSS running in WebView2. Calls into Rust via `invoke()` from `@tauri-apps/api/core`.
- **Backend** (`src-tauri/src/`) — Rust process that owns all file I/O, metadata parsing, and the SQLite database. Exposes functionality as Tauri commands.

```
User action → invoke("command_name", args) → Rust handler → serialized struct → React state
```

### Backend Modules

| File | Responsibility |
|---|---|
| `lib.rs` | Registers all Tauri commands and plugins; spawns background quality worker; app entry point |
| `db.rs` | SQLite initialization, schema migrations, WAL mode |
| `metadata.rs` | `ImageMetadata` struct shared across parsers |
| `fits.rs` | FITS binary header parser |
| `xisf.rs` | XISF XML header parser |
| `indexer.rs` | Async directory walk, mtime+size change detection, database writes, throttled progress events, cancel flag |
| `queries.rs` | Tauri command handlers for search, filter, stats, and on-demand quality computation |
| `preview.rs` | Pixel reading (`load_fits_pixels`, `load_xisf_pixels`) + async preview command (stretch, PNG encode, base64) |
| `quality.rs` | Star detection, FWHM measurement on a center 2048×2048 crop, and background backfill worker |

### Frontend Components

| Component | Responsibility |
|---|---|
| `App.tsx` | Root state management (directories, images, filters, scan progress, active view); listens for background quality events |
| `TopBar.tsx` | Library stats, Add Directory and Rescan All buttons, background quality progress bar |
| `Sidebar.tsx` | Directory list; clicking a directory opens it in Windows Explorer |
| `FilterBar.tsx` | Search input and image type / filter / object dropdowns |
| `ImageTable.tsx` | Sortable table of indexed images including FWHM and star-count columns; double-click opens file |
| `CalendarView.tsx` | Monthly calendar grid; each day shows moon phase icon, object names, and total exposure for frames taken that night |
| `DetailPanel.tsx` | Full metadata view for the selected image, including auto-stretched preview and on-demand quality analysis |
| `ScanProgress.tsx` | Modal progress popup with cancel button, elapsed time, ETA; rendered via React portal |

## Tauri Commands (IPC API)

| Command | Arguments | Returns | Notes |
|---|---|---|---|
| `index_directory` | `dir: string` | `ScanResult` | async; registers dir and scans it |
| `rescan_all` | — | `ScanResult` | async; rescans all registered dirs |
| `cancel_scan` | — | `void` | sets cancel flag; scan stops at next file |
| `list_images` | `search?, image_type?, filter_name?, object_name?` | `ImageRow[]` | async; runs on blocking thread |
| `get_image_detail` | `id: number` | `ImageDetail` | async; runs on blocking thread |
| `get_image_preview` | `file_path: string` | `string` (data URL) | async; returns base64 PNG |
| `compute_quality` | `file_path: string` | `QualityResult` | async; computes FWHM + star count on demand |
| `open_file` | `path: string` | `void` | opens file with its default Windows application |
| `get_object_options` | — | `string[]` | async; runs on blocking thread |
| `list_directories` | — | `DirectoryEntry[]` | async; runs on blocking thread |
| `remove_directory` | `path: string` | `void` | async; runs on blocking thread |
| `get_library_stats` | — | `LibraryStats` | async; runs on blocking thread |
| `get_filter_options` | — | `string[]` | async; runs on blocking thread |

**Events emitted by Rust:**

| Event | Payload | Notes |
|---|---|---|
| `indexer://progress` | `{ current, total, file_name }` | throttled to max once per 100 ms |
| `quality://update` | `{ file_path, fwhm, star_count }` | emitted by background worker when a file's quality is computed |

## Scanning Design

### Header-only parsing

Scanning only reads file headers (a few KB per file) to extract metadata — it never reads the full pixel data. This makes initial scans extremely fast even for large libraries with multi-megabyte image files.

### Fast skip (mtime + size)

Before parsing, the scanner checks if the file's size and modification time match a pre-fetched in-memory cache of all existing DB records. If both match, the file is assumed unchanged and skipped with zero disk IO. This makes rescan of an unchanged library nearly instant — a single `stat()` + HashMap lookup per file.

### Batched transactions

DB writes are grouped into batches of 50 files, each wrapped in a `BEGIN`/`COMMIT` transaction. This eliminates the per-file `fsync` overhead that SQLite imposes and, critically, releases the `Mutex<Connection>` between batches so UI queries (which also need the lock) are never blocked for more than one batch at a time. On commit failure the transaction is rolled back to keep the connection clean.

### Async execution

Every Tauri command that touches the database is `async` and runs its work inside `tauri::async_runtime::spawn_blocking`, moving it onto a dedicated blocking thread. This keeps the Tauri IPC loop permanently free — commands like `cancel_scan` and `list_images` are never blocked waiting for an in-progress scan to release the mutex.

### Cancellation

`AppState` holds an `Arc<AtomicBool>` cancel flag shared between the command handlers and `scan_dir`.

1. Any new scan resets the flag to `false` before starting.
2. `cancel_scan` sets it to `true` from the frontend at any time.
3. `scan_dir` checks the flag at the start of each file iteration and breaks early if set.
4. After the loop, any remaining parsed-but-not-yet-written files are flushed to the DB before returning, so no already-parsed work is lost on cancel.
5. The scan command returns normally with a partial `ScanResult`; the frontend refreshes the table once the command resolves, ensuring the table always reflects what was actually written.

### Progress throttling

`scan_dir` tracks the last emit time with `std::time::Instant` and skips the `app.emit()` call unless at least 100 ms have elapsed. This prevents flooding WebView2's message queue on large libraries. The frontend shows elapsed time and an ETA smoothed with an exponential moving average (α = 0.3) — the ETA only appears after at least 5 % progress and 2 s elapsed to avoid wild early estimates.

### Progress popup

`ScanProgress.tsx` is rendered via `ReactDOM.createPortal` into `document.body`, outside the app's root `<div>`. This avoids being clipped by the root container's `overflow: hidden`. All styles are inline to guarantee correct rendering regardless of Tailwind's stylesheet scope. The popup displays elapsed time, ETA, and total scan duration on the completion screen.

## Background Quality Analysis

Quality analysis (FWHM and star count) is decoupled from scanning to keep scans fast and the UI responsive:

1. **Background worker** — A dedicated thread starts on app launch and continuously processes light frames with missing quality data. It pauses automatically while a scan is in progress.
2. **On-demand** — When a user clicks on a light frame in the detail panel, `compute_quality` is called immediately if FWHM is not yet available, providing instant feedback.
3. **Real-time updates** — The background worker emits `quality://update` events as each file is processed; the frontend patches the table in-place so FWHM and star-count columns populate progressively.
4. **No full-file reads during scan** — Pixel data (50–100 MB per file) is only loaded by the background worker or on-demand command, never during the scan itself.

The analysis pipeline:

1. **Crop** — extracts the center 2048×2048 region to limit cost on large sensors
2. **Background** — estimates sky level (median) and noise (MAD × 1.4826)
3. **Detection** — finds local maxima above a 10σ threshold using a 5×5 neighborhood window
4. **FWHM** — for each candidate, walks outward in four axis-aligned directions to the half-maximum level; uses linear interpolation for sub-pixel accuracy; rejects stars with FWHM outside [1.5, 25] px
5. **Output** — median FWHM in pixels and star count, written to `images.fwhm` / `images.star_count`

## Database Schema

The SQLite database is stored at `%APPDATA%\tauri-app\index.db`.

### `scan_directories`

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | |
| path | TEXT UNIQUE | Absolute path |
| added_at | TEXT | ISO 8601 |
| last_scanned_at | TEXT | ISO 8601, nullable |

### `images`

| Column Group | Fields |
|---|---|
| File | `file_path` (unique), `file_name`, `file_size`, `file_modified_at`, `file_hash`, `format` |
| Target | `object_name`, `ra`, `dec` (J2000 decimal degrees) |
| Capture | `date_obs`, `exposure_time`, `gain`, `offset`, `iso`, `filter_name`, `binning_x`, `binning_y` |
| Equipment | `telescope`, `instrument`, `focal_length`, `aperture` |
| Conditions | `ccd_temp`, `site_lat`, `site_lon`, `airmass` |
| Image | `width`, `height`, `bit_depth`, `image_type` |
| Quality | `fwhm`, `eccentricity`, `star_count`, `snr`, `sky_background`, `quality_rejected` |
| Metadata | `software`, `indexed_at`, `parse_error` |

`image_type` is constrained to: `Light`, `Dark`, `Flat`, `Bias`, `MasterDark`, `MasterFlat`, `MasterBias`, `Unknown`.

### `raw_headers`

Stores every raw FITS keyword or XISF property for ad-hoc queries:

| Column | Type |
|---|---|
| image_id | INTEGER FK → images.id (CASCADE) |
| key | TEXT |
| value | TEXT |

## Metadata Parsing

### FITS (`.fits`, `.fit`)

Binary format with 2880-byte blocks containing 80-byte ASCII header cards (`KEY = VALUE / comment`). The parser:

1. Verifies the `SIMPLE  =` magic header
2. Reads cards until the `END` card (up to 100 blocks)
3. Maps known keywords to `ImageMetadata` fields
4. Converts sexagesimal RA/Dec (e.g. `12 34 56.7`) to decimal degrees

Supported keywords include: `OBJECT`, `RA`/`OBJCTRA`, `DEC`/`OBJCTDEC`, `DATE-OBS`, `EXPTIME`, `GAIN`, `OFFSET`, `FILTER`, `TELESCOP`, `INSTRUME`, `NAXIS1`/`NAXIS2`, `BITPIX`, `CCD-TEMP`, `IMAGETYP`, `FRAME`, and more.

### XISF (`.xisf`)

PixInsight's binary format with a signature (`XISF0100`), a header length field, and an XML header. The parser:

1. Validates the `XISF0100` signature
2. Reads the XML header length and parses the XML
3. Extracts `FITSKeyword` elements (same keywords as FITS)
4. Extracts native XISF `Property` elements, which take precedence over FITS keywords
5. Reads image geometry (`width:height:channels`) and sample format

XISF properties mapped: `Observation:Object:Name`, `Observation:Time:Start`, `Instrument:ExposureTime`, `Camera:Gain`, `Filter:Name`, `Instrument:Telescope:Name`, `Creator:Application`, and more.

### Image Preview

`preview.rs` implements `get_image_preview`: reads raw pixel data from FITS or XISF, applies a median+MAD auto-stretch with a square-root tone curve, downsamples to max 800×600, and returns a base64-encoded PNG data URL. Handles LZ4 and LZ4+byte-shuffle compressed XISF blocks (the default output of N.I.N.A.).

## Key Dependencies

### Rust

| Crate | Purpose |
|---|---|
| `tauri 2` | Desktop app shell, IPC, window management |
| `tauri-plugin-fs` | File system access |
| `tauri-plugin-dialog` | Native folder picker |
| `rusqlite` (bundled) | SQLite — no separate install needed |
| `quick-xml` | XISF XML header parsing |
| `walkdir` | Recursive directory traversal |
| `image` (png feature) | PNG encoding for previews |
| `base64` | Base64 encoding of preview data URLs |
| `lz4_flex` | LZ4 decompression for compressed XISF files (N.I.N.A. default) |
| `chrono` | Date/time parsing |
| `serde` / `serde_json` | Serialization across the IPC boundary |
| `thiserror` | Ergonomic error types |

### Frontend

| Package | Purpose |
|---|---|
| React 19 | UI framework |
| TypeScript ~5.8 | Type safety |
| Tailwind CSS 4 | Utility-first styling |
| Vite 7 | Dev server and bundler |
| `@tauri-apps/api` | `invoke()`, event listeners |
| `@tauri-apps/plugin-dialog` | Folder picker from frontend |

## IDE Setup

- [VS Code](https://code.visualstudio.com/) with:
  - [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) extension
  - [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension
