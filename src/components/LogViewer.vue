<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api } from "../api";

const tail = ref<string>("");
const path = ref<string>("");
const auto = ref(true);
const error = ref<string | null>(null);
const scroller = ref<HTMLElement | null>(null);
let timer: ReturnType<typeof setInterval> | null = null;

async function refresh() {
  try {
    tail.value = await api.logTail(256 * 1024);
    if (auto.value && scroller.value) {
      scroller.value.scrollTop = scroller.value.scrollHeight;
    }
  } catch (e) {
    error.value = String(e);
  }
}

async function clear() {
  if (!confirm("Erase the log file? This cannot be undone.")) return;
  try {
    await api.logClear();
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function copyPath() {
  await navigator.clipboard.writeText(path.value);
}

onMounted(async () => {
  try {
    path.value = await api.logPath();
  } catch (e) {
    error.value = String(e);
  }
  await refresh();
  timer = setInterval(refresh, 2000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});

function levelClass(line: string): string {
  if (line.includes(" ERROR ")) return "text-error";
  if (line.includes(" WARN ")) return "text-secondary";
  if (line.includes(" PROC ") || line.startsWith("-----")) return "text-primary";
  if (line.includes(" INFO ")) return "text-on-surface-variant";
  return "text-on-surface-variant";
}
</script>

<template>
  <main class="flex-1 flex flex-col overflow-hidden bg-surface-container-lowest">
    <div
      class="flex-1 flex flex-col overflow-hidden max-w-[768px] w-full mx-auto px-xl pt-xl pb-xl gap-md"
    >
      <div class="flex flex-col gap-md pb-md border-b border-outline-variant/50">
        <div class="flex items-end justify-between gap-margin">
          <div>
            <h1 class="text-[24px] leading-[32px] font-bold text-on-surface">Application Log</h1>
            <p class="text-bodyMedium text-on-surface-variant mt-unit">
              Persistent runtime log — engine errors, model installs, transcribe runs.
            </p>
          </div>
          <div class="flex items-center gap-xs shrink-0">
            <label
              class="flex items-center gap-unit text-bodyMedium text-on-surface-variant cursor-pointer select-none"
            >
              <input v-model="auto" type="checkbox" class="accent-primary" />
              auto-scroll
            </label>
            <button
              @click="refresh"
              class="px-md py-xs rounded-full border border-outline text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors flex items-center gap-unit"
            >
              <span class="material-symbols-outlined text-[16px]">refresh</span> Refresh
            </button>
            <button
              @click="clear"
              class="px-md py-xs rounded-full border border-error/60 text-error text-titleSmall hover:bg-error/10 transition-colors flex items-center gap-unit"
            >
              <span class="material-symbols-outlined text-[16px]">delete</span> Clear
            </button>
          </div>
        </div>
        <div class="flex items-center gap-xs text-on-surface-variant min-w-0">
          <span class="material-symbols-outlined text-[16px] shrink-0">description</span>
          <code class="font-mono text-labelSmall truncate">{{ path || "—" }}</code>
          <button
            v-if="path"
            @click="copyPath"
            class="ml-xs material-symbols-outlined text-[16px] hover:text-primary transition-colors"
            aria-label="Copy path"
          >
            content_copy
          </button>
        </div>
      </div>

      <div
        v-if="error"
        class="p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium font-mono"
      >
        {{ error }}
      </div>

      <section
        class="flex-1 flex flex-col overflow-hidden bg-surface-container rounded-xl border border-outline-variant/50"
      >
        <div
          ref="scroller"
          class="flex-1 overflow-y-auto scroll-thin px-md py-md font-mono text-labelMedium leading-relaxed whitespace-pre-wrap break-all"
        >
          <p v-if="!tail" class="text-outline italic">(log is empty)</p>
          <template v-else>
            <div v-for="(line, i) in tail.split('\n')" :key="i" :class="levelClass(line)">
              {{ line || "\u00A0" }}
            </div>
          </template>
        </div>
      </section>
    </div>
  </main>
</template>
