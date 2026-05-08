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
  (e: "transcribe", entry: DirEntry): void;
  (e: "stop", entry: DirEntry): void;
  (e: "trim", entry: DirEntry): void;
  (e: "auto-rename", entry: DirEntry): void;
  (e: "rename", entry: DirEntry): void;
  (e: "export", entry: DirEntry): void;
  (e: "delete", entry: DirEntry): void;
  (e: "toggle-select", path: string): void;
  (e: "range-select", path: string): void;
}>();

const selectionActive = computed(() => props.selectedPaths.size > 0);

const openMenuPath = ref<string | null>(null);
function toggleMenu(path: string) {
  openMenuPath.value = openMenuPath.value === path ? null : path;
}
defineExpose({ closeMenus: () => (openMenuPath.value = null) });
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

  <ul v-else class="flex flex-col md:hidden">
    <li
      v-for="{ entry, pretty, title } in rows"
      :key="`m-${entry.path}`"
      class="group border-b border-outline-variant/20 px-margin py-md cursor-pointer transition-colors"
      :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
      @click="selectionActive ? emit('toggle-select', entry.path) : emit('choose', entry)"
    >
      <div class="flex items-center gap-xs">
        <div
          v-if="selectionActive || selectedPaths.has(entry.path)"
          class="shrink-0 mr-xs"
          @click.stop
        >
          <Checkbox
            :model-value="selectedPaths.has(entry.path)"
            @update:model-value="emit('toggle-select', entry.path)"
          />
        </div>
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-xs">
            <div
              class="flex-1 min-w-0 text-bodyMedium text-on-surface wrap-break-word"
              :title="title"
            >
              {{ pretty.display }}
            </div>
            <div class="flex items-center gap-unit shrink-0 -mr-xs" @click.stop>
              <Button
                v-if="busy[entry.path]"
                variant="ghost-error"
                shape="circle"
                size="md"
                icon="stop"
                :icon-size="20"
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
                <TranscribeIcon :size="20" />
              </Button>
              <div class="relative">
                <Button
                  variant="ghost"
                  shape="circle"
                  size="md"
                  icon="more_vert"
                  :icon-size="20"
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
                    Cut / select range
                  </MenuItem>
                  <MenuItem
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
                    icon="drive_file_rename_outline"
                    @click="
                      openMenuPath = null;
                      emit('rename', entry);
                    "
                  >
                    Rename
                  </MenuItem>
                  <MenuItem
                    icon="download"
                    :disabled="!entry.cache_key"
                    @click="
                      openMenuPath = null;
                      emit('export', entry);
                    "
                  >
                    Export
                  </MenuItem>
                  <div class="my-unit border-t border-outline-variant/40"></div>
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
        </div>
      </div>
    </li>
  </ul>

  <table v-if="rows.length" class="hidden md:table w-full text-bodyMedium">
    <thead class="sticky top-0 bg-surface z-10 border-b border-outline-variant/40">
      <tr
        class="group text-left font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
      >
        <th class="px-margin py-xs w-8">
          <Checkbox
            :model-value="selectionActive && selectedPaths.size === entries.length"
            class="opacity-0 group-hover:opacity-100"
            :class="selectionActive ? '!opacity-100' : ''"
            @update:model-value="emit('range-select', '__all__')"
          />
        </th>
        <th class="px-xs py-xs">Name</th>
        <th class="px-xs py-xs w-24">Duration</th>
        <th class="px-xs py-xs w-24">Size</th>
        <th class="px-xs py-xs w-28">Status</th>
        <th class="px-margin py-xs w-50"></th>
      </tr>
    </thead>
    <tbody>
      <tr
        v-for="{ entry, pretty, title } in rows"
        :key="entry.path"
        class="group border-b border-outline-variant/20 hover:bg-surface-container-high/40 cursor-pointer transition-colors"
        :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
        @click="selectionActive ? emit('toggle-select', entry.path) : emit('choose', entry)"
        @dblclick="emit('transcribe', entry)"
      >
        <td class="px-margin py-xs" @click.stop>
          <Checkbox
            :model-value="selectedPaths.has(entry.path)"
            class="opacity-0 group-hover:opacity-100 focus-within:opacity-100"
            :class="selectedPaths.has(entry.path) ? '!opacity-100' : ''"
            @update:model-value="
              (_v: boolean, _ev?: MouseEvent) => emit('toggle-select', entry.path)
            "
          />
        </td>
        <td class="px-xs py-xs truncate max-w-0">
          <span class="text-on-surface" :title="title">
            {{ pretty.display }}
          </span>
          <span v-if="pretty.timestamp" class="font-mono text-labelSmall text-secondary ml-xs">
            {{ pretty.timestamp }}
          </span>
        </td>
        <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
          {{ entry.duration_ms ? fmtMs(entry.duration_ms) : "—" }}
        </td>
        <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
          {{ fmtBytes(entry.size_bytes) }}
        </td>
        <td class="px-xs py-xs">
          <span
            v-if="entry.cache_key"
            class="font-mono text-labelSmall text-tertiary flex items-center gap-unit"
          >
            <Icon name="check_circle" :size="14" />
            transcribed
          </span>
          <span v-else class="font-mono text-labelSmall text-outline">—</span>
        </td>
        <td class="px-margin py-xs text-right">
          <div class="inline-flex gap-unit" @click.stop>
            <Button
              v-if="busy[entry.path]"
              variant="ghost-error"
              shape="icon"
              icon="stop"
              :icon-size="18"
              title="Stop transcription"
              @click="emit('stop', entry)"
            />
            <Button
              v-else
              variant="ghost"
              shape="icon"
              title="Transcribe"
              @click="emit('transcribe', entry)"
            >
              <TranscribeIcon :size="18" />
            </Button>
            <Button
              variant="ghost"
              shape="icon"
              icon="content_cut"
              :icon-size="18"
              :class="entry.trim_start_ms || entry.trim_end_ms ? 'text-primary' : ''"
              :title="
                entry.trim_start_ms || entry.trim_end_ms
                  ? `Trim: ${fmtMs(entry.trim_start_ms ?? 0)} – ${fmtMs(
                      entry.trim_end_ms ?? entry.duration_ms ?? 0,
                    )}`
                  : 'Trim — select range to transcribe'
              "
              @click="emit('trim', entry)"
            />
            <Button
              variant="ghost"
              shape="icon"
              :title="autoRenamingPath === entry.path ? 'Renaming…' : 'Auto-rename (AI)'"
              :disabled="autoRenamingPath === entry.path"
              @click="emit('auto-rename', entry)"
            >
              <Spinner v-if="autoRenamingPath === entry.path" :size="18" />
              <Icon v-else name="auto_awesome" :size="18" />
            </Button>
            <Button
              variant="ghost"
              shape="icon"
              icon="drive_file_rename_outline"
              :icon-size="18"
              title="Rename"
              @click="emit('rename', entry)"
            />
            <Button
              variant="ghost"
              shape="icon"
              icon="download"
              :icon-size="18"
              title="Export transcript"
              :disabled="!entry.cache_key"
              @click="emit('export', entry)"
            />
            <Button
              variant="ghost-error"
              shape="icon"
              icon="delete"
              :icon-size="18"
              title="Delete"
              @click="emit('delete', entry)"
            />
          </div>
        </td>
      </tr>
    </tbody>
  </table>
</template>
