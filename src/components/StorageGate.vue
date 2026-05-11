<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api } from "@/api";
import Button from "@components/ui/Button.vue";

const emit = defineEmits<{
  (e: "granted"): void;
  (e: "skipped"): void;
}>();

const opening = ref(false);
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

async function openSettings() {
  opening.value = true;
  try {
    await api.requestPersistentStorage();
    startPolling();
  } finally {
    opening.value = false;
  }
}

function skip() {
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
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface/95 backdrop-blur-sm overflow-y-auto px-xl py-xl"
  >
    <div class="w-full max-w-112 flex flex-col gap-xl">
      <div class="flex flex-col items-center gap-md">
        <div class="text-6xl" aria-hidden="true">💾</div>
        <h1 class="text-headlineSmall text-on-surface text-center">Save models permanently?</h1>
      </div>

      <div class="flex flex-col gap-md text-bodyMedium text-on-surface-variant">
        <p>
          WTranscriber needs to download about <strong>1 GB</strong> of speech and language models.
        </p>
        <p>
          Without permission, Android will <strong>delete them every time you reinstall</strong>
          the app — forcing a full redownload.
        </p>
        <p>
          Granting <em>All files access</em> stores models in
          <code class="text-labelSmall font-mono">/WTranscriber/models</code> on your device, where
          they survive uninstalls and updates.
        </p>
      </div>

      <div class="flex flex-col gap-sm">
        <Button :disabled="opening" variant="primary" size="lg" block @click="openSettings">
          {{ polling ? "Waiting for permission…" : "Open settings" }}
        </Button>
        <Button variant="ghost" size="lg" block @click="skip">
          Skip — redownload after every reinstall
        </Button>
      </div>

      <p v-if="polling" class="text-bodySmall text-on-surface-variant text-center font-mono">
        Toggle “Allow access to manage all files”, then return here.
      </p>
    </div>
  </div>
</template>
