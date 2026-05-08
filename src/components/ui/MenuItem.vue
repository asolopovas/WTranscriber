<script setup lang="ts">
import Icon from "@components/ui/Icon.vue";

withDefaults(
  defineProps<{
    icon?: string;
    iconSize?: number;
    tone?: "default" | "danger";
    disabled?: boolean;
  }>(),
  { iconSize: 18, tone: "default", disabled: false },
);
</script>

<template>
  <button
    type="button"
    :disabled="disabled"
    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium transition-colors disabled:opacity-50"
    :class="
      tone === 'danger'
        ? 'text-error hover:bg-error-container/40'
        : 'text-on-surface hover:bg-surface-container-highest'
    "
  >
    <slot name="icon">
      <Icon v-if="icon" :name="icon" :size="iconSize" />
    </slot>
    <slot />
  </button>
</template>
