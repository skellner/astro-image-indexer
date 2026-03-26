import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { DirectoryEntry, LibraryStats, ScanResult } from "../types";

interface Props {
  stats: LibraryStats | null;
  dirs: DirectoryEntry[];
  scanning: boolean;
  qualityProgress: { done: number; total: number } | null;
  onDirsChange: () => void;
  onScanStart: () => void;
  onScanEnd: (r: ScanResult) => void;
}

export function TopBar({ stats, dirs, scanning, qualityProgress, onDirsChange, onScanStart, onScanEnd }: Props) {
  async function addDirectory() {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    onScanStart();
    try {
      const result = await invoke<ScanResult>("index_directory", { dir: selected });
      onScanEnd(result);
      onDirsChange();
    } catch (e) {
      onScanEnd({ indexed: 0, skipped: 0, errors: 1, error_details: [String(e)] });
    }
  }

  async function rescanAll() {
    onScanStart();
    try {
      const result = await invoke<ScanResult>("rescan_all");
      onScanEnd(result);
      onDirsChange();
    } catch (e) {
      onScanEnd({ indexed: 0, skipped: 0, errors: 1, error_details: [String(e)] });
    }
  }

  return (
    <header className="flex items-center gap-6 px-4 h-14 shrink-0 bg-gray-900 border-b border-gray-700">
      {/* Title + quality progress */}
      <div className="shrink-0">
        <div>
          <span className="text-sm font-semibold text-white tracking-wide">AstroIndex</span>
          <span className="text-xs text-gray-500 ml-2">FITS &amp; XISF</span>
        </div>
        {qualityProgress && qualityProgress.total > 0 && qualityProgress.done < qualityProgress.total && (
          <div className="flex items-center gap-2 mt-1">
            <div className="w-24 h-1.5 bg-gray-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 rounded-full transition-all duration-300"
                style={{ width: `${Math.round((qualityProgress.done / qualityProgress.total) * 100)}%` }}
              />
            </div>
            <span className="text-[10px] text-gray-500">
              {qualityProgress.done}/{qualityProgress.total} stars
            </span>
          </div>
        )}
      </div>

      <div className="w-px h-6 bg-gray-700 shrink-0" />

      {/* Stats */}
      {stats && (
        <div className="flex items-center gap-5">
          <Stat label="Images"   value={stats.total_images.toLocaleString()} />
          <Stat label="Objects"  value={stats.unique_objects.toLocaleString()} />
          <Stat label="Filters"  value={stats.unique_filters.toLocaleString()} />
          <Stat label="Exp. hrs" value={stats.total_exposure_hours.toFixed(1)} />
        </div>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Buttons */}
      <div className="flex items-center gap-2 shrink-0">
        <button
          onClick={rescanAll}
          disabled={scanning || dirs.length === 0}
          className="bg-gray-700 hover:bg-gray-600 disabled:opacity-40 disabled:cursor-not-allowed text-gray-200 text-xs font-medium py-1.5 px-3 rounded transition-colors"
        >
          Rescan All
        </button>
        <button
          onClick={addDirectory}
          disabled={scanning}
          className="bg-blue-600 hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-xs font-medium py-1.5 px-3 rounded transition-colors"
        >
          + Add Directory
        </button>
      </div>
    </header>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-sm font-semibold text-white leading-none">{value}</p>
      <p className="text-xs text-gray-500 mt-0.5">{label}</p>
    </div>
  );
}
