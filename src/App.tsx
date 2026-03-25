import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DetailPanel } from "./components/DetailPanel";
import { FilterBar } from "./components/FilterBar";
import { ImageTable } from "./components/ImageTable";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { ScanProgressBar } from "./components/ScanProgress";
import {
  DirectoryEntry,
  ImageRow,
  LibraryStats,
  ScanProgress,
  ScanResult,
} from "./types";

export default function App() {
  const [dirs, setDirs] = useState<DirectoryEntry[]>([]);
  const [stats, setStats] = useState<LibraryStats | null>(null);
  const [images, setImages] = useState<ImageRow[]>([]);
  const [filterOptions, setFilterOptions] = useState<string[]>([]);
  const [selected, setSelected] = useState<ImageRow | null>(null);

  const [search, setSearch] = useState("");
  const [imageType, setImageType] = useState("");
  const [filterName, setFilterName] = useState("");

  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastResult, setLastResult] = useState<ScanResult | null>(null);

  const refreshDirs = useCallback(async () => {
    const [d, s, f] = await Promise.all([
      invoke<DirectoryEntry[]>("list_directories"),
      invoke<LibraryStats>("get_library_stats"),
      invoke<string[]>("get_filter_options"),
    ]);
    setDirs(d);
    setStats(s);
    setFilterOptions(f);
  }, []);

  const refreshImages = useCallback(async () => {
    const rows = await invoke<ImageRow[]>("list_images", {
      search: search || null,
      imageType: imageType || null,
      filterName: filterName || null,
    });
    setImages(rows);
  }, [search, imageType, filterName]);

  useEffect(() => { refreshDirs(); }, [refreshDirs]);
  useEffect(() => { refreshImages(); }, [refreshImages]);

  useEffect(() => {
    const unlisten = listen<ScanProgress>("indexer://progress", (e) => {
      setProgress(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  function handleScanStart() {
    setScanning(true);
    setProgress(null);
    setLastResult(null);
  }

  function handleScanEnd(result: ScanResult) {
    setScanning(false);
    setProgress(null);
    setLastResult(result);
    refreshImages();
    refreshDirs();
  }

  async function handleCancel() {
    await invoke("cancel_scan");
    setScanning(false);
    setProgress(null);
    setLastResult(null);
  }

  return (
    <div className="flex flex-col h-screen bg-gray-950 text-gray-200 overflow-hidden">
      <ScanProgressBar
        scanning={scanning}
        progress={progress}
        lastResult={lastResult}
        onDismiss={() => setLastResult(null)}
        onCancel={handleCancel}
      />
      <TopBar
        stats={stats}
        dirs={dirs}
        scanning={scanning}
        onDirsChange={refreshDirs}
        onScanStart={handleScanStart}
        onScanEnd={handleScanEnd}
      />
      <div className="flex flex-1 min-h-0">
        <Sidebar dirs={dirs} onDirsChange={refreshDirs} />
        <div className="flex flex-col flex-1 min-w-0">
          <FilterBar
            search={search}
            imageType={imageType}
            filterName={filterName}
            filterOptions={filterOptions}
            onSearchChange={setSearch}
            onImageTypeChange={setImageType}
            onFilterNameChange={setFilterName}
          />
          <div className="flex items-center px-4 py-1.5 border-b border-gray-800">
            <span className="text-xs text-gray-500">
              {images.length} image{images.length !== 1 ? "s" : ""}
            </span>
          </div>
          <div className="flex flex-1 min-h-0">
            <ImageTable
              images={images}
              onSelect={setSelected}
              selectedId={selected?.id ?? null}
            />
            {selected && (
              <DetailPanel image={selected} onClose={() => setSelected(null)} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
