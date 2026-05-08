<script setup lang="ts">
import type { ExportFormat } from "@/types";
import Modal from "@components/ui/Modal.vue";
import Button from "@components/ui/Button.vue";

const open = defineModel<boolean>("open", { required: true });
const format = defineModel<ExportFormat>("format", { required: true });

const emit = defineEmits<{ (e: "commit"): void }>();

const formats: { value: ExportFormat; label: string }[] = [
  { value: "txt", label: "Plain text (.txt)" },
  { value: "csv", label: "CSV (.csv)" },
  { value: "json", label: "JSON (.json)" },
  { value: "srt", label: "Subtitles (.srt)" },
  { value: "vtt", label: "WebVTT (.vtt)" },
];
</script>

<template>
  <Modal :open="open" title="Export transcript" @close="open = false">
    <div class="space-y-xs">
      <label
        v-for="f in formats"
        :key="f.value"
        class="flex items-center gap-xs p-xs rounded hover:bg-surface-container-high cursor-pointer"
      >
        <input v-model="format" type="radio" :value="f.value" />
        <span class="text-bodyMedium text-on-surface">{{ f.label }}</span>
      </label>
    </div>
    <template #footer>
      <span></span>
      <div class="flex gap-xs">
        <Button @click="open = false">Cancel</Button>
        <Button variant="primary" @click="emit('commit')">Save…</Button>
      </div>
    </template>
  </Modal>
</template>
