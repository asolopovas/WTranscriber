<script setup lang="ts">
import TranscribeIcon from "./icons/TranscribeIcon.vue";
import SettingsIcon from "./icons/SettingsIcon.vue";
import LogIcon from "./icons/LogIcon.vue";
import { TABS, type Tab } from "./nav-tabs";

const model = defineModel<Tab>({ required: true });
</script>

<template>
  <nav class="md:hidden flex items-stretch shrink-0 border-t border-outline-variant/40 bg-surface">
    <button
      v-for="t in TABS"
      :key="t.id"
      class="flex-1 flex flex-col items-center justify-center h-14 transition-colors"
      :class="model === t.id ? 'text-primary' : 'text-on-surface-variant hover:text-on-surface'"
      :title="t.label"
      :aria-label="t.label"
      :aria-current="model === t.id ? 'page' : undefined"
      @click="model = t.id"
    >
      <TranscribeIcon v-if="t.id === 'transcribe'" :size="26" />
      <SettingsIcon v-else-if="t.id === 'settings'" :size="26" />
      <LogIcon v-else :size="26" />
    </button>
  </nav>
</template>
