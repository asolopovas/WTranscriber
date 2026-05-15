import type { TranscribeProgress } from "@/types";

export const audioExtensions = [
  "wav",
  "wave",
  "mp3",
  "mp2",
  "mpga",
  "ogg",
  "oga",
  "ogv",
  "opus",
  "flac",
  "aac",
  "m4a",
  "m4b",
  "m4p",
  "m4r",
  "mp4",
  "m4v",
  "mov",
  "3gp",
  "3g2",
  "3gpp",
  "webm",
  "mkv",
  "mka",
  "avi",
  "wmv",
  "asf",
  "wma",
  "flv",
  "f4v",
  "f4a",
  "mpg",
  "mpeg",
  "ts",
  "mts",
  "m2ts",
  "vob",
  "aiff",
  "aif",
  "aifc",
  "au",
  "snd",
  "caf",
  "amr",
  "ac3",
  "eac3",
  "dts",
  "ape",
  "alac",
  "mpc",
  "wv",
  "tta",
  "ra",
  "rm",
  "rmvb",
  "voc",
  "gsm",
  "w64",
] as const;

export function hasAudioExt(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (audioExtensions as readonly string[]).includes(ext);
}

export function basenameOf(p: string): string {
  let s = p;
  try {
    s = decodeURIComponent(s);
  } catch {}
  const schemeStripped = s.replace(/^[a-z][a-z0-9+.-]*:(\/\/)?/i, "");
  const colonStripped = schemeStripped.replace(/^[A-Za-z0-9_-]+:/, "");
  const cleaned = colonStripped.replace(/\\/g, "/");
  const idx = cleaned.lastIndexOf("/");
  const tail = idx === -1 ? cleaned : cleaned.slice(idx + 1);
  return tail.split("?")[0] || "audio";
}

export function decodeName(name: string): string {
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
}

const NAME_PATTERNS: { re: RegExp; full4: boolean }[] = [
  { re: /(?:^|[-_\s])(\d{4})-(\d{2})-(\d{2})[-_T\s](\d{2})-(\d{2})-(\d{2})/, full4: true },
  { re: /(?:^|[-_\s])(\d{4})(\d{2})(\d{2})[-_\s](\d{2})(\d{2})(\d{2})/, full4: true },
  { re: /(?:^|[-_\s])(\d{2})(\d{2})(\d{2})[-_\s](\d{2})(\d{2})(\d{2})/, full4: false },
];

const MONTH_NAMES = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

export function formatTimestampPretty(ts: string | null): string | null {
  if (!ts) return null;
  const m = ts.match(/^(\d{2})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})$/);
  if (!m) return ts;
  const [, yy, mm, dd, hh, mi, ss] = m;
  const monthIdx = Number(mm) - 1;
  const month = monthIdx >= 0 && monthIdx < 12 ? MONTH_NAMES[monthIdx] : mm;
  return `${hh}:${mi}:${ss} ${dd}/${month}/${yy}`;
}

export function prettyName(name: string): {
  display: string;
  timestamp: string | null;
  timestampPretty: string | null;
} {
  const decoded = basenameOf(decodeName(name));
  const noExt = decoded.replace(/\.[^.]+$/, "");
  for (const { re, full4 } of NAME_PATTERNS) {
    const m = noExt.match(re);
    if (m && m.index !== undefined) {
      const before = noExt.slice(0, m.index).replace(/[-_\s]+$/, "");
      const after = noExt.slice(m.index + m[0].length).replace(/^[-_\s]+/, "");
      const display = [before, after].filter(Boolean).join(" ") || noExt;
      const [, a, b, c, d, e, f] = m;
      const yy = full4 ? a.slice(2) : a;
      const timestamp = `${yy}${b}${c}_${d}${e}${f}`;
      return { display, timestamp, timestampPretty: formatTimestampPretty(timestamp) };
    }
  }
  return { display: noExt, timestamp: null, timestampPretty: null };
}

const PHASE_LABELS: Record<TranscribeProgress["phase"], string> = {
  cache_check: "checking cache",
  loading_audio: "loading audio",
  transcribing: "transcribing",
  diarizing: "diarizing",
  writing: "writing",
  done: "done",
};

export function phaseLabel(p: TranscribeProgress["phase"]): string {
  return PHASE_LABELS[p];
}
