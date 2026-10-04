<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { Config, Transcript } from "@/types";
import { api } from "@/api";
import {
  createAudioSegmentCache,
  segmentPlaybackRange,
  mediaErrorMessage,
  waitForPlayableAudio,
} from "@utils/audioSegmentCache";
import { copyTextToClipboard } from "@utils/clipboard";
import { fmtMs as fmt } from "@utils/format";
import { fieldClass } from "@styles/fields";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import Modal from "@components/ui/Modal.vue";
import ErrorBanner from "@components/ui/ErrorBanner.vue";

const props = defineProps<{
  transcript: Transcript;
  cacheKey?: string | null;
  sourcePath: string;
  config?: Config | null;
  busy?: boolean;
}>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "updated", transcript: Transcript): void;
}>();

const reviewOnly = ref(false);
const reviewWords = computed(() =>
  props.transcript.diarizer?.startsWith("whisperx-community-1")
    ? props.transcript.words.filter((word) => word.confidence < 0.3)
    : [],
);
const transcriptRows = computed(() =>
  props.transcript.utterances.map((u, i) => ({
    u,
    i,
    uncertain: reviewWords.value.filter(
      (word) =>
        word.speaker === u.speaker && word.start_ms >= u.start_ms && word.start_ms < u.end_ms,
    ),
  })),
);
const reviewRows = computed(() =>
  transcriptRows.value.filter((row) => row.u.needs_review || row.uncertain.length),
);
const visibleRows = computed(() => (reviewOnly.value ? reviewRows.value : transcriptRows.value));
watch(
  () => props.sourcePath,
  () => {
    reviewOnly.value = false;
  },
);

const speaker = ref<string | null>(null);
const speakerDraft = ref("");
const speakerIndex = ref(0);
const speakerScope = ref<"all" | "segment">("segment");
const speakerNames = computed(() => [
  ...new Set(props.transcript.utterances.flatMap((u) => (u.speaker ? [u.speaker] : []))),
]);
const replacing = ref(false);
const searchText = ref("");
const replacementText = ref("");
const replacements = computed(() =>
  searchText.value
    ? props.transcript.utterances.flatMap((u, index) => {
        const count = u.text.split(searchText.value).length - 1;
        return count
          ? [{ index, count, text: u.text.split(searchText.value).join(replacementText.value) }]
          : [];
      })
    : [],
);
const matchCount = computed(() => replacements.value.reduce((sum, row) => sum + row.count, 0));
const speakerInput = ref<HTMLInputElement | null>(null);
const editing = ref<number | null>(null);
const draft = ref("");
const textInput = ref<HTMLTextAreaElement[]>([]);
const saving = ref(false);
const localError = ref<string | null>(null);
const saved = ref(false);
const canEdit = computed(() => !!props.cacheKey && !saving.value && !props.busy);
const canUndo = ref(false);
const retrying = ref<number | null>(null);
const undoButton = ref<InstanceType<typeof Button> | null>(null);
let historyRequest = 0;

async function refreshUndo() {
  const request = ++historyRequest;
  if (!props.cacheKey) {
    canUndo.value = false;
    return;
  }
  try {
    const available = await api.transcriptCanUndo(props.cacheKey, props.sourcePath);
    if (!disposed && request === historyRequest) canUndo.value = available;
  } catch (error) {
    if (!disposed && request === historyRequest)
      localError.value = `Could not load undo history: ${String(error)}`;
  }
}

function edited(result: Transcript) {
  emit("updated", result);
  saved.value = true;
  void refreshUndo();
}

async function segmentAction(index: number, action: "delete" | "review" | "retry") {
  if (
    !canEdit.value ||
    !props.cacheKey ||
    editing.value !== null ||
    speaker.value !== null ||
    replacing.value
  )
    return;
  if (action === "retry" && !props.config) return;
  saving.value = true;
  localError.value = null;
  stopPlayback();
  if (action === "retry") retrying.value = index;
  try {
    const result =
      action === "delete"
        ? await api.deleteTranscriptSegment(props.cacheKey, props.sourcePath, index)
        : action === "review"
          ? await api.markTranscriptReview(
              props.cacheKey,
              props.sourcePath,
              index,
              !props.transcript.utterances[index].needs_review,
            )
          : await api.retryTranscriptSegment(
              props.cacheKey,
              props.sourcePath,
              index,
              props.config!,
            );
    if (!disposed) {
      edited(result);
      await nextTick();
      if (action === "delete") (undoButton.value?.$el as HTMLButtonElement | undefined)?.focus();
    }
  } catch (error) {
    if (!disposed)
      localError.value = `Could not ${action === "retry" ? "retry recognition" : action === "delete" ? "delete segment" : "save review mark"}: ${String(error)}`;
  } finally {
    saving.value = false;
    retrying.value = null;
  }
}

