<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { Config, Suggestion, Transcript } from "./types";
import ModelManager from "./components/ModelManager.vue";
import History from "./components/History.vue";
import Settings from "./components/Settings.vue";

type Tab = "transcribe" | "models" | "history" | "settings";

const tab = ref<Tab>("transcribe");
const version = ref("");
const config = ref<Config | null>(null);
const transcript = ref<Transcript | null>(null);
const suggestion = ref<Suggestion | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const sourcePath = ref<string>("");

const tabs: { id: Tab; label: string }[] = [
  { id: "transcribe", label: "Transcribe" },
  { id: "history", label: "History" },
  { id: "models", label: "Models" },
  { id: "settings", label: "Settings" },
];

async function reloadConfig() {
  config.value = await api.loadConfig();
}

onMounted(async () => {
  version.value = await api.appVersion();
  await reloadConfig();
});

watch(tab, (t) => {
  if (t === "transcribe") void reloadConfig();
});

async function pickAndTranscribe() {
  if (!config.value) return;
  const selected = await open({
    multiple: false,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "ogg", "m4a", "flac"] }],
  });
  if (typeof selected !== "string") return;
  sourcePath.value = selected;
  status.value = "running";
  error.value = null;
  suggestion.value = null;
  try {
    transcript.value = await api.transcribeFile(selected, config.value);
    status.value = "idle";
    if (config.value.auto_rename) {
      status.value = "renaming";
      try {
        suggestion.value = await api.suggestFilename(transcript.value);
      } catch (e) {
        error.value = `auto-rename failed: ${String(e)}`;
      } finally {
        status.value = "idle";
      }
    }
  } catch (e) {
    error.value = String(e);
    status.value = "error";
  }
}

