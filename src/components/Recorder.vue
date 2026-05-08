<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { api } from "../api";
import Button from "./ui/Button.vue";

const props = defineProps<{ workdir: string; headless?: boolean }>();
const emit = defineEmits<{ (e: "saved", path: string): void }>();

const BAR_COUNT = 32;

const recording = ref(false);
const elapsedMs = ref(0);
const error = ref<string | null>(null);
const bars = ref<number[]>(new Array(BAR_COUNT).fill(0));

let stream: MediaStream | null = null;
let recorder: MediaRecorder | null = null;
let chunks: Blob[] = [];
let audioCtx: AudioContext | null = null;
let analyser: AnalyserNode | null = null;
let rafId: number | null = null;
let timerId: ReturnType<typeof setInterval> | null = null;
let startedAt = 0;

const elapsed = computed(() => {
  const ms = elapsedMs.value;
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
});

function pickMime(): string {
  const candidates = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/mp4"];
  for (const c of candidates) {
    if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(c)) return c;
  }
  return "audio/webm";
}

const TARGET_SR = 16000;

async function blobToWav16kMono(blob: Blob): Promise<Uint8Array> {
  const arr = await blob.arrayBuffer();
  const ctx = new (window.AudioContext || (window as any).webkitAudioContext)();
  const buf = await ctx.decodeAudioData(arr.slice(0));
  await ctx.close();
  const ch = buf.numberOfChannels;
  const len = buf.length;
  const mono = new Float32Array(len);
  for (let i = 0; i < ch; i++) {
    const data = buf.getChannelData(i);
    for (let j = 0; j < len; j++) mono[j] += data[j] / ch;
  }
  const resampled =
    buf.sampleRate === TARGET_SR ? mono : resampleLinear(mono, buf.sampleRate, TARGET_SR);
  return encodeWav16(resampled, TARGET_SR);
}

function resampleLinear(input: Float32Array, fromSr: number, toSr: number): Float32Array {
  const ratio = toSr / fromSr;
  const outLen = Math.floor(input.length * ratio);
  const out = new Float32Array(outLen);
  for (let i = 0; i < outLen; i++) {
    const srcIdx = i / ratio;
    const i0 = Math.floor(srcIdx);
    const i1 = Math.min(i0 + 1, input.length - 1);
    const frac = srcIdx - i0;
    out[i] = input[i0] * (1 - frac) + input[i1] * frac;
  }
  return out;
}

function encodeWav16(samples: Float32Array, sr: number): Uint8Array {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const writeStr = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(o + i, s.charCodeAt(i));
  };
  writeStr(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sr, true);
  view.setUint32(28, sr * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, "data");
  view.setUint32(40, samples.length * 2, true);
  let off = 44;
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    off += 2;
  }
  return bytes;
}

function tickBars() {
  if (!analyser) return;
  const bins = new Uint8Array(analyser.frequencyBinCount);
  analyser.getByteFrequencyData(bins);
  const step = Math.floor(bins.length / BAR_COUNT);
  const next: number[] = [];
  for (let i = 0; i < BAR_COUNT; i++) {
    let sum = 0;
    for (let j = 0; j < step; j++) sum += bins[i * step + j];
    next.push(Math.min(100, (sum / step / 255) * 140));
  }
  bars.value = next;
  rafId = requestAnimationFrame(tickBars);
}

async function start() {
  error.value = null;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const mime = pickMime();
    recorder = new MediaRecorder(stream, { mimeType: mime });
    chunks = [];
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };
    recorder.onstop = onStopped;
    recorder.start(250);
    audioCtx = new AudioContext();
    analyser = audioCtx.createAnalyser();
    analyser.fftSize = 256;
    audioCtx.createMediaStreamSource(stream).connect(analyser);
    rafId = requestAnimationFrame(tickBars);
    startedAt = Date.now();
    elapsedMs.value = 0;
    timerId = setInterval(() => (elapsedMs.value = Date.now() - startedAt), 200);
    recording.value = true;
  } catch (e) {
    error.value = `mic: ${String(e)}`;
    cleanup();
  }
}

function stop() {
  if (recorder && recorder.state !== "inactive") recorder.stop();
}

async function onStopped() {
  const mime = recorder?.mimeType || "audio/webm";
  const blob = new Blob(chunks, { type: mime });
  cleanup();
  try {
    const wav = await blobToWav16kMono(blob);
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const ts = `${pad(d.getFullYear() % 100)}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
    const path = await api.saveRecording(props.workdir, `recording_${ts}.wav`, wav);
    emit("saved", path);
  } catch (e) {
    error.value = `save: ${String(e)}`;
  }
}

function cleanup() {
  recording.value = false;
  if (rafId !== null) cancelAnimationFrame(rafId);
  rafId = null;
  if (timerId) clearInterval(timerId);
  timerId = null;
  if (stream) stream.getTracks().forEach((t) => t.stop());
  stream = null;
  if (audioCtx) audioCtx.close().catch(() => {});
  audioCtx = null;
  analyser = null;
  recorder = null;
  chunks = [];
  bars.value = new Array(BAR_COUNT).fill(0);
}

onUnmounted(cleanup);

defineExpose({
  recording,
  elapsed,
  start,
  stop,
  toggle: () => (recording.value ? stop() : start()),
});
</script>

<template>
  <section class="flex flex-col gap-md">
    <div
      v-if="recording"
      class="bg-surface-container rounded-lg border border-outline-variant/40 h-[120px] flex items-end px-xs py-md gap-[2px] overflow-hidden"
    >
      <div
        v-for="(h, i) in bars"
        :key="i"
        class="flex-1 bg-primary rounded-full transition-[height] duration-75"
        :style="{ height: `${Math.max(4, h)}%` }"
      ></div>
    </div>
    <div v-if="!props.headless" class="flex gap-xs">
      <Button
        v-if="!recording"
        variant="error"
        size="lg"
        block
        icon="fiber_manual_record"
        :icon-size="18"
        icon-fill
        @click="start"
      >
        Rec
      </Button>
      <Button
        v-else
        variant="primary"
        size="lg"
        block
        bold
        icon="stop"
        :icon-size="18"
        icon-fill
        @click="stop"
      >
        Stop · {{ elapsed }}
      </Button>
    </div>
    <p v-if="error" class="text-error font-mono text-labelSmall">{{ error }}</p>
  </section>
</template>
