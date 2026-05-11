<script setup lang="ts">
import { computed } from "vue";
import { TABS, type Tab } from "@components/nav-tabs";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";

defineProps<{
  showTranscribeActions: boolean;
  pendingCount: number;
  queueActive: boolean;
  queueDone: number;
  queueTotal: number;
  showLogControls?: boolean;
}>();

const emit = defineEmits<{
  (e: "transcribe-all"): void;
  (e: "pick-audio"): void;
  (e: "log-refresh"): void;
  (e: "log-clear"): void;
}>();

const tab = defineModel<Tab>("tab", { required: true });
const logRetain = defineModel<number>("logRetain", { default: 1 });
const logAuto = defineModel<boolean>("logAuto", { default: true });

const activeTabLabel = computed(() => TABS.find((t) => t.id === tab.value)?.label ?? "");
</script>

<template>
  <header
    class="flex justify-between items-center w-full px-margin h-14 md:h-16 shrink-0 border-b border-outline-variant/40 bg-surface gap-xs"
  >
    <div class="flex items-center gap-unit">
      <Icon name="graphic_eq" :size="24" class="text-primary" />
      <h1 class="text-headlineSmall font-semibold text-on-surface">{{ activeTabLabel }}</h1>
    </div>
    <div class="flex items-center gap-xs shrink-0">
      <template v-if="showLogControls">
        <select
          :value="logRetain"
          @change="logRetain = Number(($event.target as HTMLSelectElement).value)"
          class="h-10 min-w-23 pl-md pr-9 rounded-full border border-outline-variant text-on-surface-variant text-labelMedium bg-transparent text-center"
          title="How many recent runs to display"
        >
          <option :value="1">Latest</option>
          <option :value="5">Last 5</option>
          <option :value="20">Last 20</option>
          <option :value="0">All</option>
        </select>
        <div class="flex items-center">
          <Button
            variant="ghost"
            shape="icon"
            :icon="logAuto ? 'vertical_align_bottom' : 'pause'"
            :icon-fill="logAuto"
            :icon-size="22"
            :aria-label="logAuto ? 'Auto-scroll on' : 'Auto-scroll off'"
            :title="logAuto ? 'Auto-scroll on' : 'Auto-scroll off'"
            class="w-12 h-12"
            @click="logAuto = !logAuto"
          />
          <Button
            variant="ghost"
            shape="icon"
            icon="refresh"
            :icon-size="22"
            aria-label="Refresh"
            title="Refresh"
            class="w-12 h-12"
            @click="emit('log-refresh')"
          />
          <Button
            variant="ghost"
            shape="icon"
            icon="delete"
            :icon-size="22"
            aria-label="Clear log"
            title="Clear log"
            class="w-12 h-12 text-error hover:text-error"
            @click="emit('log-clear')"
          />
        </div>
      </template>
      <Button
        v-if="showTranscribeActions && pendingCount > 0"
        variant="ghost"
        shape="circle"
        size="md"
        icon="playlist_play"
        :icon-size="22"
        :disabled="queueActive"
        @click="emit('transcribe-all')"
        :title="
          queueActive
            ? `Transcribing ${queueDone + 1}/${queueTotal}`
            : `Transcribe all (${pendingCount})`
        "
        aria-label="Transcribe all untranscribed files"
      />
      <Button
        v-if="showTranscribeActions"
        variant="primary"
        shape="circle"
        size="md"
        icon="add"
        :icon-size="22"
        @click="emit('pick-audio')"
        title="Add audio file(s) to working folder"
        aria-label="Add audio files"
      />
    </div>
  </header>
</template>
