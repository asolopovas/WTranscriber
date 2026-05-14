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
import {
  availableDiarizerOptions,
  speakerOptionsForDiarizer,
  syncAsrEngineAndModel,
} from "@utils/models";
import { fmtClock, fmtMsLong, MB } from "@utils/format";
import { fieldClass } from "@styles/fields";
import { useMediaQuery } from "@composables/useMediaQuery";
import { usePanelResize } from "@composables/usePanelResize";
import Toggle from "@components/ui/Toggle.vue";
import FormField from "@components/ui/FormField.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import Spinner from "@components/icons/Spinner.vue";

const props = defineProps<{
  config: Config;
  sys: SystemInfo | null;
  models: ModelInfo[];
  workdir: string;
  selectedEntry: DirEntry | null;
  progress: TranscribeProgress | null;
  transcript: Transcript | null;
  status: "idle" | "running" | "renaming" | "error";
  recording: boolean;
  recElapsed: string;
}>();

const emit = defineEmits<{
  (e: "models-changed", models: ModelInfo[]): void;
  (e: "pick-audio"): void;
  (e: "rec-start"): void;
  (e: "rec-stop"): void;
}>();

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

const diarizerOptions = computed(() => availableDiarizerOptions(!!props.sys?.is_mobile));
const speakerOptions = computed(() => speakerOptionsForDiarizer(props.config.diarizer));

const languageOptions = computed(() => {
  const m = selectedAsrModel.value;
  const base = !m || !m.languages || !m.languages.length ? allLanguageOptions : m.languages;
  return base.includes("auto") ? base : ["auto", ...base];
});

function onModelChanged() {
  syncAsrEngineAndModel(props.config, asrModels.value);
  const opts = languageOptions.value;
  if (opts.length && !opts.includes(props.config.language)) {
    props.config.language = opts.includes("auto") ? "auto" : opts[0];
  }
}

const isDesktop = useMediaQuery("(min-width: 768px)");
const isMobile = computed(() => !isDesktop.value);

const CONFIG_HEADER_PX = 56;

const configOpen = ref(isDesktop.value);
const configContentEl = ref<HTMLElement | null>(null);

const {
  heightPx: configHeightPx,
  expanded: configExpandedMobile,
  resizing: resizingConfig,
  observeContent,
  beginResize: beginConfigResize,
} = usePanelResize({
  storageKey: "wt.configHeightPx",
  headerHeight: CONFIG_HEADER_PX,
  minHeight: CONFIG_HEADER_PX,
});

