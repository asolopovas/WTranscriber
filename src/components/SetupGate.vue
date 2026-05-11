<script setup lang="ts">
import { computed } from "vue";
import type { FileProgress, ModelInfo } from "@/types";
import { fmtModelSize, MB, progressPct } from "@utils/format";
import DownloadCircle from "@components/DownloadCircle.vue";

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
    let percent = progressPct(p);
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
      downloadedMb: p ? p.downloaded / MB : 0,
      totalMb: m?.size_bytes ? m.size_bytes / MB : 0,
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
    if (r.status === "downloading") return s + r.downloadedMb * MB;
    return s;
  }, 0),
);

function familyLabel(f: string): string {
  if (f === "asr") return "Speech";
  if (f === "diarizer") return "Speakers";
  if (f === "llm") return "Naming";
  return f || "Model";
}

function statusLabel(row: Row): string {
  if (row.status === "installed") return "Ready";
  if (row.status === "error") return "Failed";
  if (row.status === "downloading") {
    return `${row.downloadedMb.toFixed(0)} / ${row.totalMb.toFixed(0)} MB`;
  }
  return `Queued · ${fmtModelSize(row.sizeBytes)}`;
}
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface/95 backdrop-blur-sm overflow-y-auto px-xl py-xl"
  >
    <div class="w-full max-w-112 flex flex-col gap-lg">
      <div class="flex flex-col items-center gap-md text-center">
        <div
          class="w-20 h-20 rounded-full bg-primary text-on-primary flex items-center justify-center"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            class="w-14 h-14"
            aria-hidden="true"
          >
            <path
              fill="currentColor"
              d="M17.617 6.383a7.944 7.944 0 0 1-1.748 12.568a8.028 8.028 0 0 1-11.586-5.043a8.028 8.028 0 0 1 2.095-7.517c.451-.46-.256-1.168-.707-.707a8.946 8.946 0 0 0 9.756 14.586a8.946 8.946 0 0 0 2.9-14.594c-.451-.461-1.158.247-.707.707Z"
            />
            <path
              fill="currentColor"
              d="m15.355 10.6l-3 3a.5.5 0 0 1-.35.15a.508.508 0 0 1-.36-.15l-3-3a.5.5 0 0 1 .71-.71l2.14 2.14V3.555a.508.508 0 0 1 .5-.5a.5.5 0 0 1 .5.5v8.49l2.15-2.16a.5.5 0 0 1 .71.71Z"
            />
          </svg>
        </div>
        <div class="space-y-xs">
          <h1 class="text-headlineSmall text-on-surface">Downloading essentials</h1>
          <p class="text-bodySmall text-on-surface-variant">
            Preparing speech, speakers and naming models.
          </p>
          <p class="text-labelMedium text-on-surface-variant font-mono">
            {{ fmtModelSize(downloadedBytes) }} / {{ fmtModelSize(totalBytes) }} ·
            {{ overall.toFixed(0) }}%
          </p>
        </div>
      </div>

      <ul class="flex flex-col gap-sm">
        <li
          v-for="r in rows"
          :key="r.id"
          class="flex items-center gap-md py-sm px-margin rounded-lg bg-surface-container/60 border border-outline/30"
        >
          <DownloadCircle
            :percent="r.percent"
            :size="44"
            :done="r.status === 'installed'"
            :errored="r.status === 'error'"
          />
          <div class="flex-1 min-w-0">
            <div class="text-titleSmall text-on-surface leading-tight" :title="r.name">
              {{ familyLabel(r.family) }}
            </div>
            <div class="text-labelSmall text-on-surface-variant font-mono">
              {{ statusLabel(r) }}
            </div>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>
