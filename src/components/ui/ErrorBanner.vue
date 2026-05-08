<script setup lang="ts">
import Icon from "@components/ui/Icon.vue";

withDefaults(
  defineProps<{
    icon?: string;
    iconSize?: number;
    layout?: "block" | "inline";
  }>(),
  { iconSize: 18, layout: "block" },
);
</script>

<template>
  <div
    class="p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium font-mono"
    :class="layout === 'inline' ? 'flex items-start gap-xs' : ''"
  >
    <Icon v-if="icon" :name="icon" :size="iconSize" class="mt-[1px] shrink-0" />
    <div :class="layout === 'inline' ? 'flex-1 min-w-0' : ''">
      <slot />
    </div>
    <slot name="actions" />
  </div>
</template>
