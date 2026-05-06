<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { api, events } from "../api";
import type { FileProgress, ModelInfo } from "../types";

const models = ref<ModelInfo[]>([]);
const progress = ref<Record<string, FileProgress>>({});
const unlisten: (() => void)[] = [];

async function refresh() {
  models.value = await api.listModels();
}

async function install(id: string) {
  try {
    await api.installModel(id);
  } finally {
    delete progress.value[id];
    await refresh();
  }
}

function fmtMB(bytes: number): string {
  if (!bytes) return "—";
  return `${(bytes / 1_048_576).toFixed(0)} MB`;
}

function pct(p?: FileProgress): number {
  if (!p || !p.total) return 0;
  const fileFrac = p.downloaded / p.total;
  return ((p.file_index + fileFrac) / p.file_count) * 100;
}

onMounted(async () => {
  await refresh();
  unlisten.push(
    await events.onModelProgress((p) => {
      progress.value = { ...progress.value, [p.id]: p };
    }),
    await events.onModelDone(refresh),
    await events.onModelError(refresh),
  );
});

onUnmounted(() => unlisten.forEach((u) => u()));
</script>

<template>
  <section class="models">
    <h2>Models</h2>
    <article v-for="m in models" :key="m.id" class="model" :data-status="m.status">
      <header>
        <span class="name">{{ m.display_name }}</span>
        <span class="size">{{ fmtMB(m.size_bytes) }}</span>
      </header>
      <p class="desc">{{ m.description }}</p>
      <footer>
        <span class="status">{{ m.status }}</span>
        <button v-if="m.status === 'not_installed'" @click="install(m.id)">Install</button>
        <progress v-if="progress[m.id]" :value="pct(progress[m.id])" max="100" />
      </footer>
    </article>
  </section>
</template>

<style scoped>
.models {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 1.1rem;
}
.model {
  padding: 12px;
  border-radius: 6px;
  background: #1a1a1a;
}
header {
  display: flex;
  justify-content: space-between;
}
.name {
  font-weight: 600;
}
.size {
  color: #888;
  font-size: 0.85rem;
}
.desc {
  color: #aaa;
  margin: 4px 0;
  font-size: 0.85rem;
}
footer {
  display: flex;
  align-items: center;
  gap: 12px;
}
.status {
  color: #888;
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.model[data-status="installed"] .status {
  color: #6cf;
}
button {
  padding: 4px 10px;
  border-radius: 4px;
  border: 1px solid #444;
  background: #1f1f1f;
  color: #f0f0f0;
  cursor: pointer;
  font-size: 0.85rem;
}
progress {
  flex: 1;
  height: 6px;
}
</style>
