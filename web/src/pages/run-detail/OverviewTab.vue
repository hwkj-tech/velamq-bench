<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import ThroughputChart from '@/components/charts/ThroughputChart.vue';
import LatencyChart from '@/components/charts/LatencyChart.vue';
import WorkloadMiniCard from '@/components/run/WorkloadMiniCard.vue';
import type { Annotation, MetricSnapshot, Run } from '@/api/types';

defineProps<{
  run: Run | null;
  snapshots: MetricSnapshot[];
  annotations: Annotation[];
  latest: MetricSnapshot | null;
  snapshotsByWorkload: Record<string, MetricSnapshot[]>;
}>();
const { t } = useI18n();
</script>

<template>
  <section class="run-tab-grid">
    <div class="kpi-strip">
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.published') }}</span>
        <strong>{{ latest?.published ?? 0 }}</strong>
      </article>
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.received') }}</span>
        <strong>{{ latest?.received ?? 0 }}</strong>
      </article>
      <article class="kpi-card">
        <span>P95</span>
        <strong>{{ (latest?.latency_p95_ms ?? 0).toFixed(1) }} ms</strong>
      </article>
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.errorRate') }}</span>
        <strong>{{ (latest?.error_rate ?? 0).toFixed(2) }}/s</strong>
      </article>
    </div>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.tabs.throughput') }}</h2></div>
      <ThroughputChart :snapshots="snapshots" :annotations="annotations" aggregate height="260px" />
    </section>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.latencyBand') }}</h2></div>
      <LatencyChart :snapshots="snapshots" :annotations="annotations" height="260px" />
    </section>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('scenarios.workloads') }}</h2></div>
      <div class="workload-mini-grid">
        <WorkloadMiniCard
          v-for="workload in run?.workloads ?? []"
          :key="workload.id"
          :workload="workload"
          :latest="snapshotsByWorkload[workload.id]?.at(-1) ?? null"
        />
      </div>
    </section>
  </section>
</template>
