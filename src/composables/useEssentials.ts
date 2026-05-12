import { computed, ref, type Ref } from "vue";
import { api, events } from "@/api";
import type { FileProgress, ModelInfo, RuntimeProgress } from "@/types";
import { recordOmit, recordSet } from "@utils/records";

export interface RuntimeState {
  id: string;
  downloaded: number;
  total: number;
  phase: "starting" | "downloading" | "done" | "error";
  label: string;
}

export interface EssentialsState {
  ids: Ref<string[]>;
  progress: Ref<Record<string, FileProgress>>;
  errors: Ref<Record<string, true>>;
  ready: Ref<boolean>;
  runtimes: Ref<Record<string, RuntimeState>>;
  init: () => Promise<void>;
  start: () => Promise<void>;
  attachListeners: (refreshModels: (id?: string) => Promise<void>) => Promise<(() => void)[]>;
}

function runtimeLabel(id: string): string {
  if (id === "cudnn") return "GPU support (cuDNN)";
  if (id === "llama.cpp") return "Naming engine (llama.cpp)";
  if (id === "nemo-python") return "Diarisation engine (NeMo)";
  if (id.startsWith("sherpa-onnx-")) {
    const variant = id.slice("sherpa-onnx-".length).toUpperCase();
    return `Speech engine (sherpa-onnx ${variant})`;
  }
  return id;
}

export function useEssentials(models: Ref<ModelInfo[]>): EssentialsState {
  const ids = ref<string[]>([]);
  const progress = ref<Record<string, FileProgress>>({});
  const errors = ref<Record<string, true>>({});
  const runtimes = ref<Record<string, RuntimeState>>({});
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

  const upsertRuntime = (p: RuntimeProgress): void => {
    const existing = runtimes.value[p.id];
    const downloaded = p.downloaded ?? existing?.downloaded ?? 0;
    const total = p.total ?? existing?.total ?? 0;
    const phase: RuntimeState["phase"] =
      p.phase === "starting"
        ? "starting"
        : total > 0 || downloaded > 0
          ? "downloading"
          : "starting";
    recordSet(runtimes, p.id, {
      id: p.id,
      downloaded,
      total,
      phase,
      label: runtimeLabel(p.id),
    });
  };

  const markRuntime = (id: string, phase: "done" | "error"): void => {
    const existing = runtimes.value[id];
    recordSet(runtimes, id, {
      id,
      downloaded: existing?.downloaded ?? 0,
      total: existing?.total ?? 0,
      phase,
      label: existing?.label ?? runtimeLabel(id),
    });
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
    await events.onRuntimeProgress((p) => {
      upsertRuntime(p);
    }),
    await events.onRuntimeDone((id) => {
      if (id) markRuntime(id, "done");
    }),
    await events.onRuntimeError((id) => {
      if (id) markRuntime(id, "error");
    }),
  ];

  return { ids, progress, errors, ready, runtimes, init, start, attachListeners };
}
