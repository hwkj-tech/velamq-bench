<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import RunHero from '@/components/run/RunHero.vue';
import { api } from '@/api/client';
import { useRunDetail } from '@/composables/useRunDetail';
import OverviewTab from '@/pages/run-detail/OverviewTab.vue';
import LatencyTab from '@/pages/run-detail/LatencyTab.vue';
import ThroughputTab from '@/pages/run-detail/ThroughputTab.vue';
import ConnectionsTab from '@/pages/run-detail/ConnectionsTab.vue';
import ErrorsTab from '@/pages/run-detail/ErrorsTab.vue';
import LogsTab from '@/pages/run-detail/LogsTab.vue';
import ConfigTab from '@/pages/run-detail/ConfigTab.vue';
import NotesTab from '@/pages/run-detail/NotesTab.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';
import { useToast } from '@/composables/useToast';

const props = defineProps<{ id: string; tab?: string }>();
const router = useRouter();
const { t, locale } = useI18n();
const toast = useToast();
const detail = useRunDetail(props.id);

const tabs = [
  'overview',
  'latency',
  'throughput',
  'connections',
  'errors',
  'logs',
  'config',
  'notes',
] as const;

type RunTab = (typeof tabs)[number];

const activeTab = computed<RunTab>(() => (tabs.includes(props.tab as RunTab) ? (props.tab as RunTab) : 'overview'));
const durationMs = computed(() => detail.latest.value?.elapsed_ms ?? 0);
const canMarkBaseline = computed(() => {
  const run = detail.run.value;
  return Boolean(run?.scenario_id && (run.status === 'completed' || run.status === 'stopped'));
});

onMounted(detail.load);

function selectTab(tab: RunTab) {
  router.push(`/runs/${props.id}/${tab}`);
}

function openNotes() {
  selectTab('notes');
}

async function markBaseline() {
  const run = detail.run.value;
  if (!run?.scenario_id) return;
  await api.setScenarioBaseline(run.scenario_id, run.id);
  detail.run.value = { ...run, baseline_of_scenario_id: run.scenario_id };
}

async function exportChart(format: 'svg' | 'pdf' | 'csv') {
  const run = detail.run.value;
  if (!run) return;
  try {
    const blob =
      format === 'pdf'
        ? await api.exportRunPdf(run.id, locale.value)
        : format === 'csv'
          ? await api.exportRunCsv(run.id)
          : await api.exportRunChart(run.id, locale.value);
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `velamq-report-${run.id}.${format}`;
    anchor.click();
    URL.revokeObjectURL(url);
    toast.success(t('runDetail.exported'));
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}
</script>

<template>
  <section class="page-stack run-detail">
    <RunHero
      :run="detail.run.value"
      :status="detail.status.value"
      :duration-ms="durationMs"
      :can-mark-baseline="canMarkBaseline"
      :is-baseline="Boolean(detail.run.value?.baseline_of_scenario_id)"
      @stop="detail.stop"
      @add-annotation="openNotes"
      @mark-baseline="markBaseline"
      @export-chart="exportChart"
    />

    <nav class="run-tabs" :aria-label="t('runDetail.tabsLabel')">
      <button
        v-for="tabName in tabs"
        :key="tabName"
        type="button"
        :aria-current="activeTab === tabName ? 'page' : undefined"
        @click="selectTab(tabName)"
      >
        {{ t(`runDetail.tabs.${tabName}`) }}
      </button>
    </nav>

    <AppError :message="detail.error.value" />
    <AppLoading v-if="detail.loading.value" :label="t('runDetail.loading')" compact />

    <KeepAlive>
      <OverviewTab
        v-if="activeTab === 'overview'"
        :run="detail.run.value"
        :snapshots="detail.snapshots.value"
        :annotations="detail.annotations.value"
        :latest="detail.latest.value"
        :snapshots-by-workload="detail.snapshotsByWorkload"
      />
      <LatencyTab
        v-else-if="activeTab === 'latency'"
        :snapshots="detail.snapshots.value"
        :annotations="detail.annotations.value"
      />
      <ThroughputTab
        v-else-if="activeTab === 'throughput'"
        :snapshots="detail.snapshots.value"
        :annotations="detail.annotations.value"
      />
      <ConnectionsTab
        v-else-if="activeTab === 'connections'"
        :snapshots="detail.snapshots.value"
        :latest="detail.latest.value"
      />
      <ErrorsTab
        v-else-if="activeTab === 'errors'"
        :snapshots="detail.snapshots.value"
        :logs="detail.logs.value"
        :latest="detail.latest.value"
      />
      <LogsTab v-else-if="activeTab === 'logs'" :logs="detail.logs.value" />
      <ConfigTab v-else-if="activeTab === 'config'" :run="detail.run.value" />
      <NotesTab
        v-else
        :run="detail.run.value"
        :annotations="detail.annotations.value"
        @add="detail.addAnnotation"
      />
    </KeepAlive>
  </section>
</template>
