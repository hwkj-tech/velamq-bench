<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { init, type ECharts } from 'echarts/core';
import type { EChartsCoreOption } from 'echarts/core';
import { useUIStore } from '@/stores/ui';
import './register';

const props = withDefaults(
  defineProps<{
    option: EChartsCoreOption;
    height?: string;
  }>(),
  { height: '320px' },
);

const ui = useUIStore();
const el = ref<HTMLDivElement | null>(null);
let chart: ECharts | null = null;
let observer: ResizeObserver | null = null;

const chartStyle = computed(() => ({ width: '100%', height: props.height }));

function render() {
  if (!el.value) return;
  chart?.dispose();
  chart = init(el.value, ui.isDark ? 'dark' : undefined);
  chart.setOption(props.option, true);
}

onMounted(() => {
  render();
  if (el.value) {
    observer = new ResizeObserver(() => chart?.resize());
    observer.observe(el.value);
  }
});

watch(() => props.option, (option) => chart?.setOption(option, true), { deep: true });
watch(() => ui.resolvedTheme, render);

onBeforeUnmount(() => {
  observer?.disconnect();
  chart?.dispose();
});
</script>

<template>
  <div class="echart-frame">
    <div ref="el" class="echart-base" :style="chartStyle" />
  </div>
</template>
