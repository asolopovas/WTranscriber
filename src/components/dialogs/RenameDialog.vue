<script setup lang="ts">
import Modal from "../ui/Modal.vue";
import { fieldClass } from "../../styles/fields";

const open = defineModel<boolean>("open", { required: true });
const value = defineModel<string>("value", { required: true });

const emit = defineEmits<{ (e: "commit"): void }>();
</script>

<template>
  <Modal :open="open" title="Rename file" @close="open = false">
    <input
      v-model="value"
      :class="fieldClass"
      @keydown.enter="emit('commit')"
      @keydown.escape="open = false"
    />
    <template #footer>
      <span></span>
      <div class="flex gap-xs">
        <button
          class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high"
          @click="open = false"
        >
          Cancel
        </button>
        <button
          class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim"
          @click="emit('commit')"
        >
          Rename
        </button>
      </div>
    </template>
  </Modal>
</template>
