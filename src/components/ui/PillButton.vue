<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "outline" | "danger";
    mobileTall?: boolean;
    icon?: string;
    iconSize?: number;
  }>(),
  { variant: "outline", mobileTall: false, iconSize: 16 },
);

const variantClass = computed(
  () =>
    ({
      primary: "bg-primary text-on-primary hover:bg-primary-fixed-dim",
      outline: "border border-outline-variant text-on-surface hover:bg-surface-container-high",
      danger: "border border-error/60 text-error hover:bg-error/10",
    })[props.variant],
);
</script>

<template>
  <button
    type="button"
    class="px-md py-xs rounded-full text-titleSmall transition-colors inline-flex items-center gap-unit disabled:opacity-50"
    :class="[variantClass, mobileTall ? 'min-h-11 md:min-h-0' : '']"
  >
    <Icon v-if="icon" :name="icon" :size="iconSize" />
    <slot />
  </button>
</template>
