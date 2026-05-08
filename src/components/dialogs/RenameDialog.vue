<script setup lang="ts">
import Modal from "../ui/Modal.vue";
import Button from "../ui/Button.vue";
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
        <Button @click="open = false">Cancel</Button>
        <Button variant="primary" @click="emit('commit')">Rename</Button>
      </div>
    </template>
  </Modal>
</template>
