<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api } from "@/api";
import ErrorBanner from "@components/ui/ErrorBanner.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";

const tail = ref<string>("");
const path = ref<string>("");
const auto = ref(true);
const retain = ref<number>(Number(localStorage.getItem("wt.logRetain") ?? "1") || 1);
const error = ref<string | null>(null);
const scroller = ref<HTMLElement | null>(null);
let timer: ReturnType<typeof setInterval> | null = null;

const displayed = computed(() => {
  if (!tail.value) return "";
  if (retain.value <= 0) return tail.value;
  const lines = tail.value.split("\n");
  const starts: number[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (/^-----.*started/.test(lines[i])) starts.push(i);
  }
  if (starts.length <= retain.value) return tail.value;
  return lines.slice(starts[starts.length - retain.value]).join("\n");
});

function onRetainChange(v: number) {
  retain.value = v;
  localStorage.setItem("wt.logRetain", String(v));
}

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
  const ok = await confirm("Erase the log file? This cannot be undone.");
  if (!ok) return;
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
      class="flex-1 flex flex-col overflow-hidden max-w-[768px] w-full mx-auto px-margin md:px-xl pt-margin md:pt-xl pb-margin md:pb-xl gap-md"
    >
      <div class="flex flex-col gap-md pb-md border-b border-outline-variant/50">
        <div class="flex flex-col md:flex-row md:items-end md:justify-between gap-md">
          <div class="min-w-0">
            <h1
              class="text-[20px] md:text-[24px] leading-[28px] md:leading-[32px] font-bold text-on-surface"
            >
              Application Log
            </h1>
            <p class="text-bodyMedium text-on-surface-variant mt-unit">
              Persistent runtime log — engine errors, model installs, transcribe runs.
            </p>
          </div>
          <div class="flex items-center flex-wrap gap-xs shrink-0">
            <label
              class="flex items-center gap-unit text-bodyMedium text-on-surface-variant cursor-pointer select-none h-11 md:h-auto px-xs"
            >
              <input v-model="auto" type="checkbox" class="accent-primary w-4 h-4" />
              auto-scroll
            </label>
            <select
              :value="retain"
              @change="onRetainChange(Number(($event.target as HTMLSelectElement).value))"
              class="min-h-11 md:min-h-0 px-md py-xs rounded-full border border-outline text-on-surface text-titleSmall bg-transparent"
              title="How many recent runs to display"
            >
              <option :value="1">Latest run</option>
              <option :value="5">Last 5 runs</option>
              <option :value="20">Last 20 runs</option>
              <option :value="0">All</option>
            </select>
            <Button mobile-tall icon="refresh" :icon-size="18" @click="refresh"> Refresh </Button>
            <Button variant="danger" mobile-tall icon="delete" @click="clear"> Clear </Button>
          </div>
        </div>
        <div class="flex items-center gap-xs text-on-surface-variant min-w-0">
          <Icon name="description" :size="16" class="shrink-0" />
          <code class="font-mono text-labelSmall truncate">{{ path || "—" }}</code>
          <Button
            v-if="path"
            variant="ghost"
            shape="icon"
            icon="content_copy"
            :icon-size="16"
            class="ml-xs"
            aria-label="Copy path"
            @click="copyPath"
          />
        </div>
      </div>

      <ErrorBanner v-if="error">{{ error }}</ErrorBanner>

      <section
        class="flex-1 flex flex-col overflow-hidden bg-surface-container rounded-xl border border-outline-variant/50"
      >
        <div
          ref="scroller"
          class="flex-1 overflow-y-auto scroll-thin px-md py-md font-mono text-labelMedium leading-relaxed whitespace-pre-wrap break-all"
        >
          <p v-if="!displayed" class="text-outline italic">(log is empty)</p>
          <template v-else>
            <div v-for="(line, i) in displayed.split('\n')" :key="i" :class="levelClass(line)">
              {{ line || "\u00A0" }}
            </div>
          </template>
        </div>
      </section>
    </div>
  </main>
</template>
