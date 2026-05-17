<script setup lang="ts">
import { computed, ref } from "vue";
import type { DirEntry, TranscribeProgress } from "@/types";
import { decodeName, prettyName } from "@utils/audio";
import { fmtMs, fmtBytes } from "@utils/format";
import TranscribeIcon from "@components/icons/TranscribeIcon.vue";
import Spinner from "@components/icons/Spinner.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import MenuItem from "@components/ui/MenuItem.vue";
import Checkbox from "@components/ui/Checkbox.vue";

const isAndroid = /Android/i.test(typeof navigator !== "undefined" ? navigator.userAgent : "");

const props = withDefaults(
  defineProps<{
    entries: DirEntry[];
    selectedPath: string;
    busy: Record<string, boolean>;
    progressByPath: Record<string, TranscribeProgress>;
    autoRenamingPath: string | null;
    dragOver: boolean;
    hasListing: boolean;
    selectedPaths?: Set<string>;
  }>(),
  { selectedPaths: () => new Set<string>() },
);

const rows = computed(() =>
  props.entries.map((entry) => ({
    entry,
    pretty: prettyName(entry.name),
    title: decodeName(entry.name),
  })),
);

const emit = defineEmits<{
  (e: "choose", entry: DirEntry): void;
  (e: "view", entry: DirEntry): void;
  (e: "transcribe", entry: DirEntry): void;
  (e: "stop", entry: DirEntry): void;
  (e: "trim", entry: DirEntry): void;
  (e: "auto-rename", entry: DirEntry): void;
  (e: "rename", entry: DirEntry): void;
  (e: "share", entry: DirEntry): void;
  (e: "copy", entry: DirEntry): void;
  (e: "export", entry: DirEntry): void;
  (e: "redo-diarize", entry: DirEntry): void;
  (e: "reveal", entry: DirEntry): void;
  (e: "delete", entry: DirEntry): void;
  (e: "toggle-select", path: string): void;
  (e: "range-select", path: string): void;
}>();

const selectionActive = computed(() => props.selectedPaths.size > 0);

let longPressTimer: ReturnType<typeof setTimeout> | null = null;
let longPressFired = false;
let pressStartX = 0;
let pressStartY = 0;

function clearLongPress() {
  if (longPressTimer) {
    clearTimeout(longPressTimer);
    longPressTimer = null;
  }
}

function onRowPointerDown(e: PointerEvent, path: string) {
  if (e.pointerType !== "touch") return;
  clearLongPress();
  pressStartX = e.clientX;
  pressStartY = e.clientY;
  longPressTimer = setTimeout(() => {
    longPressTimer = null;
    longPressFired = true;
    emit("toggle-select", path);
    if (typeof navigator !== "undefined" && navigator.vibrate) navigator.vibrate(15);
  }, 450);
}

function onRowPointerMove(e: PointerEvent) {
  if (!longPressTimer) return;
  if (Math.abs(e.clientX - pressStartX) > 8 || Math.abs(e.clientY - pressStartY) > 8) {
    clearLongPress();
  }
}

function onRowClick(entry: DirEntry) {
  if (longPressFired) {
    longPressFired = false;
    return;
  }
  if (selectionActive.value) emit("toggle-select", entry.path);
  else emit("choose", entry);
}

const openMenuPath = ref<string | null>(null);
const menuPosition = ref({ left: 0, top: 0 });
const activeMenuEntry = computed(
  () => props.entries.find((entry) => entry.path === openMenuPath.value) ?? null,
);

function closeMenu() {
  openMenuPath.value = null;
}

function toggleMenu(path: string, event: MouseEvent) {
  if (openMenuPath.value === path) {
    closeMenu();
    return;
  }
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const width = 184;
  const height = 440;
  const margin = 8;
  const left = Math.min(Math.max(margin, rect.right - width), window.innerWidth - width - margin);
  let top = rect.bottom + 4;
  if (top + height > window.innerHeight - margin) {
    top = Math.max(margin, rect.top - height - 4);
  }
  menuPosition.value = { left, top };
  openMenuPath.value = path;
}

defineExpose({
  closeMenus: closeMenu,
});
</script>

