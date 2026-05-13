<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { api } from "@/api";

function guessMime(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (
    (
      {
        mp3: "audio/mpeg",
        m4a: "audio/mp4",
        mp4: "audio/mp4",
        aac: "audio/aac",
        wav: "audio/wav",
        flac: "audio/flac",
        ogg: "audio/ogg",
        oga: "audio/ogg",
        opus: "audio/ogg",
        webm: "audio/webm",
      } as Record<string, string>
    )[ext] ?? "audio/*"
  );
}
import type { AudioMeta, DirEntry } from "@/types";
import { fmtMs } from "@utils/format";
import Modal from "@components/ui/Modal.vue";
import ErrorBanner from "@components/ui/ErrorBanner.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import Spinner from "@components/icons/Spinner.vue";
import CancelIcon from "@components/icons/CancelIcon.vue";
import SaveIcon from "@components/icons/SaveIcon.vue";

const applying = ref(false);
const initialStart = ref(0);
const initialEnd = ref(0);
const initialDuration = ref(0);
let seekScrubActive = false;

const props = defineProps<{ target: DirEntry | null }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "saved", payload?: { path: string; durationMs: number | null; trimmed: boolean }): void;
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
const audioEl = ref<HTMLAudioElement | null>(null);
const audioSrc = ref<string>("");
let currentObjectUrl: string | null = null;
let playOffsetMs = 0;
let playheadRaf: number | null = null;

async function loadAudioBlob(target: DirEntry) {
  audioLoading.value = true;
  try {
    const bytes = await api.readAudioBytes(target.path);
    const blob = new Blob([new Uint8Array(bytes)], { type: guessMime(target.path) });
    if (currentObjectUrl) URL.revokeObjectURL(currentObjectUrl);
    currentObjectUrl = URL.createObjectURL(blob);
    audioSrc.value = currentObjectUrl;
  } catch (e) {
    localError.value = `audio load: ${String(e)}`;
  } finally {
    audioLoading.value = false;
  }
}

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
    initialStart.value = s;
    initialEnd.value = e;
    initialDuration.value = dur;
    playOffsetMs = s;
    playheadMs.value = s;
    await renderWaveform();
    void loadAudioBlob(target);
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

async function resumeFromOffset() {
  const el = audioEl.value;
  if (!el) return;
  let offsetMs = playOffsetMs;
  if (offsetMs < start.value || offsetMs >= end.value) offsetMs = start.value;
  el.currentTime = offsetMs / 1000;
  try {
    await el.play();
    playing.value = true;
    startPlayheadLoop();
  } catch (e) {
    localError.value = `play: ${String(e)}`;
  }
}

