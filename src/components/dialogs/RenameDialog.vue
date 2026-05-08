<script setup lang="ts">
import Modal from "../ui/Modal.vue";
import PillButton from "../ui/PillButton.vue";
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
        <PillButton @click="open = false">Cancel</PillButton>
        <PillButton variant="primary" @click="emit('commit')">Rename</PillButton>
      </div>
    </template>
  </Modal>
</template>
