<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import { useI18n } from 'vue-i18n';
import EChartBase from './EChartBase.vue';
import { chartDataZoom, chartGrid, chartLegend, chartTooltip, emptyChartGraphic, groupedSnapshots, hasVisibleSignal, lineSeriesBase, shortSeriesName, timeAxis, valueAxis, xOf } from './chart-utils';
import type { Annotation, MetricSnapshot } from '@/api/types';

const props = withDefaults(
  defineProps<{
    snapshots: MetricSnapshot[];
    annotations?: Annotation[];
    aggregate?: boolean;
    height?: string;
  }>(),
  { annotations: () => [], aggregate: false, height: '320px' },
);
const chartHeight = computed(() => props.height ?? '320px');
const { t } = useI18n();

const option = computed<EChartsCoreOption>(() => {
  const groups = groupedSnapshots(props.snapshots);
  const values = props.snapshots.flatMap((snapshot) => [snapshot.publish_rate, snapshot.receive_rate]);
  const xs = props.snapshots.map(xOf);
  const pointCount = props.snapshots.length;
  const series = props.aggregate
    ? [
        {
          ...lineSeriesBase(0, pointCount),
          name: 'publish/s',
          type: 'line',
          data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.publish_rate]),
        },
        {
          ...lineSeriesBase(1, pointCount),
          name: 'receive/s',
          type: 'line',
          data: props.snapshots.map((snapshot) => [xOf(snapshot), snapshot.receive_rate]),
        },
      ]
    : Object.entries(groups).flatMap(([id, points], groupIndex) => [
        {
          ...lineSeriesBase(groupIndex * 2, points.length),
          name: `${shortSeriesName(id)} pub`,
          type: 'line',
          data: points.map((snapshot) => [xOf(snapshot), snapshot.publish_rate]),
        },
        {
          ...lineSeriesBase(groupIndex * 2 + 1, points.length),
          name: `${shortSeriesName(id)} recv`,
          type: 'line',
          data: points.map((snapshot) => [xOf(snapshot), snapshot.receive_rate]),
        },
      ]);
  return {
    animation: true,
    animationDuration: 700,
    animationDurationUpdate: 260,
    color: ['#24c8ff', '#17e7b5', '#8b7cff', '#ffb63d', '#ff5277', '#4f7cff'],
    tooltip: chartTooltip(),
    legend: chartLegend(),
    grid: chartGrid(),
    xAxis: timeAxis(xs),
    yAxis: valueAxis(values, undefined, 100),
    dataZoom: chartDataZoom(),
    graphic: emptyChartGraphic(pointCount === 0 || !hasVisibleSignal(values), t('charts.emptyThroughput')),
    series,
  };
});
</script>

<template>
  <EChartBase :option="option" :height="chartHeight" />
</template>
