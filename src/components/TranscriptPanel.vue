<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import type { Transcript } from "@/types";
import { api } from "@/api";
import {
  createAudioSegmentCache,
  mediaErrorMessage,
  waitForPlayableAudio,
} from "@utils/audioSegmentCache";
import { copyTextToClipboard } from "@utils/clipboard";
import { fmtMs as fmt } from "@utils/format";
import { fieldClass } from "@styles/fields";
import SlidingPanel from "@components/SlidingPanel.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import Modal from "@components/ui/Modal.vue";
import ErrorBanner from "@components/ui/ErrorBanner.vue";

const props = defineProps<{
  transcript: Transcript;
  cacheKey?: string | null;
  sourcePath: string;
}>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "updated", transcript: Transcript): void;
}>();

const speaker = ref<string | null>(null);
const speakerDraft = ref("");
const speakerInput = ref<HTMLInputElement | null>(null);
const editing = ref<number | null>(null);
const draft = ref("");
const textInput = ref<HTMLTextAreaElement[]>([]);
const saving = ref(false);
const localError = ref<string | null>(null);
const saved = ref(false);
const canEdit = computed(() => !!props.cacheKey && !saving.value);
let returnFocus: HTMLElement | null = null;
let disposed = false;

async function startSpeakerEdit(name: string, event: MouseEvent) {
  if (!canEdit.value || editing.value !== null) return;
  returnFocus = event.currentTarget as HTMLElement;
  speaker.value = name;
  speakerDraft.value = name;
  localError.value = null;
  await nextTick();
  speakerInput.value?.focus();
  speakerInput.value?.select();
}

function closeSpeaker() {
  if (saving.value) return;
  speaker.value = null;
  localError.value = null;
  returnFocus?.focus();
}

async function renameSpeaker() {
  if (!props.cacheKey || !speaker.value || !speakerDraft.value.trim() || saving.value) return;
  saving.value = true;
  localError.value = null;
  try {
    const result = await api.renameSpeaker(
      props.cacheKey,
      speaker.value,
      speakerDraft.value.trim(),
      props.sourcePath,
    );
    if (disposed) return;
    emit("updated", result);
    speaker.value = null;
    saved.value = true;
    await nextTick();
    returnFocus?.focus();
  } catch (error) {
    if (!disposed) localError.value = `Could not save speaker name: ${String(error)}`;
  } finally {
    saving.value = false;
  }
}

async function startTextEdit(index: number) {
  if (!canEdit.value || editing.value !== null) return;
  editing.value = index;
  draft.value = props.transcript.utterances[index].text;
  localError.value = null;
  saved.value = false;
  await nextTick();
  textInput.value[0]?.focus();
}

async function saveText() {
  if (!props.cacheKey || editing.value === null || saving.value) return;
  saving.value = true;
  localError.value = null;
  try {
    const result = await api.updateTranscriptText(
      props.cacheKey,
      props.sourcePath,
      editing.value,
      draft.value,
    );
    if (disposed) return;
    emit("updated", result);
    editing.value = null;
    saved.value = true;
  } catch (error) {
    if (!disposed) localError.value = `Could not save text: ${String(error)}`;
  } finally {
    saving.value = false;
  }
}

const audio = ref<HTMLAudioElement | null>(null);
const audioSrc = ref("");
const activeSegment = ref<number | null>(null);
const audioLoading = ref(false);
const fragments = createAudioSegmentCache((start, end) =>
  api.readAudioSegment(props.sourcePath, start, end),
);
let playbackAbort: AbortController | null = null;
let playRequest = 0;
let frame: number | null = null;
let segmentEndMs = 0;

function checkPlaybackBoundary() {
  if (
    !audioLoading.value &&
    activeSegment.value !== null &&
    audio.value &&
    audio.value.currentTime * 1000 >= segmentEndMs
  )
    stopPlayback();
}

function stopPlayback() {
  playRequest += 1;
  playbackAbort?.abort();
  playbackAbort = null;
  audio.value?.pause();
  activeSegment.value = null;
  audioLoading.value = false;
  if (frame !== null) cancelAnimationFrame(frame);
  frame = null;
}

