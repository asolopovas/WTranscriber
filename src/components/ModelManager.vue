<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, events } from "../api";
import type { FileProgress, ModelInfo } from "../types";

const models = ref<ModelInfo[]>([]);
const progress = ref<Record<string, FileProgress>>({});
const unlisten: (() => void)[] = [];

async function refresh() {
  models.value = await api.listModels();
}

async function install(id: string) {
  try {
    await api.installModel(id);
  } finally {
    delete progress.value[id];
    await refresh();
  }
}

function fmtSize(bytes: number): string {
  if (!bytes) return "—";
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  return `${(bytes / 1_048_576).toFixed(0)} MB`;
}

function pct(p?: FileProgress): number {
  if (!p || !p.total) return 0;
  const fileFrac = p.downloaded / p.total;
  return ((p.file_index + fileFrac) / p.file_count) * 100;
}

const grouped = computed(() => {
  const families: Record<string, ModelInfo[]> = { asr: [], diarizer: [], llm: [] };
  for (const m of models.value) families[m.family]?.push(m);
  return [
    { id: "asr", label: "ASR Engines", icon: "graphic_eq", items: families.asr },
    { id: "diarizer", label: "Diarizers", icon: "groups", items: families.diarizer },
    { id: "llm", label: "Language Models", icon: "model_training", items: families.llm },
  ].filter((g) => g.items.length);
});

onMounted(async () => {
  await refresh();
  unlisten.push(
    await events.onModelProgress((p) => {
      progress.value = { ...progress.value, [p.id]: p };
    }),
    await events.onModelDone(refresh),
    await events.onModelError(refresh),
  );
});

onUnmounted(() => unlisten.forEach((u) => u()));
</script>

<template>
  <main class="flex-1 overflow-y-auto p-xl bg-surface-container-lowest scroll-thin">
    <div class="max-w-5xl mx-auto flex flex-col gap-xl pb-xl">
      <div class="flex items-end justify-between pb-md border-b border-outline-variant/50">
        <div>
          <h1 class="text-[24px] leading-[32px] font-bold text-on-surface">Local Models</h1>
          <p class="text-bodyMedium text-on-surface-variant mt-unit">Install and manage on-device models for transcription, diarization, and naming.</p>
        </div>
      </div>

      <section
        v-for="g in grouped"
        :key="g.id"
        class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden"
      >
        <div class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs">
          <span class="material-symbols-outlined text-tertiary">{{ g.icon }}</span>
          <h2 class="text-titleMedium text-on-surface">{{ g.label }}</h2>
        </div>
        <table class="w-full text-left border-collapse">
          <thead>
            <tr class="border-b border-outline-variant/40 bg-surface-container-highest/40">
              <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium">Name</th>
              <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-28">Size</th>
              <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-48">Status</th>
              <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium text-right w-32">Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="m in g.items"
              :key="m.id"
              class="border-b border-outline-variant/30 last:border-b-0 hover:bg-surface-container-high/40 transition-colors"
            >
              <td class="px-margin py-md align-top">
                <div class="font-mono text-labelMedium text-on-surface">{{ m.id }}</div>
                <div class="text-bodyMedium text-on-surface-variant mt-unit">{{ m.description }}</div>
                <div v-if="m.default_active" class="font-mono text-labelSmall text-secondary mt-unit uppercase tracking-wide">default</div>
              </td>
              <td class="px-margin py-md text-on-surface-variant align-top whitespace-nowrap">{{ fmtSize(m.size_bytes) }}</td>
              <td class="px-margin py-md align-top">
                <div v-if="progress[m.id]" class="flex flex-col gap-unit w-40">
                  <div class="h-1 bg-surface-container-highest rounded-full overflow-hidden">
                    <div class="h-full bg-primary transition-all" :style="{ width: pct(progress[m.id]) + '%' }"></div>
                  </div>
                  <span class="font-mono text-labelSmall text-primary">{{ pct(progress[m.id]).toFixed(0) }}% · file {{ progress[m.id].file_index + 1 }}/{{ progress[m.id].file_count }}</span>
                </div>
                <span
                  v-else-if="m.status === 'installed'"
                  class="inline-flex items-center gap-unit bg-tertiary-container/30 text-tertiary border border-tertiary/30 px-xs py-unit rounded-full font-mono text-labelSmall"
                >
                  <span class="w-2 h-2 rounded-full bg-tertiary"></span> Installed
                </span>
                <span
                  v-else-if="m.status === 'downloading'"
                  class="inline-flex items-center gap-unit bg-secondary-container/40 text-secondary border border-secondary/30 px-xs py-unit rounded-full font-mono text-labelSmall"
                >
                  <span class="w-2 h-2 rounded-full bg-secondary animate-pulse"></span> Downloading
                </span>
                <span
                  v-else
                  class="inline-flex items-center gap-unit border border-outline-variant text-on-surface-variant px-xs py-unit rounded-full font-mono text-labelSmall"
                >
                  <span class="w-2 h-2 rounded-full bg-outline-variant"></span> Not installed
                </span>
              </td>
              <td class="px-margin py-md text-right align-top">
                <button
                  v-if="m.status === 'not_installed'"
                  class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim transition-colors inline-flex items-center gap-unit"
                  @click="install(m.id)"
                >
                  <span class="material-symbols-outlined text-[16px]">download</span>
                  Install
                </button>
                <span
                  v-else-if="m.status === 'installed'"
                  class="text-outline material-symbols-outlined text-[20px] cursor-not-allowed"
                  title="Installed"
                >check</span>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
  </main>
</template>
