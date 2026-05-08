<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { usePanelResize } from "@composables/usePanelResize";

const props = withDefaults(
  defineProps<{
    storageKey?: string;
    headerHeight?: number;
    minHeight?: number;
    maxHeight?: number;
    autoMax?: boolean;
    initialHeight?: number;
    capCalc?: string;
    bordered?: boolean;
    bgClass?: string;
  }>(),
  {
    headerHeight: 56,
    minHeight: 56,
    autoMax: false,
    bordered: true,
    bgClass: "bg-surface-container",
    capCalc: "calc(100% - 96px)",
  },
);

const emit = defineEmits<{
  (e: "update:height", v: number): void;
  (e: "update:expanded", v: boolean): void;
}>();

const { heightPx, expanded, resizing, beginResize, observeContent } = usePanelResize({
  storageKey: props.storageKey,
  headerHeight: props.headerHeight,
  minHeight: props.minHeight,
  maxHeight: props.autoMax ? undefined : (props.maxHeight ?? Number.MAX_SAFE_INTEGER),
  initialHeight: props.initialHeight,
});

const contentEl = ref<HTMLElement | null>(null);
watch(contentEl, (el, _prev, onCleanup) => {
  if (!props.autoMax) return;
  onCleanup(observeContent(el));
});

watch(heightPx, (v) => emit("update:height", v));
watch(expanded, (v) => emit("update:expanded", v));

const bodyMaxHeight = computed(() => `${Math.max(0, heightPx.value - props.headerHeight)}px`);

defineExpose({ heightPx, expanded, beginResize });
</script>

<template>
  <aside
    :class="[
      bgClass,
      bordered ? 'border-t border-outline-variant/40' : '',
      'w-full flex flex-col shrink-0 overflow-hidden relative',
      resizing ? '' : 'transition-[max-height,height] duration-200 ease-out',
    ]"
    :style="{
      maxHeight: `min(${heightPx}px, ${capCalc})`,
      height: `min(${heightPx}px, ${capCalc})`,
    }"
  >
    <div
      :class="[
        'shrink-0 w-full flex items-center justify-between relative select-none cursor-row-resize touch-none transition-colors',
        resizing ? 'bg-primary/15' : 'active:bg-primary/10',
      ]"
      :style="{ height: `${headerHeight}px` }"
      role="button"
      :aria-expanded="expanded"
      @pointerdown="beginResize"
    >
      <span
        class="absolute top-1 left-1/2 -translate-x-1/2 w-12 h-1 rounded-full transition-colors pointer-events-none"
        :class="resizing ? 'bg-primary' : 'bg-outline-variant'"
      ></span>
      <slot name="header" :expanded="expanded" :resizing="resizing"></slot>
    </div>

    <div
      class="flex-1 min-h-0 overflow-y-auto overscroll-contain scroll-thin"
      :style="{ maxHeight: bodyMaxHeight }"
    >
      <div ref="contentEl" class="px-md pt-md pb-md space-y-md">
        <slot></slot>
      </div>
    </div>
  </aside>
</template>
