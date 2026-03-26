import { createPortal } from "react-dom";
import { useEffect, useRef, useState } from "react";
import { ScanProgress as Progress, ScanResult } from "../types";

interface Props {
  scanning: boolean;
  progress: Progress | null;
  lastResult: ScanResult | null;
  onDismiss: () => void;
  onCancel: () => void;
}

function formatDuration(seconds: number): string {
  const s = Math.floor(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return rem > 0 ? `${m}m ${rem}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const mrem = m % 60;
  return mrem > 0 ? `${h}h ${mrem}m` : `${h}h`;
}

export function ScanProgressBar({ scanning, progress, lastResult, onDismiss, onCancel }: Props) {
  const startTimeRef = useRef<number | null>(null);
  const finalElapsedRef = useRef<number>(0);
  const [now, setNow] = useState(() => Date.now());

  // Record start time when scan begins; freeze elapsed when it ends
  useEffect(() => {
    if (scanning && startTimeRef.current === null) {
      startTimeRef.current = Date.now();
    }
    if (!scanning && startTimeRef.current !== null) {
      finalElapsedRef.current = (Date.now() - startTimeRef.current) / 1000;
      startTimeRef.current = null;
    }
  }, [scanning]);

  // Tick every second while scanning
  useEffect(() => {
    if (!scanning) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [scanning]);

  if (!scanning && !progress && !lastResult) return null;

  const pct = progress ? Math.round((progress.current / progress.total) * 100) : 0;

  const elapsedSec = scanning && startTimeRef.current
    ? (now - startTimeRef.current) / 1000
    : finalElapsedRef.current;
  const etaSec = (progress && progress.current > 0)
    ? (elapsedSec / progress.current) * (progress.total - progress.current)
    : null;

  const modal = (
    <div style={{
      position: "fixed", inset: 0, zIndex: 9999,
      display: "flex", alignItems: "center", justifyContent: "center",
      backgroundColor: "rgba(0,0,0,0.75)",
    }}>
      <div style={{
        backgroundColor: "#000", border: "1px solid #444",
        borderRadius: 12, boxShadow: "0 25px 50px rgba(0,0,0,0.8)",
        width: "100%", maxWidth: 440, margin: "0 16px", padding: 28,
        fontFamily: "inherit",
      }}>
        {scanning ? (
          <>
            <div style={{ fontSize: 15, fontWeight: 600, color: "#fff", marginBottom: 6 }}>
              {progress ? "Scanning…" : "Preparing…"}
            </div>

            <div style={{ fontSize: 13, color: "#aaa", marginBottom: 8 }}>
              {progress ? (
                <>
                  <span style={{ color: "#fff", fontWeight: 500 }}>{progress.current}</span>
                  {" of "}
                  <span style={{ color: "#fff", fontWeight: 500 }}>{progress.total}</span>
                  {" files"}
                </>
              ) : "Collecting files…"}
            </div>

            {/* Timing row */}
            {elapsedSec >= 1 && (
              <div style={{
                fontSize: 12, color: "#666", marginBottom: 14,
                display: "flex", gap: 16,
              }}>
                <span>Elapsed: <span style={{ color: "#aaa" }}>{formatDuration(elapsedSec)}</span></span>
                {etaSec !== null && etaSec > 0 && (
                  <span>ETA: <span style={{ color: "#aaa" }}>{formatDuration(etaSec)}</span></span>
                )}
              </div>
            )}

            {/* Progress bar */}
            <div style={{ width: "100%", height: 6, backgroundColor: "#222", borderRadius: 3, marginBottom: 10 }}>
              <div style={{
                height: 6, borderRadius: 3,
                backgroundColor: "#fff",
                width: progress ? `${pct}%` : "0%",
                transition: "width 150ms ease",
              }} />
            </div>

            {progress && (
              <div style={{
                fontSize: 11, color: "#666", marginBottom: 20,
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }} title={progress.file_name}>
                {progress.file_name}
              </div>
            )}

            <button onClick={onCancel} style={{
              width: "100%", padding: "9px 0",
              border: "1px solid #555", borderRadius: 8,
              backgroundColor: "transparent", color: "#fff",
              fontSize: 13, fontWeight: 500, cursor: "pointer",
            }}>
              Cancel
            </button>
          </>
        ) : lastResult ? (
          <>
            <div style={{ fontSize: 15, fontWeight: 600, color: "#fff", marginBottom: 18 }}>
              Scan complete
            </div>

            {/* Full bar */}
            <div style={{ width: "100%", height: 6, backgroundColor: "#222", borderRadius: 3, marginBottom: 18 }}>
              <div style={{ height: 6, borderRadius: 3, backgroundColor: "#fff", width: "100%" }} />
            </div>

            <div style={{ fontSize: 13, color: "#aaa", marginBottom: 22, lineHeight: 1.7 }}>
              <div><span style={{ color: "#fff", fontWeight: 500 }}>{lastResult.indexed}</span> indexed</div>
              <div><span style={{ color: "#fff", fontWeight: 500 }}>{lastResult.skipped}</span> skipped</div>
              {lastResult.errors > 0 && (
                <div><span style={{ color: "#f87171", fontWeight: 500 }}>{lastResult.errors}</span> errors</div>
              )}
              {elapsedSec >= 1 && (
                <div style={{ marginTop: 4 }}>
                  <span style={{ color: "#fff", fontWeight: 500 }}>{formatDuration(elapsedSec)}</span> total
                </div>
              )}
            </div>

            <button onClick={onDismiss} style={{
              width: "100%", padding: "9px 0",
              border: "none", borderRadius: 8,
              backgroundColor: "#fff", color: "#000",
              fontSize: 13, fontWeight: 600, cursor: "pointer",
            }}>
              Done
            </button>
          </>
        ) : null}
      </div>
    </div>
  );

  return createPortal(modal, document.body);
}
