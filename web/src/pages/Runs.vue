<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { RouterLink, useRoute, useRouter } from 'vue-router';
import { GitCompare, RefreshCw, Trash2, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { useRunsStore } from '@/stores/runs';
import { useToast } from '@/composables/useToast';
import { useCompareData } from '@/composables/useCompareData';
import KpiDeltaTable from '@/components/compare/KpiDeltaTable.vue';
import OverlayChart from '@/components/compare/OverlayChart.vue';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';

const runs = useRunsStore();
const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const toast = useToast();
const compare = useCompareData();
const compareMode = ref(Boolean(route.query.ids));
const selectedIds = ref<string[]>(String(route.query.ids ?? '').split(',').filter(Boolean).slice(0, 4));
const baselineId = ref(selectedIds.value[0] ?? '');
const canCompare = computed(() => selectedIds.value.length >= 2);

onMounted(async () => {
  await runs.load(80);
  if (selectedIds.value.length) {
    compareMode.value = true;
    await compare.load(selectedIds.value);
  }
});

watch(selectedIds, async (ids) => {
  baselineId.value = ids.includes(baselineId.value) ? baselineId.value : (ids[0] ?? '');
  await router.replace({ path: '/runs', query: ids.length ? { ids: ids.join(',') } : {} });
  await compare.load(ids);
});

function toggleCompareMode() {
  compareMode.value = !compareMode.value;
  if (!compareMode.value) {
    selectedIds.value = [];
  }
}

function toggleRunSelection(id: string) {
  selectedIds.value = selectedIds.value.includes(id)
    ? selectedIds.value.filter((item) => item !== id)
    : [...selectedIds.value, id].slice(0, 4);
}

async function removeRun(id: string, name: string) {
  if (!window.confirm(t('runs.confirmDelete', { name }))) return;
  try {
    await runs.remove(id);
    selectedIds.value = selectedIds.value.filter((item) => item !== id);
    toast.success(t('runs.deleted'));
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('runs.title') }}</h1>
        <p>{{ t('runs.subtitle') }}</p>
      </div>
      <div class="page-title-actions">
        <select v-if="compareMode && selectedIds.length" v-model="baselineId" class="control compact">
          <option v-for="item in compare.items.value" :key="item.run.id" :value="item.run.id">{{ item.run.name }}</option>
        </select>
        <button class="secondary-action" type="button" @click="toggleCompareMode">
          <component :is="compareMode ? X : GitCompare" :size="16" />
          {{ compareMode ? t('runs.exitCompare') : t('runs.compareAction') }}
        </button>
        <button class="secondary-action" type="button" @click="runs.load(80)">
          <RefreshCw :size="16" />
          {{ t('common.refresh') }}
        </button>
      </div>
    </div>
    <section class="panel">
      <div class="run-table">
        <div class="run-table-head run-row-runs" :class="{ comparing: compareMode }" aria-hidden="true">
          <span v-if="compareMode"></span>
          <span>{{ t('runs.table.name') }}</span>
          <span>{{ t('runs.table.status') }}</span>
          <span>{{ t('runs.table.workloads') }}</span>
          <span>{{ t('runs.table.startedAt') }}</span>
          <span>{{ t('runs.table.actions') }}</span>
        </div>
        <div v-for="run in runs.list" :key="run.id" class="run-row run-row-runs" :class="{ comparing: compareMode }">
          <label v-if="compareMode" class="compare-check" :aria-label="t('runs.selectForCompare', { name: run.name })">
            <input
              type="checkbox"
              :checked="selectedIds.includes(run.id)"
              :disabled="!selectedIds.includes(run.id) && selectedIds.length >= 4"
              @change="toggleRunSelection(run.id)"
            />
          </label>
          <RouterLink class="row-primary-link" :to="`/runs/${run.id}`">{{ run.name }}</RouterLink>
          <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
          <span>{{ t('runs.workloadCount', { count: run.workloads.length }) }}</span>
          <span>{{ new Date(run.started_at).toLocaleString() }}</span>
          <button class="table-action danger" type="button" :disabled="run.status === 'running'" @click="removeRun(run.id, run.name)">
            <Trash2 :size="15" />
            {{ t('common.delete') }}
          </button>
        </div>
        <AppEmpty v-if="!runs.loading && runs.list.length === 0" :title="t('runs.empty')" compact />
      </div>
    </section>

    <template v-if="compareMode">
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
    </template>
  </section>
</template>
