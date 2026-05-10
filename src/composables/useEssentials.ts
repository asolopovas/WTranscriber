import { computed, ref, type Ref } from "vue";
import { api, events } from "@/api";
import type { FileProgress, ModelInfo } from "@/types";
import { recordOmit, recordSet } from "@composables/records";

export interface EssentialsState {
  ids: Ref<string[]>;
  progress: Ref<Record<string, FileProgress>>;
  errors: Ref<Record<string, true>>;
  ready: Ref<boolean>;
  init: () => Promise<void>;
  start: () => Promise<void>;
  attachListeners: (refreshModels: (id?: string) => Promise<void>) => Promise<(() => void)[]>;
}

export function useEssentials(models: Ref<ModelInfo[]>): EssentialsState {
  const ids = ref<string[]>([]);
  const progress = ref<Record<string, FileProgress>>({});
  const errors = ref<Record<string, true>>({});
  const forceReady = ref(false);

  const ready = computed(() => {
    if (forceReady.value) return true;
    if (!ids.value.length) return true;
    return ids.value.every((id) => models.value.find((m) => m.id === id)?.status === "installed");
  });

  const init = async () => {
    try {
      ids.value = await api.essentialModels();
    } catch {
      ids.value = [];
    }
  };

  const start = async () => {
    await api.startEssentials();
  };

  const attachListeners = async (refreshModels: (id?: string) => Promise<void>) => [
    await events.onModelProgress((p) => {
      if (ids.value.includes(p.id)) recordSet(progress, p.id, p);
    }),
    await events.onModelDone((id) => {
      if (id) recordOmit(errors, id);
      void refreshModels(id);
    }),
    await events.onModelError((id) => {
      if (id) recordSet(errors, id, true);
      void refreshModels();
    }),
    await events.onEssentialsDone(() => {
      forceReady.value = true;
      void refreshModels();
    }),
  ];

  return { ids, progress, errors, ready, init, start, attachListeners };
}