function fmt(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function fmtLong(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
}

function openHistoryItem(t: Transcript) {
  transcript.value = t;
  suggestion.value = null;
  tab.value = "transcribe";
}
</script>

<template>
  <div class="h-screen flex flex-col bg-background text-on-background overflow-hidden">
    <header class="flex justify-between items-center w-full px-margin h-16 shrink-0 border-b border-outline-variant/40 bg-surface">
      <div class="flex items-center gap-xs">
        <span class="material-symbols-outlined text-primary text-[24px]">graphic_eq</span>
        <span class="font-mono tracking-tighter font-bold text-primary text-labelMedium ml-xs uppercase">wt</span>
      </div>
      <nav class="flex items-center gap-xl h-full">
        <button
          v-for="t in tabs"
          :key="t.id"
          @click="tab = t.id"
          class="h-full flex items-center text-titleSmall border-b-2 px-unit transition-colors"
          :class="tab === t.id
            ? 'border-primary text-on-surface'
            : 'border-transparent text-on-surface-variant hover:text-on-surface'"
        >
          {{ t.label }}
        </button>
      </nav>
      <div class="flex items-center gap-xs text-on-surface-variant">
        <span class="font-mono text-labelSmall">v{{ version }}</span>
        <span class="material-symbols-outlined text-[20px]">more_vert</span>
      </div>
    </header>

    <main class="flex-1 flex overflow-hidden">
      <template v-if="tab === 'transcribe'">
        <section class="flex-1 flex flex-col p-margin overflow-hidden bg-surface">
          <div class="flex items-center justify-between mb-margin">
            <div class="flex items-center gap-xs text-on-surface-variant min-w-0">
              <span class="material-symbols-outlined text-[18px] shrink-0">folder_open</span>
              <span class="font-mono text-labelMedium truncate">{{ sourcePath || "no file selected" }}</span>
            </div>
            <div class="flex gap-xs shrink-0">
              <button
                :disabled="status === 'running' || status === 'renaming'"
                @click="pickAndTranscribe"
                class="bg-primary text-on-primary px-margin py-xs rounded-full font-medium text-titleSmall flex items-center gap-unit hover:bg-primary-fixed-dim transition-colors disabled:opacity-60 disabled:cursor-progress"
              >
                <span class="material-symbols-outlined text-[18px]">{{ status === 'running' ? 'hourglass_top' : status === 'renaming' ? 'auto_awesome' : 'upload_file' }}</span>
                <span>{{ status === "running" ? "Transcribing…" : status === "renaming" ? "Naming…" : "Pick audio" }}</span>
              </button>
            </div>
          </div>

          <div v-if="error" class="mb-margin p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium">
            {{ error }}
          </div>

          <div v-if="suggestion" class="mb-margin p-md rounded-lg bg-secondary-container/40 border border-secondary/30 flex items-center gap-xs text-bodyMedium">
            <span class="material-symbols-outlined text-secondary text-[18px]">auto_awesome</span>
            <span class="text-on-surface-variant">Suggested:</span>
            <code class="font-mono text-secondary bg-surface-container px-xs py-unit rounded">{{ suggestion.topic }}_{{ suggestion.stamp }}</code>
          </div>

          <div class="flex-1 overflow-y-auto pr-xs scroll-thin">
            <div v-if="!transcript" class="h-full flex flex-col items-center justify-center text-on-surface-variant gap-md">
              <span class="material-symbols-outlined text-[64px] text-outline-variant">graphic_eq</span>
              <p class="text-bodyMedium">Pick an audio file to begin transcription.</p>
            </div>
            <div v-else class="space-y-md">
              <article
                v-for="(u, i) in transcript.utterances"
                :key="i"
                class="flex gap-md items-start group hover:bg-surface-container-high/30 -mx-xs px-xs py-unit rounded transition-colors"
              >
                <span class="font-mono text-labelSmall text-secondary w-20 shrink-0 pt-unit">{{ fmt(u.start_ms) }}</span>
                <div class="flex-1 min-w-0">
                  <div v-if="u.speaker" class="font-mono text-labelSmall text-primary mb-unit">{{ u.speaker }}</div>
                  <p class="text-bodyMedium text-on-surface-variant group-hover:text-on-surface transition-colors leading-relaxed">{{ u.text }}</p>
                </div>
              </article>
            </div>
          </div>
        </section>

        <aside class="w-[340px] bg-surface-container border-l border-outline-variant/40 flex flex-col h-full shrink-0 overflow-y-auto scroll-thin">
          <div class="p-margin space-y-xl">
            <div>
              <h3 class="text-titleSmall text-on-surface mb-md">Configuration</h3>
              <div v-if="config" class="grid grid-cols-2 gap-md">
                <div class="bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div class="font-mono text-labelSmall text-on-surface-variant mb-unit uppercase tracking-wide">Engine</div>
                  <div class="text-bodyMedium text-on-surface truncate">{{ config.engine }}</div>
                </div>
                <div class="bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div class="font-mono text-labelSmall text-on-surface-variant mb-unit uppercase tracking-wide">Language</div>
                  <div class="text-bodyMedium text-on-surface truncate">{{ config.language }}</div>
                </div>
                <div class="bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div class="font-mono text-labelSmall text-on-surface-variant mb-unit uppercase tracking-wide">Device</div>
                  <div class="text-bodyMedium text-on-surface truncate">{{ config.device }}</div>
                </div>
                <div class="bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div class="font-mono text-labelSmall text-on-surface-variant mb-unit uppercase tracking-wide">Diarize</div>
                  <div class="text-bodyMedium text-on-surface truncate">
                    {{ config.diarize ? (config.speakers ? `${config.speakers} spk` : "auto") : "off" }}
                  </div>
                </div>
                <div class="col-span-2 bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div class="font-mono text-labelSmall text-on-surface-variant mb-unit uppercase tracking-wide">Model</div>
                  <div class="text-bodyMedium text-on-surface truncate">{{ config.model || "—" }}</div>
                </div>
              </div>
            </div>

            <div>
              <h3 class="text-titleSmall text-on-surface mb-md">Session</h3>
              <div class="bg-surface-container-high p-md rounded-lg space-y-xs font-mono text-labelMedium">
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Status</span>
                  <span :class="status === 'error' ? 'text-error' : status === 'idle' ? 'text-tertiary' : 'text-secondary'">
                    {{ status === "idle" && transcript ? "ready" : status }}
                  </span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Duration</span>
                  <span class="text-on-surface">{{ transcript ? fmtLong(transcript.duration_ms) : "—" }}</span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Utterances</span>
                  <span class="text-on-surface">{{ transcript?.utterances.length ?? 0 }}</span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Speakers</span>
                  <span class="text-primary">{{ transcript?.speakers_detected ?? 0 }}</span>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <ModelManager v-else-if="tab === 'models'" />
      <Settings v-else-if="tab === 'settings'" />
      <History v-else-if="tab === 'history'" @open="openHistoryItem" />
    </main>
  </div>
</template>
