<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api } from "../api";
import type { Config, ModelInfo } from "../types";

const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const status = ref<"idle" | "saving" | "saved" | "error">("idle");
const error = ref<string | null>(null);

const asrModels = computed(() =>
  models.value.filter((m) => m.family === "asr" && m.status === "installed"),
);

const engineOptions = [
  { value: "whisper-onnx", label: "Whisper (ONNX)" },
  { value: "zipformer", label: "Zipformer" },
  { value: "parakeet", label: "Parakeet (NeMo)" },
  { value: "canary", label: "Canary" },
  { value: "nemo-ctc", label: "NeMo CTC" },
] as const;

const languageOptions = ["auto", "en", "de", "fr", "es", "it", "pt", "ru", "uk", "zh", "ja", "ko"];

onMounted(async () => {
  config.value = await api.loadConfig();
  models.value = await api.listModels();
});

let saveTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  config,
  (next) => {
    if (!next) return;
    if (saveTimer) clearTimeout(saveTimer);
    status.value = "saving";
    saveTimer = setTimeout(async () => {
      try {
        await api.saveConfig(next);
        status.value = "saved";
        error.value = null;
      } catch (e) {
        status.value = "error";
        error.value = String(e);
      }
    }, 250);
  },
  { deep: true },
);
</script>

<template>
  <section v-if="config" class="settings">
    <header>
      <h2>Settings</h2>
      <span class="status" :data-state="status">{{
        status === "saving" ? "saving…" : status === "saved" ? "saved" : status === "error" ? "error" : ""
      }}</span>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <div class="row">
      <label>ASR engine</label>
      <select v-model="config.engine">
        <option v-for="o in engineOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
      </select>
    </div>

    <div class="row">
      <label>Model</label>
      <select v-model="config.model">
        <option v-for="m in asrModels" :key="m.id" :value="m.id">{{ m.display_name }}</option>
        <option v-if="!asrModels.length" :value="config.model" disabled>
          No installed ASR models — install one in the Models tab
        </option>
      </select>
    </div>

    <div class="row">
      <label>Language</label>
      <select v-model="config.language">
        <option v-for="l in languageOptions" :key="l" :value="l">{{ l }}</option>
      </select>
    </div>

    <div class="row">
      <label>Device</label>
      <select v-model="config.device">
        <option value="cpu">CPU</option>
        <option value="cuda">CUDA</option>
      </select>
    </div>

    <div class="row">
      <label>Threads</label>
      <input v-model.number="config.threads" type="number" min="1" max="32" />
    </div>

    <div class="row">
      <label>Diarize speakers</label>
      <input v-model="config.diarize" type="checkbox" />
    </div>

    <div class="row">
      <label>Speaker count (0 = auto)</label>
      <input
        :value="config.speakers ?? 0"
        type="number"
        min="0"
        max="20"
        @input="(e) => {
          const n = Number((e.target as HTMLInputElement).value);
          if (config) config.speakers = n > 0 ? n : null;
        }"
      />
    </div>

    <div class="row">
      <label>Auto-rename via LLM</label>
      <input v-model="config.auto_rename" type="checkbox" />
    </div>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 520px;
}
header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
h2 {
  margin: 0;
  font-size: 1.1rem;
}
.status {
  font-size: 0.8rem;
  color: #888;
  min-height: 1em;
}
.status[data-state="saved"] {
  color: #6cf;
}
.status[data-state="error"] {
  color: #ffb4b4;
}
.error {
  color: #ffb4b4;
  font-size: 0.85rem;
}
.row {
  display: grid;
  grid-template-columns: 220px 1fr;
  align-items: center;
  gap: 12px;
}
label {
  color: #aaa;
  font-size: 0.9rem;
}
select,
input[type="number"] {
  padding: 6px 10px;
  border-radius: 4px;
  border: 1px solid #333;
  background: #1a1a1a;
  color: #f0f0f0;
  font-size: 0.9rem;
}
input[type="checkbox"] {
  justify-self: start;
}
</style>
