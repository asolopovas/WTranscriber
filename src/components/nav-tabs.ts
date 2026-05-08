export type Tab = "transcribe" | "settings" | "logs";

export const TABS: { id: Tab; label: string }[] = [
  { id: "transcribe", label: "Transcribe" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];
