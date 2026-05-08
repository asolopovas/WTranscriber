<script setup lang="ts">
import { computed } from "vue";

interface Props {
  percent: number;
  size?: number;
  done?: boolean;
  errored?: boolean;
}
const props = withDefaults(defineProps<Props>(), {
  size: 40,
  done: false,
  errored: false,
});

const radius = computed(() => props.size / 2 - 3);
const circumference = computed(() => 2 * Math.PI * radius.value);
const offset = computed(() => {
  const p = Math.max(0, Math.min(100, props.percent));
  return circumference.value * (1 - p / 100);
});
const center = computed(() => props.size / 2);
const stroke = computed(() =>
  props.errored ? "var(--md-sys-color-error)" : "var(--md-sys-color-primary)",
);
const viewBox = computed(() => `0 0 ${props.size} ${props.size}`);
</script>

<template>
  <div class="dl-circle relative shrink-0" :style="{ width: size + 'px', height: size + 'px' }">
    <svg :width="size" :height="size" :viewBox="viewBox" class="-rotate-90">
      <circle
        :cx="center"
        :cy="center"
        :r="radius"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        class="text-on-surface/15"
      />
      <circle
        :cx="center"
        :cy="center"
        :r="radius"
        fill="none"
        :stroke="stroke"
        stroke-width="3"
        stroke-linecap="round"
        :stroke-dasharray="circumference"
        :stroke-dashoffset="offset"
        class="dl-progress"
      />
    </svg>
    <span
      v-if="done"
      class="absolute inset-0 grid place-items-center material-symbols-outlined text-primary"
      :style="{ fontSize: size * 0.55 + 'px' }"
      >check</span
    >
    <span
      v-else-if="errored"
      class="absolute inset-0 grid place-items-center material-symbols-outlined text-error"
      :style="{ fontSize: size * 0.55 + 'px' }"
      >close</span
    >
    <span
      v-else
      class="absolute inset-0 grid place-items-center font-mono text-on-surface/80 tabular-nums"
      :style="{ fontSize: size * 0.28 + 'px' }"
      >{{ Math.round(percent) }}</span
    >
  </div>
</template>

<style scoped>
.dl-progress {
  transition:
    stroke-dashoffset 220ms cubic-bezier(0.22, 1, 0.36, 1),
    stroke 220ms ease;
}
.dl-circle:not(:has(.material-symbols-outlined)) .dl-progress {
  animation: dl-pulse 1.6s ease-in-out infinite;
}
@keyframes dl-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.55;
  }
}
</style>
