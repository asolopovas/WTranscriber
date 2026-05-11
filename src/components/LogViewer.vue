<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api } from "@/api";
import ErrorBanner from "@components/ui/ErrorBanner.vue";
import Button from "@components/ui/Button.vue";

const retain = defineModel<number>("retain", { default: 1 });
const auto = defineModel<boolean>("auto", { default: true });

const tail = ref<string>("");
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

const copied = ref(false);
let copiedTimer: ReturnType<typeof setTimeout> | null = null;
async function copyContents() {
  try {
    await navigator.clipboard.writeText(displayed.value);
    copied.value = true;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copied.value = false;
    }, 1500);
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(async () => {
  await refresh();
  timer = setInterval(refresh, 2000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});

defineExpose({ refresh, clear });

function levelClass(line: string): string {
  if (line.includes(" ERROR ")) return "text-error";
  if (line.includes(" WARN ")) return "text-secondary";
  if (line.includes(" PROC ") || line.startsWith("-----")) return "text-primary";
  return "text-on-surface-variant/80";
}
</script>

<template>
  <main class="flex-1 flex flex-col overflow-hidden bg-surface-container-lowest">
    <div class="flex-1 flex flex-col overflow-hidden w-full px-xs md:px-md py-md gap-md">
      <ErrorBanner v-if="error">{{ error }}</ErrorBanner>

      <section
        class="relative flex-1 flex flex-col overflow-hidden bg-surface-container/40 rounded-xl border border-outline-variant/40"
      >
        <div
          ref="scroller"
          class="flex-1 overflow-y-auto scroll-thin px-xl py-lg font-mono text-labelSmall leading-snug whitespace-pre-wrap wrap-break-word"
        >
          <p v-if="!displayed" class="text-outline italic">(log is empty)</p>
          <template v-else>
            <div v-for="(line, i) in displayed.split('\n')" :key="i" :class="levelClass(line)">
              {{ line || "\u00A0" }}
            </div>
          </template>
        </div>
        <Button
          v-if="displayed"
          variant="neutral"
          shape="icon"
          :icon="copied ? 'check' : 'content_copy'"
          :icon-size="22"
          :aria-label="copied ? 'Copied' : 'Copy log to clipboard'"
          :title="copied ? 'Copied' : 'Copy log to clipboard'"
          class="absolute bottom-md right-md w-12 h-12 shadow-md bg-surface-container-high"
          @click="copyContents"
        />
      </section>
    </div>
  </main>
</template>
