<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import ConnectionsChart from '@/components/charts/ConnectionsChart.vue';
import type { MetricSnapshot } from '@/api/types';

defineProps<{ snapshots: MetricSnapshot[]; latest: MetricSnapshot | null }>();
const { t } = useI18n();
</script>

<template>
  <section class="run-tab-grid">
    <div class="kpi-strip">
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.connected') }}</span>
        <strong>{{ latest?.connected ?? 0 }}</strong>
      </article>
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.connectRate') }}</span>
        <strong>{{ (latest?.connect_rate ?? 0).toFixed(1) }}</strong>
      </article>
    </div>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.tabs.connections') }}</h2></div>
      <ConnectionsChart :snapshots="snapshots" height="420px" />
    </section>
  </section>
</template>
