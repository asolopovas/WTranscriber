<script setup lang="ts">
withDefaults(
  defineProps<{
    name: string;
    size?: number | string;
    fill?: boolean;
  }>(),
  { size: 18, fill: false },
);
</script>

<template>
  <span
    class="material-symbols-outlined"
    :class="fill ? 'fill' : ''"
    :style="{ fontSize: typeof size === 'number' ? `${size}px` : size }"
    aria-hidden="true"
    >{{ name }}</span
  >
</template>
