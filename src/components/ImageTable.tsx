import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ImageRow } from "../types";

interface Props {
  images: ImageRow[];
  onSelect: (img: ImageRow) => void;
  selectedId: number | null;
}

type SortKey = keyof Pick<
  ImageRow,
  "file_name" | "object_name" | "image_type" | "filter_name" | "exposure_time" | "date_obs" | "instrument" | "fwhm" | "star_count"
>;

export function ImageTable({ images, onSelect, selectedId }: Props) {
  const [sortKey, setSortKey] = useState<SortKey>("date_obs");
  const [sortAsc, setSortAsc] = useState(false);

  function toggleSort(key: SortKey) {
    if (sortKey === key) setSortAsc((a) => !a);
    else { setSortKey(key); setSortAsc(true); }
  }

  const sorted = [...images].sort((a, b) => {
    const av = a[sortKey] ?? "";
    const bv = b[sortKey] ?? "";
    const numA = typeof av === "number" ? av : null;
    const numB = typeof bv === "number" ? bv : null;
    let cmp: number;
    if (numA !== null && numB !== null) {
      cmp = numA - numB;
    } else if (numA !== null) {
      cmp = 1; // nulls last
    } else if (numB !== null) {
      cmp = -1;
    } else {
      cmp = String(av).localeCompare(String(bv), undefined, { numeric: true });
    }
    return sortAsc ? cmp : -cmp;
  });

  if (images.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-600 text-sm">
        No images found. Add a directory to get started.
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto">
      <table className="w-full text-sm text-left border-collapse">
        <thead className="sticky top-0 bg-gray-900 z-10">
          <tr>
            <Th label="File" sortKey="file_name" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Object" sortKey="object_name" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Type" sortKey="image_type" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Filter" sortKey="filter_name" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Exp (s)" sortKey="exposure_time" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Date" sortKey="date_obs" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Instrument" sortKey="instrument" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <th className="px-3 py-2 text-xs font-medium text-gray-500 uppercase tracking-wider whitespace-nowrap">
              Dims
            </th>
            <th className="px-3 py-2 text-xs font-medium text-gray-500 uppercase tracking-wider">
              Temp
            </th>
            <Th label="FWHM" sortKey="fwhm" current={sortKey} asc={sortAsc} onSort={toggleSort} />
            <Th label="Stars" sortKey="star_count" current={sortKey} asc={sortAsc} onSort={toggleSort} />
          </tr>
        </thead>
        <tbody>
          {sorted.map((img) => (
            <tr
              key={img.id}
              onClick={() => onSelect(img)}
              onDoubleClick={() => invoke("open_file", { path: img.file_path })}
              className={`border-t border-gray-800 cursor-pointer transition-colors ${
                img.id === selectedId
                  ? "bg-blue-900/40"
                  : img.quality_rejected
                  ? "bg-red-950/20 hover:bg-red-900/20"
                  : "hover:bg-gray-800/60"
              }`}
            >
              <td className="px-3 py-2 text-gray-300 font-mono text-xs max-w-[200px] truncate" title={img.file_name}>
                {img.parse_error ? (
                  <span className="text-red-400" title={img.parse_error}>⚠ {img.file_name}</span>
                ) : img.file_name}
              </td>
              <td className="px-3 py-2 text-gray-200 whitespace-nowrap">{img.object_name ?? "—"}</td>
              <td className="px-3 py-2 whitespace-nowrap">
                <TypeBadge type={img.image_type} />
              </td>
              <td className="px-3 py-2 text-gray-300 whitespace-nowrap">{img.filter_name ?? "—"}</td>
              <td className="px-3 py-2 text-gray-300 text-right whitespace-nowrap">
                {img.exposure_time != null ? img.exposure_time.toFixed(1) : "—"}
              </td>
              <td className="px-3 py-2 text-gray-400 whitespace-nowrap text-xs">
                {img.date_obs ? img.date_obs.replace("T", " ").slice(0, 19) : "—"}
              </td>
              <td className="px-3 py-2 text-gray-400 text-xs max-w-[160px] truncate" title={img.instrument ?? ""}>
                {img.instrument ?? "—"}
              </td>
              <td className="px-3 py-2 text-gray-500 text-xs whitespace-nowrap">
                {img.width && img.height ? `${img.width}×${img.height}` : "—"}
              </td>
              <td className="px-3 py-2 text-gray-500 text-xs whitespace-nowrap">
                {img.ccd_temp != null ? `${img.ccd_temp.toFixed(1)}°C` : "—"}
              </td>
              <td className="px-3 py-2 text-gray-300 text-xs whitespace-nowrap">
                {img.fwhm != null ? `${img.fwhm.toFixed(2)}"` : "—"}
              </td>
              <td className="px-3 py-2 text-gray-300 text-xs whitespace-nowrap text-right">
                {img.star_count != null ? img.star_count.toLocaleString() : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Th({
  label, sortKey, current, asc, onSort,
}: {
  label: string;
  sortKey: SortKey;
  current: SortKey;
  asc: boolean;
  onSort: (k: SortKey) => void;
}) {
  const active = current === sortKey;
  return (
    <th
      onClick={() => onSort(sortKey)}
      className="px-3 py-2 text-xs font-medium text-gray-500 uppercase tracking-wider whitespace-nowrap cursor-pointer select-none hover:text-gray-300 transition-colors"
    >
      {label}
      {active && <span className="ml-1 text-blue-400">{asc ? "↑" : "↓"}</span>}
    </th>
  );
}

function TypeBadge({ type }: { type: string | null }) {
  const colors: Record<string, string> = {
    "light frame": "bg-blue-900/60 text-blue-300",
    "dark frame": "bg-gray-700 text-gray-400",
    "flat frame": "bg-yellow-900/60 text-yellow-300",
    "bias frame": "bg-purple-900/60 text-purple-300",
  };
  const key = (type ?? "").toLowerCase();
  const cls = colors[key] ?? "bg-gray-800 text-gray-400";
  const label = type ? type.replace(" Frame", "") : "—";
  return (
    <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${cls}`}>{label}</span>
  );
}
