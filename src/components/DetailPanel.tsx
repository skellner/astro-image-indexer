import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { ImageDetail, ImageRow } from "../types";

interface Props {
  image: ImageRow;
  onClose: () => void;
}

export function DetailPanel({ image, onClose }: Props) {
  const [detail, setDetail] = useState<ImageDetail | null>(null);
  const [showRaw, setShowRaw] = useState(false);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  useEffect(() => {
    setDetail(null);
    invoke<{ row: ImageRow } & Omit<ImageDetail, keyof ImageRow>>("get_image_detail", { id: image.id })
      .then((d) => setDetail(d as unknown as ImageDetail))
      .catch(console.error);
  }, [image.id]);

  useEffect(() => {
    setPreviewUrl(null);
    setPreviewError(null);
    setPreviewLoading(true);
    invoke<string>("get_image_preview", { filePath: image.file_path })
      .then((url) => { setPreviewUrl(url); setPreviewLoading(false); })
      .catch((e) => { setPreviewError(String(e)); setPreviewLoading(false); });
  }, [image.file_path]);

  return (
    <div className="w-80 shrink-0 bg-gray-900 border-l border-gray-700 flex flex-col h-full">
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-700">
        <h2 className="text-sm font-semibold text-white truncate" title={image.file_name}>
          {image.file_name}
        </h2>
        <button onClick={onClose} className="text-gray-500 hover:text-gray-300 ml-2 shrink-0">
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4 text-sm">
        {previewLoading && (
          <div className="flex items-center justify-center h-32 bg-gray-800 rounded text-xs text-gray-500">
            Loading preview…
          </div>
        )}
        {previewUrl && (
          <img
            src={previewUrl}
            alt="Preview"
            className="w-full rounded border border-gray-700"
            style={{ imageRendering: "pixelated" }}
          />
        )}
        {previewError && !previewLoading && (
          <div className="flex items-center justify-center h-16 bg-gray-800 rounded text-xs text-gray-600 italic px-2 text-center">
            Preview unavailable
          </div>
        )}
        <Section title="Target">
          <Row label="Object" value={image.object_name} />
          <Row label="RA" value={detail?.ra != null ? `${detail.ra.toFixed(5)}°` : null} />
          <Row label="Dec" value={detail?.dec != null ? `${detail.dec.toFixed(5)}°` : null} />
        </Section>

        <Section title="Capture">
          <Row label="Date" value={image.date_obs?.replace("T", " ").slice(0, 19)} />
          <Row label="Exposure" value={image.exposure_time != null ? `${image.exposure_time} s` : null} />
          <Row label="Filter" value={image.filter_name} />
          <Row label="Gain" value={image.gain?.toString()} />
          <Row label="ISO" value={detail?.iso?.toString()} />
          <Row label="Offset" value={detail?.offset?.toString()} />
          <Row
            label="Binning"
            value={
              detail?.binning_x && detail?.binning_y
                ? `${detail.binning_x}×${detail.binning_y}`
                : null
            }
          />
          <Row label="CCD Temp" value={image.ccd_temp != null ? `${image.ccd_temp.toFixed(1)} °C` : null} />
        </Section>

        <Section title="Equipment">
          <Row label="Telescope" value={image.telescope} />
          <Row label="Camera" value={image.instrument} />
          <Row label="Focal length" value={detail?.focal_length != null ? `${detail.focal_length} mm` : null} />
          <Row label="Aperture" value={detail?.aperture != null ? `${detail.aperture} mm` : null} />
          <Row label="Software" value={image.software} />
        </Section>

        <Section title="Image">
          <Row
            label="Dimensions"
            value={image.width && image.height ? `${image.width} × ${image.height}` : null}
          />
          <Row label="Bit depth" value={detail?.bit_depth?.toString()} />
          <Row label="Format" value={image.format} />
          <Row label="Type" value={image.image_type} />
        </Section>

        {(image.fwhm || image.eccentricity || image.star_count || image.snr) && (
          <Section title="Quality">
            <Row label="FWHM" value={image.fwhm != null ? `${image.fwhm.toFixed(2)}"` : null} />
            <Row label="Eccentricity" value={image.eccentricity?.toFixed(3)} />
            <Row label="Stars" value={image.star_count?.toLocaleString()} />
            <Row label="SNR" value={image.snr?.toFixed(1)} />
            <Row label="Sky bg" value={detail?.sky_background?.toFixed(2)} />
            {image.quality_rejected && (
              <p className="text-xs text-red-400 mt-1">Marked as rejected</p>
            )}
          </Section>
        )}

        <Section title="Site">
          <Row label="Latitude" value={detail?.site_lat != null ? `${detail.site_lat.toFixed(4)}°` : null} />
          <Row label="Longitude" value={detail?.site_lon != null ? `${detail.site_lon.toFixed(4)}°` : null} />
          <Row label="Airmass" value={detail?.airmass?.toFixed(3)} />
        </Section>

        <Section title="File">
          <Row
            label="Size"
            value={detail?.file_size != null ? formatBytes(detail.file_size) : null}
          />
          <Row label="Path" value={image.file_path} mono />
        </Section>

        {image.parse_error && (
          <Section title="Parse Error">
            <p className="text-xs text-red-400 break-all">{image.parse_error}</p>
          </Section>
        )}

        {detail && detail.raw_headers.length > 0 && (
          <div>
            <button
              onClick={() => setShowRaw((v) => !v)}
              className="text-xs text-gray-500 hover:text-gray-300 transition-colors"
            >
              {showRaw ? "▾" : "▸"} Raw headers ({detail.raw_headers.length})
            </button>
            {showRaw && (
              <div className="mt-2 bg-gray-800 rounded p-2 max-h-64 overflow-y-auto font-mono text-xs text-gray-400 space-y-0.5">
                {detail.raw_headers.map(([k, v]) => (
                  <div key={k} className="flex gap-2">
                    <span className="text-gray-500 shrink-0 w-28">{k}</span>
                    <span className="break-all">{v}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1.5">
        {title}
      </p>
      <div className="space-y-1">{children}</div>
    </div>
  );
}

function Row({ label, value, mono = false }: { label: string; value: string | null | undefined; mono?: boolean }) {
  if (value == null || value === "") return null;
  return (
    <div className="flex gap-2 items-baseline">
      <span className="text-gray-500 text-xs w-24 shrink-0">{label}</span>
      <span className={`text-gray-300 text-xs break-all ${mono ? "font-mono" : ""}`}>{value}</span>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
