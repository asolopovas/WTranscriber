<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    align?: "left" | "right";
    breakAll?: boolean;
  }>(),
  { align: "left", breakAll: false },
);
</script>

<template>
  <div class="flex justify-between gap-md">
    <dt class="text-on-surface-variant">{{ label }}</dt>
    <dd
      class="text-on-surface font-mono"
      :class="[align === 'right' ? 'text-right' : '', breakAll ? 'break-all' : '']"
    >
      <slot />
    </dd>
  </div>
</template>
