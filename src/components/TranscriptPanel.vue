<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import type { Transcript } from "@/types";
import { api } from "@/api";
import { copyTextToClipboard } from "@utils/clipboard";
import { fmtMs as fmt } from "@utils/format";
import SlidingPanel from "@components/SlidingPanel.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";

const props = defineProps<{ transcript: Transcript; cacheKey?: string | null }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "rename-speaker", payload: { old: string; name: string }): void;
}>();

const editing = ref<string | null>(null);
const draft = ref("");
const inputRef = ref<HTMLInputElement | null>(null);

const canRename = computed(() => !!props.cacheKey);

async function startEdit(speaker: string) {
  if (!canRename.value) return;
  editing.value = speaker;
  draft.value = speaker;
  await nextTick();
  inputRef.value?.focus();
  inputRef.value?.select();
}

function commit() {
  const old = editing.value;
  editing.value = null;
  if (!old) return;
  const name = draft.value.trim();
  if (!name || name === old) return;
  emit("rename-speaker", { old, name });
}

function cancel() {
  editing.value = null;
}

const copied = ref(false);

async function copyTranscript() {
  const text = await api.formatTranscript(props.transcript, "txt");
  await copyTextToClipboard(text);
  copied.value = true;
  setTimeout(() => {
    copied.value = false;
  }, 1500);
}
</script>

<template>
  <SlidingPanel storage-key="wt.transcriptHeightPx" :initial-height="360" :auto-max="true">
    <template #header>
      <h3 class="text-titleSmall text-on-surface flex items-center gap-xs pl-md">
        <Icon name="subtitles" :size="18" class="text-primary" />
        Transcript
      </h3>
      <div class="flex items-center gap-xs mr-md">
        <Button
          variant="ghost"
          shape="circle"
          size="sm"
          :icon="copied ? 'check' : 'content_copy'"
          :icon-size="18"
          :title="copied ? 'Copied' : 'Copy transcript'"
          :aria-label="copied ? 'Copied' : 'Copy transcript'"
          @pointerdown.stop
          @click.stop="copyTranscript"
        />
        <Button
          variant="ghost"
          shape="circle"
          size="sm"
          icon="close"
          :icon-size="18"
          title="Close transcript"
          aria-label="Close transcript"
          @pointerdown.stop
          @click.stop="emit('close')"
        />
      </div>
    </template>
    <article
      v-for="(u, i) in transcript.utterances"
      :key="i"
      class="flex gap-xs items-start group hover:bg-surface-container-high/30 -mx-xs px-xs py-unit rounded transition-colors"
    >
      <span class="font-mono text-labelSmall text-secondary w-12 shrink-0 pt-unit">
        {{ fmt(u.start_ms) }}
      </span>
      <div class="flex-1 min-w-0">
        <div v-if="u.speaker" class="mb-unit">
          <input
            v-if="editing === u.speaker"
            ref="inputRef"
            v-model="draft"
            class="font-mono text-labelSmall text-primary bg-surface-container-high border border-outline-variant rounded px-unit py-0 outline-none focus:border-primary"
            @keydown.enter.prevent="commit"
            @keydown.escape.prevent="cancel"
            @blur="commit"
          />
          <button
            v-else
            type="button"
            class="font-mono text-labelSmall text-primary hover:underline cursor-pointer"
            :title="canRename ? 'Rename speaker' : ''"
            :disabled="!canRename"
            @click="startEdit(u.speaker)"
          >
            {{ u.speaker }}
          </button>
        </div>
        <p
          class="text-bodyMedium text-on-surface-variant group-hover:text-on-surface transition-colors leading-relaxed"
        >
          {{ u.text }}
        </p>
      </div>
    </article>
  </SlidingPanel>
</template>
