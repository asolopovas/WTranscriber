<script setup lang="ts">
import Icon from "@components/ui/Icon.vue";

withDefaults(
  defineProps<{
    icon?: string;
    iconColor?: string;
    title?: string;
  }>(),
  { iconColor: "text-primary" },
);
</script>

<template>
  <section class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden">
    <header
      v-if="title || $slots.header"
      class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
    >
      <slot name="header">
        <Icon v-if="icon" :name="icon" :size="22" :class="iconColor" />
        <h2 class="text-titleMedium text-on-surface">{{ title }}</h2>
      </slot>
    </header>
    <slot />
  </section>
</template>
