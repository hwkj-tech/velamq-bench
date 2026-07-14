<script setup lang="ts">
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import LatencyChart from '@/components/charts/LatencyChart.vue';
import LatencyHistogram from '@/components/charts/LatencyHistogram.vue';
import LatencyHeatmap from '@/components/charts/LatencyHeatmap.vue';
import type { Annotation, MetricSnapshot } from '@/api/types';

defineProps<{ snapshots: MetricSnapshot[]; annotations: Annotation[] }>();

const mode = ref<'lines' | 'histogram' | 'heatmap'>('lines');
const logScale = ref(false);
const { t } = useI18n();
</script>

<template>
  <section class="panel run-tab-panel">
    <div class="panel-head split">
      <h2>{{ t('runDetail.tabs.latency') }}</h2>
      <div class="segmented compact">
        <button :class="{ active: mode === 'lines' }" type="button" @click="mode = 'lines'">{{ t('runDetail.latency.lines') }}</button>
        <button :class="{ active: mode === 'histogram' }" type="button" @click="mode = 'histogram'">{{ t('runDetail.latency.histogram') }}</button>
        <button :class="{ active: mode === 'heatmap' }" type="button" @click="mode = 'heatmap'">{{ t('runDetail.latency.heatmap') }}</button>
        <button :class="{ active: logScale }" type="button" @click="logScale = !logScale">{{ t('runDetail.latency.log') }}</button>
      </div>
    </div>
    <LatencyChart v-if="mode === 'lines'" :snapshots="snapshots" :annotations="annotations" :log-scale="logScale" height="460px" />
    <LatencyHistogram v-else-if="mode === 'histogram'" :snapshots="snapshots" height="460px" />
    <LatencyHeatmap v-else :snapshots="snapshots" height="460px" />
  </section>
</template>
