import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { DirectoryEntry } from "../types";

interface Props {
  dirs: DirectoryEntry[];
  onDirsChange: () => void;
  onImagesChange: () => void;
}

export function Sidebar({ dirs, onDirsChange, onImagesChange }: Props) {
  async function removeDir(path: string) {
    await invoke("remove_directory", { path });
    onDirsChange();
    onImagesChange();
  }

  return (
    <aside className="w-52 shrink-0 bg-gray-900 border-r border-gray-700 flex flex-col overflow-y-auto">
      <div className="px-3 pt-3 pb-2 border-b border-gray-800">
        <p className="text-xs font-semibold text-gray-400 uppercase tracking-wider">Directories</p>
      </div>
      {dirs.length === 0 ? (
        <p className="text-xs text-gray-600 italic px-3">No directories added yet.</p>
      ) : (
        <ul className="px-2 pb-3 space-y-0.5">
          {dirs.map((d) => (
            <li
              key={d.id}
              className="group flex items-start justify-between gap-1 rounded px-2 py-1.5 hover:bg-gray-800 cursor-pointer"
              onClick={() => openPath(d.path)}
            >
              <div className="min-w-0">
                <p className="text-xs text-gray-300 truncate" title={d.path}>
                  {d.path.split(/[\\/]/).pop() || d.path}
                </p>
                <p className="text-xs text-gray-600">{d.image_count} images</p>
              </div>
              <button
                onClick={(e) => { e.stopPropagation(); removeDir(d.path); }}
                className="opacity-0 group-hover:opacity-100 text-gray-600 hover:text-red-400 text-xs shrink-0 transition-opacity"
                title="Remove directory"
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}
