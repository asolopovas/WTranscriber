import type { Ref } from "vue";

export function recordSet<V>(target: Ref<Record<string, V>>, key: string, value: V) {
  target.value = { ...target.value, [key]: value };
}

export function recordOmit<V>(target: Ref<Record<string, V>>, key: string) {
  if (!(key in target.value)) return;
  const next = { ...target.value };
  delete next[key];
  target.value = next;
}
