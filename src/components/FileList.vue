<script setup lang="ts">
import { computed, ref } from "vue";
import type { DirEntry, TranscribeProgress } from "@/types";
import { decodeName, phaseLabel, prettyName } from "@utils/audio";
import { fmtMs, fmtBytes } from "@composables/format";
import TranscribeIcon from "@components/icons/TranscribeIcon.vue";
import Spinner from "@components/icons/Spinner.vue";

const props = defineProps<{
  entries: DirEntry[];
  selectedPath: string;
  busy: Record<string, boolean>;
  progressByPath: Record<string, TranscribeProgress>;
  autoRenamingPath: string | null;
  dragOver: boolean;
  hasListing: boolean;
}>();

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
}>();

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
      class="border-b border-outline-variant/20 px-margin py-md cursor-pointer transition-colors"
      :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
      @click="emit('choose', entry)"
    >
      <div class="flex items-center gap-xs">
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-xs">
            <div class="flex-1 min-w-0 text-bodyMedium text-on-surface break-words" :title="title">
              {{ pretty.display }}
            </div>
            <div class="flex items-center gap-unit shrink-0 -mr-xs" @click.stop>
              <button
                v-if="busy[entry.path]"
                class="material-symbols-outlined w-10 h-10 flex items-center justify-center rounded-full text-error hover:bg-error-container/40 transition-colors"
                title="Stop"
                @click="emit('stop', entry)"
              >
                stop
              </button>
              <button
                v-else
                class="w-10 h-10 flex items-center justify-center rounded-full text-primary hover:bg-surface-container-highest transition-colors"
                title="Transcribe"
                @click="emit('transcribe', entry)"
              >
                <TranscribeIcon :size="20" />
              </button>
              <div class="relative">
                <button
                  class="material-symbols-outlined w-10 h-10 flex items-center justify-center rounded-full text-on-surface-variant hover:bg-surface-container-highest transition-colors"
                  title="More"
                  @click="toggleMenu(entry.path)"
                >
                  more_vert
                </button>
                <div
                  v-if="openMenuPath === entry.path"
                  class="absolute right-0 top-full mt-unit z-30 min-w-[180px] bg-surface-container-high border border-outline-variant/60 rounded-lg shadow-2xl py-unit"
                >
                  <button
                    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors"
                    @click="
                      openMenuPath = null;
                      emit('trim', entry);
                    "
                  >
                    <span class="material-symbols-outlined text-[18px]">content_cut</span>
                    Cut / select range
                  </button>
                  <button
                    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors disabled:opacity-50"
                    :disabled="autoRenamingPath === entry.path"
                    @click="
                      openMenuPath = null;
                      emit('auto-rename', entry);
                    "
                  >
                    <Spinner v-if="autoRenamingPath === entry.path" :size="18" />
                    <span v-else class="material-symbols-outlined text-[18px]">auto_awesome</span>
                    {{ autoRenamingPath === entry.path ? "Renaming…" : "Auto-rename" }}
                  </button>
                  <button
                    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors"
                    @click="
                      openMenuPath = null;
                      emit('rename', entry);
                    "
                  >
                    <span class="material-symbols-outlined text-[18px]">
                      drive_file_rename_outline
                    </span>
                    Rename
                  </button>
                  <button
                    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors disabled:opacity-30"
                    :disabled="!entry.cache_key"
                    @click="
                      openMenuPath = null;
                      emit('export', entry);
                    "
                  >
                    <span class="material-symbols-outlined text-[18px]">download</span>
                    Export
                  </button>
                  <div class="my-unit border-t border-outline-variant/40"></div>
                  <button
                    class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-error hover:bg-error-container/40 transition-colors"
                    @click="
                      openMenuPath = null;
                      emit('delete', entry);
                    "
                  >
                    <span class="material-symbols-outlined text-[18px]">delete</span>
                    Delete
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div
        v-if="busy[entry.path]"
        class="flex items-center gap-xs mt-xs font-mono text-labelSmall text-secondary"
      >
        <span class="material-symbols-outlined text-[14px] animate-pulse">graphic_eq</span>
        <span>
          {{
            progressByPath[entry.path]
              ? phaseLabel(progressByPath[entry.path].phase)
              : "transcribing"
          }}
        </span>
      </div>
    </li>
  </ul>

  <table v-if="rows.length" class="hidden md:table w-full text-bodyMedium">
    <thead class="sticky top-0 bg-surface z-10 border-b border-outline-variant/40">
      <tr
        class="text-left font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
      >
        <th class="px-margin py-xs w-8"></th>
        <th class="px-xs py-xs">Name</th>
        <th class="px-xs py-xs w-24">Duration</th>
        <th class="px-xs py-xs w-24">Size</th>
        <th class="px-xs py-xs w-28">Status</th>
        <th class="px-margin py-xs w-[200px]"></th>
      </tr>
    </thead>
    <tbody>
      <tr
        v-for="{ entry, pretty, title } in rows"
        :key="entry.path"
        class="border-b border-outline-variant/20 hover:bg-surface-container-high/40 cursor-pointer transition-colors"
        :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
        @click="emit('choose', entry)"
        @dblclick="emit('transcribe', entry)"
      >
        <td class="px-margin py-xs">
          <span class="material-symbols-outlined text-[20px] text-on-surface-variant">
            graphic_eq
          </span>
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
          <template v-if="busy[entry.path]">
            <span class="font-mono text-labelSmall text-secondary flex items-center gap-unit">
              <span class="material-symbols-outlined text-[14px] animate-pulse">graphic_eq</span>
              <span>
                {{
                  progressByPath[entry.path]
                    ? phaseLabel(progressByPath[entry.path].phase)
                    : "transcribing"
                }}
              </span>
            </span>
          </template>
          <span
            v-else-if="entry.cache_key"
            class="font-mono text-labelSmall text-tertiary flex items-center gap-unit"
          >
            <span class="material-symbols-outlined text-[14px]">check_circle</span>
            transcribed
          </span>
          <span v-else class="font-mono text-labelSmall text-outline">—</span>
        </td>
        <td class="px-margin py-xs text-right">
          <div class="inline-flex gap-unit" @click.stop>
            <button
              v-if="busy[entry.path]"
              class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-error-container/40 text-error transition-colors"
              title="Stop transcription"
              @click="emit('stop', entry)"
            >
              stop
            </button>
            <button
              v-else
              class="p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-primary transition-colors"
              title="Transcribe"
              @click="emit('transcribe', entry)"
            >
              <TranscribeIcon :size="18" />
            </button>
            <button
              class="p-unit rounded hover:bg-surface-container-highest text-on-surface-variant transition-colors"
              :class="
                entry.trim_start_ms || entry.trim_end_ms ? 'text-primary' : 'hover:text-primary'
              "
              :title="
                entry.trim_start_ms || entry.trim_end_ms
                  ? `Trim: ${fmtMs(entry.trim_start_ms ?? 0)} – ${fmtMs(
                      entry.trim_end_ms ?? entry.duration_ms ?? 0,
                    )}`
                  : 'Trim — select range to transcribe'
              "
              @click="emit('trim', entry)"
            >
              <span class="material-symbols-outlined text-[18px]">content_cut</span>
            </button>
            <button
              class="p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-secondary transition-colors disabled:opacity-50"
              :title="autoRenamingPath === entry.path ? 'Renaming…' : 'Auto-rename (AI)'"
              :disabled="autoRenamingPath === entry.path"
              @click="emit('auto-rename', entry)"
            >
              <Spinner v-if="autoRenamingPath === entry.path" :size="18" />
              <span v-else class="material-symbols-outlined text-[18px]">auto_awesome</span>
            </button>
            <button
              class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
              title="Rename"
              @click="emit('rename', entry)"
            >
              drive_file_rename_outline
            </button>
            <button
              class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
              title="Export transcript"
              :disabled="!entry.cache_key"
              :class="!entry.cache_key ? 'opacity-30 cursor-not-allowed' : ''"
              @click="emit('export', entry)"
            >
              download
            </button>
            <button
              class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-error-container/40 text-on-surface-variant hover:text-error transition-colors"
              title="Delete"
              @click="emit('delete', entry)"
            >
              delete
            </button>
          </div>
        </td>
      </tr>
    </tbody>
  </table>
</template>
