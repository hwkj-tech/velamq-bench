<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { RouterLink } from 'vue-router';
import {
  PhArrowRight,
  PhBroadcast,
  PhChartLineUp,
  PhClock,
  PhCloud,
  PhPlus,
  PhPulse,
  PhStack,
  PhWarningCircle,
} from '@phosphor-icons/vue';
import { useI18n } from 'vue-i18n';
import { useRunsStore } from '@/stores/runs';
import { useRuntimeStore } from '@/stores/runtime';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';

const runs = useRunsStore();
const runtime = useRuntimeStore();
const { t, locale } = useI18n();

onMounted(() => runs.load(20));

const totalRuns = computed(() => runs.list.length);
const activeRuns = computed(() => (runtime.activeRunId ? 1 : 0));
const failedRuns = computed(() => runs.list.filter((run) => run.status === 'failed').length);
const completedRuns = computed(() => runs.list.filter((run) => run.status === 'completed').length);
const successRate = computed(() => {
  const finished = completedRuns.value + failedRuns.value;
  return finished ? `${Math.round((completedRuns.value / finished) * 100)}%` : '—';
});
const latest = computed(() => runs.list.slice(0, 7));
const snapshot = computed(() => runtime.state?.latest ?? null);

function formatTime(value: string) {
  return new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}
</script>

<template>
  <section class="page-stack dashboard-page">
    <section class="dashboard-hero" :data-status="runtime.status">
      <div class="dashboard-hero__copy">
        <span class="dashboard-eyebrow"><i />{{ t(`status.${runtime.status}`) }} · {{ t('dashboard.controlCenter') }}</span>
        <h1>{{ t('dashboard.title') }}</h1>
        <p>{{ t('dashboard.heroBody') }}</p>
        <div class="dashboard-hero__actions">
          <RouterLink class="primary-action" to="/scenarios/new">
            <PhPlus :size="17" weight="bold" />
            {{ t('dashboard.createScenario') }}
          </RouterLink>
          <RouterLink class="secondary-action" to="/runs">
            <PhChartLineUp :size="17" weight="duotone" />
            {{ t('dashboard.viewRuns') }}
          </RouterLink>
        </div>
      </div>
      <div class="dashboard-live-card">
        <div class="dashboard-live-card__head">
          <span>{{ t('dashboard.liveTelemetry') }}</span>
          <strong>{{ runtime.activeRunId ? t('dashboard.streaming') : t('dashboard.standby') }}</strong>
        </div>
        <div class="dashboard-live-metrics">
          <span><small>{{ t('dashboard.live.connected') }}</small><strong>{{ snapshot?.connected?.toLocaleString() ?? '—' }}</strong></span>
          <span><small>{{ t('dashboard.live.publishRate') }}</small><strong>{{ snapshot ? snapshot.publish_rate.toFixed(1) : '—' }}</strong></span>
          <span><small>{{ t('dashboard.live.p95') }}</small><strong>{{ snapshot ? `${snapshot.latency_p95_ms.toFixed(1)} ms` : '—' }}</strong></span>
        </div>
        <RouterLink v-if="runtime.activeRunId" :to="`/runs/${runtime.activeRunId}`">
          {{ t('dashboard.openActiveRun') }} <PhArrowRight :size="15" />
        </RouterLink>
        <span v-else class="dashboard-live-card__hint">{{ t('dashboard.noActiveHint') }}</span>
      </div>
    </section>

    <div class="kpi-strip">
      <article class="kpi-card">
        <PhPulse :size="20" weight="duotone" />
        <span>{{ t('dashboard.kpi.active') }}</span>
        <strong>{{ activeRuns }}</strong>
      </article>
      <article class="kpi-card">
        <PhClock :size="20" weight="duotone" />
        <span>{{ t('dashboard.kpi.recent') }}</span>
        <strong>{{ totalRuns }}</strong>
      </article>
      <article class="kpi-card">
        <PhWarningCircle :size="20" weight="duotone" />
        <span>{{ t('dashboard.kpi.failed') }}</span>
        <strong>{{ failedRuns }}</strong>
      </article>
      <article class="kpi-card">
        <PhBroadcast :size="20" weight="duotone" />
        <span>{{ t('dashboard.kpi.successRate') }}</span>
        <strong>{{ successRate }}</strong>
      </article>
    </div>

    <div class="dashboard-grid dashboard-grid--actions">
      <section class="panel dashboard-active-panel">
        <div class="panel-head split">
          <div><h2>{{ t('dashboard.activeRuns') }}</h2><p>{{ t('dashboard.activeHint') }}</p></div>
          <span class="status-chip fit" :data-status="runtime.status">{{ t(`status.${runtime.status}`) }}</span>
        </div>
        <div v-if="runtime.activeRunId" class="active-run-card">
          <div><small>{{ t('dashboard.runId') }}</small><RouterLink :to="`/runs/${runtime.activeRunId}`">{{ runtime.activeRunId }}</RouterLink></div>
          <PhPulse class="active-run-card__pulse" :size="28" weight="duotone" />
        </div>
        <AppEmpty v-else :title="t('dashboard.noActive')" :hint="t('dashboard.noActiveHint')" compact />
      </section>

      <section class="panel quick-links-panel">
        <div class="panel-head"><h2>{{ t('dashboard.quickLinks') }}</h2></div>
        <div class="quick-link-grid">
          <RouterLink to="/scenarios"><PhStack :size="20" weight="duotone" /><span><strong>{{ t('nav.scenarios') }}</strong><small>{{ t('dashboard.quick.scenarios') }}</small></span><PhArrowRight :size="15" /></RouterLink>
          <RouterLink to="/nodes"><PhCloud :size="20" weight="duotone" /><span><strong>{{ t('nav.nodes') }}</strong><small>{{ t('dashboard.quick.nodes') }}</small></span><PhArrowRight :size="15" /></RouterLink>
        </div>
      </section>
    </div>

    <AppError :message="runs.error ?? ''" />
    <section class="panel">
      <div class="panel-head split">
        <div><h2>{{ t('dashboard.recentRuns') }}</h2><p>{{ t('dashboard.recentHint') }}</p></div>
        <RouterLink class="text-action" to="/runs">{{ t('dashboard.viewAll') }} <PhArrowRight :size="14" /></RouterLink>
      </div>
      <AppLoading v-if="runs.loading" :label="t('common.loading')" compact />
      <div v-else-if="latest.length" class="run-table">
        <div class="run-table-head run-row-dashboard" aria-hidden="true">
          <span>{{ t('dashboard.table.scenario') }}</span>
          <span>{{ t('dashboard.table.status') }}</span>
          <span>{{ t('dashboard.table.startedAt') }}</span>
        </div>
        <RouterLink v-for="run in latest" :key="run.id" class="run-row run-row-dashboard" :to="`/runs/${run.id}`">
          <span class="run-name-cell"><strong>{{ run.name }}</strong><small>{{ run.workloads.length }} workload</small></span>
          <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
          <span>{{ formatTime(run.started_at) }}</span>
        </RouterLink>
      </div>
      <AppEmpty v-else :title="t('dashboard.noRecentRuns')" :hint="t('dashboard.noRecentRunsHint')" compact />
    </section>
  </section>
</template>