watch(configContentEl, (el, _prev, onCleanup) => {
  onCleanup(observeContent(el));
});

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
    class="w-full md:w-85 bg-surface-container border-t md:border-t-0 md:border-l border-outline-variant/40 flex flex-col md:h-full shrink-0 overflow-hidden md:overflow-y-auto md:scroll-thin md:max-h-none touch-none md:touch-auto relative"
    :class="resizingConfig ? '' : 'transition-[max-height,height] duration-200 ease-out'"
    :style="{
      maxHeight: isMobile ? `min(${configHeightPx}px, calc(100% - 96px))` : undefined,
      height: isMobile ? `min(${configHeightPx}px, calc(100% - 96px))` : undefined,
    }"
  >
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
        <Icon name="tune" :size="18" class="md:hidden" />
        Configuration
      </h3>
      <div class="flex items-center gap-xs pr-md">
        <Button
          v-if="!recording"
          variant="error"
          size="lg"
          icon="fiber_manual_record"
          :icon-size="22"
          icon-fill
          title="Record"
          @pointerdown.stop
          @click.stop.prevent="emit('rec-start')"
        >
          Rec
        </Button>
        <Button
          v-else
          variant="primary"
          size="lg"
          bold
          icon="stop"
          :icon-size="22"
          icon-fill
          :title="`Stop recording \u00b7 ${recElapsed}`"
          @pointerdown.stop
          @click.stop.prevent="emit('rec-stop')"
        >
          {{ recElapsed }}
        </Button>
        <Button
          variant="ghost"
          shape="circle"
          size="lg"
          icon="add"
          :icon-size="22"
          class="border border-outline-variant/60 bg-transparent text-on-surface-variant/80 hover:border-outline hover:bg-surface-container-high/60 hover:text-on-surface"
          title="Add audio file(s) to working folder"
          aria-label="Add audio files"
          @pointerdown.stop
          @click.stop.prevent="emit('pick-audio')"
        />
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
        <FormField label="Model">
          <select v-model="config.model" :class="fieldClass" @change="onModelChanged">
            <option v-for="m in allAsrModels" :key="m.id" :value="m.id">
              {{ m.display_name }}{{ m.status === "installed" ? "" : " \u2014 not installed" }}
            </option>
          </select>
        </FormField>

        <div
          v-if="selectedAsrModel && !selectedModelInstalled"
          class="flex items-center gap-md p-md rounded-lg bg-error-container/40 border border-error/40"
        >
          <Icon name="cloud_download" class="text-error" />
          <div class="flex-1 min-w-0">
            <div class="text-bodyMedium text-on-surface">Model not installed</div>
            <div class="text-labelSmall text-on-surface-variant truncate">
              {{ selectedAsrModel.display_name }} ·
              {{ (selectedAsrModel.size_bytes / MB).toFixed(0) }} MB
            </div>
          </div>
          <Button
            variant="primary"
            shape="square"
            size="md"
            :disabled="installingSelected || selectedAsrModel.status === 'downloading'"
            @click="installSelectedModel"
          >
            <Spinner
              v-if="installingSelected || selectedAsrModel.status === 'downloading'"
              :size="16"
            />
            <Icon v-else name="download" :size="18" />
            {{
              selectedAsrModel.status === "downloading"
                ? "Downloading…"
                : installingSelected
                  ? "Starting…"
                  : "Download"
            }}
          </Button>
        </div>

        <div class="grid grid-cols-2 gap-md">
          <FormField label="Language">
            <select v-model="config.language" :class="fieldClass">
              <option v-for="l in languageOptions" :key="l" :value="l">
                {{ l === "auto" ? "Auto" : l }}
              </option>
            </select>
          </FormField>
          <FormField label="Device">
            <select v-model="config.device" :class="fieldClass">
              <option value="cpu">CPU</option>
              <option v-if="sys?.cuda_available" value="cuda">CUDA</option>
            </select>
          </FormField>
        </div>

        <div class="grid grid-cols-2 gap-md">
          <FormField label="Diarizer">
            <select
              v-model="config.diarizer"
              :disabled="!config.diarize"
              :class="[fieldClass, !config.diarize ? 'opacity-50' : '']"
            >
              <option v-for="option in diarizerOptions" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
          </FormField>
          <FormField label="Speakers">
            <select
              :value="config.speakers ?? 0"
              :class="fieldClass"
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
          </FormField>
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
            class="text-on-surface truncate ml-md max-w-45"
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
            <template
              v-if="
                selectedEntry &&
                progress &&
                progress.path === selectedEntry.path &&
                progress.phase !== 'done'
              "
            >
              {{ phaseLabel(progress.phase) }}
              <span v-if="progress.phase === 'transcribing' || progress.phase === 'diarizing'">
                · {{ progress.displayPct.toFixed(1) }}%
              </span>
            </template>
            <template v-else>{{ status === "idle" && transcript ? "ready" : status }}</template>
          </span>
        </div>
        <div
          v-if="
            selectedEntry &&
            progress &&
            progress.path === selectedEntry.path &&
            progress.phase !== 'done'
          "
          class="flex justify-between items-center"
        >
          <span class="text-on-surface-variant">Elapsed · Total</span>
          <span class="text-on-surface">
            {{ fmtClock(progress.elapsedSec) }} ·
            <span class="text-secondary">{{ fmtClock(progress.totalSec) }}</span>
          </span>
        </div>
        <div class="flex justify-between items-center">
          <span class="text-on-surface-variant">Duration</span>
          <span class="text-on-surface">
            {{ transcript ? fmtMsLong(transcript.duration_ms) : "—" }}
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
