import { onUnmounted, ref, watch, type Ref } from "vue";

export type SaveState = "idle" | "saving" | "saved" | "error";

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

  const stop = watch(
    source,
    (next) => {
      if (!next) return;
      if (saveTimer) clearTimeout(saveTimer);
      state.value = "saving";
      saveTimer = setTimeout(async () => {
        try {
          await save(next);
          state.value = "saved";
          error.value = null;
          if (resetTimer) clearTimeout(resetTimer);
          resetTimer = setTimeout(() => {
            if (state.value === "saved") state.value = "idle";
          }, resetMs);
        } catch (e) {
          error.value = String(e);
          state.value = "error";
        }
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
