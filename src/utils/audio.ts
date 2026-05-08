import type { TranscribeProgress } from "@/types";

export const audioExtensions = [
  "wav",
  "mp3",
  "ogg",
  "m4a",
  "flac",
  "opus",
  "webm",
  "aac",
  "wma",
] as const;

export function hasAudioExt(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (audioExtensions as readonly string[]).includes(ext);
}

export function basenameOf(p: string): string {
  let s = p;
  try {
    s = decodeURIComponent(s);
  } catch {
    /* leave as-is */
  }
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

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const NAME_PATTERNS: { re: RegExp; full4: boolean }[] = [
  { re: /[-_](\d{4})-(\d{2})-(\d{2})[-_T](\d{2})-(\d{2})-(\d{2})$/, full4: true },
  { re: /[-_](\d{4})(\d{2})(\d{2})[-_](\d{2})(\d{2})(\d{2})$/, full4: true },
  { re: /[-_](\d{2})(\d{2})(\d{2})[-_](\d{2})(\d{2})(\d{2})$/, full4: false },
];

export function prettyName(name: string): { display: string; timestamp: string | null } {
  const decoded = decodeName(name);
  const noExt = decoded.replace(/\.[^.]+$/, "");
  for (const { re, full4 } of NAME_PATTERNS) {
    const m = noExt.match(re);
    if (m && m.index !== undefined) {
      const display = noExt.slice(0, m.index);
      const [, a, b, c, d, e] = m;
      const yy = full4 ? a.slice(2) : a;
      const mi = parseInt(b, 10) - 1;
      const mon = mi >= 0 && mi < 12 ? MONTHS[mi] : b;
      return { display: display || noExt, timestamp: `${yy}-${mon}-${c} ${d}:${e}` };
    }
  }
  return { display: noExt, timestamp: null };
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
