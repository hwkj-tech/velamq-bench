<script setup lang="ts">
defineProps<{
  label: string;
  compact?: boolean;
}>();
</script>

<template>
  <div class="app-feedback app-feedback-loading" :class="{ compact }" role="status" aria-live="polite">
    <span class="loading-dot" aria-hidden="true" />
    <strong>{{ label }}</strong>
  </div>
</template>
