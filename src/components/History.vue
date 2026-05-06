<script setup lang="ts">
import { onMounted, ref } from "vue";
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
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function fmtDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

onMounted(refresh);
</script>

<template>
  <section class="history">
    <header>
      <h2>History</h2>
      <button class="ghost" @click="refresh">Refresh</button>
    </header>
    <p v-if="!entries.length" class="empty">No transcripts cached yet.</p>
    <article v-for="e in entries" :key="e.key" class="entry">
      <div class="head">
        <span class="name">{{ e.source_name }}</span>
        <span class="when">{{ fmtDate(e.created_at) }}</span>
      </div>
      <div class="meta">
        {{ e.model }} · {{ e.language }} · {{ fmtDuration(e.duration_ms) }} ·
        {{ e.utterances }} utterances
      </div>
      <div class="actions">
        <button @click="open(e.key)">Open</button>
        <button class="ghost" @click="remove(e.key)">Delete</button>
      </div>
    </article>
  </section>
</template>

<style scoped>
.history {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
h2 {
  margin: 0;
  font-size: 1.1rem;
}
.empty {
  color: #666;
}
.entry {
  padding: 12px;
  border-radius: 6px;
  background: #1a1a1a;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.head {
  display: flex;
  justify-content: space-between;
  font-weight: 600;
}
.when {
  color: #888;
  font-size: 0.85rem;
  font-weight: 400;
}
.meta {
  color: #aaa;
  font-size: 0.85rem;
}
.actions {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}
button {
  padding: 4px 12px;
  border-radius: 4px;
  border: 1px solid #444;
  background: #1f1f1f;
  color: #f0f0f0;
  font-size: 0.85rem;
  cursor: pointer;
}
button.ghost {
  background: transparent;
}
</style>
