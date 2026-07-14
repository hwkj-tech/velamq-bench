<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import { useI18n } from 'vue-i18n';
import EChartBase from './EChartBase.vue';
import { chartDataZoom, chartGrid, chartLegend, chartTooltip, emptyChartGraphic, hasVisibleSignal, lineSeriesBase, timeAxis, valueAxis, xOf } from './chart-utils';
import type { MetricSnapshot } from '@/api/types';

const props = withDefaults(defineProps<{ snapshots: MetricSnapshot[]; height?: string }>(), { height: '320px' });
const chartHeight = computed(() => props.height ?? '320px');
const { t } = useI18n();

const option = computed<EChartsCoreOption>(() => {
  const values = props.snapshots.flatMap((snapshot) => [snapshot.connected, snapshot.connect_rate]);
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
    yAxis: valueAxis(values, undefined, 10),
    dataZoom: chartDataZoom(),
    graphic: emptyChartGraphic(pointCount === 0 || !hasVisibleSignal(values), t('charts.emptyConnections')),
    series: [
      { ...lineSeriesBase(0, pointCount), name: 'connected', type: 'line', data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.connected]) },
      { ...lineSeriesBase(1, pointCount), name: 'connect/s', type: 'line', data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.connect_rate]) },
    ],
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
