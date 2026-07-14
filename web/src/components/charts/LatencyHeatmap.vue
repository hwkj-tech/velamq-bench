<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import EChartBase from './EChartBase.vue';
import { chartTooltip } from './chart-utils';
import type { MetricSnapshot } from '@/api/types';

const props = withDefaults(defineProps<{ snapshots: MetricSnapshot[]; height?: string }>(), { height: '300px' });
const chartHeight = computed(() => props.height ?? '300px');

const option = computed<EChartsCoreOption>(() => {
  const points = props.snapshots.slice(-80);
  const buckets = ['p50', 'p90', 'p95', 'p99', 'max'];
  const values = points.flatMap((snapshot, x) => [
    [x, 0, snapshot.latency_p50_ms],
    [x, 1, snapshot.latency_p90_ms],
    [x, 2, snapshot.latency_p95_ms],
    [x, 3, snapshot.latency_p99_ms],
    [x, 4, snapshot.latency_max_ms],
  ]);
  return {
    animation: true,
    animationDuration: 700,
    tooltip: { ...chartTooltip(), position: 'top' },
    grid: { left: 54, right: 28, top: 28, bottom: 76, containLabel: true },
    xAxis: { type: 'category', data: points.map((snapshot) => String(Math.round(snapshot.elapsed_ms / 1000))), axisLabel: { color: '#6f93a8' }, axisLine: { lineStyle: { color: 'rgba(70,151,187,.35)' } } },
    yAxis: { type: 'category', data: buckets, axisLabel: { color: '#6f93a8' }, axisLine: { lineStyle: { color: 'rgba(70,151,187,.35)' } } },
    visualMap: {
      min: 0,
      max: Math.max(...values.map((item) => Number(item[2])), 1),
      calculable: true,
      orient: 'horizontal',
      left: 'center',
      bottom: 12,
      itemHeight: 90,
      textStyle: { color: '#89aabd' },
      inRange: { color: ['#071a2c', '#075c87', '#16d5dc', '#ffe46b', '#ff5277'] },
    },
    series: [{ name: 'latency', type: 'heatmap', data: values, itemStyle: { borderColor: 'rgba(3,16,27,.8)', borderWidth: 2 }, emphasis: { itemStyle: { borderColor: '#dffaff', borderWidth: 1, shadowColor: '#24c8ff', shadowBlur: 12 } } }],
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
