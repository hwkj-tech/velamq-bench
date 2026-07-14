<script setup lang="ts">
import AppEmpty from '@/components/feedback/AppEmpty.vue';

defineProps<{ title: string; note: string }>();
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <h1>{{ title }}</h1>
      <p>{{ note }}</p>
    </div>
    <AppEmpty :title="title" :hint="note" />
  </section>
</template>
