<script setup lang="ts">
import { computed } from "vue";
import type { FileProgress, ModelInfo } from "../types";
import DownloadCircle from "./DownloadCircle.vue";

interface Props {
  essentialIds: string[];
  models: ModelInfo[];
  progress: Record<string, FileProgress>;
  errors: Record<string, true>;
}
const props = defineProps<Props>();

interface Row {
  id: string;
  name: string;
  family: string;
  sizeBytes: number;
  status: "installed" | "downloading" | "queued" | "error";
  percent: number;
  downloadedMb: number;
  totalMb: number;
}

const rows = computed<Row[]>(() => {
  return props.essentialIds.map((id) => {
    const m = props.models.find((x) => x.id === id);
    const p = props.progress[id];
    const errored = !!props.errors[id];
    let percent = 0;
    if (p && p.total > 0) {
      const fileFrac = p.downloaded / p.total;
      percent = ((p.file_index + fileFrac) / p.file_count) * 100;
    }
    let status: Row["status"];
    if (m?.status === "installed") {
      status = "installed";
      percent = 100;
    } else if (errored) {
      status = "error";
    } else if (p) {
      status = "downloading";
    } else {
      status = "queued";
    }
    return {
      id,
      name: m?.display_name ?? id,
      family: m?.family ?? "",
      sizeBytes: m?.size_bytes ?? 0,
      status,
      percent,
      downloadedMb: p ? p.downloaded / 1_048_576 : 0,
      totalMb: m?.size_bytes ? m.size_bytes / 1_048_576 : 0,
    };
  });
});

const overall = computed(() => {
  if (!rows.value.length) return 0;
  const total = rows.value.reduce((s, r) => s + r.percent, 0);
  return total / rows.value.length;
});

const totalBytes = computed(() => rows.value.reduce((s, r) => s + r.sizeBytes, 0));
const downloadedBytes = computed(() =>
  rows.value.reduce((s, r) => {
    if (r.status === "installed") return s + r.sizeBytes;
    if (r.status === "downloading") return s + (r.downloadedMb * 1_048_576);
    return s;
  }, 0)
);

function fmtSize(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function familyLabel(f: string): string {
  if (f === "asr") return "Speech recognition";
  if (f === "diarizer") return "Speaker separation";
  if (f === "llm") return "Auto-naming";
  return f;
}
</script>

<template>
  <div
    class="fixed inset-0 z-50 grid place-items-center bg-surface/95 backdrop-blur-sm overflow-y-auto px-xl py-xl"
  >
    <div class="w-full max-w-md flex flex-col gap-xl">
      <div class="flex flex-col items-center gap-md">
        <DownloadCircle :percent="overall" :size="96" />
        <h1 class="text-headlineSmall text-on-surface text-center">
          Setting up WTranscriber
        </h1>
        <p class="text-bodyMedium text-on-surface-variant text-center font-mono">
          {{ fmtSize(downloadedBytes) }} / {{ fmtSize(totalBytes) }}
        </p>
      </div>

      <ul class="flex flex-col gap-md">
        <li
          v-for="r in rows"
          :key="r.id"
          class="flex items-center gap-md py-md px-margin rounded-lg bg-surface-container/60 border border-outline/30"
        >
          <DownloadCircle
            :percent="r.percent"
            :size="44"
            :done="r.status === 'installed'"
            :errored="r.status === 'error'"
          />
          <div class="flex-1 min-w-0">
            <div class="text-titleSmall text-on-surface leading-tight break-words">
              {{ r.name }}
            </div>
            <div class="text-labelSmall text-on-surface-variant font-mono">
              {{ familyLabel(r.family) }}
              <span v-if="r.status === 'installed'"> · ready</span>
              <span v-else-if="r.status === 'queued'"> · queued · {{ fmtSize(r.sizeBytes) }}</span>
              <span v-else-if="r.status === 'downloading'">
                · {{ r.downloadedMb.toFixed(0) }} / {{ r.totalMb.toFixed(0) }} MB
              </span>
              <span v-else-if="r.status === 'error'"> · failed</span>
            </div>
          </div>
        </li>
      </ul>

      <p class="text-bodySmall text-on-surface-variant text-center font-mono">
        Stay on Wi-Fi until setup completes.
      </p>
    </div>
  </div>
</template>
