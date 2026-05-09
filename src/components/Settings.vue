<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api, events } from "@/api";
import type {
  Config,
  DiarizerChoice,
  Engine,
  Family,
  FileProgress,
  ModelInfo,
  SystemInfo,
} from "@/types";
import { fmtBytes } from "@composables/format";
import { useDebouncedSave } from "@composables/useDebouncedSave";
import { recordOmit, recordSet } from "@composables/records";
import { fieldClass } from "@styles/fields";
import ModelTable from "@components/ModelTable.vue";
import Card from "@components/ui/Card.vue";
import DefRow from "@components/ui/DefRow.vue";
import Button from "@components/ui/Button.vue";

const config = ref<Config | null>(null);
const sys = ref<SystemInfo | null>(null);
const models = ref<ModelInfo[]>([]);
const maintenanceStatus = ref<string | null>(null);
const modelProgress = ref<Record<string, FileProgress>>({});
const unlisten: (() => void)[] = [];

const { error } = useDebouncedSave(config, (next) => api.saveConfig(next));

const isAndroid = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
const persistentEnabled = ref(false);
const persistentGranted = ref(false);
const persistentBusy = ref<"idle" | "requesting" | "enabling" | "saved">("idle");
const persistentMessage = ref<string | null>(null);
let persistentVisibilityHandler: (() => void) | null = null;

async function refreshPersistentState() {
  if (!isAndroid) return;
  try {
    persistentGranted.value = await api.hasPersistentStorage();
    if (config.value) persistentEnabled.value = config.value.use_persistent_models;
  } catch (e) {
    persistentMessage.value = String(e);
  }
}

async function togglePersistent(next: boolean) {
  if (!isAndroid) return;
  if (next) {
    persistentBusy.value = "requesting";
    persistentMessage.value =
      "Opening system settings\u2026 toggle \u201cAllow access to manage all files\u201d, then return to WTranscriber.";
    await api.requestPersistentStorage();
  } else {
    await api.disablePersistentStorage();
    persistentEnabled.value = false;
    persistentMessage.value =
      "Persistent storage disabled. Existing files in /storage/emulated/0/WTranscriber/ are kept; the app will not restore from them on next launch.";
    if (config.value) config.value.use_persistent_models = false;
  }
}

async function applyPersistentGrantIfReady() {
  if (!isAndroid) return;
  await refreshPersistentState();
  if (persistentBusy.value !== "requesting") return;
  if (!persistentGranted.value) return;
  persistentBusy.value = "enabling";
  persistentMessage.value = "Permission granted. Backing up models to shared storage\u2026";
  try {
    const ok = await api.enablePersistentStorage();
    if (ok) {
      persistentEnabled.value = true;
      persistentBusy.value = "saved";
      persistentMessage.value =
        "Done. Models are now mirrored to /storage/emulated/0/WTranscriber/models and will survive uninstall.";
      if (config.value) config.value.use_persistent_models = true;
    } else {
      persistentBusy.value = "idle";
      persistentMessage.value = "Permission not yet granted. Try again from system settings.";
    }
  } catch (e) {
    persistentBusy.value = "idle";
    persistentMessage.value = String(e);
  }
}

async function refreshModels() {
  models.value = await api.listModels();
}

const DIARIZER_BY_ID: Record<string, DiarizerChoice> = {
  "nemo-sortformer-v2": "nemo",
  "diar-eres2net-base": "eres2net",
  "sherpa-pyannote-titanet": "titanet",
};

const diarizerSelectedId = computed<string | null>(() => {
  if (!config.value) return null;
  const choice = config.value.diarizer;
  for (const [id, c] of Object.entries(DIARIZER_BY_ID)) if (c === choice) return id;
  return null;
});

const selectedByFamily = computed<Partial<Record<Family, string | null>>>(() => ({
  asr: config.value?.model ?? null,
  diarizer: diarizerSelectedId.value,
  llm: config.value?.llm_model ?? null,
}));

function onSelectDefault(payload: { family: Family; id: string }) {
  if (!config.value) return;
  const m: ModelInfo | undefined = models.value.find((x) => x.id === payload.id);
  if (payload.family === "asr") {
    config.value.model = payload.id;
    if (m && m.engine) config.value.engine = m.engine as Engine;
  } else if (payload.family === "diarizer") {
    const choice = DIARIZER_BY_ID[payload.id];
    if (choice) config.value.diarizer = choice;
  } else if (payload.family === "llm") {
    config.value.llm_model = payload.id;
  }
}

async function installModel(id: string) {
  try {
    await api.installModel(id);
  } finally {
    recordOmit(modelProgress, id);
    await refreshModels();
  }
}

onMounted(async () => {
  config.value = await api.loadConfig();
  sys.value = await api.systemInfo();
  if (config.value && sys.value && config.value.threads > sys.value.cpu_threads) {
    config.value.threads = sys.value.cpu_threads;
  }
  await refreshModels();
  unlisten.push(
    await events.onModelProgress((p) => recordSet(modelProgress, p.id, p)),
    await events.onModelDone(refreshModels),
    await events.onModelError(refreshModels),
  );
  await refreshPersistentState();
  if (isAndroid && typeof document !== "undefined") {
    persistentVisibilityHandler = () => {
      if (document.visibilityState === "visible") void applyPersistentGrantIfReady();
    };
    document.addEventListener("visibilitychange", persistentVisibilityHandler);
  }
});

onUnmounted(() => {
  unlisten.forEach((u) => u());
  if (persistentVisibilityHandler && typeof document !== "undefined") {
    document.removeEventListener("visibilitychange", persistentVisibilityHandler);
  }
  persistentVisibilityHandler = null;
});

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
</script>

