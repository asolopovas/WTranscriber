<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api, events } from "@/api";
import type { Config, Family, FileProgress, ModelInfo, SystemInfo } from "@/types";
import { fmtBytes } from "@utils/format";
import {
  DIARIZER_BY_MODEL_ID,
  applyAsrModel,
  applyMissingModelDefaults,
  modelIdForDiarizer,
} from "@utils/models";
import { useDebouncedSave } from "@composables/useDebouncedSave";
import { recordOmit, recordSet } from "@utils/records";
import { fieldClass } from "@styles/fields";
import ModelTable from "@components/ModelTable.vue";
import Card from "@components/ui/Card.vue";
import DefRow from "@components/ui/DefRow.vue";
import Button from "@components/ui/Button.vue";

const emit = defineEmits<{
  (e: "app-data-reset"): void;
}>();

const config = ref<Config | null>(null);
const sys = ref<SystemInfo | null>(null);
const models = ref<ModelInfo[]>([]);
const maintenanceStatus = ref<string | null>(null);
const modelProgress = ref<Record<string, FileProgress>>({});
const unlisten: (() => void)[] = [];

const { error } = useDebouncedSave(config, (next) => api.saveConfig(next));

const isAndroid = computed(() => sys.value?.os === "android");
const persistentEnabled = ref(false);
const persistentGranted = ref(false);
const persistentBusy = ref<"idle" | "requesting" | "enabling" | "saved">("idle");
const persistentMessage = ref<string | null>(null);
let persistentVisibilityHandler: (() => void) | null = null;

async function refreshPersistentState() {
  if (!isAndroid.value) return;
  try {
    persistentGranted.value = await api.hasPersistentStorage();
    if (config.value && persistentBusy.value === "idle") {
      persistentEnabled.value = config.value.use_persistent_models;
    }
  } catch (e) {
    persistentMessage.value = String(e);
  }
}

async function togglePersistent(next: boolean) {
  if (!isAndroid.value) return;
  if (next) {
    persistentGranted.value = await api.hasPersistentStorage();
    persistentBusy.value = persistentGranted.value ? "enabling" : "requesting";
    persistentMessage.value = persistentGranted.value
      ? "Backing up models to shared storage…"
      : "Opening system settings… toggle “Allow access to manage all files”, then return to WTranscriber.";
    if (persistentGranted.value) {
      await applyPersistentGrantIfReady();
    } else {
      await api.requestPersistentStorage();
    }
  } else {
    await api.disablePersistentStorage();
    persistentEnabled.value = false;
    persistentBusy.value = "idle";
    persistentMessage.value =
      "Persistent storage disabled. Existing files in /storage/emulated/0/WTranscriber/ are kept; the app will not restore from them on next launch.";
    if (config.value) config.value.use_persistent_models = false;
  }
}

async function applyPersistentGrantIfReady() {
  if (!isAndroid.value) return;
  await refreshPersistentState();
  if (persistentBusy.value !== "requesting" && persistentBusy.value !== "enabling") return;
  if (!persistentGranted.value) return;
  persistentBusy.value = "enabling";
  persistentMessage.value = "Permission granted. Backing up models to shared storage…";
  try {
    const ok = await api.enablePersistentStorage();
    if (ok) {
      persistentEnabled.value = true;
      persistentBusy.value = "saved";
      persistentMessage.value =
        "Done. Models are now mirrored to /storage/emulated/0/WTranscriber/models and will survive uninstall.";
      if (config.value) config.value.use_persistent_models = true;
      setTimeout(() => {
        if (persistentBusy.value === "saved") persistentBusy.value = "idle";
      }, 1500);
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

const diarizerSelectedId = computed<string | null>(() =>
  config.value ? modelIdForDiarizer(config.value.diarizer) : null,
);

const selectedByFamily = computed<Partial<Record<Family, string | null>>>(() => ({
  asr: config.value?.model ?? null,
  diarizer: diarizerSelectedId.value,
  llm: config.value?.llm_model ?? null,
}));

function onSelectDefault(payload: { family: Family; id: string }) {
  if (!config.value) return;
  const m: ModelInfo | undefined = models.value.find((x) => x.id === payload.id);
  if (payload.family === "asr") {
    if (m) applyAsrModel(config.value, m);
  } else if (payload.family === "diarizer") {
    const choice = DIARIZER_BY_MODEL_ID[payload.id];
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
  if (config.value) applyMissingModelDefaults(config.value, models.value);
  unlisten.push(
    await events.onModelProgress((p) => recordSet(modelProgress, p.id, p)),
    await events.onModelDone(refreshModels),
    await events.onModelError(refreshModels),
  );
  await refreshPersistentState();
  if (isAndroid.value && typeof document !== "undefined") {
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

async function resetAppData() {
  const ok = await confirm(
    "Clear logs, cached transcripts, converted audio and file history? This keeps installed models and settings.",
  );
  if (!ok) return;
  const removed = await api.resetAppData();
  maintenanceStatus.value = `App data cleared (${removed.cache_entries_removed} cache entries, ${removed.workdir_entries_removed} file entries removed).`;
  emit("app-data-reset");
}
</script>

<template>
  <main class="flex-1 overflow-y-auto px-xs md:px-md py-md bg-surface-container-lowest scroll-thin">
    <div class="max-w-5xl mx-auto flex flex-col gap-xl pb-xl">
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
          </dl>
          <div class="px-margin pb-margin">
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

        <Card icon="cleaning_services" icon-color="text-tertiary" title="Storage">
          <div class="p-margin flex flex-col gap-md">
            <div class="grid grid-cols-1 gap-md">
              <Button class="w-full justify-center" @click="resetAppData">Clear cache</Button>
              <Button
                v-if="isAndroid"
                class="w-full justify-center"
                :variant="persistentEnabled ? 'primary' : 'neutral'"
                @click="togglePersistent(!persistentEnabled)"
              >
                {{ persistentEnabled ? "Keep models: On" : "Keep models: Off" }}
              </Button>
            </div>
            <p
              v-if="maintenanceStatus || persistentMessage"
              class="text-bodyMedium text-on-surface-variant"
            >
              {{ maintenanceStatus || persistentMessage }}
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
