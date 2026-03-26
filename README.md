# Astrophotography Image Indexer

A Tauri 2 desktop application for Windows that scans local directories, parses metadata from FITS and XISF astronomical image files, and stores an indexed catalog in a local SQLite database.

## Features

- Recursive directory scanning for `.fits`, `.fit`, and `.xisf` files
- Metadata extraction from 40+ fields (target, exposure, gain, filter, telescope, RA/Dec, temperature, etc.)
- SHA-256 deduplication — unchanged files are skipped on rescan
- Full-text search and filtering by object name, image type, and filter
- Sortable image table with FWHM and star-count columns for light frames
- **Calendar view** — monthly grid showing which objects were imaged each day, with month navigation
- **Image preview** — auto-stretched grayscale thumbnail in the detail panel (FITS and XISF, including LZ4/LZ4+sh-compressed files)
- **Automatic quality analysis** — FWHM and star count computed for every light frame during scanning; stored in the database and shown in the table and detail panel
- Click a directory in the sidebar to open it in Windows Explorer
- Raw header/property storage for arbitrary ad-hoc queries
- Library statistics (total images, unique objects/filters, total exposure hours)
- Non-blocking async scanning with a cancellable progress popup
- Real-time scan progress events (throttled to 100 ms)

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
| `lib.rs` | Registers all Tauri commands and plugins; app entry point |
| `db.rs` | SQLite initialization, schema migrations, WAL mode |
| `metadata.rs` | `ImageMetadata` struct shared across parsers |
| `fits.rs` | FITS binary header parser |
| `xisf.rs` | XISF XML header parser |
| `indexer.rs` | Async directory walk, SHA-256 hashing, database writes, throttled progress events, cancel flag |
| `queries.rs` | Tauri command handlers for search, filter, and stats queries |
| `preview.rs` | Pixel reading (`load_fits_pixels`, `load_xisf_pixels`) + async preview command (stretch, PNG encode, base64) |
| `quality.rs` | Star detection and FWHM measurement on a center 2048×2048 crop |

### Frontend Components

| Component | Responsibility |
|---|---|
| `App.tsx` | Root state management (directories, images, filters, scan progress, active view) |
| `TopBar.tsx` | Library stats, Add Directory and Rescan All buttons |
| `Sidebar.tsx` | Directory list; clicking a directory opens it in Windows Explorer |
| `FilterBar.tsx` | Search input and image type / filter / object dropdowns |
| `ImageTable.tsx` | Sortable table of indexed images including FWHM and star-count columns |
| `CalendarView.tsx` | Monthly calendar grid; each day shows object names for frames taken that night |
| `DetailPanel.tsx` | Full metadata view for the selected image, including auto-stretched preview |
| `ScanProgress.tsx` | Modal progress popup with cancel button, rendered via React portal |

## Tauri Commands (IPC API)

| Command | Arguments | Returns | Notes |
|---|---|---|---|
| `index_directory` | `dir: string` | `ScanResult` | async; registers dir and scans it |
| `rescan_all` | — | `ScanResult` | async; rescans all registered dirs |
| `cancel_scan` | — | `void` | sets cancel flag; scan stops at next file |
| `list_images` | `search?, image_type?, filter_name?, object_name?` | `ImageRow[]` | |
| `get_image_detail` | `id: number` | `ImageDetail` | |
| `get_image_preview` | `file_path: string` | `string` (data URL) | async; returns base64 PNG |
| `open_file` | `path: string` | `void` | opens file with its default Windows application |
| `get_object_options` | — | `string[]` | distinct object names for filter dropdown |
| `list_directories` | — | `DirectoryEntry[]` | |
| `remove_directory` | `path: string` | `void` | |
| `get_library_stats` | — | `LibraryStats` | |
| `get_filter_options` | — | `string[]` | |

**Events emitted by Rust:**

