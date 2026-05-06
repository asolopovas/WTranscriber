<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "../api";
import type { CacheEntry, Transcript } from "../types";

const emit = defineEmits<{ open: [transcript: Transcript] }>();
const entries = ref<CacheEntry[]>([]);

async function refresh() {
  entries.value = await api.historyList();
}

async function open(key: string) {
  const t = await api.historyLoad(key);
  if (t) emit("open", t);
}

async function remove(key: string) {
  await api.historyDelete(key);
  await refresh();
}

function fmtDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return h ? `${h}h ${m}m` : `${m}m ${String(r).padStart(2, "0")}s`;
}

function fmtTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function dayBucket(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const y = new Date();
  y.setDate(today.getDate() - 1);
  const same = (a: Date, b: Date) =>
    a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  if (same(d, today)) return "Today";
  if (same(d, y)) return "Yesterday";
  return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
}

const grouped = computed(() => {
  const buckets = new Map<string, CacheEntry[]>();
  for (const e of entries.value) {
    const k = dayBucket(e.created_at);
    if (!buckets.has(k)) buckets.set(k, []);
    buckets.get(k)!.push(e);
  }
  return Array.from(buckets.entries());
});

onMounted(refresh);
</script>

<template>
  <div class="flex-1 flex overflow-hidden">
    <div class="flex-1 bg-surface-container-lowest flex items-center justify-center p-xl">
      <div class="text-center text-on-surface-variant max-w-sm">
        <span class="material-symbols-outlined text-[64px] text-outline-variant">history</span>
        <p class="text-bodyMedium mt-md">Select a session from the list to reopen it in the workspace.</p>
      </div>
    </div>

    <aside class="w-[480px] bg-surface-container border-l border-outline-variant/40 flex flex-col h-full shrink-0">
      <div class="px-margin pt-xl pb-md border-b border-outline-variant/30 shrink-0 flex justify-between items-end">
        <div>
          <h2 class="text-titleMedium text-on-surface flex items-center gap-xs">
            <span class="material-symbols-outlined fill text-primary text-[20px]">history</span>
            History
          </h2>
          <p class="font-mono text-labelSmall text-on-surface-variant mt-1">{{ entries.length }} sessions</p>
        </div>
        <button
          @click="refresh"
          class="w-8 h-8 rounded-full flex items-center justify-center hover:bg-surface-container-high text-on-surface-variant hover:text-on-surface transition-colors"
          aria-label="Refresh"
        >
          <span class="material-symbols-outlined text-[18px]">refresh</span>
        </button>
      </div>

      <div class="flex-1 overflow-y-auto py-md px-margin scroll-thin">
        <p v-if="!entries.length" class="text-on-surface-variant text-bodyMedium text-center py-xl">
          No transcripts cached yet.
        </p>

        <template v-for="[label, items] in grouped" :key="label">
          <div class="font-mono text-labelSmall text-outline tracking-wider uppercase pt-md pb-xs">{{ label }}</div>
          <div class="space-y-unit">
            <div
              v-for="e in items"
              :key="e.key"
              class="group flex items-center gap-md px-md py-xs rounded-lg hover:bg-surface-container-high transition-colors"
            >
              <button
                class="flex-1 flex items-center gap-md text-left min-w-0"
                @click="open(e.key)"
              >
                <div class="w-8 h-8 rounded bg-surface-container-highest flex items-center justify-center shrink-0 group-hover:bg-primary/10 transition-colors">
                  <span class="material-symbols-outlined fill text-on-surface-variant text-[16px] group-hover:text-primary transition-colors">description</span>
                </div>
                <div class="flex-1 min-w-0">
                  <div class="text-bodyMedium text-on-surface truncate">{{ e.source_name }}</div>
                  <div class="flex items-center gap-2 mt-[2px]">
                    <span class="font-mono text-labelSmall text-secondary">{{ fmtTime(e.created_at) }}</span>
                    <span class="w-1 h-1 rounded-full bg-outline-variant"></span>
                    <span class="font-mono text-labelSmall text-on-surface-variant truncate">{{ fmtDuration(e.duration_ms) }}</span>
                    <span class="w-1 h-1 rounded-full bg-outline-variant"></span>
                    <span class="font-mono text-labelSmall text-on-surface-variant truncate">{{ e.model }}</span>
                  </div>
                </div>
              </button>
              <button
                class="w-8 h-8 rounded-full flex items-center justify-center text-outline opacity-0 group-hover:opacity-100 hover:bg-error/10 hover:text-error transition-all"
                @click="remove(e.key)"
                aria-label="Delete"
              >
                <span class="material-symbols-outlined text-[18px]">delete</span>
              </button>
            </div>
          </div>
        </template>
      </div>
    </aside>
  </div>
</template>
