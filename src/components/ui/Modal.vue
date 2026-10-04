<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import Button from "@components/ui/Button.vue";

const props = withDefaults(
  defineProps<{
    open: boolean;
    title?: string;
    width?: string;
    showClose?: boolean;
    backdropClose?: boolean;
    inactive?: boolean;
  }>(),
  { width: "420px", showClose: false, backdropClose: true },
);

const emit = defineEmits<{ (e: "close"): void }>();

let previousFocus: HTMLElement | null = null;
const dialogElement = ref<HTMLElement | null>(null);
watch(
  () => props.open,
  async (open) => {
    if (open) {
      previousFocus = document.activeElement as HTMLElement | null;
      await nextTick();
      const focusTarget =
        dialogElement.value?.querySelector<HTMLElement>("[autofocus]:not(:disabled)") ??
        dialogElement.value?.querySelector<HTMLElement>(
          "button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled)",
        );
      focusTarget?.focus();
    } else {
      previousFocus?.focus();
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => previousFocus?.focus());

function onBackdrop() {
  emit("close");
}

function trapFocus(event: KeyboardEvent) {
  const dialog = (event.currentTarget as HTMLElement).querySelector('[role="dialog"]');
  const controls = Array.from(
    dialog?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]',
    ) ?? [],
  ).filter((element) => element.getClientRects().length > 0);
  const first = controls[0];
  const last = controls[controls.length - 1];
  if (!first || !last) return;
  if (!dialog?.contains(document.activeElement)) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus();
  } else if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <Transition
    enter-active-class="transition-opacity duration-150"
    enter-from-class="opacity-0"
    leave-active-class="transition-opacity duration-100"
    leave-to-class="opacity-0"
  >
    <div
      v-if="open"
      class="fixed inset-0 z-40 bg-black/50 flex items-center justify-center p-margin"
      @click.self="backdropClose && onBackdrop()"
      @keydown.escape="backdropClose && emit('close')"
      @keydown.tab="trapFocus"
    >
      <div
        class="bg-surface-container max-h-[calc(100dvh-2rem)] rounded-xl border border-outline-variant/40 w-full max-w-[90vw] flex flex-col overflow-hidden shadow-2xl"
        :style="{ maxWidth: width }"
        ref="dialogElement"
        :inert="inactive || undefined"
        :aria-hidden="inactive || undefined"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
      >
        <header
          v-if="title || showClose || $slots.header"
          class="px-margin py-md border-b border-outline-variant/40 bg-surface-container-low shrink-0 flex items-start gap-md"
        >
          <slot name="header">
            <h3 class="flex-1 text-titleSmall text-on-surface">{{ title }}</h3>
          </slot>
          <Button
            v-if="showClose"
            variant="ghost"
            shape="icon"
            icon="close"
            :icon-size="20"
            title="Close"
            @click="emit('close')"
          />
        </header>
        <div class="px-margin py-md space-y-md min-h-0 overflow-y-auto">
          <slot></slot>
        </div>
        <footer
          v-if="$slots.footer"
          class="px-margin py-md border-t border-outline-variant/40 bg-surface-container-low shrink-0 flex justify-between items-center gap-xs"
        >
          <slot name="footer"></slot>
        </footer>
      </div>
    </div>
  </Transition>
</template>
