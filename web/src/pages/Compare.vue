<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import RunPicker from '@/components/compare/RunPicker.vue';
import KpiDeltaTable from '@/components/compare/KpiDeltaTable.vue';
import OverlayChart from '@/components/compare/OverlayChart.vue';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import { api } from '@/api/client';
import { useCompareData } from '@/composables/useCompareData';
import type { Run } from '@/api/types';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const runs = ref<Run[]>([]);
const selectedIds = ref<string[]>(String(route.query.ids ?? '').split(',').filter(Boolean).slice(0, 4));
const baselineId = ref(selectedIds.value[0] ?? '');
const compare = useCompareData();

const canCompare = computed(() => selectedIds.value.length >= 2);

onMounted(async () => {
  runs.value = await api.runs(50);
  await compare.load(selectedIds.value);
});

watch(selectedIds, async (ids) => {
  baselineId.value = ids.includes(baselineId.value) ? baselineId.value : (ids[0] ?? '');
  await router.replace({ path: '/compare', query: ids.length ? { ids: ids.join(',') } : {} });
  await compare.load(ids);
});

function toggleRun(id: string) {
  selectedIds.value = selectedIds.value.includes(id)
    ? selectedIds.value.filter((item) => item !== id)
    : [...selectedIds.value, id].slice(0, 4);
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('compare.title') }}</h1>
        <p>{{ t('compare.subtitle') }}</p>
      </div>
      <select v-if="selectedIds.length" v-model="baselineId" class="control compact">
        <option v-for="item in compare.items.value" :key="item.run.id" :value="item.run.id">{{ item.run.name }}</option>
      </select>
    </div>

    <RunPicker :runs="runs" :selected-ids="selectedIds" @toggle="toggleRun" />
    <AppError :message="compare.error.value" />

    <AppEmpty v-if="!canCompare" :title="t('compare.empty')" compact />
    <template v-else>
      <KpiDeltaTable :rows="compare.stats.value" :baseline-id="baselineId" />
      <div class="compare-chart-grid">
        <OverlayChart :title="t('compare.charts.publishRate')" metric="publish_rate" :items="compare.items.value" />
        <OverlayChart :title="t('compare.charts.p95')" metric="latency_p95_ms" :items="compare.items.value" />
        <OverlayChart :title="t('compare.charts.p99')" metric="latency_p99_ms" :items="compare.items.value" />
        <OverlayChart :title="t('compare.charts.errors')" metric="error_rate" :items="compare.items.value" />
      </div>
    </template>
  </section>
</template>
