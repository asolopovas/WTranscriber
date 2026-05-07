<script setup lang="ts">
defineProps<{ size?: number | string }>();
</script>

<template>
  <svg
    class="wt-spinner"
    xmlns="http://www.w3.org/2000/svg"
    :width="size ?? 24"
    :height="size ?? 24"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    <circle cx="12" cy="2.5" r="1.5" fill="currentColor" opacity=".14" />
    <circle cx="16.75" cy="3.77" r="1.5" fill="currentColor" opacity=".29" />
    <circle cx="20.23" cy="7.25" r="1.5" fill="currentColor" opacity=".43" />
    <circle cx="21.5" cy="12" r="1.5" fill="currentColor" opacity=".57" />
    <circle cx="20.23" cy="16.75" r="1.5" fill="currentColor" opacity=".71" />
    <circle cx="16.75" cy="20.23" r="1.5" fill="currentColor" opacity=".86" />
    <circle cx="12" cy="21.5" r="1.5" fill="currentColor" />
  </svg>
</template>

<style>
.wt-spinner {
  animation: wt-spin 0.75s steps(12, end) infinite;
  transform-origin: center;
}
@keyframes wt-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
