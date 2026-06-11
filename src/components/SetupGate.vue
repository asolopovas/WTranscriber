<script setup lang="ts">
import { computed } from "vue";
import type { FileProgress, ModelInfo } from "@/types";
import type { RuntimeState } from "@composables/useEssentials";
import { fmtModelSize, MB, progressPct } from "@utils/format";
import DownloadCircle from "@components/DownloadCircle.vue";

interface Props {
  essentialIds: string[];
  models: ModelInfo[];
  progress: Record<string, FileProgress>;
  errors: Record<string, true>;
  runtimes: Record<string, RuntimeState>;
}
const props = defineProps<Props>();

interface Row {
  id: string;
  name: string;
  kind: "runtime" | "model";
  family: string;
  sizeBytes: number;
  status: "installed" | "downloading" | "queued" | "error" | "starting";
  percent: number;
  downloadedMb: number;
  totalMb: number;
}

const runtimeRows = computed<Row[]>(() => {
  const list = Object.values(props.runtimes);
  return list
    .filter((r) => r.phase !== "done")
    .map((r) => {
      const percent = r.total > 0 ? Math.min(100, (r.downloaded / r.total) * 100) : 0;
      let status: Row["status"];
      if (r.phase === "error") status = "error";
      else if (r.phase === "starting") status = "starting";
      else status = "downloading";
      return {
        id: r.id,
        name: r.label,
        kind: "runtime",
        family: "runtime",
        sizeBytes: r.total,
        status,
        percent,
        downloadedMb: r.downloaded / MB,
        totalMb: r.total / MB,
      };
    });
});

const modelRows = computed<Row[]>(() => {
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
      kind: "model",
      family: m?.family ?? "",
      sizeBytes: m?.size_bytes ?? 0,
      status,
      percent,
      downloadedMb: p ? p.downloaded / MB : 0,
      totalMb: m?.size_bytes ? m.size_bytes / MB : 0,
    };
  });
});

const allRows = computed<Row[]>(() => [...runtimeRows.value, ...modelRows.value]);

const overall = computed(() => {
  if (!allRows.value.length) return 0;
  const total = allRows.value.reduce((s, r) => s + r.percent, 0);
  return total / allRows.value.length;
});

const totalBytes = computed(() => allRows.value.reduce((s, r) => s + r.sizeBytes, 0));
const downloadedBytes = computed(() =>
  allRows.value.reduce((s, r) => {
    if (r.status === "installed") return s + r.sizeBytes;
    if (r.status === "downloading") return s + r.downloadedMb * MB;
    return s;
  }, 0),
);

const activeRuntime = computed<Row | null>(() => {
  return runtimeRows.value.find((r) => r.status === "downloading") ?? runtimeRows.value[0] ?? null;
});

const subtitle = computed(() => {
  const active = activeRuntime.value;
  if (active) {
    if (active.status === "error") return `${active.name} failed — see logs.`;
    if (active.status === "starting") return `Preparing ${active.name}…`;
    return `Installing ${active.name}…`;
  }
  return "Preparing speech, speakers and naming models.";
});

function familyLabel(row: Row): string {
  if (row.kind === "runtime") return row.name;
  if (row.family === "asr") return "Speech";
  if (row.family === "diarizer") return "Speakers";
  if (row.family === "llm") return "Naming";
  if (row.family === "langid") return "Language";
  return row.family || "Model";
}

function statusLabel(row: Row): string {
  if (row.status === "installed") return "Ready";
  if (row.status === "error") return "Failed";
  if (row.status === "starting") return "Starting…";
  if (row.status === "downloading") {
    if (row.totalMb > 0) {
      return `${row.downloadedMb.toFixed(0)} / ${row.totalMb.toFixed(0)} MB`;
    }
    return `${row.downloadedMb.toFixed(0)} MB downloaded`;
  }
  return row.sizeBytes > 0 ? `Queued · ${fmtModelSize(row.sizeBytes)}` : "Queued";
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
          <p class="text-bodySmall text-on-surface-variant">{{ subtitle }}</p>
          <p class="text-labelMedium text-on-surface-variant font-mono">
            {{ fmtModelSize(downloadedBytes) }} / {{ fmtModelSize(totalBytes) }} ·
            {{ overall.toFixed(0) }}%
          </p>
        </div>
      </div>

      <ul class="flex flex-col gap-md">
        <li
          v-for="r in allRows"
          :key="`${r.kind}:${r.id}`"
          class="flex items-center gap-md py-md px-margin rounded-lg bg-surface-container/60 border border-outline/30"
        >
          <DownloadCircle
            :percent="r.percent"
            :size="44"
            :done="r.status === 'installed'"
            :errored="r.status === 'error'"
          />
          <div class="flex-1 min-w-0">
            <div class="text-titleSmall text-on-surface leading-tight" :title="r.name">
              {{ familyLabel(r) }}
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
