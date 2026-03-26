import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CalendarView } from "./components/CalendarView";
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
  const [objectOptions, setObjectOptions] = useState<string[]>([]);
  const [selected, setSelected] = useState<ImageRow | null>(null);

  const [search, setSearch] = useState("");
  const [imageType, setImageType] = useState("");
  const [filterName, setFilterName] = useState("");
  const [objectName, setObjectName] = useState("");

  const [activeView, setActiveView] = useState<"table" | "calendar">("table");

  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastResult, setLastResult] = useState<ScanResult | null>(null);
  const cancelledRef = useRef(false);

  const refreshDirs = useCallback(async () => {
    const [d, s, f, o] = await Promise.all([
      invoke<DirectoryEntry[]>("list_directories"),
      invoke<LibraryStats>("get_library_stats"),
      invoke<string[]>("get_filter_options"),
      invoke<string[]>("get_object_options"),
    ]);
    setDirs(d);
    setStats(s);
    setFilterOptions(f);
    setObjectOptions(o);
  }, []);

  const refreshImages = useCallback(async () => {
    try {
      const rows = await invoke<ImageRow[]>("list_images", {
        search: search || null,
        imageType: imageType || null,
        filterName: filterName || null,
        objectName: objectName || null,
      });
      setImages(rows);
    } catch (e) {
      console.error("list_images failed:", e);
      // Retry once after a short delay in case of transient mutex contention.
      setTimeout(async () => {
        try {
          const rows = await invoke<ImageRow[]>("list_images", {
            search: search || null,
            imageType: imageType || null,
            filterName: filterName || null,
            objectName: objectName || null,
          });
          setImages(rows);
        } catch (e2) {
          console.error("list_images retry failed:", e2);
        }
      }, 500);
    }
  }, [search, imageType, filterName, objectName]);

  useEffect(() => { refreshDirs(); }, [refreshDirs]);
  useEffect(() => { refreshImages(); }, [refreshImages]);

  useEffect(() => {
    const unlisten = listen<ScanProgress>("indexer://progress", (e) => {
      setProgress(e.payload);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // Listen for background quality updates and patch the images array in-place.
  useEffect(() => {
    const unlisten = listen<{ file_path: string; fwhm: number | null; star_count: number | null }>(
      "quality://update",
      (e) => {
        const { file_path, fwhm, star_count } = e.payload;
        setImages((prev) =>
          prev.map((img) =>
            img.file_path === file_path ? { ...img, fwhm, star_count } : img
          )
        );
        setSelected((prev) =>
          prev && prev.file_path === file_path ? { ...prev, fwhm, star_count } : prev
        );
      },
    );
    return () => { unlisten.then((f) => f()); };
  }, []);

  // Derive background quality progress from loaded images.
  const qualityProgress = useMemo(() => {
    const lights = images.filter((img) => img.image_type === "Light");
    if (lights.length === 0) return null;
    const done = lights.filter((img) => img.fwhm != null && img.fwhm > 0).length;
    return { done, total: lights.length };
  }, [images]);

  function handleScanStart() {
    cancelledRef.current = false;
    setScanning(true);
    setProgress(null);
    setLastResult(null);
  }

  function handleScanEnd(result: ScanResult) {
    setScanning(false);
    setProgress(null);
    // Show the summary only if the user didn't cancel.
    if (!cancelledRef.current) {
      setLastResult(result);
    }
    // Always refresh to show whatever was indexed (full scan or partial).
    refreshImages();
    refreshDirs();
  }

  async function handleCancel() {
    cancelledRef.current = true;
    await invoke("cancel_scan");
    setScanning(false);
    setProgress(null);
    setLastResult(null);
    // Don't refresh here — handleScanEnd will be called once the Rust command
    // finishes its last batch write and returns, giving us consistent data.
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
        qualityProgress={qualityProgress}
        onDirsChange={refreshDirs}
        onScanStart={handleScanStart}
        onScanEnd={handleScanEnd}
      />
      <div className="flex flex-1 min-h-0">
        <Sidebar dirs={dirs} onDirsChange={refreshDirs} onImagesChange={refreshImages} />
        <div className="flex flex-col flex-1 min-w-0">
          {/* Tab bar */}
          <div className="flex items-center border-b border-gray-800 px-4 gap-1 pt-1">
            {(["table", "calendar"] as const).map((view) => (
              <button
                key={view}
                onClick={() => setActiveView(view)}
                className={`px-4 py-1.5 text-sm font-medium rounded-t transition-colors capitalize ${
                  activeView === view
                    ? "bg-gray-800 text-gray-100 border border-b-0 border-gray-700"
                    : "text-gray-500 hover:text-gray-300"
                }`}
              >
                {view === "table" ? "Table" : "Calendar"}
              </button>
            ))}
          </div>

          {activeView === "table" && (
            <>
              <FilterBar
                search={search}
                imageType={imageType}
                filterName={filterName}
                objectName={objectName}
                filterOptions={filterOptions}
                objectOptions={objectOptions}
                onSearchChange={setSearch}
                onImageTypeChange={setImageType}
                onFilterNameChange={setFilterName}
                onObjectNameChange={setObjectName}
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
                  <DetailPanel
                    image={selected}
                    onClose={() => setSelected(null)}
                    onQualityComputed={(id, fwhm, starCount) => {
                      setImages((prev) =>
                        prev.map((img) =>
                          img.id === id ? { ...img, fwhm, star_count: starCount } : img
                        )
                      );
                      setSelected((prev) =>
                        prev && prev.id === id ? { ...prev, fwhm, star_count: starCount } : prev
                      );
                    }}
                  />
                )}
              </div>
            </>
          )}

          {activeView === "calendar" && (
            <CalendarView images={images} />
          )}
        </div>
      </div>
    </div>
  );
}
