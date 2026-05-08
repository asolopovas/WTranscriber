<script setup lang="ts">
import type { Tab } from "@components/BottomNav.vue";

defineProps<{
  version: string;
  showTranscribeActions: boolean;
  pendingCount: number;
  queueActive: boolean;
  queueDone: number;
  queueTotal: number;
}>();

const emit = defineEmits<{
  (e: "transcribe-all"): void;
  (e: "pick-audio"): void;
}>();

const tab = defineModel<Tab>("tab", { required: true });

const tabs: { id: Tab; label: string }[] = [
  { id: "transcribe", label: "Transcribe" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];
</script>

<template>
  <header
    class="flex justify-between items-center w-full px-margin h-14 md:h-16 shrink-0 border-b border-outline-variant/40 bg-surface gap-xs"
  >
    <div class="flex items-center gap-xs">
      <span class="material-symbols-outlined text-primary text-[24px]">graphic_eq</span>
      <span
        class="font-mono tracking-tighter font-bold text-primary text-labelMedium ml-xs uppercase"
      >
        wt
      </span>
    </div>
    <nav
      class="hidden md:flex items-center gap-md md:gap-xl h-full overflow-x-auto scroll-thin min-w-0"
    >
      <button
        v-for="t in tabs"
        :key="t.id"
        @click="tab = t.id"
        class="h-full flex items-center text-titleSmall border-b-2 px-unit transition-colors whitespace-nowrap shrink-0"
        :class="
          tab === t.id
            ? 'border-primary text-on-surface'
            : 'border-transparent text-on-surface-variant hover:text-on-surface'
        "
      >
        {{ t.label }}
      </button>
    </nav>
    <div class="flex items-center gap-xs shrink-0">
      <button
        v-if="showTranscribeActions && pendingCount > 0"
        class="w-11 h-11 inline-flex items-center justify-center rounded-full text-on-surface-variant hover:text-on-surface hover:bg-surface-container-high transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        :disabled="queueActive"
        @click="emit('transcribe-all')"
        :title="
          queueActive
            ? `Transcribing ${queueDone + 1}/${queueTotal}`
            : `Transcribe all (${pendingCount})`
        "
        aria-label="Transcribe all untranscribed files"
      >
        <span class="material-symbols-outlined text-[22px]">playlist_play</span>
      </button>
      <button
        v-if="showTranscribeActions"
        class="w-11 h-11 inline-flex items-center justify-center rounded-full bg-primary text-on-primary hover:bg-primary-fixed-dim transition-colors"
        @click="emit('pick-audio')"
        title="Add audio file(s) to working folder"
        aria-label="Add audio files"
      >
        <span class="material-symbols-outlined text-[22px]">add</span>
      </button>
      <button
        class="flex items-center justify-center w-11 h-11 -mr-xs text-on-surface-variant shrink-0 gap-xs"
        aria-label="More options"
      >
        <span class="font-mono text-labelSmall hidden sm:inline">v{{ version }}</span>
        <span class="material-symbols-outlined text-[22px]">more_vert</span>
      </button>
    </div>
  </header>
</template>
