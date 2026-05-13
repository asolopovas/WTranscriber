<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api } from "@/api";
import Button from "@components/ui/Button.vue";
import Icon from "@components/ui/Icon.vue";

const emit = defineEmits<{
  (e: "granted"): void;
  (e: "skipped"): void;
}>();

const polling = ref(false);
const checking = ref(false);

async function pollGrant() {
  if (checking.value) return;
  checking.value = true;
  try {
    const ok = await api.hasPersistentStorage();
    if (ok) {
      await api.enablePersistentStorage();
      stopPolling();
      emit("granted");
    }
  } finally {
    checking.value = false;
  }
}

let intervalId: ReturnType<typeof setInterval> | null = null;
function startPolling() {
  if (polling.value) return;
  polling.value = true;
  intervalId = setInterval(pollGrant, 1500);
}
function stopPolling() {
  polling.value = false;
  if (intervalId !== null) {
    clearInterval(intervalId);
    intervalId = null;
  }
}

function onVisibility() {
  if (document.visibilityState === "visible") void pollGrant();
}

async function onContinue() {
  await api.requestPersistentStorage();
  startPolling();
}

function onSkip() {
  stopPolling();
  emit("skipped");
}

onMounted(() => {
  document.addEventListener("visibilitychange", onVisibility);
  window.addEventListener("focus", onVisibility);
});
onUnmounted(() => {
  stopPolling();
  document.removeEventListener("visibilitychange", onVisibility);
  window.removeEventListener("focus", onVisibility);
});
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface/95 backdrop-blur-sm px-margin"
  >
    <div
      class="w-full max-w-96 flex flex-col gap-lg p-lg rounded-2xl bg-surface-container-high border border-outline-variant/40 shadow-2xl"
    >
      <div class="flex items-center gap-md">
        <Icon name="folder_special" :size="28" class="text-primary shrink-0" />
        <h1 class="text-titleLarge text-on-surface">Keep your files</h1>
      </div>

      <p class="text-bodyMedium text-on-surface-variant">
        Save recordings, transcripts and edits to shared storage so they survive reinstalls.
      </p>

      <div class="flex flex-col gap-xs">
        <Button variant="primary" size="lg" block @click="onContinue">
          {{ polling ? "Waiting for permission…" : "Continue" }}
        </Button>
        <Button variant="ghost" size="lg" block @click="onSkip">Skip</Button>
      </div>

      <p
        v-if="polling"
        class="text-labelSmall font-mono text-on-surface-variant text-center leading-tight"
      >
        Allow “Manage all files”, then return.
      </p>
    </div>
  </div>
</template>
