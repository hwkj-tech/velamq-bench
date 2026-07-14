<script setup lang="ts">
import { computed } from 'vue';
import type { EChartsCoreOption } from 'echarts/core';
import EChartBase from '@/components/charts/EChartBase.vue';
import { chartDataZoom, chartGrid, chartLegend, chartTooltip, lineSeriesBase, timeAxis, valueAxis } from '@/components/charts/chart-utils';
import type { MetricSnapshot, Run } from '@/api/types';

const props = defineProps<{
  title: string;
  metric: keyof MetricSnapshot;
  items: Array<{ run: Run; snapshots: MetricSnapshot[] }>;
}>();

const option = computed<EChartsCoreOption>(() => {
  const values = props.items.flatMap((item) => item.snapshots.map((snapshot) => Number(snapshot[props.metric] ?? 0)));
  const xs = props.items.flatMap((item) => item.snapshots.map((snapshot) => snapshot.elapsed_ms / 1000));
  return {
    animation: true,
    animationDuration: 700,
    animationDurationUpdate: 260,
    tooltip: chartTooltip(),
    legend: chartLegend(),
    grid: chartGrid(),
    xAxis: timeAxis(xs),
    yAxis: valueAxis(values),
    dataZoom: chartDataZoom(),
    series: props.items.map((item, index) => ({
      ...lineSeriesBase(index, item.snapshots.length),
      name: item.run.name,
      type: 'line',
      data: item.snapshots.map((snapshot) => [snapshot.elapsed_ms / 1000, Number(snapshot[props.metric] ?? 0)]),
    })),
  };
});
</script>

<template>
  <section class="panel">
    <div class="panel-head"><h2>{{ title }}</h2></div>
    <EChartBase :option="option" height="320px" />
  </section>
</template>
