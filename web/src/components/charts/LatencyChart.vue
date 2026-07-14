<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import { useI18n } from 'vue-i18n';
import EChartBase from './EChartBase.vue';
import { chartDataZoom, chartGrid, chartLegend, chartTooltip, emptyChartGraphic, hasVisibleSignal, lineSeriesBase, timeAxis, valueAxis, xOf } from './chart-utils';
import type { Annotation, MetricSnapshot } from '@/api/types';

const props = withDefaults(
  defineProps<{
    snapshots: MetricSnapshot[];
    annotations?: Annotation[];
    logScale?: boolean;
    height?: string;
  }>(),
  { annotations: () => [], logScale: false, height: '320px' },
);
const chartHeight = computed(() => props.height ?? '320px');
const { t } = useI18n();

const metrics = [
  ['avg', 'latency_avg_ms'],
  ['p50', 'latency_p50_ms'],
  ['p90', 'latency_p90_ms'],
  ['p95', 'latency_p95_ms'],
  ['p99', 'latency_p99_ms'],
  ['p99.9', 'latency_p999_ms'],
  ['max', 'latency_max_ms'],
] as const;

const option = computed<EChartsCoreOption>(() => {
  const values = metrics.flatMap(([, key]) => props.snapshots.map((snapshot) => Number(snapshot[key] ?? 0)));
  const xs = props.snapshots.map(xOf);
  const pointCount = props.snapshots.length;
  return {
    animation: true,
    animationDuration: 700,
    animationDurationUpdate: 260,
    tooltip: chartTooltip(),
    legend: chartLegend(),
    grid: chartGrid(),
    xAxis: timeAxis(xs),
    yAxis: props.logScale ? { ...valueAxis(values, 'ms', 10), type: 'log', min: 1 } : valueAxis(values, 'ms', 10),
    dataZoom: chartDataZoom(),
    graphic: emptyChartGraphic(pointCount === 0 || !hasVisibleSignal(values), t('charts.emptyLatency')),
    series: metrics.map(([name, key], index) => ({
      ...lineSeriesBase(index, pointCount),
      name,
      type: 'line',
      data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot[key]]),
    })),
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