<template>
  <main class="flex-1 overflow-y-auto p-margin md:p-xl bg-surface-container-lowest scroll-thin">
    <div class="max-w-5xl mx-auto flex flex-col gap-xl pb-xl">
      <div
        class="flex flex-col md:flex-row md:items-end justify-between gap-margin pb-md border-b border-outline-variant/50"
      >
        <div>
          <h1 class="text-[20px] md:text-[24px] leading-7 md:leading-8 font-bold text-on-surface">
            Settings
          </h1>
          <p class="text-bodyMedium text-on-surface-variant mt-unit">
            Runtime, local models, and storage maintenance.
          </p>
        </div>
      </div>

      <p v-if="error" class="text-error text-bodyMedium">{{ error }}</p>

      <div v-if="config" class="flex flex-col gap-margin">
        <Card v-if="sys" icon="phone_android" title="Device">
          <dl
            class="p-margin grid grid-cols-1 md:grid-cols-2 gap-x-margin gap-y-md text-bodyMedium"
          >
            <DefRow label="OS">{{ sys.os }}</DefRow>
            <DefRow label="Architecture">{{ sys.arch }}</DefRow>
            <DefRow label="CPU threads">{{ sys.cpu_threads }}</DefRow>
            <DefRow label="Memory">{{ fmtBytes(sys.total_memory_bytes) }}</DefRow>
            <DefRow label="Form factor">{{ sys.is_mobile ? "mobile" : "desktop" }}</DefRow>
            <DefRow label="Acceleration">
              <span v-if="sys.cuda_available">CUDA</span>
              <span v-else-if="sys.nnapi_available">NNAPI (experimental)</span>
              <span v-else>CPU only</span>
            </DefRow>
            <div
              v-if="sys.workdir"
              class="col-span-1 md:col-span-2 flex flex-col gap-xs border-t border-outline-variant/30 pt-md"
            >
              <DefRow label="Workdir" align="right" break-all>{{ sys.workdir }}</DefRow>
              <DefRow v-if="sys.models_dir" label="Models" align="right" break-all>
                {{ sys.models_dir }}
              </DefRow>
              <DefRow v-if="sys.cache_dir" label="Cache" align="right" break-all>
                {{ sys.cache_dir }}
              </DefRow>
              <DefRow v-if="sys.config_dir" label="Config" align="right" break-all>
                {{ sys.config_dir }}
              </DefRow>
            </div>
          </dl>
        </Card>

        <Card icon="memory" icon-color="text-secondary" title="Runtime">
          <div class="p-margin flex flex-col gap-margin">
            <label class="flex flex-col gap-unit max-w-sm">
              <span class="text-titleSmall text-on-surface">Threads</span>
              <input
                v-model.number="config.threads"
                type="number"
                min="1"
                :max="sys?.cpu_threads ?? 32"
                :class="fieldClass"
              />
              <span class="text-bodyMedium text-on-surface-variant">
                CPU worker threads (1–{{ sys?.cpu_threads ?? 32 }}). 0 = auto.
              </span>
            </label>
          </div>
        </Card>

        <ModelTable
          :models="models"
          :progress="modelProgress"
          :selected="selectedByFamily"
          show-stats
          @install="installModel"
          @select="onSelectDefault"
        />

        <Card v-if="isAndroid" icon="save" icon-color="text-tertiary" title="Storage">
          <div class="p-margin flex flex-col gap-md">
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Keep AI models when uninstalling</h3>
                <p class="text-bodyMedium text-on-surface-variant mt-unit">
                  Mirrors downloaded models to
                  <span class="font-mono">/storage/emulated/0/WTranscriber/models</span> so a future
                  reinstall doesn’t need to re-download ~1&nbsp;GB. Requires
                  <span class="font-medium">All Files Access</span> in Android Settings.
                </p>
              </div>
              <Button
                :variant="persistentEnabled ? 'primary' : 'neutral'"
                :icon="persistentEnabled ? 'check_circle' : 'shield'"
                :icon-size="18"
                class="w-fit"
                @click="togglePersistent(!persistentEnabled)"
              >
                {{ persistentEnabled ? "Enabled" : "Grant access & enable" }}
              </Button>
            </div>
            <p
              v-if="persistentMessage"
              class="text-bodyMedium"
              :class="persistentBusy === 'saved' ? 'text-tertiary' : 'text-on-surface-variant'"
            >
              {{ persistentMessage }}
            </p>
            <p class="text-labelSmall text-on-surface-variant font-mono">
              Status:
              {{
                persistentEnabled && persistentGranted
                  ? "enabled"
                  : persistentGranted
                    ? "permission granted, not enabled"
                    : "permission not granted"
              }}
            </p>
          </div>
        </Card>

        <Card icon="cleaning_services" icon-color="text-tertiary" title="Maintenance">
          <div class="p-margin grid grid-cols-1 md:grid-cols-2 gap-margin">
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Transcript cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears saved transcript previews and cached transcription results.
                </p>
              </div>
              <Button
                icon="delete_sweep"
                :icon-size="18"
                class="w-fit"
                @click="resetTranscriptCache"
              >
                Reset transcript cache
              </Button>
            </div>
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Audio cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears converted WAV files created for non-WAV audio inputs.
                </p>
              </div>
              <Button icon="delete_sweep" :icon-size="18" class="w-fit" @click="resetAudioCache">
                Reset audio cache
              </Button>
            </div>
            <p v-if="maintenanceStatus" class="md:col-span-2 text-bodyMedium text-tertiary">
              {{ maintenanceStatus }}
            </p>
          </div>
        </Card>
      </div>

      <p v-if="sys" class="font-mono text-labelSmall text-on-surface-variant text-center pt-xl">
        WTranscriber v{{ sys.app_version }}
      </p>
    </div>
  </main>
</template>