<template>
  <div
    v-if="!hasListing || entries.length === 0"
    class="h-full flex flex-col items-center justify-center gap-md text-center px-xl text-on-surface-variant"
  >
    <span
      class="material-symbols-outlined text-[48px]"
      :class="dragOver ? 'text-primary' : 'text-outline-variant'"
    >
      {{ dragOver ? "download" : "library_music" }}
    </span>
    <p class="text-bodyMedium">{{ dragOver ? "Drop to add" : "No audio in this folder" }}</p>
    <p class="font-mono text-labelSmall text-outline">Drag files here or click Add audio</p>
  </div>

  <ul v-else class="flex flex-col">
    <li
      v-for="{ entry, pretty, title } in rows"
      :key="entry.path"
      class="group border-b border-outline-variant/20 pl-margin pr-unit py-xs cursor-pointer transition-colors select-none [-webkit-touch-callout:none] [-webkit-tap-highlight-color:transparent] [-webkit-user-select:none]"
      :class="
        selectedPaths.has(entry.path)
          ? 'bg-primary/10'
          : selectedPath === entry.path
            ? 'bg-primary/[0.06]'
            : ''
      "
      @click="onRowClick(entry)"
      @dblclick="emit('transcribe', entry)"
      @pointerdown="onRowPointerDown($event, entry.path)"
      @pointermove="onRowPointerMove"
      @pointerup="clearLongPress"
      @pointercancel="clearLongPress"
      @contextmenu.prevent
    >
      <div class="flex items-center gap-xs">
        <div v-if="selectionActive || selectedPaths.has(entry.path)" class="shrink-0" @click.stop>
          <Checkbox
            :model-value="selectedPaths.has(entry.path)"
            @update:model-value="emit('toggle-select', entry.path)"
          />
        </div>
        <div class="flex-1 min-w-0">
          <div class="text-bodyMedium text-on-surface truncate" :title="title">
            {{ pretty.display }}
          </div>
          <div class="mt-[2px] font-mono text-labelSmall leading-tight">
            <div class="flex flex-wrap items-center gap-x-md gap-y-unit text-on-surface-variant">
              <span>{{ entry.duration_ms ? fmtMs(entry.duration_ms) : "—" }}</span>
              <span>{{ fmtBytes(entry.size_bytes) }}</span>
              <span v-if="entry.trim_start_ms || entry.trim_end_ms" class="text-primary"
                >trimmed</span
              >
            </div>
            <div v-if="pretty.timestampPretty" class="mt-unit text-secondary">
              {{ pretty.timestampPretty }}
            </div>
          </div>
        </div>
        <div v-if="!selectionActive" class="flex items-center gap-unit shrink-0" @click.stop>
          <Button
            v-if="busy[entry.path]"
            variant="ghost-error"
            shape="circle"
            size="md"
            icon="stop"
            :icon-size="24"
            title="Stop"
            @click="emit('stop', entry)"
          />
          <Button
            v-else
            variant="ghost-primary"
            shape="circle"
            size="md"
            title="Transcribe"
            @click="emit('transcribe', entry)"
          >
            <TranscribeIcon :size="24" />
          </Button>
          <Button
            v-if="!isAndroid"
            class="hidden md:inline-flex"
            variant="ghost"
            shape="circle"
            size="md"
            :title="autoRenamingPath === entry.path ? 'Renaming…' : 'Auto-rename (AI)'"
            :disabled="autoRenamingPath === entry.path"
            @click="emit('auto-rename', entry)"
          >
            <Spinner v-if="autoRenamingPath === entry.path" :size="20" />
            <Icon v-else name="auto_awesome" :size="20" />
          </Button>
          <Button
            v-if="entry.cache_key"
            class="hidden md:inline-flex"
            variant="ghost"
            shape="circle"
            size="md"
            icon="visibility"
            :icon-size="20"
            title="Transcript ready — view"
            @click="emit('view', entry)"
          />
          <Button
            variant="ghost"
            shape="circle"
            size="md"
            icon="more_vert"
            :icon-size="20"
            title="More"
            @click="toggleMenu(entry.path, $event)"
          />
        </div>
      </div>
    </li>
  </ul>

  <Teleport to="body">
    <div v-if="activeMenuEntry" class="fixed inset-0 z-[1000]" @click="closeMenu">
      <div
        class="absolute max-w-[calc(100vw-16px)] max-h-[calc(100vh-16px)] overflow-y-auto bg-surface-container-high border border-outline-variant/60 rounded-lg shadow-2xl py-unit scroll-thin"
        :style="{ left: `${menuPosition.left}px`, top: `${menuPosition.top}px`, width: '184px' }"
        @click.stop
      >
        <MenuItem
          icon="content_cut"
          @click="
            closeMenu();
            emit('trim', activeMenuEntry);
          "
        >
          {{
            activeMenuEntry.trim_start_ms || activeMenuEntry.trim_end_ms
              ? `Cut: ${fmtMs(activeMenuEntry.trim_start_ms ?? 0)} – ${fmtMs(
                  activeMenuEntry.trim_end_ms ?? activeMenuEntry.duration_ms ?? 0,
                )}`
              : "Cut / select range"
          }}
        </MenuItem>
        <MenuItem
          class="md:hidden"
          :disabled="autoRenamingPath === activeMenuEntry.path"
          @click="
            closeMenu();
            emit('auto-rename', activeMenuEntry);
          "
        >
          <template #icon>
            <Spinner v-if="autoRenamingPath === activeMenuEntry.path" :size="18" />
            <Icon v-else name="auto_awesome" :size="18" />
          </template>
          {{ autoRenamingPath === activeMenuEntry.path ? "Renaming…" : "Auto-rename" }}
        </MenuItem>
        <MenuItem
          icon="ios_share"
          :disabled="!activeMenuEntry.cache_key"
          @click="
            closeMenu();
            emit('share', activeMenuEntry);
          "
        >
          Share
        </MenuItem>
        <MenuItem
          icon="content_copy"
          :disabled="!activeMenuEntry.cache_key"
          @click="
            closeMenu();
            emit('copy', activeMenuEntry);
          "
        >
          Copy
        </MenuItem>
        <MenuItem
          icon="file_save"
          :disabled="!activeMenuEntry.cache_key"
          @click="
            closeMenu();
            emit('export', activeMenuEntry);
          "
        >
          Export…
        </MenuItem>
        <div class="md:hidden my-unit border-t border-outline-variant/40"></div>
        <MenuItem
          icon="groups"
          :disabled="!activeMenuEntry.cache_key"
          @click="
            closeMenu();
            emit('redo-diarize', activeMenuEntry);
          "
        >
          Re-diarize…
        </MenuItem>
        <MenuItem
          icon="drive_file_rename_outline"
          @click="
            closeMenu();
            emit('rename', activeMenuEntry);
          "
        >
          Rename
        </MenuItem>
        <MenuItem
          icon="folder_open"
          @click="
            closeMenu();
            emit('reveal', activeMenuEntry);
          "
        >
          Reveal in folder
        </MenuItem>
        <MenuItem
          icon="delete"
          tone="danger"
          @click="
            closeMenu();
            emit('delete', activeMenuEntry);
          "
        >
          Delete
        </MenuItem>
      </div>
    </div>
  </Teleport>
</template>
