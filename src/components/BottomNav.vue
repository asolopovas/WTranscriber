<script setup lang="ts">
import TranscribeIcon from "./icons/TranscribeIcon.vue";
import SettingsIcon from "./icons/SettingsIcon.vue";
import LogIcon from "./icons/LogIcon.vue";
import Button from "./ui/Button.vue";
import { TABS, type Tab } from "./nav-tabs";

const model = defineModel<Tab>({ required: true });
</script>

<template>
  <nav class="md:hidden flex items-stretch shrink-0 border-t border-outline-variant/40 bg-surface">
    <Button
      v-for="t in TABS"
      :key="t.id"
      shape="tab-mobile"
      :active="model === t.id"
      :title="t.label"
      :aria-label="t.label"
      :aria-current="model === t.id ? 'page' : undefined"
      @click="model = t.id"
    >
      <TranscribeIcon v-if="t.id === 'transcribe'" :size="26" />
      <SettingsIcon v-else-if="t.id === 'settings'" :size="26" />
      <LogIcon v-else :size="26" />
    </Button>
  </nav>
</template>
