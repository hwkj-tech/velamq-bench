<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { RouterLink, useRoute, useRouter } from 'vue-router';
import { GitCompare, RefreshCw, Search, Trash2, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { useRunsStore } from '@/stores/runs';
import { useToast } from '@/composables/useToast';
import { useCompareData } from '@/composables/useCompareData';
import KpiDeltaTable from '@/components/compare/KpiDeltaTable.vue';
import OverlayChart from '@/components/compare/OverlayChart.vue';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';
import type { RunStatus } from '@/api/types';

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
const query = ref('');
const statusFilter = ref<'all' | RunStatus>('all');
const filteredRuns = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase();
  return runs.list.filter((run) => {
    const matchesStatus = statusFilter.value === 'all' || run.status === statusFilter.value;
    const searchable = `${run.name} ${run.description} ${run.tags.join(' ')}`.toLocaleLowerCase();
    return matchesStatus && (!needle || searchable.includes(needle));
  });
});
const hasFilters = computed(() => Boolean(query.value.trim()) || statusFilter.value !== 'all');

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

function clearFilters() {
  query.value = '';
  statusFilter.value = 'all';
}

function formatTime(value: string) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
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
    <AppError :message="runs.error ?? ''" />
    <section class="panel">
      <div class="list-toolbar">
        <label class="search-control">
          <Search :size="16" aria-hidden="true" />
          <span class="sr-only">{{ t('runs.search') }}</span>
          <input v-model="query" type="search" :placeholder="t('runs.searchPlaceholder')" />
        </label>
        <label class="filter-control">
          <span>{{ t('runs.statusFilter') }}</span>
          <select v-model="statusFilter">
            <option value="all">{{ t('runs.allStatuses') }}</option>
            <option value="pending">{{ t('status.pending') }}</option>
            <option value="running">{{ t('status.running') }}</option>
            <option value="completed">{{ t('status.completed') }}</option>
            <option value="stopped">{{ t('status.stopped') }}</option>
            <option value="failed">{{ t('status.failed') }}</option>
          </select>
        </label>
        <span class="result-count">{{ t('runs.resultCount', { count: filteredRuns.length }) }}</span>
        <button v-if="hasFilters" class="text-action" type="button" @click="clearFilters">{{ t('runs.clearFilters') }}</button>
      </div>

      <div v-if="compareMode" class="compare-selection-bar" role="status">
        <GitCompare :size="18" />
        <span><strong>{{ t('runs.selectedCount', { count: selectedIds.length }) }}</strong><small>{{ t('runs.compareHint') }}</small></span>
        <button v-if="selectedIds.length" class="text-action" type="button" @click="selectedIds = []">{{ t('runs.clearSelection') }}</button>
      </div>

      <AppLoading v-if="runs.loading" :label="t('common.loading')" compact />
      <div v-else-if="filteredRuns.length" class="run-table">
        <div class="run-table-head run-row-runs" :class="{ comparing: compareMode }" aria-hidden="true">
          <span v-if="compareMode"></span>
          <span>{{ t('runs.table.name') }}</span>
          <span>{{ t('runs.table.status') }}</span>
          <span>{{ t('runs.table.workloads') }}</span>
          <span>{{ t('runs.table.startedAt') }}</span>
          <span>{{ t('runs.table.actions') }}</span>
        </div>
        <div v-for="run in filteredRuns" :key="run.id" class="run-row run-row-runs" :class="{ comparing: compareMode }">
          <label v-if="compareMode" class="compare-check" :aria-label="t('runs.selectForCompare', { name: run.name })">
            <input
              type="checkbox"
              :checked="selectedIds.includes(run.id)"
              :disabled="!selectedIds.includes(run.id) && selectedIds.length >= 4"
              @change="toggleRunSelection(run.id)"
            />
          </label>
          <RouterLink class="row-primary-link run-name-cell" :to="`/runs/${run.id}`"><strong>{{ run.name }}</strong><small>{{ run.tags.join(' · ') || t('runs.noTags') }}</small></RouterLink>
          <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
          <span>{{ t('runs.workloadCount', { count: run.workloads.length }) }}</span>
          <span>{{ formatTime(run.started_at) }}</span>
          <button class="table-action danger" type="button" :disabled="run.status === 'running'" @click="removeRun(run.id, run.name)">
            <Trash2 :size="15" />
            {{ t('common.delete') }}
          </button>
        </div>
      </div>
      <AppEmpty
        v-else
        :title="hasFilters ? t('runs.noMatches') : t('runs.empty')"
        :hint="hasFilters ? t('runs.noMatchesHint') : t('runs.emptyHint')"
        compact
      >
        <button v-if="hasFilters" class="secondary-action" type="button" @click="clearFilters">{{ t('runs.clearFilters') }}</button>
      </AppEmpty>
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
