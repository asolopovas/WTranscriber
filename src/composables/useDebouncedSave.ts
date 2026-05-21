import { onUnmounted, ref, toRaw, watch, type Ref } from "vue";

export type SaveState = "idle" | "saving" | "saved" | "error";

function snapshot<T>(value: T): T {
  const raw = toRaw(value);
  if (typeof structuredClone === "function") {
    try {
      return structuredClone(raw);
    } catch {
      return JSON.parse(JSON.stringify(raw)) as T;
    }
  }
  return JSON.parse(JSON.stringify(raw)) as T;
}

export function useDebouncedSave<T>(
  source: Ref<T | null>,
  save: (value: T) => Promise<void>,
  options: { delayMs?: number; resetMs?: number } = {},
) {
  const { delayMs = 250, resetMs = 1500 } = options;
  const state = ref<SaveState>("idle");
  const error = ref<string | null>(null);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let resetTimer: ReturnType<typeof setTimeout> | null = null;
  let pending: T | undefined;
  let saving = false;
  let scheduled = false;

  function armReset() {
    if (resetTimer) clearTimeout(resetTimer);
    resetTimer = setTimeout(() => {
      if (state.value === "saved") state.value = "idle";
    }, resetMs);
  }

  async function flush() {
    if (saving || pending === undefined) return;
    const value = pending;
    pending = undefined;
    saving = true;
    try {
      await save(value);
      if (pending === undefined && !scheduled) {
        state.value = "saved";
        error.value = null;
        armReset();
      }
    } catch (e) {
      if (pending === undefined && !scheduled) {
        error.value = String(e);
        state.value = "error";
      }
    } finally {
      saving = false;
      if (pending !== undefined) void flush();
    }
  }

  const stop = watch(
    source,
    (next) => {
      if (!next) return;
      if (saveTimer) clearTimeout(saveTimer);
      if (resetTimer) clearTimeout(resetTimer);
      state.value = "saving";
      const nextSnapshot = snapshot(next);
      scheduled = true;
      saveTimer = setTimeout(() => {
        scheduled = false;
        pending = nextSnapshot;
        void flush();
      }, delayMs);
    },
    { deep: true },
  );

  onUnmounted(() => {
    stop();
    if (saveTimer) clearTimeout(saveTimer);
    if (resetTimer) clearTimeout(resetTimer);
  });

  return { state, error };
}
