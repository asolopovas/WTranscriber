<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api } from "@/api";
import type {
  Config,
  DirEntry,
  ModelInfo,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "@/types";
import { decodeName, phaseLabel } from "@utils/audio";
import { fmtMsLong as fmtLong } from "@composables/format";
import { fieldClass } from "@styles/fields";
import { useMediaQuery } from "@composables/useMediaQuery";
import Recorder from "@components/Recorder.vue";
import Toggle from "@components/ui/Toggle.vue";
import SaveIndicator from "@components/ui/SaveIndicator.vue";
import Spinner from "@components/icons/Spinner.vue";
import type { SaveState } from "@composables/useDebouncedSave";

const props = defineProps<{
  config: Config;
  sys: SystemInfo | null;
  models: ModelInfo[];
  workdir: string;
  selectedEntry: DirEntry | null;
  progress: TranscribeProgress | null;
  transcript: Transcript | null;
  status: "idle" | "running" | "renaming" | "error";
  saveState: SaveState;
}>();

const emit = defineEmits<{
  (e: "models-changed", models: ModelInfo[]): void;
  (e: "recording-saved", path: string): void;
}>();

const recorderRef = ref<InstanceType<typeof Recorder> | null>(null);
defineExpose({
  get recording() {
    return recorderRef.value?.recording ?? false;
  },
  get elapsed() {
    return recorderRef.value?.elapsed ?? "";
  },
});

const allLanguageOptions = [
  "auto",
  "en",
  "de",
  "fr",
  "es",
  "it",
  "pt",
  "nl",
  "pl",
  "ru",
  "uk",
  "zh",
  "ja",
  "ko",
  "ar",
  "tr",
  "hi",
];

const asrModels = computed(() =>
  props.models.filter((m) => m.family === "asr" && m.status === "installed"),
);
const allAsrModels = computed(() => props.models.filter((m) => m.family === "asr"));
const selectedAsrModel = computed(
  () => allAsrModels.value.find((m) => m.id === props.config.model) ?? null,
);
const selectedModelInstalled = computed(() => selectedAsrModel.value?.status === "installed");

const installingSelected = ref(false);
async function installSelectedModel() {
  const id = props.config.model;
  if (!id) return;
  installingSelected.value = true;
  try {
    await api.installModel(id);
    emit("models-changed", await api.listModels());
  } catch (e) {
    console.error("install failed", e);
  } finally {
    installingSelected.value = false;
  }
}

const speakerOptions = computed<{ value: number; label: string }[]>(() => {
  const cap = props.config.diarizer === "nemo" ? 4 : 10;
  const opts: { value: number; label: string }[] = [{ value: 0, label: "Auto" }];
  for (let i = 1; i <= cap; i++) opts.push({ value: i, label: String(i) });
  return opts;
});

const languageOptions = computed(() => {
  const m = selectedAsrModel.value;
  const base = !m || !m.languages || !m.languages.length ? allLanguageOptions : m.languages;
  return base.includes("auto") ? base : ["auto", ...base];
});

function syncEngineAndModel(next: Config) {
  const installed = asrModels.value;
  if (!installed.length) return;
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

function onModelChanged() {
  syncEngineAndModel(props.config);
  const opts = languageOptions.value;
  if (opts.length && !opts.includes(props.config.language)) {
    props.config.language = opts.includes("auto") ? "auto" : opts[0];
  }
}

const isDesktop = useMediaQuery("(min-width: 768px)");
const isMobile = computed(() => !isDesktop.value);

const CONFIG_HEIGHT_KEY = "wt.configHeightPx";
const CONFIG_HEADER_PX = 56;
const CONFIG_OPEN_THRESHOLD_PX = CONFIG_HEADER_PX + 16;

const configOpen = ref(isDesktop.value);
const configContentEl = ref<HTMLElement | null>(null);
const configContentHeightPx = ref(0);
const configHeightPx = ref(
  (() => {
    if (typeof window === "undefined") return CONFIG_HEADER_PX;
    const v = Number(localStorage.getItem(CONFIG_HEIGHT_KEY) ?? "");
    return Number.isFinite(v) && v >= CONFIG_HEADER_PX ? v : CONFIG_HEADER_PX;
  })(),
);
const configMaxPx = computed(() => CONFIG_HEADER_PX + configContentHeightPx.value);
const configExpandedMobile = computed(() => configHeightPx.value > CONFIG_OPEN_THRESHOLD_PX);
const resizingConfig = ref(false);

watch(configHeightPx, (v) => {
  if (typeof window !== "undefined") localStorage.setItem(CONFIG_HEIGHT_KEY, String(Math.round(v)));
});

watch(configContentEl, (el, _prev, onCleanup) => {
  if (!el || typeof window === "undefined") return;
  const measure = () => {
    const h = el.scrollHeight;
    if (h > 0) {
      configContentHeightPx.value = h;
      if (configHeightPx.value > configMaxPx.value) configHeightPx.value = configMaxPx.value;
    }
  };
  const ro = new ResizeObserver(measure);
  ro.observe(el);
  measure();
  onCleanup(() => ro.disconnect());
});

function snapConfig(px: number): number {
  const stops = [CONFIG_HEADER_PX, configMaxPx.value];
  return stops.reduce((best, s) => (Math.abs(s - px) < Math.abs(best - px) ? s : best));
}

function beginConfigResize(ev: PointerEvent) {
  ev.preventDefault();
  resizingConfig.value = true;
  const startY = ev.clientY;
  const startPx = configHeightPx.value;
  let dragged = false;
  const move = (e: PointerEvent) => {
    const deltaPx = startY - e.clientY;
    if (Math.abs(deltaPx) > 3) dragged = true;
    configHeightPx.value = Math.max(
      CONFIG_HEADER_PX,
      Math.min(configMaxPx.value, startPx + deltaPx),
    );
  };
  const up = () => {
    resizingConfig.value = false;
    if (dragged) {
      configHeightPx.value = snapConfig(configHeightPx.value);
    } else {
      configHeightPx.value =
        startPx > CONFIG_OPEN_THRESHOLD_PX ? CONFIG_HEADER_PX : configMaxPx.value;
    }
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

const headerAriaLabel = computed(() => {
  if (isMobile.value)
    return configExpandedMobile.value
      ? "Drag or tap to collapse configuration"
      : "Drag or tap to expand configuration";
  return configOpen.value ? "Collapse configuration" : "Expand configuration";
});
</script>

<template>
  <aside
    class="w-full md:w-[340px] bg-surface-container border-t md:border-t-0 md:border-l border-outline-variant/40 flex flex-col md:h-full shrink-0 overflow-hidden md:overflow-y-auto md:scroll-thin md:max-h-none touch-none md:touch-auto relative"
    :class="resizingConfig ? '' : 'transition-[max-height,height] duration-200 ease-out'"
    :style="{
      maxHeight: isMobile ? `min(${configHeightPx}px, calc(100% - 96px))` : undefined,
      height: isMobile ? `min(${configHeightPx}px, calc(100% - 96px))` : undefined,
    }"
  >
    <Recorder
      ref="recorderRef"
      :workdir="workdir"
      :headless="true"
      @saved="(p) => emit('recording-saved', p)"
    />
    <div
      class="shrink-0 h-14 w-full flex items-center justify-between relative select-none cursor-row-resize md:cursor-pointer touch-none md:touch-auto transition-colors"
      :class="resizingConfig ? 'bg-primary/15' : 'active:bg-primary/10'"
      role="button"
      :aria-expanded="isMobile ? configExpandedMobile : configOpen"
      :aria-label="headerAriaLabel"
      @pointerdown="(e: PointerEvent) => isMobile && beginConfigResize(e)"
      @click="
        () => {
          if (!isMobile) configOpen = !configOpen;
        }
      "
    >
      <span
        v-if="isMobile"
        class="absolute top-1 left-1/2 -translate-x-1/2 w-12 h-1 rounded-full transition-colors pointer-events-none"
        :class="resizingConfig ? 'bg-primary' : 'bg-outline-variant group-hover:bg-primary/60'"
      ></span>
      <h3 class="text-titleSmall text-on-surface flex items-center gap-unit pl-md">
        <span class="material-symbols-outlined text-[18px] md:hidden">tune</span>
        Configuration
      </h3>
      <div class="flex items-center gap-xs pr-md">
        <button
          v-if="recorderRef && !recorderRef.recording"
          @pointerdown.stop
          @click.stop.prevent="recorderRef?.start()"
          class="min-h-9 px-md inline-flex items-center gap-unit bg-error-container text-on-error-container rounded-full font-titleSmall hover:opacity-90 transition-opacity"
          title="Record"
        >
          <span class="material-symbols-outlined fill text-[16px]">fiber_manual_record</span>
          Rec
        </button>
        <button
          v-else-if="recorderRef"
          @pointerdown.stop
          @click.stop.prevent="recorderRef?.stop()"
          class="min-h-9 px-md inline-flex items-center gap-unit bg-primary text-on-primary rounded-full font-titleSmall font-bold hover:opacity-90 transition-opacity"
          :title="`Stop recording \u00b7 ${recorderRef?.elapsed}`"
        >
          <span class="material-symbols-outlined fill text-[16px]">stop</span>
          {{ recorderRef?.elapsed }}
        </button>
        <SaveIndicator v-else :state="saveState" />
      </div>
    </div>

    <div
      class="flex-1 min-h-0 overflow-hidden"
      :style="{
        maxHeight: isMobile
          ? `${Math.max(0, configHeightPx - CONFIG_HEADER_PX)}px`
          : configOpen
            ? 'none'
            : '0px',
      }"
    >
      <div
        ref="configContentEl"
        class="px-md md:px-margin pt-md pb-md md:py-margin space-y-md overflow-y-auto scroll-thin"
      >
        <label class="block">
          <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
            Model
          </span>
          <select v-model="config.model" :class="[fieldClass, 'mt-unit']" @change="onModelChanged">
            <option v-for="m in allAsrModels" :key="m.id" :value="m.id">
              {{ m.display_name }}{{ m.status === "installed" ? "" : " \u2014 not installed" }}
            </option>
          </select>
        </label>

        <div
          v-if="selectedAsrModel && !selectedModelInstalled"
          class="flex items-center gap-md p-md rounded-lg bg-error-container/40 border border-error/40"
        >
          <span class="material-symbols-outlined text-error">cloud_download</span>
          <div class="flex-1 min-w-0">
            <div class="text-bodyMedium text-on-surface">Model not installed</div>
            <div class="text-labelSmall text-on-surface-variant truncate">
              {{ selectedAsrModel.display_name }} ·
              {{ (selectedAsrModel.size_bytes / 1048576).toFixed(0) }} MB
            </div>
          </div>
          <button
            type="button"
            class="px-md h-9 rounded-md bg-primary text-on-primary text-labelLarge inline-flex items-center gap-xs disabled:opacity-50"
            :disabled="installingSelected || selectedAsrModel.status === 'downloading'"
            @click="installSelectedModel"
          >
            <Spinner
              v-if="installingSelected || selectedAsrModel.status === 'downloading'"
              :size="16"
            />
            <span v-else class="material-symbols-outlined text-[18px]">download</span>
            {{
              selectedAsrModel.status === "downloading"
                ? "Downloading…"
                : installingSelected
                  ? "Starting…"
                  : "Download"
            }}
          </button>
        </div>

        <div class="grid grid-cols-2 gap-md">
          <label class="block">
            <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
              Language
            </span>
            <select v-model="config.language" :class="[fieldClass, 'mt-unit']">
              <option v-for="l in languageOptions" :key="l" :value="l">
                {{ l === "auto" ? "Auto" : l }}
              </option>
            </select>
          </label>
          <label class="block">
            <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
              Device
            </span>
            <select v-model="config.device" :class="[fieldClass, 'mt-unit']">
              <option value="cpu">CPU</option>
              <option v-if="sys?.cuda_available" value="cuda">CUDA</option>
            </select>
          </label>
        </div>

        <div class="grid grid-cols-2 gap-md">
          <label class="block">
            <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
              Diarizer
            </span>
            <select
              v-model="config.diarizer"
              :disabled="!config.diarize"
              :class="[fieldClass, 'mt-unit', !config.diarize ? 'opacity-50' : '']"
            >
              <option value="auto">Auto</option>
              <option v-if="!sys?.is_mobile" value="nemo">NVIDIA NeMo Sortformer</option>
              <option value="eres2net">pyannote-3.0 + ERes2Net-base</option>
              <option value="titanet">pyannote-3.0 + TitaNet-Large</option>
            </select>
          </label>
          <label class="block">
            <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
              Speakers
            </span>
            <select
              :value="config.speakers ?? 0"
              :class="[fieldClass, 'mt-unit']"
              @change="
                (e) => {
                  const n = Number((e.target as HTMLSelectElement).value);
                  config.speakers = n > 0 ? n : null;
                  if (n > 0 && !config.diarize) config.diarize = true;
                }
              "
            >
              <option v-for="o in speakerOptions" :key="o.value" :value="o.value">
                {{ o.label }}
              </option>
            </select>
          </label>
        </div>

        <div class="flex items-center justify-between gap-xl py-xs">
          <div class="flex items-center justify-between gap-xs flex-1 min-w-0">
            <div class="text-bodyMedium text-on-surface truncate">Auto-Diarize</div>
            <Toggle v-model="config.diarize" aria-label="Auto-Diarize" />
          </div>
          <div class="flex items-center justify-between gap-xs flex-1 min-w-0">
            <div class="text-bodyMedium text-on-surface truncate">Auto-Rename</div>
            <Toggle v-model="config.auto_rename" aria-label="Auto-Rename" />
          </div>
        </div>
      </div>
    </div>

    <div class="hidden md:block px-margin pb-margin">
      <h3 class="text-titleSmall text-on-surface mb-md">Selection</h3>
      <div class="bg-surface-container-high p-md rounded-lg space-y-xs font-mono text-labelMedium">
        <div class="flex justify-between items-center">
          <span class="text-on-surface-variant">File</span>
          <span
            class="text-on-surface truncate ml-md max-w-[180px]"
            :title="selectedEntry ? decodeName(selectedEntry.name) : ''"
          >
            {{ selectedEntry ? decodeName(selectedEntry.name) : "—" }}
          </span>
        </div>
        <div class="flex justify-between items-center">
          <span class="text-on-surface-variant">Status</span>
          <span
            :class="
              status === 'error'
                ? 'text-error'
                : status === 'idle'
                  ? 'text-tertiary'
                  : 'text-secondary'
            "
          >
            <template v-if="selectedEntry && progress && status === 'running'">
              {{ phaseLabel(progress.phase) }}
              <span v-if="progress.phase === 'transcribing' || progress.phase === 'diarizing'">
                · {{ progress.displayPct.toFixed(1) }}%
              </span>
            </template>
            <template v-else>{{ status === "idle" && transcript ? "ready" : status }}</template>
          </span>
        </div>
        <div class="flex justify-between items-center">
          <span class="text-on-surface-variant">Duration</span>
          <span class="text-on-surface">
            {{ transcript ? fmtLong(transcript.duration_ms) : "—" }}
          </span>
        </div>
        <div v-if="transcript" class="flex justify-between items-center">
          <span class="text-on-surface-variant">Utterances · Speakers</span>
          <span class="text-on-surface">
            {{ transcript.utterances.length }} ·
            <span class="text-primary">{{ transcript.speakers_detected }}</span>
          </span>
        </div>
      </div>
    </div>
  </aside>
</template>
