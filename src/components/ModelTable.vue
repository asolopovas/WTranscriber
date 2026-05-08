<script setup lang="ts">
import { computed } from "vue";
import type { FileProgress, ModelInfo } from "../types";
import { fmtModelSize, progressPct } from "../composables/format";
import Icon from "./ui/Icon.vue";
import StatusPill from "./ui/StatusPill.vue";

const props = defineProps<{
  models: ModelInfo[];
  progress: Record<string, FileProgress>;
  showStats?: boolean;
}>();

defineEmits<{ (e: "install", id: string): void }>();

const SIZE_CAP_BYTES = 2_000_000_000;

const grouped = computed(() => {
  const families: Record<string, ModelInfo[]> = { asr: [], diarizer: [], llm: [] };
  for (const m of props.models) families[m.family]?.push(m);
  return [
    { id: "asr", label: "ASR engines", icon: "graphic_eq", items: families.asr },
    { id: "diarizer", label: "Diarizers", icon: "groups", items: families.diarizer },
    { id: "llm", label: "Language models", icon: "model_training", items: families.llm },
  ].filter((g) => g.items.length);
});

function sizePct(m: ModelInfo): number {
  return Math.min(100, Math.round((m.size_bytes / SIZE_CAP_BYTES) * 100));
}

function perfPct(m: ModelInfo): number {
  const sizeFrac = Math.min(1, m.size_bytes / SIZE_CAP_BYTES);
  return Math.round((1 - sizeFrac * 0.85) * 100);
}

function accPct(m: ModelInfo): number {
  const buckets: Record<string, number> = {
    "whisper-onnx": 92,
    canary: 88,
    parakeet: 84,
    "nemo-ctc": 80,
    zipformer: 75,
  };
  if (m.family === "diarizer") return 78;
  if (m.family === "llm") return 70;
  return buckets[m.engine] ?? 70;
}
</script>

<template>
  <section
    v-for="g in grouped"
    :key="g.id"
    class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden"
  >
    <header
      class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
    >
      <Icon :name="g.icon" :size="22" class="text-tertiary" />
      <h2 class="text-titleMedium text-on-surface">{{ g.label }}</h2>
    </header>

    <ul v-if="showStats" class="flex flex-col md:hidden gap-xs p-md">
      <li
        v-for="m in g.items"
        :key="`m-${m.id}`"
        class="bg-surface-container-low rounded-lg p-md flex flex-col gap-md"
      >
        <div class="flex items-center justify-between gap-md">
          <div class="flex items-center gap-md min-w-0 flex-1">
            <Icon
              name="deployed_code"
              :size="24"
              :class="m.status === 'installed' ? 'text-primary' : 'text-on-surface-variant'"
            />
            <div class="flex flex-col min-w-0 flex-1">
              <span
                class="text-bodyMedium truncate"
                :class="m.status === 'installed' ? 'text-on-surface' : 'text-on-surface-variant'"
                :title="m.id"
                >{{ m.display_name || m.id }}</span
              >
              <span class="font-mono text-labelSmall text-secondary">
                {{ fmtModelSize(m.size_bytes) }}
                <template v-if="m.status === 'installed'"> · Installed</template>
                <template v-else-if="m.default_active"> · Default</template>
              </span>
            </div>
          </div>
          <button
            v-if="progress[m.id]"
            class="shrink-0 w-9 h-9 rounded-full bg-surface-container-high text-secondary flex items-center justify-center"
            disabled
            :title="`Downloading · ${progressPct(progress[m.id]).toFixed(0)}%`"
          >
            <Icon name="progress_activity" :size="20" class="animate-pulse" />
          </button>
          <button
            v-else-if="m.status === 'not_installed'"
            class="shrink-0 w-10 h-10 rounded-full bg-primary-container text-on-primary-container hover:bg-primary transition-colors flex items-center justify-center"
            title="Install"
            @click="$emit('install', m.id)"
          >
            <Icon name="download" :size="20" />
          </button>
        </div>
        <div v-if="progress[m.id]" class="flex flex-col gap-unit">
          <div class="h-1 bg-surface-variant rounded-full overflow-hidden">
            <div
              class="h-full bg-primary transition-all"
              :style="{ width: `${progressPct(progress[m.id])}%` }"
            ></div>
          </div>
          <span class="font-mono text-labelSmall text-primary">
            {{ progressPct(progress[m.id]).toFixed(0) }}% · file
            {{ progress[m.id].file_index + 1 }}/{{ progress[m.id].file_count }}
          </span>
        </div>
        <div v-else class="flex flex-col gap-unit">
          <div
            v-for="row in [
              { label: 'Perf', val: perfPct(m), color: 'bg-tertiary' },
              { label: 'Acc', val: accPct(m), color: 'bg-primary' },
              { label: 'Size', val: sizePct(m), color: 'bg-secondary' },
            ]"
            :key="row.label"
            class="flex items-center gap-xs"
          >
            <span class="font-mono text-[10px] text-on-surface-variant w-8">{{ row.label }}</span>
            <div class="h-1 flex-1 bg-surface-variant rounded-full overflow-hidden">
              <div class="h-full" :class="row.color" :style="{ width: `${row.val}%` }"></div>
            </div>
          </div>
        </div>
      </li>
    </ul>

    <table class="w-full text-left border-collapse" :class="showStats ? 'hidden md:table' : ''">
      <thead>
        <tr class="border-b border-outline-variant/40 bg-surface-container-highest/40">
          <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium">Name</th>
          <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-28">
            Size
          </th>
          <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-48">
            Status
          </th>
          <th
            class="px-margin py-md text-titleSmall text-on-surface-variant font-medium text-right w-32"
          >
            Actions
          </th>
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
            <div
              v-if="m.default_active"
              class="font-mono text-labelSmall text-secondary mt-unit uppercase tracking-wide"
            >
              default
            </div>
          </td>
          <td class="px-margin py-md text-on-surface-variant align-top whitespace-nowrap">
            {{ fmtModelSize(m.size_bytes) }}
          </td>
          <td class="px-margin py-md align-top">
            <div v-if="progress[m.id]" class="flex flex-col gap-unit w-40">
              <div class="h-1 bg-surface-container-highest rounded-full overflow-hidden">
                <div
                  class="h-full bg-primary transition-all"
                  :style="{ width: `${progressPct(progress[m.id])}%` }"
                ></div>
              </div>
              <span class="font-mono text-labelSmall text-primary">
                {{ progressPct(progress[m.id]).toFixed(0) }}% · file
                {{ progress[m.id].file_index + 1 }}/{{ progress[m.id].file_count }}
              </span>
            </div>
            <StatusPill v-else-if="m.status === 'installed'" tone="success">Installed</StatusPill>
            <StatusPill v-else-if="m.status === 'downloading'" tone="info" pulse>
              Downloading
            </StatusPill>
            <StatusPill v-else tone="muted">Not installed</StatusPill>
          </td>
          <td class="px-margin py-md text-right align-top">
            <button
              v-if="m.status === 'not_installed'"
              class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim transition-colors inline-flex items-center gap-unit"
              @click="$emit('install', m.id)"
            >
              <Icon name="download" :size="16" />
              Install
            </button>
            <Icon
              v-else-if="m.status === 'installed'"
              name="check"
              :size="20"
              class="text-outline cursor-not-allowed"
            />
          </td>
        </tr>
      </tbody>
    </table>
  </section>
</template>
