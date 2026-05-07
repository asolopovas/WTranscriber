<script setup lang="ts">
defineProps<{ size?: number | string }>();
</script>

<template>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    :width="size ?? 22"
    :height="size ?? 22"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="m5 19l7-7m0 0l7-7m-7 7L5 5m7 7l7 7" />
  </svg>
</template>
