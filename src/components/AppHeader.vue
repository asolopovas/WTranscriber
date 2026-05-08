<script setup lang="ts">
import { TABS, type Tab } from "@components/nav-tabs";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";

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
</script>

<template>
  <header
    class="flex justify-between items-center w-full px-margin h-14 md:h-16 shrink-0 border-b border-outline-variant/40 bg-surface gap-xs"
  >
    <div class="flex items-center gap-xs">
      <Icon name="graphic_eq" :size="24" class="text-primary" />
      <span
        class="font-mono tracking-tighter font-bold text-primary text-labelMedium ml-xs uppercase"
      >
        wt
      </span>
    </div>
    <nav
      class="hidden md:flex items-center gap-md md:gap-xl h-full overflow-x-auto scroll-thin min-w-0"
    >
      <Button v-for="t in TABS" :key="t.id" shape="tab" :active="tab === t.id" @click="tab = t.id">
        {{ t.label }}
      </Button>
    </nav>
    <div class="flex items-center gap-xs shrink-0">
      <Button
        v-if="showTranscribeActions && pendingCount > 0"
        variant="ghost"
        shape="circle"
        size="md"
        icon="playlist_play"
        :icon-size="22"
        :disabled="queueActive"
        @click="emit('transcribe-all')"
        :title="
          queueActive
            ? `Transcribing ${queueDone + 1}/${queueTotal}`
            : `Transcribe all (${pendingCount})`
        "
        aria-label="Transcribe all untranscribed files"
      />
      <Button
        v-if="showTranscribeActions"
        variant="primary"
        shape="circle"
        size="md"
        icon="add"
        :icon-size="22"
        @click="emit('pick-audio')"
        title="Add audio file(s) to working folder"
        aria-label="Add audio files"
      />
      <Button variant="ghost" shape="circle" size="md" class="-mr-xs" aria-label="More options">
        <span class="font-mono text-labelSmall hidden sm:inline">v{{ version }}</span>
        <Icon name="more_vert" :size="22" />
      </Button>
    </div>
  </header>
</template>
