import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { DirectoryEntry, LibraryStats, ScanResult } from "../types";

interface Props {
  dirs: DirectoryEntry[];
  stats: LibraryStats | null;
  scanning: boolean;
  onDirsChange: () => void;
  onScanStart: () => void;
  onScanEnd: (r: ScanResult) => void;
}

export function Sidebar({ dirs, stats, scanning, onDirsChange, onScanStart, onScanEnd }: Props) {
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

  async function removeDir(path: string) {
    await invoke("remove_directory", { path });
    onDirsChange();
  }

  return (
    <aside className="w-64 shrink-0 bg-gray-900 border-r border-gray-700 flex flex-col h-full">
      <div className="p-4 border-b border-gray-700">
        <h1 className="text-lg font-semibold text-white tracking-wide">AstroIndex</h1>
        <p className="text-xs text-gray-400 mt-0.5">FITS &amp; XISF Library</p>
      </div>

      {stats && (
        <div className="p-4 border-b border-gray-700 grid grid-cols-2 gap-2 text-center">
          <StatBox label="Images" value={stats.total_images.toLocaleString()} />
          <StatBox label="Objects" value={stats.unique_objects.toLocaleString()} />
          <StatBox label="Filters" value={stats.unique_filters.toLocaleString()} />
          <StatBox label="Exp. hrs" value={stats.total_exposure_hours.toFixed(1)} />
        </div>
      )}

      <div className="p-4 flex flex-col gap-2">
        <button
          onClick={addDirectory}
          disabled={scanning}
          className="w-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium py-2 px-3 rounded transition-colors"
        >
          + Add Directory
        </button>
        <button
          onClick={rescanAll}
          disabled={scanning || dirs.length === 0}
          className="w-full bg-gray-700 hover:bg-gray-600 disabled:opacity-50 disabled:cursor-not-allowed text-gray-200 text-sm font-medium py-2 px-3 rounded transition-colors"
        >
          Rescan All
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 pb-4">
        <p className="text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">
          Directories
        </p>
        {dirs.length === 0 ? (
          <p className="text-xs text-gray-600 italic">No directories added yet.</p>
        ) : (
          <ul className="space-y-1">
            {dirs.map((d) => (
              <li
                key={d.id}
                className="group flex items-start justify-between gap-1 rounded px-2 py-1.5 hover:bg-gray-800"
              >
                <div className="min-w-0">
                  <p className="text-xs text-gray-300 truncate" title={d.path}>
                    {d.path.split(/[\\/]/).pop() || d.path}
                  </p>
                  <p className="text-xs text-gray-600">{d.image_count} images</p>
                </div>
                <button
                  onClick={() => removeDir(d.path)}
                  className="opacity-0 group-hover:opacity-100 text-gray-600 hover:text-red-400 text-xs shrink-0 transition-opacity"
                  title="Remove directory"
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </aside>
  );
}

function StatBox({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-gray-800 rounded p-2">
      <p className="text-lg font-semibold text-white leading-none">{value}</p>
      <p className="text-xs text-gray-500 mt-0.5">{label}</p>
    </div>
  );
}
