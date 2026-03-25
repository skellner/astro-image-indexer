# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Astrophotography image indexer — a Tauri 2 desktop app for Windows that scans local directories, parses metadata from FITS and XISF image files, and stores an indexed catalog in a local SQLite database.

## Commands

```bash
# Run in development (starts Vite + Rust hot-reload)
npm run tauri dev

# Production build
npm run tauri build

# Frontend only (no Tauri shell)
npm run dev

# Type-check frontend
npx tsc --noEmit

# Lint Rust
cargo clippy --manifest-path src-tauri/Cargo.toml

# Run Rust tests
cargo test --manifest-path src-tauri/Cargo.toml
```

> `rustc`/`cargo` may not be on PATH in some shells. Prepend `export PATH="$PATH:/c/Users/stefan/.cargo/bin" &&` if needed.

## Architecture

### Two-process model (Tauri)
- **Frontend** (`src/`) — React + TypeScript + Tailwind running in WebView2. Calls into Rust via `invoke()` from `@tauri-apps/api/core`.
- **Backend** (`src-tauri/src/`) — Rust process that owns all file I/O, metadata parsing, and the SQLite database. Exposes functionality as Tauri commands (`#[tauri::command]`).

### Rust backend structure
- `src-tauri/src/lib.rs` — registers all Tauri commands and plugins; app entry point
- Tauri commands are the IPC boundary: the frontend can only call what is explicitly registered here
- File system access uses `tauri-plugin-fs` and `walkdir` for directory scanning
- Folder picker uses `tauri-plugin-dialog`

### Metadata parsing
- **FITS** (`.fits`, `.fit`) — binary format with a plain-text header block (80-byte records, `KEY = VALUE` pairs). Parse by reading the header directly; no external C library dependency.
- **XISF** (`.xisf`) — PixInsight's format. XML header followed by binary data. Parse with `quick-xml`.
- Key metadata fields to extract: object/target name, date-obs, exposure time, gain/ISO, filter, telescope, instrument, RA/Dec, CCD temperature, binning, image dimensions.

### Database
- Single SQLite file stored in the app's data directory (`tauri::api::path::app_data_dir`)
- Managed via `rusqlite` with the `bundled` feature (no separate SQLite install needed)
- Schema lives in Rust; run migrations on startup

### Frontend → Rust data flow
```
User action → invoke("command_name", { args }) → Rust handler → returns serialized struct → React state
```
All data types crossing the boundary must implement `serde::Serialize` / `serde::Deserialize`.

## Key dependencies

| Crate | Purpose |
|---|---|
| `tauri 2` | App shell, window, IPC |
| `tauri-plugin-fs` | File system access |
| `tauri-plugin-dialog` | Native folder/file picker |
| `rusqlite` (bundled) | SQLite database |
| `quick-xml` | XISF XML header parsing |
| `walkdir` | Recursive directory scanning |
| `chrono` | Date/time parsing and storage |
| `thiserror` | Ergonomic error types |

## Tauri capabilities

File system and dialog permissions must be declared in `src-tauri/capabilities/`. If a command fails with a permission error at runtime, add the relevant permission to the capability JSON file.
