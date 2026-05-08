<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";

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

const collapsedPx = computed(() => props.minHeight);
const openThresholdPx = computed(() => props.minHeight + 16);

const stored = (() => {
  if (typeof window === "undefined" || !props.storageKey) return null;
  const v = Number(localStorage.getItem(props.storageKey) ?? "");
  return Number.isFinite(v) && v >= collapsedPx.value ? v : null;
})();

const heightPx = ref(stored ?? props.initialHeight ?? collapsedPx.value);
const contentHeightPx = ref(0);
const contentEl = ref<HTMLElement | null>(null);
const resizing = ref(false);

const computedMaxPx = computed(() => {
  if (props.autoMax) return props.headerHeight + contentHeightPx.value;
  if (props.maxHeight) return props.maxHeight;
  return Number.MAX_SAFE_INTEGER;
});

const expanded = computed(() => heightPx.value > openThresholdPx.value);

watch(heightPx, (v) => {
  emit("update:height", v);
  emit("update:expanded", v > openThresholdPx.value);
  if (typeof window !== "undefined" && props.storageKey)
    localStorage.setItem(props.storageKey, String(Math.round(v)));
});

watch(contentEl, (el, _prev, onCleanup) => {
  if (!el || typeof window === "undefined" || !props.autoMax) return;
  const measure = () => {
    const h = el.scrollHeight;
    if (h <= 0) return;
    contentHeightPx.value = h;
    if (heightPx.value > computedMaxPx.value) heightPx.value = computedMaxPx.value;
  };
  const ro = new ResizeObserver(measure);
  ro.observe(el);
  measure();
  onCleanup(() => ro.disconnect());
});

function beginResize(ev: PointerEvent) {
  ev.preventDefault();
  resizing.value = true;
  const startY = ev.clientY;
  const startPx = heightPx.value;
  let dragged = false;
  let lastY = startY;
  let lastT = ev.timeStamp;
  let velocity = 0;
  const move = (e: PointerEvent) => {
    const delta = startY - e.clientY;
    if (Math.abs(delta) > 3) dragged = true;
    const dt = e.timeStamp - lastT;
    if (dt > 0) velocity = (lastY - e.clientY) / dt;
    lastY = e.clientY;
    lastT = e.timeStamp;
    heightPx.value = Math.max(collapsedPx.value, Math.min(computedMaxPx.value, startPx + delta));
  };
  const up = () => {
    resizing.value = false;
    const moved = heightPx.value - startPx;
    if (!dragged) {
      heightPx.value = startPx > openThresholdPx.value ? collapsedPx.value : computedMaxPx.value;
    } else if (Math.abs(velocity) > 0.3) {
      heightPx.value = velocity > 0 ? computedMaxPx.value : collapsedPx.value;
    } else if (moved > 0) {
      heightPx.value = computedMaxPx.value;
    } else if (moved < 0) {
      heightPx.value = collapsedPx.value;
    } else {
      heightPx.value = startPx > openThresholdPx.value ? computedMaxPx.value : collapsedPx.value;
    }
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

if (typeof window !== "undefined") {
  onUnmounted(() => {
    /* noop, handlers are removed in up() */
  });
}

defineExpose({ heightPx, expanded, beginResize });
</script>

<template>
  <aside
    :class="[
      bgClass,
      bordered ? 'border-t border-outline-variant/40' : '',
      'w-full flex flex-col shrink-0 overflow-hidden touch-none relative',
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
      class="flex-1 min-h-0 overflow-hidden"
      :style="{ maxHeight: `${Math.max(0, heightPx - headerHeight)}px` }"
    >
      <div ref="contentEl" class="px-md pt-md pb-md space-y-md overflow-y-auto scroll-thin">
        <slot></slot>
      </div>
    </div>
  </aside>
</template>