function handleWaveformPointerDown(ev: PointerEvent) {
  if (ev.button !== 0) return;
  const box = waveformBox.value;
  if (!box || duration.value === 0) return;
  ev.preventDefault();
  seekScrubActive = true;
  const wasPlaying = playing.value;

  if (wasPlaying && audioEl.value) audioEl.value.pause();
  playing.value = false;
  stopPlayheadLoop();

  const computeMs = (e: PointerEvent): number => {
    const rect = box.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const ms = (x / rect.width) * duration.value;
    return Math.min(end.value, Math.max(start.value, Math.round(ms)));
  };
  const updateVisual = (e: PointerEvent) => {
    const ms = computeMs(e);
    playOffsetMs = ms;
    playheadMs.value = ms;
  };
  updateVisual(ev);

  const move = (e: PointerEvent) => {
    if (!seekScrubActive) return;
    updateVisual(e);
  };
  const up = (e: PointerEvent) => {
    seekScrubActive = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
    updateVisual(e);
    if (audioEl.value) audioEl.value.currentTime = playOffsetMs / 1000;
    if (wasPlaying) {
      void resumeFromOffset().catch(() => {});
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
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

function stopPlayheadLoop() {
  if (playheadRaf !== null) cancelAnimationFrame(playheadRaf);
  playheadRaf = null;
}

function startPlayheadLoop() {
  stopPlayheadLoop();
  const tick = () => {
    const el = audioEl.value;
    if (!playing.value || !el) {
      playheadRaf = null;
      return;
    }
    const ms = el.currentTime * 1000;
    if (ms >= end.value) {
      el.pause();
      playOffsetMs = end.value;
      playheadMs.value = end.value;
      playing.value = false;
      stopPlayheadLoop();
      return;
    }
    playOffsetMs = ms;
    playheadMs.value = Math.min(end.value, Math.max(start.value, ms));
    playheadRaf = requestAnimationFrame(tick);
  };
  playheadRaf = requestAnimationFrame(tick);
}

function pause() {
  const el = audioEl.value;
  if (el) {
    el.pause();
    playOffsetMs = el.currentTime * 1000;
    playheadMs.value = playOffsetMs;
  }
  playing.value = false;
  stopPlayheadLoop();
}

function stop() {
  const el = audioEl.value;
  if (el) {
    try {
      el.pause();
      el.currentTime = start.value / 1000;
    } catch {}
  }
  playOffsetMs = start.value;
  playheadMs.value = start.value;
  playing.value = false;
  stopPlayheadLoop();
}

async function togglePlay() {
  const el = audioEl.value;
  if (!el) return;
  if (playing.value) {
    pause();
    return;
  }
  let offsetMs = playOffsetMs;
  if (offsetMs < start.value || offsetMs >= end.value) offsetMs = start.value;
  audioLoading.value = el.readyState < 2;
  try {
    el.currentTime = offsetMs / 1000;
    await el.play();
    playOffsetMs = offsetMs;
    playheadMs.value = offsetMs;
    playing.value = true;
    startPlayheadLoop();
  } catch (e) {
    localError.value = `play: ${String(e)}`;
  } finally {
    audioLoading.value = false;
  }
}

function onAudioLoaded() {
  audioLoading.value = false;
}

function onAudioEnded() {
  playOffsetMs = start.value;
  playheadMs.value = start.value;
  playing.value = false;
  stopPlayheadLoop();
}

function onAudioError(ev: Event) {
  const el = ev.target as HTMLAudioElement | null;
  const code = el?.error?.code ?? -1;
  const msg = el?.error?.message ?? "";
  const codeStr =
    code === 1
      ? "ABORTED"
      : code === 2
        ? "NETWORK"
        : code === 3
          ? "DECODE"
          : code === 4
            ? "SRC_NOT_SUPPORTED"
            : `code=${code}`;
  console.error("audio error", { code: codeStr, msg, src: el?.currentSrc });
  localError.value = `audio failed to load (${codeStr}): ${msg || el?.currentSrc || "unknown"}`;
  audioLoading.value = false;
  playing.value = false;
  stopPlayheadLoop();
}

function reset() {
  start.value = 0;
  end.value = duration.value;
}

function markIn() {
  if (duration.value === 0) return;
  const playMs = playheadMs.value;
  const gap = Math.min(MIN_GAP_MS, Math.max(100, Math.floor(duration.value / 4)));
  const newStart = Math.max(0, Math.min(playMs, end.value - gap));
  start.value = newStart;
  if (playOffsetMs < newStart) {
    playOffsetMs = newStart;
    playheadMs.value = newStart;
  }
}

function markOut() {
  if (duration.value === 0) return;
  const playMs = playheadMs.value;
  const gap = Math.min(MIN_GAP_MS, Math.max(100, Math.floor(duration.value / 4)));
  const newEnd = Math.min(duration.value, Math.max(playMs, start.value + gap));
  end.value = newEnd;
  if (playOffsetMs > newEnd) {
    playOffsetMs = newEnd;
    playheadMs.value = newEnd;
  }
}

function handleShortcut(ev: KeyboardEvent) {
  if (!open.value) return;
  const target = ev.target as HTMLElement | null;
  if (
    target &&
    (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
  ) {
    return;
  }
  if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
  switch (ev.key) {
    case " ":
    case "k":
    case "K":
      ev.preventDefault();
      void togglePlay();
      break;
    case "i":
    case "I":
      ev.preventDefault();
      markIn();
      break;
    case "o":
    case "O":
      ev.preventDefault();
      markOut();
      break;
    default:
  }
}

watch(open, (isOpen) => {
  if (isOpen) {
    window.addEventListener("keydown", handleShortcut);
  } else {
    window.removeEventListener("keydown", handleShortcut);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleShortcut);
  cleanup();
});

function isDirty(): boolean {
  return (
    Math.floor(start.value) !== Math.floor(initialStart.value) ||
    Math.floor(end.value) !== Math.floor(initialEnd.value)
  );
}

async function persistMeta(target: DirEntry): Promise<boolean> {
  const s = Math.max(0, Math.floor(start.value));
  const e = Math.min(duration.value, Math.floor(end.value));
  const meta: AudioMeta = {
    trim_start_ms: s,
    trim_end_ms: e < duration.value ? e : null,
    duration_ms: null,
  };
  try {
    await api.saveAudioMeta(target.path, meta);
    initialStart.value = s;
    initialEnd.value = e;
    return true;
  } catch (err) {
    localError.value = String(err);
    return false;
  }
}

async function commit() {
  if (!props.target) return;
  const target = props.target;
  if (!(await persistMeta(target))) return;
  cleanup();
  emit("saved", { path: target.path, durationMs: null, trimmed: false });
}

async function applyPermanent() {
  if (!props.target) return;
  const target = props.target;
  const willTrim = start.value > 0 || end.value < duration.value;
  if (!willTrim) {
    localError.value = "nothing to trim — selection covers the whole track";
    return;
  }
  const ok = window.confirm(
    `Replace ${target.name} with the trimmed version (${fmtMs(
      Math.max(0, end.value - start.value),
    )})? This rewrites the original file and cannot be undone.`,
  );
  if (!ok) return;
  applying.value = true;
  localError.value = null;
  try {
    if (isDirty() && !(await persistMeta(target))) return;
    pause();
    const newDuration = await api.applyTrim(target.path);
    initialStart.value = 0;
    initialEnd.value = 0;
    initialDuration.value = 0;
    cleanup();
    emit("saved", {
      path: target.path,
      durationMs: newDuration ?? null,
      trimmed: true,
    });
  } catch (err) {
    localError.value = String(err);
  } finally {
    applying.value = false;
  }
}

function cleanup() {
  stop();
  playOffsetMs = 0;
  const el = audioEl.value;
  if (el) {
    try {
      el.pause();
      el.removeAttribute("src");
      el.load();
    } catch {}
  }
  audioSrc.value = "";
  if (currentObjectUrl) {
    URL.revokeObjectURL(currentObjectUrl);
    currentObjectUrl = null;
  }
}

async function close() {
  if (props.target && isDirty()) {
    await persistMeta(props.target);
  }
  cleanup();
  emit("close");
}
</script>

<template>
  <Modal :open="open" :width="'768px'" :backdrop-close="false" @close="close">
    <template #header>
      <Icon name="content_cut" :size="22" class="text-primary mt-unit" />
      <div class="flex-1 min-w-0">
        <h3 class="text-titleSmall text-on-surface">Select range to transcribe</h3>
        <p class="font-mono text-labelSmall text-on-surface-variant truncate" :title="target?.name">
          {{ target?.name ?? "—" }}
        </p>
      </div>
      <Button
        variant="ghost"
        shape="icon"
        icon="close"
        :icon-size="20"
        title="Close"
        @click="close"
      />
    </template>

    <ErrorBanner v-if="localError">{{ localError }}</ErrorBanner>

    <audio
      v-if="audioSrc"
      ref="audioEl"
      :src="audioSrc"
      preload="metadata"
      class="hidden"
      @loadedmetadata="onAudioLoaded"
      @canplay="onAudioLoaded"
      @ended="onAudioEnded"
      @error="onAudioError"
    ></audio>

    <div class="rounded-lg border border-outline-variant/50 bg-surface-container-low p-md relative">
      <div
        v-if="loading"
        class="absolute inset-0 flex items-center justify-center text-on-surface-variant gap-xs"
      >
        <Icon name="graphic_eq" :size="20" class="animate-pulse" />
        <span class="font-mono text-labelSmall">analysing…</span>
      </div>
      <div
        ref="waveformBox"
        class="relative select-none touch-none cursor-pointer"
        @pointerdown="handleWaveformPointerDown"
      >
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
          class="absolute -top-2 -bottom-2 -ml-7 w-14 pointer-fine:-ml-1.5 pointer-fine:w-3 cursor-ew-resize flex items-center justify-center touch-none"
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
            class="absolute -top-1.5 -translate-x-1/2 left-0 w-3 h-3 rounded-full bg-tertiary shadow pointer-events-none"
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
      <Button
        variant="neutral"
        size="lg"
        icon="restart_alt"
        :icon-size="18"
        title="Reset to full track"
        @click="reset"
      >
        Full track
      </Button>
      <div class="flex gap-xs">
        <Button
          variant="neutral"
          shape="circle"
          size="lg"
          title="Mark in (I) — set start to playhead"
          :disabled="!target"
          @click="markIn"
        >
          <Icon name="first_page" :size="22" />
        </Button>
        <Button
          variant="neutral"
          shape="circle"
          size="lg"
          :title="playing ? 'Pause (Space)' : 'Play selection (Space)'"
          :disabled="audioLoading || !target"
          class="text-primary"
          @click="togglePlay"
        >
          <Spinner v-if="audioLoading" :size="20" />
          <Icon v-else :name="playing ? 'pause' : 'play_arrow'" :size="22" fill />
        </Button>
        <Button
          variant="neutral"
          shape="circle"
          size="lg"
          title="Mark out (O) — set end to playhead"
          :disabled="!target"
          @click="markOut"
        >
          <Icon name="last_page" :size="22" />
        </Button>
        <span class="w-px self-stretch bg-outline-variant/40 mx-xs"></span>
        <Button
          variant="neutral"
          shape="circle"
          size="lg"
          title="Apply trim permanently (rewrites the original file)"
          :disabled="applying || !target"
          @click="applyPermanent"
        >
          <Spinner v-if="applying" :size="20" />
          <Icon v-else name="content_cut" :size="20" />
        </Button>
        <Button variant="neutral" shape="circle" size="lg" title="Cancel" @click="close">
          <CancelIcon :size="20" />
        </Button>
        <Button variant="primary" shape="circle" size="lg" title="Save" @click="commit">
          <SaveIcon :size="20" />
        </Button>
      </div>
    </template>
  </Modal>
</template>
