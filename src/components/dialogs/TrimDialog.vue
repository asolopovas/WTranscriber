<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { api } from "../../api";
import type { AudioMeta, DirEntry } from "../../types";
import { fmtMs } from "../../composables/format";
import Modal from "../ui/Modal.vue";
import ErrorBanner from "../ui/ErrorBanner.vue";
import Icon from "../ui/Icon.vue";
import Spinner from "../icons/Spinner.vue";
import CancelIcon from "../icons/CancelIcon.vue";
import SaveIcon from "../icons/SaveIcon.vue";

const props = defineProps<{ target: DirEntry | null }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "saved"): void;
  (e: "error", msg: string): void;
}>();

const open = computed(() => !!props.target);

const duration = ref(0);
const start = ref(0);
const end = ref(0);
const peaks = ref<number[]>([]);
const canvas = ref<HTMLCanvasElement | null>(null);
const waveformBox = ref<HTMLElement | null>(null);
const localError = ref<string | null>(null);
const loading = ref(false);

const startFrac = computed(() => (duration.value > 0 ? start.value / duration.value : 0));
const endFrac = computed(() => (duration.value > 0 ? end.value / duration.value : 1));

const playing = ref(false);
const playheadMs = ref(0);
const playheadFrac = computed(() =>
  duration.value > 0 ? Math.max(0, Math.min(1, playheadMs.value / duration.value)) : 0,
);
const audioLoading = ref(false);
let audioCtx: AudioContext | null = null;
let audioBuffer: AudioBuffer | null = null;
let audioSource: AudioBufferSourceNode | null = null;
let playStartCtx = 0;
let playOffsetMs = 0;
let audioPath: string | null = null;
let stopTimer: ReturnType<typeof setTimeout> | null = null;
let playheadRaf: number | null = null;

const MIN_GAP_MS = 3_000;

watch(
  () => props.target,
  async (next) => {
    if (!next) {
      cleanup();
      return;
    }
    await load(next);
  },
  { immediate: true },
);

watch([start, end, peaks], () => {
  if (open.value) void renderWaveform();
});

async function load(target: DirEntry) {
  loading.value = true;
  localError.value = null;
  try {
    const [durMs, meta, peaksArr] = await Promise.all([
      api.probeAudio(target.path).catch(() => null),
      api.loadAudioMeta(target.path),
      api.audioWaveform(target.path, 320),
    ]);
    const dur = Math.max(0, Math.floor((durMs ?? target.duration_ms ?? 0) as number));
    let s = Math.min(meta.trim_start_ms ?? 0, dur);
    let e = Math.min(meta.trim_end_ms ?? dur, dur);
    if (e <= s) e = dur;
    duration.value = dur;
    start.value = s;
    end.value = e;
    peaks.value = peaksArr;
    playOffsetMs = s;
    audioBuffer = null;
    audioPath = null;
    await renderWaveform();
  } catch (e) {
    emit("error", `prepare: ${String(e)}`);
    emit("close");
  } finally {
    loading.value = false;
  }
}

async function renderWaveform() {
  await nextTick();
  const c = canvas.value;
  if (!c) return;
  const ctx = c.getContext("2d");
  if (!ctx) return;
  ctx.clearRect(0, 0, c.width, c.height);
  if (!peaks.value.length) {
    ctx.strokeStyle = "#6750a4";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(0, c.height / 2);
    ctx.lineTo(c.width, c.height / 2);
    ctx.stroke();
    return;
  }
  const step = c.width / peaks.value.length;
  peaks.value.forEach((peak, i) => {
    const amp = Math.max(0.02, Math.min(1, peak));
    const barHeight = amp * c.height;
    const x = i * step;
    const y = (c.height - barHeight) / 2;
    const frac = (i + 0.5) / peaks.value.length;
    const inside = frac >= startFrac.value && frac <= endFrac.value;
    ctx.fillStyle = inside ? "#6750a4" : "rgba(103, 80, 164, 0.22)";
    ctx.fillRect(x, y, Math.max(1, step - 1), barHeight);
  });
}

