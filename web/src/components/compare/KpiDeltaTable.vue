<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import type { Run } from '@/api/types';

interface Row {
  run: Run;
  published: number;
  received: number;
  p95: number;
  p99: number;
  errorRate: number;
  publishRate: number;
}

const props = defineProps<{ rows: Row[]; baselineId: string }>();
const { t } = useI18n();

function baselineValue(key: keyof Omit<Row, 'run'>) {
  return props.rows.find((row) => row.run.id === props.baselineId)?.[key] ?? 0;
}

function delta(value: number, key: keyof Omit<Row, 'run'>) {
  const baseline = baselineValue(key);
  if (!baseline) return '0.0%';
  const pct = ((value - baseline) / baseline) * 100;
  return `${pct >= 0 ? '+' : ''}${pct.toFixed(1)}%`;
}
</script>

<template>
  <section class="panel">
    <div class="panel-head"><h2>{{ t('compare.kpiDelta') }}</h2></div>
    <table class="kpi-table">
      <thead>
        <tr>
          <th>{{ t('compare.table.run') }}</th>
          <th>{{ t('compare.table.published') }}</th>
          <th>{{ t('compare.table.publishRate') }}</th>
          <th>P95</th>
          <th>P99</th>
          <th>{{ t('compare.table.errorRate') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.run.id">
          <td>{{ row.run.name }}</td>
          <td>{{ row.published }} <span>{{ delta(row.published, 'published') }}</span></td>
          <td>{{ row.publishRate.toFixed(1) }} <span>{{ delta(row.publishRate, 'publishRate') }}</span></td>
          <td>{{ row.p95.toFixed(1) }} <span>{{ delta(row.p95, 'p95') }}</span></td>
          <td>{{ row.p99.toFixed(1) }} <span>{{ delta(row.p99, 'p99') }}</span></td>
          <td>{{ row.errorRate.toFixed(2) }} <span>{{ delta(row.errorRate, 'errorRate') }}</span></td>
        </tr>
      </tbody>
    </table>
  </section>
</template>