| Event | Payload | Notes |
|---|---|---|
| `indexer://progress` | `{ current: number, total: number, file_name: string }` | throttled to max once per 100 ms |

## Scanning Design

### Three-phase parallel architecture

Scanning uses a three-phase pipeline for maximum throughput:

| Phase | Work | Threading |
|---|---|---|
| 1. **Classify** | `stat()` each file + HashMap lookup against pre-fetched DB records | Serial (fast — just syscalls) |
| 2. **Process** | SHA-256 hashing, metadata parsing, FWHM/star analysis | **Parallel** via `rayon` (all CPU cores) |
| 3. **Write DB** | Upsert images + raw headers | Serial, single `BEGIN`/`COMMIT` transaction |

### Fast skip (mtime + size)

Before computing a SHA-256 hash (which reads the entire file), the scanner checks if the file's size and modification time match the existing DB record. If both match, the file is assumed unchanged and skipped with zero disk IO. This makes rescan of an unchanged library nearly instant.

### Async execution

`index_directory` and `rescan_all` are `async` Tauri commands. The actual file work runs inside `tauri::async_runtime::spawn_blocking`, which moves it off the async runtime onto a dedicated blocking thread. This keeps the Tauri IPC loop free to process other commands — including `cancel_scan` — while a scan is in progress.

### Cancellation

`AppState` holds an `Arc<AtomicBool>` cancel flag shared between the command handlers and the rayon worker threads.

1. Any new scan resets the flag to `false` before starting.
2. `cancel_scan` sets it to `true` from the frontend at any time.
3. Each rayon worker checks the flag before processing a file and returns early if set.
4. The frontend dismisses the popup immediately on cancel without waiting for the scan to return.

### Panic safety

The parallel processing phase is wrapped in `std::panic::catch_unwind` so that a panic in any rayon worker thread does not poison the `Mutex<Connection>`. Without this, a single bad file could make all subsequent DB operations fail until the app is restarted. Transaction failures trigger an explicit `ROLLBACK` to keep the connection clean.

### Progress throttling

Worker threads share an `AtomicUsize` counter and a `Mutex<Instant>` for last-emit tracking. Progress events are emitted at most once per 100 ms to prevent flooding WebView2's message queue. The frontend shows elapsed time and an estimated ETA based on the processing rate.

### Progress popup

`ScanProgress.tsx` is rendered via `ReactDOM.createPortal` into `document.body`, outside the app's root `<div>`. This avoids being clipped by the root container's `overflow: hidden`. All styles are inline to guarantee correct rendering regardless of Tailwind's stylesheet scope. The popup displays elapsed time, ETA, and total scan duration on the completion screen.

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
| File | `file_path` (unique), `file_name`, `file_size`, `file_modified_at`, `file_hash` (SHA-256), `format` |
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

### Quality Analysis (FWHM and Star Count)

`quality.rs` runs automatically for every light frame during scanning:

1. **Crop** — extracts the center 2048×2048 region to limit analysis cost on large sensors
2. **Background** — estimates sky level (median) and noise (MAD × 1.4826)
3. **Detection** — finds local maxima above a 10σ threshold using a 5×5 neighborhood window
4. **FWHM** — for each candidate, walks outward in four axis-aligned directions to the half-maximum level; uses linear interpolation for sub-pixel accuracy; rejects stars with FWHM outside [1.5, 25] px
5. **Output** — median FWHM in pixels and star count, written to `images.fwhm` / `images.star_count`

Pixel-read errors are silently swallowed so metadata is always indexed even if the pixel data cannot be loaded.

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
| `sha2` + `hex` | SHA-256 file hashing |
| `image` (png feature) | PNG encoding for previews |
| `base64` | Base64 encoding of preview data URLs |
| `lz4_flex` | LZ4 decompression for compressed XISF files (N.I.N.A. default) |
| `rayon` | Parallel file processing (hashing, parsing, quality analysis) |
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
