import { ref, type Ref } from "vue";

export interface FileSelection {
  selected: Ref<Set<string>>;
  toggle: (path: string) => void;
  range: (anchor: string, allPaths: () => string[]) => void;
  selectAll: (allPaths: () => string[]) => void;
  clear: () => void;
  hasSelection: () => boolean;
  size: () => number;
  asArray: () => string[];
}

export function useFileSelection(): FileSelection {
  const selected = ref(new Set<string>());
  let lastAnchor: string | null = null;

  const toggle = (path: string) => {
    const next = new Set(selected.value);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selected.value = next;
    lastAnchor = path;
  };

  const range = (anchor: string, allPaths: () => string[]) => {
    if (anchor === "__all__") {
      selectAll(allPaths);
      return;
    }
    const paths = allPaths();
    const anchorIdx = paths.indexOf(anchor);
    const from = lastAnchor ? paths.indexOf(lastAnchor) : anchorIdx;
    if (anchorIdx < 0 || from < 0) return;
    const lo = Math.min(from, anchorIdx);
    const hi = Math.max(from, anchorIdx);
    const next = new Set(selected.value);
    for (let i = lo; i <= hi; i++) next.add(paths[i]);
    selected.value = next;
    lastAnchor = anchor;
  };

  const selectAll = (allPaths: () => string[]) => {
    const paths = allPaths();
    selected.value = selected.value.size === paths.length ? new Set() : new Set(paths);
  };

  const clear = () => {
    selected.value = new Set();
    lastAnchor = null;
  };

  return {
    selected,
    toggle,
    range,
    selectAll,
    clear,
    hasSelection: () => selected.value.size > 0,
    size: () => selected.value.size,
    asArray: () => [...selected.value],
  };
}
