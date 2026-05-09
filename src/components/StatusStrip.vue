<script setup lang="ts">
import { computed } from "vue";
import type { DirEntry, TranscribeProgress, Transcript } from "@/types";
import { phaseLabel, prettyName } from "@utils/audio";
import { fmtClock, fmtMs, fmtMsLong, fmtBytes } from "@composables/format";
import Icon from "@components/ui/Icon.vue";

const props = defineProps<{
  recording: boolean;
  recElapsed: string;
  status: "idle" | "running" | "renaming" | "error";
  selectedEntry: DirEntry | null;
  progress: TranscribeProgress | null;
  transcript: Transcript | null;
  audioCount: number;
  transcribedCount: number;
}>();

const todayLabel = computed(() =>
  new Date().toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
  }),
);

const isLiveProgress = computed(
  () =>
    !!props.progress &&
    !!props.selectedEntry &&
    props.progress.path === props.selectedEntry.path &&
    props.progress.phase !== "done",
);
</script>

<template>
  <div
    class="shrink-0 border-b border-outline-variant/40 bg-surface-container-low px-margin py-xs flex items-center gap-xs font-mono text-labelSmall overflow-hidden"
  >
    <template v-if="recording">
      <span class="w-1.5 h-1.5 rounded-full bg-error animate-pulse shrink-0"></span>
      <span class="text-error uppercase tracking-wide">REC</span>
      <span class="text-on-surface ml-auto">{{ recElapsed }}</span>
    </template>
    <template v-else-if="isLiveProgress && progress">
      <span class="w-1.5 h-1.5 rounded-full bg-secondary animate-pulse shrink-0"></span>
      <span class="text-on-surface-variant shrink-0">{{ phaseLabel(progress.phase) }}</span>
      <span class="text-secondary shrink-0 ml-auto">
        <template v-if="progress.phase === 'transcribing' || progress.phase === 'diarizing'">
          {{ progress.displayPct.toFixed(0) }}% · {{ fmtClock(progress.elapsedSec) }} / ETA
          {{ fmtClock(progress.etaSec) }}
        </template>
        <template v-else>{{ fmtClock(progress.elapsedSec) }}</template>
      </span>
    </template>
    <template v-else-if="transcript">
      <Icon name="check_circle" :size="14" class="text-tertiary shrink-0" />
      <span class="text-on-surface-variant shrink-0">ready</span>
      <span class="text-on-surface-variant shrink-0 ml-auto">
        {{ fmtMsLong(transcript.duration_ms) }} · {{ transcript.utterances.length }} utt ·
        {{ transcript.speakers_detected }} spk
      </span>
    </template>
    <template v-else-if="selectedEntry">
      <Icon name="graphic_eq" :size="14" class="text-on-surface-variant shrink-0" />
      <span class="text-on-surface truncate min-w-0">
        {{ prettyName(selectedEntry.name).display }}
      </span>
      <span class="text-on-surface-variant shrink-0 ml-auto">
        {{ selectedEntry.duration_ms ? fmtMs(selectedEntry.duration_ms) : "—" }} ·
        {{ fmtBytes(selectedEntry.size_bytes) }}
        <template v-if="selectedEntry.cache_key"> · transcribed </template>
      </span>
    </template>
    <template v-else>
      <Icon name="today" :size="14" class="text-on-surface-variant shrink-0" />
      <span class="text-on-surface-variant shrink-0">{{ todayLabel }}</span>
      <span class="text-on-surface-variant shrink-0 ml-auto">
        {{ audioCount }} {{ audioCount === 1 ? "file" : "files" }}
        <template v-if="transcribedCount > 0"> · {{ transcribedCount }} transcribed </template>
      </span>
    </template>
  </div>
</template>
