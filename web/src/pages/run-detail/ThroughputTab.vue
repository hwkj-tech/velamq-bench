<script setup lang="ts">
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import ThroughputChart from '@/components/charts/ThroughputChart.vue';
import type { Annotation, MetricSnapshot } from '@/api/types';

defineProps<{ snapshots: MetricSnapshot[]; annotations: Annotation[] }>();
const aggregate = ref(false);
const { t } = useI18n();
</script>

<template>
  <section class="panel run-tab-panel">
    <div class="panel-head split">
      <h2>{{ t('runDetail.tabs.throughput') }}</h2>
      <div class="segmented compact">
        <button :class="{ active: !aggregate }" type="button" @click="aggregate = false">{{ t('runDetail.throughput.perWorkload') }}</button>
        <button :class="{ active: aggregate }" type="button" @click="aggregate = true">{{ t('runDetail.throughput.aggregate') }}</button>
      </div>
    </div>
    <ThroughputChart :snapshots="snapshots" :annotations="annotations" :aggregate="aggregate" height="460px" />
  </section>
</template>