function beginHandleDrag(side: "start" | "end", ev: PointerEvent) {
  ev.preventDefault();
  ev.stopPropagation();
  const box = waveformBox.value;
  if (!box || duration.value === 0) return;
  const dur = duration.value;
  const gap = Math.min(MIN_GAP_MS, Math.max(100, Math.floor(dur / 4)));
  const move = (e: PointerEvent) => {
    const rect = box.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const ms = Math.round((x / rect.width) * dur);
    if (side === "start") start.value = Math.max(0, Math.min(ms, end.value - gap));
    else end.value = Math.min(dur, Math.max(ms, start.value + gap));
  };
  const up = (e: PointerEvent) => {
    move(e);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

async function ensureBuffer(): Promise<AudioBuffer | null> {
  const target = props.target;
  if (!target) return null;
  if (audioBuffer && audioPath === target.path) return audioBuffer;
  audioLoading.value = true;
  try {
    const bytes = await api.readAudioBytes(target.path);
    if (!audioCtx) audioCtx = new AudioContext();
    audioBuffer = await audioCtx.decodeAudioData(new Uint8Array(bytes).buffer);
    audioPath = target.path;
    return audioBuffer;
  } finally {
    audioLoading.value = false;
  }
}

function stopPlayheadLoop() {
  if (playheadRaf !== null) cancelAnimationFrame(playheadRaf);
  playheadRaf = null;
}

function startPlayheadLoop() {
  stopPlayheadLoop();
  const tick = () => {
    if (!playing.value || !audioCtx) {
      playheadRaf = null;
      return;
    }
    const ms = playOffsetMs + (audioCtx.currentTime - playStartCtx) * 1000;
    playheadMs.value = Math.min(end.value, Math.max(start.value, ms));
    playheadRaf = requestAnimationFrame(tick);
  };
  playheadRaf = requestAnimationFrame(tick);
}

function clearSource() {
  if (audioSource) {
    try {
      audioSource.onended = null;
      audioSource.stop();
    } catch {
      /* */
    }
    try {
      audioSource.disconnect();
    } catch {
      /* */
    }
    audioSource = null;
  }
  if (stopTimer) clearTimeout(stopTimer);
  stopTimer = null;
}

function pause() {
  if (playing.value && audioCtx) {
    const elapsed = (audioCtx.currentTime - playStartCtx) * 1000;
    playOffsetMs = Math.min(end.value, playOffsetMs + elapsed);
    playheadMs.value = playOffsetMs;
  }
  clearSource();
  playing.value = false;
  stopPlayheadLoop();
}

function stop() {
  clearSource();
  playOffsetMs = start.value;
  playheadMs.value = start.value;
  playing.value = false;
  stopPlayheadLoop();
}

async function togglePlay() {
  if (playing.value) {
    pause();
    return;
  }
  const buffer = await ensureBuffer().catch((e) => {
    localError.value = `load: ${String(e)}`;
    return null;
  });
  if (!buffer || !audioCtx) return;
  if (audioCtx.state === "suspended") await audioCtx.resume();
  let offsetMs = playOffsetMs;
  if (offsetMs < start.value || offsetMs >= end.value) offsetMs = start.value;
  const durMs = Math.max(0, end.value - offsetMs);
  const src = audioCtx.createBufferSource();
  src.buffer = buffer;
  src.connect(audioCtx.destination);
  src.onended = () => {
    if (audioSource === src) {
      audioSource = null;
      playOffsetMs = start.value;
      playheadMs.value = start.value;
      playing.value = false;
      stopPlayheadLoop();
    }
  };
  audioSource = src;
  playStartCtx = audioCtx.currentTime;
  playOffsetMs = offsetMs;
  src.start(0, offsetMs / 1000, durMs / 1000);
  playheadMs.value = offsetMs;
  playing.value = true;
  startPlayheadLoop();
  stopTimer = setTimeout(() => {
    if (audioSource === src) clearSource();
    playOffsetMs = start.value;
    playheadMs.value = start.value;
    playing.value = false;
    stopPlayheadLoop();
  }, durMs + 200);
}

function reset() {
  start.value = 0;
  end.value = duration.value;
}

async function commit() {
  if (!props.target) return;
  const target = props.target;
  const s = Math.max(0, Math.floor(start.value));
  const e = Math.min(duration.value, Math.floor(end.value));
  const meta: AudioMeta = {
    trim_start_ms: s,
    trim_end_ms: e < duration.value ? e : null,
  };
  try {
    await api.saveAudioMeta(target.path, meta);
    cleanup();
    emit("saved");
  } catch (e) {
    localError.value = String(e);
  }
}

function cleanup() {
  stop();
  audioBuffer = null;
  audioPath = null;
  playOffsetMs = 0;
}

function close() {
  cleanup();
  emit("close");
}
</script>

<template>
  <Modal :open="open" :width="'768px'" @close="close">
    <template #header>
      <Icon name="content_cut" :size="22" class="text-primary mt-unit" />
      <div class="flex-1 min-w-0">
        <h3 class="text-titleSmall text-on-surface">Select range to transcribe</h3>
        <p class="font-mono text-labelSmall text-on-surface-variant truncate" :title="target?.name">
          {{ target?.name ?? "—" }}
        </p>
      </div>
      <button
        class="p-xs rounded hover:bg-surface-container-high text-on-surface-variant hover:text-on-surface transition-colors"
        title="Close"
        @click="close"
      >
        <Icon name="close" :size="20" />
      </button>
    </template>

    <ErrorBanner v-if="localError">{{ localError }}</ErrorBanner>

    <div class="rounded-lg border border-outline-variant/50 bg-surface-container-low p-md relative">
      <div
        v-if="loading"
        class="absolute inset-0 flex items-center justify-center text-on-surface-variant gap-xs"
      >
        <Icon name="graphic_eq" :size="20" class="animate-pulse" />
        <span class="font-mono text-labelSmall">analysing…</span>
      </div>
      <div ref="waveformBox" class="relative select-none touch-none">
        <canvas
          ref="canvas"
          width="720"
          height="120"
          class="w-full h-28 block pointer-events-none"
        ></canvas>
        <div
          class="absolute top-0 bottom-0 bg-primary/15 border-l-2 border-r-2 border-primary pointer-events-none"
          :style="{ left: `${startFrac * 100}%`, right: `${(1 - endFrac) * 100}%` }"
        ></div>
        <div
          v-for="side in ['start', 'end'] as const"
          :key="side"
          class="absolute -top-2 -bottom-2 -ml-7 w-14 cursor-ew-resize flex items-center justify-center touch-none"
          :style="{ left: `${(side === 'start' ? startFrac : endFrac) * 100}%` }"
          @pointerdown="(e) => beginHandleDrag(side, e)"
        >
          <div class="w-2 h-full bg-primary rounded-full shadow-lg"></div>
        </div>
        <div
          v-if="duration > 0"
          class="absolute top-0 bottom-0 w-px bg-tertiary pointer-events-none shadow-[0_0_4px_rgba(255,255,255,0.6)]"
          :style="{ left: `${playheadFrac * 100}%` }"
        >
          <div
            class="absolute -top-1 -translate-x-1/2 left-0 w-2 h-2 rounded-full bg-tertiary"
          ></div>
        </div>
      </div>
    </div>

    <div
      class="flex justify-between font-mono text-labelMedium text-on-surface-variant pt-xs border-t border-outline-variant/40"
    >
      <span class="text-primary">{{ fmtMs(start) }}</span>
      <span class="text-on-surface">selected {{ fmtMs(Math.max(0, end - start)) }}</span>
      <span class="text-primary">{{ fmtMs(end) }}</span>
    </div>

    <template #footer>
      <button
        class="min-h-12 px-lg rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors flex items-center gap-unit"
        title="Reset to full track"
        @click="reset"
      >
        <Icon name="restart_alt" :size="18" />
        Full track
      </button>
      <div class="flex gap-xs">
        <button
          class="min-h-12 w-12 rounded-full border border-outline-variant text-primary hover:bg-surface-container-high transition-colors flex items-center justify-center"
          :title="playing ? 'Stop' : 'Play selection'"
          :disabled="audioLoading || !target"
          @click="togglePlay"
        >
          <Spinner v-if="audioLoading" :size="20" />
          <Icon v-else :name="playing ? 'pause' : 'play_arrow'" :size="22" fill />
        </button>
        <button
          class="min-h-12 w-12 rounded-full border border-outline-variant text-on-surface hover:bg-surface-container-high transition-colors flex items-center justify-center"
          title="Cancel"
          @click="close"
        >
          <CancelIcon :size="20" />
        </button>
        <button
          class="min-h-12 w-12 rounded-full bg-primary text-on-primary hover:bg-primary-fixed-dim transition-colors flex items-center justify-center"
          title="Save"
          @click="commit"
        >
          <SaveIcon :size="20" />
        </button>
      </div>
    </template>
  </Modal>
</template>
