<script setup lang="ts">
import type { MetricSnapshot, RunWorkload } from '@/api/types';

defineProps<{
  workload: RunWorkload;
  latest?: MetricSnapshot | null;
}>();
</script>

<template>
  <article class="workload-mini">
    <strong>{{ workload.kind }} · {{ workload.workload_id }}</strong>
    <span>{{ latest?.connected ?? 0 }} connected</span>
    <span>{{ (latest?.publish_rate ?? 0).toFixed(1) }}/s pub</span>
    <span>{{ (latest?.latency_p95_ms ?? 0).toFixed(1) }} ms p95</span>
  </article>
</template>
