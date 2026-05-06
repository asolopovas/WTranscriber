<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { Config, ModelInfo } from "../types";

const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const status = ref<"idle" | "saving" | "saved" | "error">("idle");
const error = ref<string | null>(null);
const maintenanceStatus = ref<string | null>(null);

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

const availableEngineOptions = computed(() => {
  const engines = new Set(asrModels.value.map((m) => m.engine));
  const options = engineOptions.filter((o) => engines.has(o.value));
  return options.length ? options : engineOptions;
});

const compatibleAsrModels = computed(() =>
  asrModels.value.filter((m) => m.engine === config.value?.engine),
);

const languageOptions = ["auto", "en", "de", "fr", "es", "it", "pt", "ru", "uk", "zh", "ja", "ko"];

onMounted(async () => {
  config.value = await api.loadConfig();
  models.value = await api.listModels();
  if (config.value) syncEngineAndModel(config.value);
});

function syncEngineAndModel(next: Config, preferEngine = false) {
  const installed = asrModels.value;
  if (!installed.length) return;

  if (preferEngine) {
    const engineModel = installed.find((m) => m.engine === next.engine);
    if (engineModel && !installed.some((m) => m.id === next.model && m.engine === next.engine)) {
      next.model = engineModel.id;
    }
    return;
  }

  const model = installed.find((m) => m.id === next.model);
  if (model) {
    if (next.engine !== model.engine) next.engine = model.engine as Config["engine"];
    return;
  }

  const engineModel = installed.find((m) => m.engine === next.engine);
  const fallback = engineModel ?? installed.find((m) => m.default_active) ?? installed[0];
  next.engine = fallback.engine as Config["engine"];
  next.model = fallback.id;
}

function onEngineChanged() {
  if (config.value) syncEngineAndModel(config.value, true);
}

function onModelChanged() {
  if (config.value) syncEngineAndModel(config.value);
}

async function resetTranscriptCache() {
  const ok = await confirm("Clear saved transcript previews and cached transcription results?");
  if (!ok) return;
  const removed = await api.resetTranscriptCache();
  maintenanceStatus.value = `Transcript cache reset (${removed} files removed).`;
}

async function resetAudioCache() {
  const ok = await confirm("Clear converted audio cache files?");
  if (!ok) return;
  const removed = await api.resetAudioCache();
  maintenanceStatus.value = `Audio cache reset (${removed} files removed).`;
}

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

const fieldClass =
  "w-full bg-surface-container-high border border-outline-variant/60 text-on-surface text-bodyMedium px-md py-xs rounded-lg appearance-none focus:outline-none focus:border-primary transition-colors";
</script>

