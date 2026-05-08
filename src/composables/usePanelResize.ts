import { computed, ref, watch, type Ref } from "vue";

export interface PanelResizeOptions {
  storageKey?: string;
  headerHeight?: number;
  minHeight?: number;
  maxHeight?: number | Ref<number>;
  initialHeight?: number;
}

export function usePanelResize(opts: PanelResizeOptions = {}) {
  const headerHeight = opts.headerHeight ?? 56;
  const minHeight = opts.minHeight ?? headerHeight;
  const openThreshold = minHeight + 16;

  const stored = (() => {
    if (typeof window === "undefined" || !opts.storageKey) return null;
    const v = Number(localStorage.getItem(opts.storageKey) ?? "");
    return Number.isFinite(v) && v >= minHeight ? v : null;
  })();

  const heightPx = ref(stored ?? opts.initialHeight ?? minHeight);
  const contentHeightPx = ref(0);
  const resizing = ref(false);

  const maxHeightPx = computed(() => {
    if (opts.maxHeight !== undefined) {
      return typeof opts.maxHeight === "number" ? opts.maxHeight : opts.maxHeight.value;
    }
    return headerHeight + contentHeightPx.value;
  });
  const expanded = computed(() => heightPx.value > openThreshold);

  if (opts.storageKey) {
    watch(heightPx, (v) => {
      if (typeof window !== "undefined")
        localStorage.setItem(opts.storageKey!, String(Math.round(v)));
    });
  }

  function observeContent(el: HTMLElement | null): () => void {
    if (!el || typeof window === "undefined") return () => {};
    const measure = () => {
      const h = el.scrollHeight;
      if (h <= 0) return;
      contentHeightPx.value = h;
      if (heightPx.value > maxHeightPx.value) heightPx.value = maxHeightPx.value;
    };
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    measure();
    return () => ro.disconnect();
  }

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
      heightPx.value = Math.max(minHeight, Math.min(maxHeightPx.value, startPx + delta));
    };
    const up = () => {
      resizing.value = false;
      const moved = heightPx.value - startPx;
      if (!dragged) {
        heightPx.value = startPx > openThreshold ? minHeight : maxHeightPx.value;
      } else if (Math.abs(velocity) > 0.3) {
        heightPx.value = velocity > 0 ? maxHeightPx.value : minHeight;
      } else {
        heightPx.value = moved >= 0 ? maxHeightPx.value : minHeight;
      }
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }

  return {
    heightPx,
    expanded,
    resizing,
    maxHeightPx,
    headerHeight,
    minHeight,
    observeContent,
    beginResize,
  };
}