async function undoEdit() {
  if (
    !canEdit.value ||
    !props.cacheKey ||
    !canUndo.value ||
    editing.value !== null ||
    speaker.value !== null ||
    replacing.value
  )
    return;
  saving.value = true;
  localError.value = null;
  stopPlayback();
  try {
    const result = await api.undoTranscriptEdit(props.cacheKey, props.sourcePath);
    if (!disposed) edited(result);
  } catch (error) {
    if (!disposed) localError.value = `Could not undo edit: ${String(error)}`;
  } finally {
    saving.value = false;
  }
}

function onUndoKey(event: KeyboardEvent) {
  if (
    !(event.ctrlKey || event.metaKey) ||
    event.key.toLowerCase() !== "z" ||
    event.shiftKey ||
    event.altKey ||
    event.repeat ||
    event.isComposing
  )
    return;
  const target = event.target as HTMLElement | null;
  if (
    target instanceof HTMLElement &&
    target.closest('input, textarea, select, [contenteditable="true"]')
  )
    return;
  if (speaker.value !== null || replacing.value || editing.value !== null) return;
  event.preventDefault();
  void undoEdit();
}

function closeTranscript() {
  if (saving.value || editing.value !== null || speaker.value !== null || replacing.value) return;
  emit("close");
}
let returnFocus: HTMLElement | null = null;
let disposed = false;

async function startSpeakerEdit(name: string, index: number, event: MouseEvent) {
  if (!canEdit.value || editing.value !== null) return;
  returnFocus = event.currentTarget as HTMLElement;
  speaker.value = name;
  speakerIndex.value = index;
  speakerScope.value = "segment";
  speakerDraft.value = name;
  localError.value = null;
  await nextTick();
  speakerInput.value?.focus();
  speakerInput.value?.select();
}

async function closeSpeaker() {
  if (saving.value) return;
  speaker.value = null;
  localError.value = null;
  await nextTick();
  returnFocus?.focus();
}

async function renameSpeaker() {
  if (!props.cacheKey || speaker.value === null || !speakerDraft.value.trim() || saving.value)
    return;
  saving.value = true;
  localError.value = null;
  try {
    const result =
      speakerScope.value === "segment"
        ? await api.setTranscriptSpeaker(
            props.cacheKey,
            props.sourcePath,
            speakerIndex.value,
            speakerDraft.value.trim(),
          )
        : await api.renameSpeaker(
            props.cacheKey,
            speaker.value,
            speakerDraft.value.trim(),
            props.sourcePath,
          );
    if (disposed) return;
    edited(result);
    speaker.value = null;
    saved.value = true;
  } catch (error) {
    if (!disposed) localError.value = `Could not save speaker name: ${String(error)}`;
  } finally {
    saving.value = false;
    await nextTick();
    if (!disposed && speaker.value === null) returnFocus?.focus();
  }
}

function openReplace() {
  localError.value = null;
  replacing.value = true;
}

function closeReplace() {
  if (saving.value) return;
  replacing.value = false;
  localError.value = null;
}

