<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { Config, Utterance } from "./types";

const version = ref("");
const config = ref<Config | null>(null);
const utterances = ref<Utterance[]>([]);
const status = ref<"idle" | "running" | "error">("idle");
const error = ref<string | null>(null);

onMounted(async () => {
  version.value = await api.appVersion();
  config.value = await api.loadConfig();
});

async function pickAndTranscribe() {
  if (!config.value) return;
  const selected = await open({
    multiple: false,
    filters: [{ name: "Audio", extensions: ["wav", "mp3", "ogg", "m4a", "flac"] }],
  });
  if (typeof selected !== "string") return;
  status.value = "running";
  error.value = null;
  try {
    utterances.value = await api.transcribeFile(selected, config.value);
    status.value = "idle";
  } catch (e) {
    error.value = String(e);
    status.value = "error";
  }
}

function fmt(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}
</script>

<template>
  <main class="app">
    <header>
      <h1>WTranscriber</h1>
      <span class="version">v{{ version }}</span>
    </header>

    <section class="controls">
      <button :disabled="status === 'running'" @click="pickAndTranscribe">
        {{ status === "running" ? "Transcribing…" : "Pick audio file" }}
      </button>
      <span v-if="config" class="meta">
        {{ config.model }} · {{ config.language }} · {{ config.device }}
      </span>
    </section>

    <section v-if="error" class="error">{{ error }}</section>

    <section class="transcript">
      <p v-if="!utterances.length" class="empty">No transcript yet.</p>
      <article v-for="(u, i) in utterances" :key="i" class="utterance">
        <span class="ts">{{ fmt(u.start_ms) }} → {{ fmt(u.end_ms) }}</span>
        <span v-if="u.speaker" class="speaker">{{ u.speaker }}</span>
        <p>{{ u.text }}</p>
      </article>
    </section>
  </main>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 24px;
  max-width: 960px;
  margin: 0 auto;
  font-family: Inter, system-ui, sans-serif;
}
header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
h1 {
  margin: 0;
  font-size: 1.5rem;
}
.version {
  color: #888;
  font-size: 0.85rem;
}
.controls {
  display: flex;
  gap: 12px;
  align-items: center;
}
.meta {
  color: #888;
  font-size: 0.85rem;
}
button {
  padding: 8px 16px;
  border-radius: 6px;
  border: 1px solid #444;
  background: #1f1f1f;
  color: #f0f0f0;
  cursor: pointer;
}
button:disabled {
  opacity: 0.6;
  cursor: progress;
}
.error {
  padding: 12px;
  border-radius: 6px;
  background: #3a1f1f;
  color: #ffb4b4;
}
.transcript {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.empty {
  color: #666;
}
.utterance {
  padding: 12px;
  border-radius: 6px;
  background: #1a1a1a;
}
.ts {
  color: #888;
  font-family: ui-monospace, monospace;
  font-size: 0.8rem;
  margin-right: 8px;
}
.speaker {
  color: #6cf;
  font-size: 0.8rem;
  font-weight: 600;
}
.utterance p {
  margin: 4px 0 0;
}
</style>

<style>
:root {
  color-scheme: dark;
  background: #0e0e0e;
  color: #f0f0f0;
}
body {
  margin: 0;
}
</style>
