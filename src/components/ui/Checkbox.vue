<script setup lang="ts">
defineProps<{ modelValue: boolean; disabled?: boolean }>();
defineEmits<{ (e: "update:modelValue", v: boolean): void }>();
</script>
<template>
  <input
    type="checkbox"
    class="w-4 h-4 accent-primary rounded cursor-pointer disabled:cursor-not-allowed disabled:opacity-40 shrink-0"
    :checked="modelValue"
    :disabled="disabled"
    @change="$emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
  />
</template>