<template>
  <main class="flex-1 overflow-y-auto p-xl bg-surface-container-lowest scroll-thin">
    <div class="max-w-5xl mx-auto flex flex-col gap-xl pb-xl">
      <div
        class="flex flex-col md:flex-row md:items-end justify-between gap-margin pb-md border-b border-outline-variant/50"
      >
        <div>
          <h1 class="text-[24px] leading-[32px] font-bold text-on-surface">Configuration</h1>
          <p class="text-bodyMedium text-on-surface-variant mt-unit">
            Manage transcription parameters, models, and runtime preferences.
          </p>
        </div>
        <div class="flex items-center gap-xs shrink-0 font-mono text-labelMedium">
          <span
            class="w-2 h-2 rounded-full"
            :class="
              status === 'saving'
                ? 'bg-secondary animate-pulse'
                : status === 'error'
                  ? 'bg-error'
                  : status === 'saved'
                    ? 'bg-tertiary'
                    : 'bg-outline-variant'
            "
          ></span>
          <span class="text-on-surface-variant uppercase tracking-wide">
            {{
              status === "saving"
                ? "saving…"
                : status === "saved"
                  ? "saved"
                  : status === "error"
                    ? "error"
                    : "synced"
            }}
          </span>
        </div>
      </div>

      <p v-if="error" class="text-error text-bodyMedium">{{ error }}</p>

      <div v-if="config" class="grid grid-cols-1 lg:grid-cols-2 gap-margin">
        <section
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden flex flex-col"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-primary">tune</span>
            <h2 class="text-titleMedium text-on-surface">Transcription</h2>
          </div>
          <div class="p-margin flex flex-col gap-margin">
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">ASR engine</span>
              <select v-model="config.engine" :class="fieldClass" @change="onEngineChanged">
                <option v-for="o in availableEngineOptions" :key="o.value" :value="o.value">
                  {{ o.label }}
                </option>
              </select>
            </label>
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">Model</span>
              <select v-model="config.model" :class="fieldClass" @change="onModelChanged">
                <option v-for="m in compatibleAsrModels" :key="m.id" :value="m.id">
                  {{ m.display_name }}
                </option>
                <option v-if="!asrModels.length" :value="config.model" disabled>
                  No installed ASR models — install one in the Models tab
                </option>
              </select>
            </label>
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">Language</span>
              <select v-model="config.language" :class="fieldClass">
                <option v-for="l in languageOptions" :key="l" :value="l">{{ l }}</option>
              </select>
            </label>
            <div class="flex justify-between items-start gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Diarize speakers</h3>
                <p class="text-bodyMedium text-on-surface-variant">Identify who spoke when.</p>
              </div>
              <button
                type="button"
                aria-label="Toggle diarize speakers"
                class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
                :class="
                  config.diarize
                    ? 'bg-primary'
                    : 'bg-surface-container-highest border border-outline-variant'
                "
                @click="config.diarize = !config.diarize"
              >
                <span
                  class="absolute top-1 w-4 h-4 rounded-full transition-all"
                  :class="config.diarize ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
                ></span>
              </button>
            </div>
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">Speaker count</span>
              <input
                :value="config.speakers ?? 0"
                type="number"
                min="0"
                max="20"
                :disabled="!config.diarize"
                :class="[fieldClass, !config.diarize ? 'opacity-50' : '']"
                @input="
                  (e) => {
                    const n = Number((e.target as HTMLInputElement).value);
                    if (config) config.speakers = n > 0 ? n : null;
                  }
                "
              />
              <span class="text-bodyMedium text-on-surface-variant">0 = auto-detect.</span>
            </label>
            <div class="flex justify-between items-start gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Auto-rename via LLM</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Suggest filenames after transcription.
                </p>
              </div>
              <button
                type="button"
                aria-label="Toggle auto rename"
                class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
                :class="
                  config.auto_rename
                    ? 'bg-primary'
                    : 'bg-surface-container-highest border border-outline-variant'
                "
                @click="config.auto_rename = !config.auto_rename"
              >
                <span
                  class="absolute top-1 w-4 h-4 rounded-full transition-all"
                  :class="config.auto_rename ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
                ></span>
              </button>
            </div>
          </div>
        </section>

        <section
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden flex flex-col"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-secondary">memory</span>
            <h2 class="text-titleMedium text-on-surface">Compute</h2>
          </div>
          <div class="p-margin flex flex-col gap-margin">
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">Device</span>
              <select v-model="config.device" :class="fieldClass">
                <option value="cpu">CPU</option>
                <option value="cuda">CUDA</option>
              </select>
            </label>
            <label class="flex flex-col gap-unit">
              <span class="text-titleSmall text-on-surface">Threads</span>
              <input
                v-model.number="config.threads"
                type="number"
                min="1"
                max="32"
                :class="fieldClass"
              />
              <span class="text-bodyMedium text-on-surface-variant"
                >CPU worker threads (1–32).</span
              >
            </label>
          </div>
        </section>

        <section
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden flex flex-col lg:col-span-2"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-tertiary">cleaning_services</span>
            <h2 class="text-titleMedium text-on-surface">Maintenance</h2>
          </div>
          <div class="p-margin grid grid-cols-1 md:grid-cols-2 gap-margin">
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Transcript cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears saved transcript previews and cached transcription results.
                </p>
              </div>
              <button
                type="button"
                class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors inline-flex items-center gap-unit w-fit"
                @click="resetTranscriptCache"
              >
                <span class="material-symbols-outlined text-[18px]">delete_sweep</span>
                Reset transcript cache
              </button>
            </div>
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Audio cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears converted WAV files created for non-WAV audio inputs.
                </p>
              </div>
              <button
                type="button"
                class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors inline-flex items-center gap-unit w-fit"
                @click="resetAudioCache"
              >
                <span class="material-symbols-outlined text-[18px]">delete_sweep</span>
                Reset audio cache
              </button>
            </div>
            <p v-if="maintenanceStatus" class="md:col-span-2 text-bodyMedium text-tertiary">
              {{ maintenanceStatus }}
            </p>
          </div>
        </section>
      </div>
    </div>
  </main>
</template>
