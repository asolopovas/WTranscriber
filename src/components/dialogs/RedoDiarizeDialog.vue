<script setup lang="ts">
import { computed } from "vue";
import type { DiarizerChoice, SystemInfo } from "@/types";
import Modal from "@components/ui/Modal.vue";
import Button from "@components/ui/Button.vue";
import { fieldClass } from "@styles/fields";

const props = defineProps<{ sys: SystemInfo | null }>();
const open = defineModel<boolean>("open", { required: true });
const diarizer = defineModel<DiarizerChoice>("diarizer", { required: true });
const speakers = defineModel<number>("speakers", { required: true });

const emit = defineEmits<{ (e: "commit"): void }>();

const speakerCap = computed(() => (diarizer.value === "nemo" ? 4 : 10));
const speakerOptions = computed<{ value: number; label: string }[]>(() => {
  const opts: { value: number; label: string }[] = [{ value: 0, label: "Auto" }];
  for (let i = 1; i <= speakerCap.value; i++) opts.push({ value: i, label: String(i) });
  return opts;
});

function onDiarizerChange(value: DiarizerChoice) {
  diarizer.value = value;
  if (speakers.value > speakerCap.value) speakers.value = speakerCap.value;
}
</script>

<template>
  <Modal :open="open" title="Re-diarize" @close="open = false">
    <div class="space-y-md">
      <p class="text-bodySmall text-on-surface-variant">
        Reuses the existing transcript text and reassigns speaker labels using the chosen diarizer.
      </p>
      <div class="grid grid-cols-2 gap-md">
        <label class="space-y-unit">
          <span class="text-labelSmall text-on-surface-variant">Diarizer</span>
          <select
            :value="diarizer"
            :class="fieldClass"
            @change="onDiarizerChange(($event.target as HTMLSelectElement).value as DiarizerChoice)"
          >
            <option v-if="!props.sys?.is_mobile" value="nemo">NVIDIA NeMo Sortformer</option>
            <option value="eres2net">3D-Speaker ERes2Net</option>
            <option value="titanet">NVIDIA TitaNet</option>
          </select>
        </label>
        <label class="space-y-unit">
          <span class="text-labelSmall text-on-surface-variant">Speakers</span>
          <select v-model.number="speakers" :class="fieldClass">
            <option v-for="opt in speakerOptions" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </option>
          </select>
        </label>
      </div>
    </div>
    <template #footer>
      <span></span>
      <div class="flex gap-xs">
        <Button @click="open = false">Cancel</Button>
        <Button variant="primary" @click="emit('commit')">Re-diarize</Button>
      </div>
    </template>
  </Modal>
</template>
