import { onUnmounted, ref } from "vue";

export function useMediaQuery(query: string) {
  const matches = ref(typeof window !== "undefined" && window.matchMedia(query).matches);
  if (typeof window === "undefined") return matches;
  const mq = window.matchMedia(query);
  const onChange = (e: MediaQueryListEvent) => {
    matches.value = e.matches;
  };
  mq.addEventListener?.("change", onChange);
  onUnmounted(() => mq.removeEventListener?.("change", onChange));
  return matches;
}
