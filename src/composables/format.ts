import type { FileProgress } from "@/types";

const pad2 = (n: number) => String(n).padStart(2, "0");

export function fmtClock(secs: number): string {
  const total = Math.max(0, Math.round(Number.isFinite(secs) ? secs : 0));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0 ? `${h}:${pad2(m)}:${pad2(s)}` : `${m}:${pad2(s)}`;
}

export function fmtMs(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${pad2(Math.floor(s / 60))}:${pad2(s % 60)}`;
}

export function fmtMsLong(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${pad2(Math.floor(s / 3600))}:${pad2(Math.floor((s % 3600) / 60))}:${pad2(s % 60)}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function fmtModelSize(bytes: number): string {
  if (!bytes) return "—";
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  return `${(bytes / 1_048_576).toFixed(0)} MB`;
}

export function progressPct(p?: FileProgress): number {
  if (!p || !p.total) return 0;
  const fileFrac = p.downloaded / p.total;
  return ((p.file_index + fileFrac) / p.file_count) * 100;
}
