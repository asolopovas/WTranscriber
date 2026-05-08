<script setup lang="ts">
defineProps<{ label: string }>();
</script>

<template>
  <label class="block">
    <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
      {{ label }}
    </span>
    <div class="mt-unit"><slot /></div>
  </label>
</template>
