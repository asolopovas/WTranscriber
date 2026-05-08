<script setup lang="ts">
type Tone = "success" | "info" | "muted" | "warning" | "error";

withDefaults(defineProps<{ tone?: Tone; pulse?: boolean }>(), {
  tone: "muted",
  pulse: false,
});

const dotClass: Record<Tone, string> = {
  success: "bg-tertiary",
  info: "bg-secondary",
  muted: "bg-outline-variant",
  warning: "bg-secondary",
  error: "bg-error",
};

const wrapClass: Record<Tone, string> = {
  success: "bg-tertiary-container/30 text-tertiary border border-tertiary/30",
  info: "bg-secondary-container/40 text-secondary border border-secondary/30",
  muted: "border border-outline-variant text-on-surface-variant",
  warning: "bg-secondary-container/40 text-secondary border border-secondary/30",
  error: "bg-error-container/30 text-error border border-error/30",
};
</script>

<template>
  <span
    class="inline-flex items-center gap-unit px-xs py-unit rounded-full font-mono text-labelSmall"
    :class="wrapClass[tone]"
  >
    <span
      class="w-2 h-2 rounded-full"
      :class="[dotClass[tone], pulse ? 'animate-pulse' : '']"
    ></span>
    <slot></slot>
  </span>
</template>
