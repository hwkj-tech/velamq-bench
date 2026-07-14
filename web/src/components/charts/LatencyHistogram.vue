<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import EChartBase from './EChartBase.vue';
import { chartGrid, chartTooltip, valueAxis } from './chart-utils';
import type { MetricSnapshot } from '@/api/types';

const props = withDefaults(defineProps<{ snapshots: MetricSnapshot[]; height?: string }>(), { height: '300px' });
const chartHeight = computed(() => props.height ?? '300px');

const option = computed<EChartsCoreOption>(() => {
  const recent = props.snapshots.slice(-60);
  const values = recent.flatMap((snapshot) => [
    snapshot.latency_p50_ms,
    snapshot.latency_p90_ms,
    snapshot.latency_p95_ms,
    snapshot.latency_p99_ms,
  ]);
  const max = Math.max(...values, 1);
  const bucketSize = Math.max(1, Math.ceil(max / 12));
  const buckets = Array.from({ length: 12 }, (_, index) => ({
    label: `${index * bucketSize}-${(index + 1) * bucketSize}`,
    count: 0,
  }));
  for (const value of values) {
    const bucket = Math.min(buckets.length - 1, Math.floor(value / bucketSize));
    const target = buckets[bucket];
    if (target) target.count += 1;
  }
  return {
    animation: true,
    animationDuration: 700,
    tooltip: chartTooltip(),
    grid: chartGrid(28, 48),
    xAxis: {
      type: 'category',
      data: buckets.map((bucket) => bucket.label),
      axisLabel: { color: '#6f93a8' },
      axisLine: { lineStyle: { color: 'rgba(70,151,187,.35)' } },
      axisTick: { show: false },
    },
    yAxis: valueAxis(buckets.map((bucket) => bucket.count)),
    series: [{
      name: 'samples',
      type: 'bar',
      barMaxWidth: 28,
      itemStyle: {
        color: { type: 'linear', x: 0, y: 0, x2: 0, y2: 1, colorStops: [{ offset: 0, color: '#2ce5ff' }, { offset: 1, color: '#1465d4' }] },
        borderColor: '#6eeaff',
        borderWidth: 1,
        borderRadius: [6, 6, 1, 1],
        shadowColor: 'rgba(36,200,255,.4)',
        shadowBlur: 12,
      },
      data: buckets.map((bucket) => bucket.count),
    }],
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
