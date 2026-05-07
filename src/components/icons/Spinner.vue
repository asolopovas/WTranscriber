<script setup lang="ts">
defineProps<{ size?: number | string }>();
</script>

<template>
  <svg
    class="spinner"
    xmlns="http://www.w3.org/2000/svg"
    :width="size ?? 24"
    :height="size ?? 24"
    viewBox="0 0 24 24"
    fill="currentColor"
    aria-hidden="true"
  >
    <rect class="bar bar-1" width="2.8" height="22" x="1" y="1" rx="0.5" />
    <rect class="bar bar-2" width="2.8" height="22" x="5.8" y="1" rx="0.5" />
    <rect class="bar bar-3" width="2.8" height="22" x="10.6" y="1" rx="0.5" />
    <rect class="bar bar-4" width="2.8" height="22" x="15.4" y="1" rx="0.5" />
    <rect class="bar bar-5" width="2.8" height="22" x="20.2" y="1" rx="0.5" />
  </svg>
</template>

<style scoped>
.spinner .bar {
  transform-box: fill-box;
  transform-origin: center;
  animation: bar-scale 1s ease-in-out infinite;
}
.spinner .bar-1 {
  animation-delay: 0.4s;
}
.spinner .bar-2 {
  animation-delay: 0.2s;
}
.spinner .bar-3 {
  animation-delay: 0s;
}
.spinner .bar-4 {
  animation-delay: 0.2s;
}
.spinner .bar-5 {
  animation-delay: 0.4s;
}
@keyframes bar-scale {
  0%,
  100% {
    transform: scaleY(0.45);
  }
  50% {
    transform: scaleY(1);
  }
}
</style>
