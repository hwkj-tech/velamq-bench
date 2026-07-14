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
  const values = props.snapshots.flatMap((snapshot) => [snapshot.errors, snapshot.error_rate]);
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
    yAxis: valueAxis(values, undefined, 1),
    dataZoom: chartDataZoom(),
    graphic: emptyChartGraphic(pointCount === 0 || !hasVisibleSignal(values), t('charts.emptyErrors')),
    series: [
      {
        name: 'errors',
        type: 'bar',
        barMaxWidth: 14,
        itemStyle: {
          color: { type: 'linear', x: 0, y: 0, x2: 0, y2: 1, colorStops: [{ offset: 0, color: '#ff6688' }, { offset: 1, color: '#9d183d' }] },
          borderColor: '#ff7895',
          borderWidth: 1,
          borderRadius: [5, 5, 1, 1],
          shadowColor: 'rgba(255,82,119,.45)',
          shadowBlur: 10,
        },
        data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.errors]),
      },
      { ...lineSeriesBase(1, pointCount), name: 'error/s', type: 'line', data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.error_rate]) },
    ],
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