async function replaceAll() {
  if (!props.cacheKey || !matchCount.value || saving.value) return;
  saving.value = true;
  localError.value = null;
  try {
    const result = await api.replaceTranscriptText(
      props.cacheKey,
      props.sourcePath,
      searchText.value,
      replacementText.value,
    );
    if (disposed) return;
    edited(result);
    replacing.value = false;
    saved.value = true;
  } catch (error) {
    if (!disposed) localError.value = `Could not replace text: ${String(error)}`;
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
    edited(result);
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

function toggleTimingReview() {
  reviewOnly.value = !reviewOnly.value;
  if (
    reviewOnly.value &&
    activeSegment.value !== null &&
    !reviewRows.value.some((row) => row.i === activeSegment.value)
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
  return segmentPlaybackRange(props.transcript.utterances[index], props.transcript.duration_ms);
}

function prefetch(index: number) {
  if (disposed || !props.transcript.utterances[index]) return;
  const { start, end } = segmentRange(index);
  if (end > start) void fragments.load(start, end).catch(() => {});
}

onMounted(() => {
  prefetch(0);
  void refreshUndo();
  window.addEventListener("keydown", onUndoKey);
});

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
  window.removeEventListener("keydown", onUndoKey);
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
  <Modal
    :open="true"
    title="Transcript"
    width="1000px"
    :inactive="speaker !== null || replacing"
    :backdrop-close="!saving && editing === null && speaker === null && !replacing"
    @close="closeTranscript"
  >
    <template #header>
      <h3 class="text-titleSmall text-on-surface flex-1 flex items-center gap-xs">
        <Icon name="subtitles" :size="18" class="text-primary" />
        Transcript
      </h3>
      <div class="flex items-center gap-xs">
        <Button
          ref="undoButton"
          variant="ghost"
          shape="circle"
          size="sm"
          icon="undo"
          title="Undo edit (Ctrl+Z)"
          aria-label="Undo edit"
          :disabled="!canEdit || !canUndo || editing !== null || speaker !== null || replacing"
          @click="undoEdit"
        />
        <span
          v-if="saved"
          role="status"
          class="hidden sm:inline text-labelSmall text-on-surface-variant"
          >Saved to .txt</span
        >
        <Button
          variant="ghost"
          shape="circle"
          size="sm"
          icon="find_replace"
          title="Find and replace"
          aria-label="Find and replace"
          :disabled="!canEdit || editing !== null || speaker !== null"
          @click="openReplace"
        />
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
          :disabled="saving || editing !== null || speaker !== null || replacing"
          @click.stop="closeTranscript"
        />
      </div>
    </template>
    <ErrorBanner v-if="localError && speaker === null && !replacing">{{ localError }}</ErrorBanner>
    <div v-if="reviewRows.length || reviewOnly" class="flex items-center justify-between gap-xs">
      <span class="text-labelSmall text-on-surface-variant"
        >{{ reviewRows.length }} {{ reviewRows.length === 1 ? "segment" : "segments" }} to
        review</span
      >
      <Button
        :aria-pressed="reviewOnly"
        :disabled="saving || editing !== null"
        @click="toggleTimingReview"
      >
        {{
          reviewOnly
            ? "Show all segments"
            : `Show ${reviewRows.length} segment${reviewRows.length === 1 ? "" : "s"} to review`
        }}
      </Button>
    </div>
    <p
      v-if="!visibleRows.length"
      role="status"
      class="text-bodyMedium text-on-surface-variant py-md"
    >
      {{ reviewOnly ? "No segments to review." : "No segments. Use Undo to restore a deletion." }}
    </p>
    <details v-if="reviewWords.length" class="mb-sm text-bodySmall text-on-surface-variant">
      <summary class="cursor-pointer text-on-surface py-xs">
        Timing review: {{ reviewWords.length }} of {{ transcript.words.length }} words
      </summary>
      <p class="mb-xs">
        These words have lower alignment scores. This is a review hint, not a count of incorrect
        words or speaker labels. Listen to the affected segments to check their timing.
      </p>
    </details>
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
      v-for="{ u, i, uncertain } in visibleRows"
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
            type="button"
            class="text-labelSmall text-primary hover:underline cursor-pointer"
            title="Rename speaker"
            :disabled="!canEdit || editing !== null"
            @click="startSpeakerEdit(u.speaker ?? '', i, $event)"
          >
            {{ u.speaker || "Assign speaker" }}
          </button>
          <span v-if="u.needs_review" class="text-labelSmall text-secondary">Needs review</span>
          <span v-if="retrying === i" role="status" class="text-labelSmall text-on-surface-variant"
            >Recognising…</span
          >
          <span v-if="uncertain.length" class="text-labelSmall text-on-surface-variant">
            Check timing
          </span>
          <span
            v-if="activeSegment === i && audioLoading"
            role="status"
            class="text-labelSmall text-on-surface-variant"
            >Loading audio…</span
          >
        </div>
        <p v-if="reviewOnly && uncertain.length" class="text-labelSmall text-secondary mb-xs">
          Lower-scoring words: {{ uncertain.map((word) => word.text).join(", ") }}
        </p>
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
      <div
        v-if="editing !== i"
        class="flex flex-wrap justify-end gap-xs shrink-0 max-w-24 sm:max-w-none"
      >
        <Button
          variant="ghost"
          shape="circle"
          icon="edit"
          :title="`Edit segment ${i + 1}`"
          :aria-label="`Edit segment ${i + 1}`"
          :disabled="!canEdit || editing !== null"
          @click="startTextEdit(i)"
        />
        <Button
          variant="ghost"
          shape="circle"
          icon="flag"
          :title="`${u.needs_review ? 'Clear review mark for' : 'Mark for review:'} segment ${i + 1}`"
          :aria-label="`${u.needs_review ? 'Clear review mark for' : 'Mark for review:'} segment ${i + 1}`"
          :aria-pressed="!!u.needs_review"
          :disabled="!canEdit || editing !== null"
          @click="segmentAction(i, 'review')"
        />
        <Button
          variant="ghost"
          shape="circle"
          icon="refresh"
          :title="`Retry recognition for segment ${i + 1}`"
          :aria-label="`Retry recognition for segment ${i + 1}`"
          :disabled="!canEdit || !config || editing !== null || u.end_ms <= u.start_ms"
          @click="segmentAction(i, 'retry')"
        />
        <Button
          variant="ghost"
          shape="circle"
          icon="delete"
          :title="`Delete segment ${i + 1}`"
          :aria-label="`Delete segment ${i + 1}`"
          :disabled="!canEdit || editing !== null"
          @click="segmentAction(i, 'delete')"
        />
      </div>
    </article>
  </Modal>
  <Modal
    :open="speaker !== null"
    title="Rename speaker"
    :backdrop-close="!saving"
    @close="closeSpeaker"
  >
    <ErrorBanner v-if="localError">{{ localError }}</ErrorBanner>
    <label class="block text-labelSmall text-on-surface-variant">
      Apply to
      <select v-model="speakerScope" :class="fieldClass" :disabled="saving">
        <option value="all" :disabled="!speaker">All segments for this speaker</option>
        <option value="segment">This segment only</option>
      </select>
    </label>
    <label class="block text-labelSmall text-on-surface-variant">
      Speaker name
      <input
        ref="speakerInput"
        autofocus
        v-model="speakerDraft"
        :class="fieldClass"
        :disabled="saving"
        list="transcript-speaker-names"
        @keydown.enter.prevent="renameSpeaker"
      />
      <datalist id="transcript-speaker-names">
        <option v-for="name in speakerNames" :key="name" :value="name" />
      </datalist>
    </label>
    <template #footer>
      <Button :disabled="saving" @click="closeSpeaker">Cancel</Button>
      <Button variant="primary" :disabled="saving || !speakerDraft.trim()" @click="renameSpeaker">{{
        saving ? "Saving…" : "Rename"
      }}</Button>
    </template>
  </Modal>
  <Modal :open="replacing" title="Find and replace" :backdrop-close="!saving" @close="closeReplace">
    <p class="text-bodyMedium text-on-surface-variant">
      Replace matching text throughout this transcript. Matches are case-sensitive; speaker names
      and timestamps stay unchanged.
    </p>
    <ErrorBanner v-if="localError">{{ localError }}</ErrorBanner>
    <label class="block text-labelSmall text-on-surface-variant"
      >Find
      <input v-model="searchText" :class="fieldClass" :disabled="saving" />
    </label>
    <label class="block text-labelSmall text-on-surface-variant"
      >Replace with
      <input v-model="replacementText" :class="fieldClass" :disabled="saving" />
    </label>
    <p role="status" class="text-bodyMedium text-on-surface-variant">
      {{ matchCount }} matches in {{ replacements.length }} segments
    </p>
    <div v-if="replacements.length" class="max-h-48 overflow-y-auto flex flex-col gap-xs">
      <p class="text-labelSmall text-on-surface-variant">
        Preview (first {{ Math.min(3, replacements.length) }} segments)
      </p>
      <p
        v-for="row in replacements.slice(0, 3)"
        :key="row.index"
        class="text-bodyMedium text-on-surface whitespace-pre-wrap"
      >
        {{ row.text }}
      </p>
    </div>
    <template #footer>
      <Button :disabled="saving" @click="closeReplace">Cancel</Button>
      <Button variant="primary" :disabled="saving || !matchCount" @click="replaceAll">{{
        saving ? "Saving…" : "Replace all"
      }}</Button>
    </template>
  </Modal>
</template>
