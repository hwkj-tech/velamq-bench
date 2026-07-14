<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import ErrorChart from '@/components/charts/ErrorChart.vue';
import type { LogLine, MetricSnapshot } from '@/api/types';

defineProps<{ snapshots: MetricSnapshot[]; logs: LogLine[]; latest: MetricSnapshot | null }>();
const { t } = useI18n();
</script>

<template>
  <section class="run-tab-grid">
    <div class="kpi-strip">
      <article class="kpi-card">
        <span>{{ t('runDetail.tabs.errors') }}</span>
        <strong>{{ latest?.errors ?? 0 }} {{ t('runDetail.units.events') }}</strong>
      </article>
      <article class="kpi-card">
        <span>{{ t('runDetail.metrics.errorRate') }}</span>
        <strong>{{ (latest?.error_rate ?? 0).toFixed(2) }} {{ t('runDetail.units.eventsPerSecond') }}</strong>
      </article>
    </div>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.errorTrend') }}</h2></div>
      <ErrorChart :snapshots="snapshots" height="360px" />
    </section>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.recentErrors') }}</h2></div>
      <div class="log-list">
        <div v-for="log in logs.filter((item) => item.level === 'error').slice(-50)" :key="`${log.ts}-${log.message}`" class="log-row">
          <span class="run-chip">{{ log.level }}</span>
          <span>{{ new Date(log.ts).toLocaleTimeString() }}</span>
          <code>{{ log.message }}</code>
        </div>
      </div>
    </section>
  </section>
</template>
