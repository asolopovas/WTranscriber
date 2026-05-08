<script setup lang="ts">
const model = defineModel<boolean>({ required: true });
defineProps<{ label?: string; ariaLabel?: string }>();
</script>

<template>
  <button
    type="button"
    role="switch"
    :aria-checked="model"
    :aria-label="ariaLabel ?? label"
    class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
    :class="model ? 'bg-primary' : 'bg-surface-container-highest border border-outline-variant'"
    @click="model = !model"
  >
    <span
      class="absolute top-1 w-4 h-4 rounded-full transition-all"
      :class="model ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
    ></span>
  </button>
</template>