function segmentRange(index: number) {
  const segment = props.transcript.utterances[index];
  const end = Math.floor(Math.min(segment.end_ms, props.transcript.duration_ms));
  const start = Math.floor(Math.max(0, Math.min(segment.start_ms, end)));
  return { start, end };
}

function prefetch(index: number) {
  if (disposed || !props.transcript.utterances[index]) return;
  const { start, end } = segmentRange(index);
  if (end > start) void fragments.load(start, end).catch(() => {});
}

onMounted(() => prefetch(0));

async function playSegment(index: number) {
  if (activeSegment.value === index) {
    stopPlayback();
    return;
  }
  stopPlayback();
  const request = playRequest;
  activeSegment.value = index;
  audioLoading.value = true;
  localError.value = null;
  try {
    const { start, end } = segmentRange(index);
    if (end <= start) {
      stopPlayback();
      return;
    }
    const blob = await fragments.load(start, end);
    if (disposed || request !== playRequest) return;
    const oldSrc = audioSrc.value;
    audioSrc.value = URL.createObjectURL(blob);
    await nextTick();
    if (oldSrc) URL.revokeObjectURL(oldSrc);
    if (disposed || request !== playRequest || !audio.value) return;
    const element = audio.value;
    playbackAbort = new AbortController();
    await waitForPlayableAudio(element, playbackAbort.signal);
    if (disposed || request !== playRequest) return;
    const endMs = end - start;
    segmentEndMs = endMs;
    element.currentTime = 0;
    await element.play();
    if (disposed || request !== playRequest) return;
    prefetch(index + 1);
    const tick = () => {
      if (!audio.value || request !== playRequest) return;
      if (audio.value.currentTime * 1000 >= endMs || audio.value.ended) {
        stopPlayback();
        return;
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
  } catch (error) {
    if (!disposed && request === playRequest) {
      stopPlayback();
      localError.value = `Could not play segment: ${String(error)}`;
    }
  } finally {
    if (request === playRequest) audioLoading.value = false;
  }
}

function audioEnded(event: Event) {
  if (event.target === audio.value) stopPlayback();
}

function audioError(event: Event) {
  const element = event.target as HTMLAudioElement;
  if (disposed || element !== audio.value || activeSegment.value === null) return;
  const message = mediaErrorMessage(element);
  console.error(message);
  stopPlayback();
  localError.value = message;
}

onBeforeUnmount(() => {
  disposed = true;
  fragments.clear();
  stopPlayback();
  if (audioSrc.value) URL.revokeObjectURL(audioSrc.value);
});

const copied = ref(false);
async function copyTranscript() {
  try {
    const text = await api.formatTranscript(props.transcript, "txt");
    await copyTextToClipboard(text);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 1500);
  } catch (error) {
    localError.value = String(error);
  }
}
</script>

<template>
  <SlidingPanel storage-key="wt.transcriptHeightPx" :initial-height="360" :auto-max="true">
    <template #header>
      <h3 class="text-titleSmall text-on-surface flex items-center gap-xs pl-md">
        <Icon name="subtitles" :size="18" class="text-primary" />
        Transcript
      </h3>
      <div class="flex items-center gap-xs mr-md">
        <span v-if="saved" role="status" class="text-labelSmall text-on-surface-variant"
          >Saved to .txt</span
        >
        <Button
          variant="ghost"
          shape="circle"
          size="sm"
          :icon="copied ? 'check' : 'content_copy'"
          :icon-size="18"
          :title="copied ? 'Copied' : 'Copy transcript'"
          :aria-label="copied ? 'Copied' : 'Copy transcript'"
          @pointerdown.stop
          @click.stop="copyTranscript"
        />
        <Button
          variant="ghost"
          shape="circle"
          size="sm"
          icon="close"
          :icon-size="18"
          title="Close transcript"
          aria-label="Close transcript"
          @pointerdown.stop
          :disabled="saving || editing !== null || speaker !== null"
          @click.stop="emit('close')"
        />
      </div>
    </template>
    <ErrorBanner v-if="localError && !speaker">{{ localError }}</ErrorBanner>
    <audio
      v-if="audioSrc"
      :key="audioSrc"
      ref="audio"
      :src="audioSrc"
      preload="auto"
      class="hidden"
      @ended="audioEnded"
      @timeupdate="checkPlaybackBoundary"
      @error="audioError"
    ></audio>
    <article
      v-for="(u, i) in transcript.utterances"
      :key="i"
      class="flex gap-xs items-start group hover:bg-surface-container-high/30 -mx-xs px-xs py-xs rounded transition-colors"
      :class="activeSegment === i ? 'bg-surface-container-high' : ''"
    >
      <Button
        variant="ghost"
        shape="circle"
        :icon="activeSegment === i ? 'pause' : 'play_arrow'"
        :title="`${activeSegment === i ? 'Stop' : 'Play'} segment ${i + 1}`"
        :aria-label="`${activeSegment === i ? 'Stop' : 'Play'} segment ${i + 1}`"
        :aria-pressed="activeSegment === i"
        @click="playSegment(i)"
      />
      <div class="flex-1 min-w-0">
        <div class="flex flex-wrap items-center gap-xs mb-unit">
          <span class="font-mono text-labelSmall text-secondary"
            >{{ fmt(u.start_ms) }} – {{ fmt(u.end_ms) }}</span
          >
          <button
            v-if="u.speaker"
            type="button"
            class="text-labelSmall text-primary hover:underline cursor-pointer"
            title="Rename speaker"
            :disabled="!canEdit || editing !== null"
            @click="startSpeakerEdit(u.speaker, $event)"
          >
            {{ u.speaker }}
          </button>
          <span
            v-if="activeSegment === i && audioLoading"
            role="status"
            class="text-labelSmall text-on-surface-variant"
            >Loading audio…</span
          >
        </div>
        <div v-if="editing === i" class="flex flex-col gap-xs">
          <label class="text-labelSmall text-on-surface-variant">
            Segment text
            <textarea
              ref="textInput"
              v-model="draft"
              :class="fieldClass"
              rows="3"
              :disabled="saving"
              @keydown.ctrl.enter.prevent="saveText"
              @keydown.meta.enter.prevent="saveText"
              @keydown.escape.prevent="!saving && (editing = null)"
            ></textarea>
          </label>
          <div class="flex justify-end gap-xs">
            <Button :disabled="saving" @click="editing = null">Cancel</Button>
            <Button variant="primary" :disabled="saving" @click="saveText">{{
              saving ? "Saving…" : "Save text"
            }}</Button>
          </div>
        </div>
        <p
          v-else
          class="text-bodyMedium text-on-surface-variant group-hover:text-on-surface transition-colors leading-relaxed whitespace-pre-wrap"
        >
          {{ u.text }}
        </p>
      </div>
      <Button
        v-if="editing !== i"
        variant="ghost"
        shape="circle"
        icon="edit"
        :title="`Edit segment ${i + 1}`"
        :aria-label="`Edit segment ${i + 1}`"
        :disabled="!canEdit || editing !== null"
        @click="startTextEdit(i)"
      />
    </article>
  </SlidingPanel>
  <Modal
    :open="speaker !== null"
    title="Rename speaker"
    :backdrop-close="!saving"
    @close="closeSpeaker"
  >
    <p class="text-bodyMedium text-on-surface-variant">
      Update this speaker’s name in all segments and the saved text transcript.
    </p>
    <ErrorBanner v-if="localError">{{ localError }}</ErrorBanner>
    <label class="block text-labelSmall text-on-surface-variant">
      Speaker name
      <input
        ref="speakerInput"
        v-model="speakerDraft"
        :class="fieldClass"
        :disabled="saving"
        @keydown.enter.prevent="renameSpeaker"
      />
    </label>
    <template #footer>
      <Button :disabled="saving" @click="closeSpeaker">Cancel</Button>
      <Button variant="primary" :disabled="saving || !speakerDraft.trim()" @click="renameSpeaker">{{
        saving ? "Saving…" : "Rename"
      }}</Button>
    </template>
  </Modal>
</template>
