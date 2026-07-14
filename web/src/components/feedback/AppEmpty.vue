<script setup lang="ts">
defineProps<{
  title: string;
  hint?: string;
  compact?: boolean;
}>();
</script>

<template>
  <div class="app-feedback app-feedback-empty" :class="{ compact }" role="status">
    <strong>{{ title }}</strong>
    <span v-if="hint">{{ hint }}</span>
    <slot />
  </div>
</template>
