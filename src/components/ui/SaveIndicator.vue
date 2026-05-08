<script setup lang="ts">
import type { SaveState } from "../../composables/useDebouncedSave";

defineProps<{ state: SaveState }>();

const labels: Record<SaveState, string> = {
  saving: "saving",
  saved: "saved",
  error: "error",
  idle: "synced",
};
</script>

<template>
  <span
    class="font-mono text-labelSmall flex items-center gap-unit"
    :class="{
      'text-secondary': state === 'saving',
      'text-tertiary': state === 'saved',
      'text-error': state === 'error',
      'text-outline': state === 'idle',
    }"
  >
    <span
      class="w-1.5 h-1.5 rounded-full"
      :class="{
        'bg-secondary animate-pulse': state === 'saving',
        'bg-tertiary': state === 'saved',
        'bg-error': state === 'error',
        'bg-outline-variant': state === 'idle',
      }"
    ></span>
    {{ labels[state] }}
  </span>
</template>
