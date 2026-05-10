<script setup lang="ts">
import { computed, ref } from "vue";
import type { DirEntry, TranscribeProgress } from "@/types";
import { decodeName, prettyName } from "@utils/audio";
import { fmtMs, fmtBytes } from "@composables/format";
import TranscribeIcon from "@components/icons/TranscribeIcon.vue";
import Spinner from "@components/icons/Spinner.vue";
import Icon from "@components/ui/Icon.vue";
import Button from "@components/ui/Button.vue";
import MenuItem from "@components/ui/MenuItem.vue";
import Checkbox from "@components/ui/Checkbox.vue";

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
  (e: "export", entry: DirEntry): void;
  (e: "redo-diarize", entry: DirEntry): void;
  (e: "delete", entry: DirEntry): void;
  (e: "toggle-select", path: string): void;
  (e: "range-select", path: string): void;
}>();

const selectionActive = computed(() => props.selectedPaths.size > 0);

const openMenuPath = ref<string | null>(null);
function toggleMenu(path: string) {
  openMenuPath.value = openMenuPath.value === path ? null : path;
}
defineExpose({
  closeMenus: () => {
    openMenuPath.value = null;
  },
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
      class="group border-b border-outline-variant/20 px-margin py-xs cursor-pointer transition-colors hover:bg-surface-container-high/40"
      :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
      @click="selectionActive ? emit('toggle-select', entry.path) : emit('choose', entry)"
      @dblclick="emit('transcribe', entry)"
    >
      <div class="flex items-center gap-xs">
        <div v-if="selectionActive || selectedPaths.has(entry.path)" class="shrink-0" @click.stop>
          <Checkbox
            :model-value="selectedPaths.has(entry.path)"
            @update:model-value="emit('toggle-select', entry.path)"
          />
        </div>
        <div class="flex-1 min-w-0">
          <div class="flex items-baseline gap-xs min-w-0">
            <div class="flex-1 min-w-0 text-bodyMedium text-on-surface truncate" :title="title">
              {{ pretty.display }}
            </div>
            <span v-if="pretty.timestamp" class="font-mono text-labelSmall text-secondary shrink-0">
              {{ pretty.timestamp }}
            </span>
          </div>
          <div
            class="flex flex-wrap items-center gap-x-xs gap-y-unit font-mono text-labelSmall text-on-surface-variant leading-tight"
          >
            <span>{{ entry.duration_ms ? fmtMs(entry.duration_ms) : "—" }}</span>
            <span class="text-outline-variant">·</span>
            <span>{{ fmtBytes(entry.size_bytes) }}</span>
            <template v-if="entry.trim_start_ms || entry.trim_end_ms">
              <span class="text-outline-variant">·</span>
              <span class="text-primary inline-flex items-center gap-unit">trimmed</span>
            </template>
          </div>
        </div>
        <div class="flex items-center gap-unit shrink-0" @click.stop>
          <Button
            v-if="busy[entry.path]"
            variant="ghost-error"
            shape="icon"
            size="sm"
            icon="stop"
            :icon-size="18"
            title="Stop"
            @click="emit('stop', entry)"
          />
          <Button
            v-else
            variant="ghost-primary"
            shape="icon"
            size="sm"
            title="Transcribe"
            @click="emit('transcribe', entry)"
          >
            <TranscribeIcon :size="18" />
          </Button>
          <Button
            class="hidden md:inline-flex"
            variant="ghost"
            shape="icon"
            size="sm"
            :title="autoRenamingPath === entry.path ? 'Renaming…' : 'Auto-rename (AI)'"
            :disabled="autoRenamingPath === entry.path"
            @click="emit('auto-rename', entry)"
          >
            <Spinner v-if="autoRenamingPath === entry.path" :size="18" />
            <Icon v-else name="auto_awesome" :size="18" />
          </Button>
          <Button
            v-if="entry.cache_key"
            class="hidden md:inline-flex"
            variant="ghost"
            shape="icon"
            size="sm"
            icon="visibility"
            :icon-size="18"
            title="Transcript ready — view"
            @click="emit('view', entry)"
          />
          <div class="relative">
            <Button
              variant="ghost"
              shape="icon"
              size="sm"
              icon="more_vert"
              :icon-size="18"
              title="More"
              @click="toggleMenu(entry.path)"
            />
            <div
              v-if="openMenuPath === entry.path"
              class="absolute right-0 top-full mt-unit z-30 min-w-45 bg-surface-container-high border border-outline-variant/60 rounded-lg shadow-2xl py-unit"
            >
              <MenuItem
                icon="content_cut"
                @click="
                  openMenuPath = null;
                  emit('trim', entry);
                "
              >
                {{
                  entry.trim_start_ms || entry.trim_end_ms
                    ? `Cut: ${fmtMs(entry.trim_start_ms ?? 0)} – ${fmtMs(
                        entry.trim_end_ms ?? entry.duration_ms ?? 0,
                      )}`
                    : "Cut / select range"
                }}
              </MenuItem>
              <MenuItem
                class="md:hidden"
                :disabled="autoRenamingPath === entry.path"
                @click="
                  openMenuPath = null;
                  emit('auto-rename', entry);
                "
              >
                <template #icon>
                  <Spinner v-if="autoRenamingPath === entry.path" :size="18" />
                  <Icon v-else name="auto_awesome" :size="18" />
                </template>
                {{ autoRenamingPath === entry.path ? "Renaming…" : "Auto-rename" }}
              </MenuItem>
              <MenuItem
                icon="ios_share"
                :disabled="!entry.cache_key"
                @click="
                  openMenuPath = null;
                  emit('share', entry);
                "
              >
                Share
              </MenuItem>
              <MenuItem
                icon="file_save"
                :disabled="!entry.cache_key"
                @click="
                  openMenuPath = null;
                  emit('export', entry);
                "
              >
                Export…
              </MenuItem>
              <div class="md:hidden my-unit border-t border-outline-variant/40"></div>
              <MenuItem
                icon="groups"
                :disabled="!entry.cache_key"
                @click="
                  openMenuPath = null;
                  emit('redo-diarize', entry);
                "
              >
                Re-diarize…
              </MenuItem>
              <MenuItem
                icon="drive_file_rename_outline"
                @click="
                  openMenuPath = null;
                  emit('rename', entry);
                "
              >
                Rename
              </MenuItem>
              <MenuItem
                icon="delete"
                tone="danger"
                @click="
                  openMenuPath = null;
                  emit('delete', entry);
                "
              >
                Delete
              </MenuItem>
            </div>
          </div>
        </div>
      </div>
    </li>
  </ul>
</template>
