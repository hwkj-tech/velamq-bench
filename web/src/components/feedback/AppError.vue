<script setup lang="ts">
defineProps<{
  message: string;
}>();
</script>

<template>
  <p v-if="message" class="app-feedback app-feedback-error" role="alert">
    {{ message }}
  </p>
</template>
