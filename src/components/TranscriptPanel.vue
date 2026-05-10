<script setup lang="ts">
import type { Transcript } from "@/types";
import { fmtMs as fmt } from "@utils/format";
import SlidingPanel from "@components/SlidingPanel.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";

defineProps<{ transcript: Transcript }>();
const emit = defineEmits<{ (e: "close"): void }>();
</script>

<template>
  <SlidingPanel storage-key="wt.transcriptHeightPx" :initial-height="360" :auto-max="true">
    <template #header>
      <h3 class="text-titleSmall text-on-surface flex items-center gap-xs pl-md">
        <Icon name="subtitles" :size="18" class="text-primary" />
        Transcript
      </h3>
      <Button
        variant="ghost"
        shape="circle"
        size="sm"
        icon="close"
        :icon-size="18"
        class="mr-md"
        title="Close transcript"
        aria-label="Close transcript"
        @pointerdown.stop
        @click.stop="emit('close')"
      />
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
        <div v-if="u.speaker" class="font-mono text-labelSmall text-primary mb-unit">
          {{ u.speaker }}
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
