<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { RouterLink } from 'vue-router';
import { PhBroadcast, PhChartLineUp, PhClock, PhPulse, PhWarningCircle } from '@phosphor-icons/vue';
import { useI18n } from 'vue-i18n';
import { useRunsStore } from '@/stores/runs';
import { useRuntimeStore } from '@/stores/runtime';
import AppEmpty from '@/components/feedback/AppEmpty.vue';

const runs = useRunsStore();
const runtime = useRuntimeStore();
const { t } = useI18n();

onMounted(() => runs.load(12));

const totalRuns = computed(() => runs.list.length);
const activeRuns = computed(() => (runtime.activeRunId ? 1 : 0));
const recentErrors = computed(() => runs.list.reduce((sum, run) => sum + (run.status === 'failed' ? 1 : 0), 0));
const latest = computed(() => runs.list.slice(0, 8));
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('dashboard.title') }}</h1>
        <p>{{ t('dashboard.subtitle') }}</p>
      </div>
      <RouterLink class="primary-action" to="/runs">
        <PhChartLineUp :size="16" weight="duotone" />
        {{ t('dashboard.viewRuns') }}
      </RouterLink>
    </div>

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
        <strong>{{ recentErrors }}</strong>
      </article>
      <article class="kpi-card">
        <PhBroadcast :size="20" weight="duotone" />
        <span>{{ t('dashboard.kpi.status') }}</span>
        <strong>{{ t(`status.${runtime.status}`) }}</strong>
      </article>
    </div>

    <div class="dashboard-grid">
      <section class="panel">
        <div class="panel-head">
          <h2>{{ t('dashboard.activeRuns') }}</h2>
        </div>
        <div v-if="runtime.activeRunId" class="active-run-row">
          <span class="status-chip" :data-status="runtime.status">{{ t(`status.${runtime.status}`) }}</span>
          <RouterLink :to="`/runs/${runtime.activeRunId}`">{{ runtime.activeRunId }}</RouterLink>
        </div>
        <AppEmpty v-else :title="t('dashboard.noActive')" compact />
      </section>

      <section class="panel">
        <div class="panel-head">
          <h2>{{ t('dashboard.regressions') }}</h2>
        </div>
        <AppEmpty :title="t('dashboard.noRegressions')" compact />
      </section>
    </div>

    <section class="panel">
      <div class="panel-head">
        <h2>{{ t('dashboard.recentRuns') }}</h2>
      </div>
      <div class="run-table">
        <div class="run-table-head run-row-dashboard" aria-hidden="true">
          <span>{{ t('dashboard.table.scenario') }}</span>
          <span>{{ t('dashboard.table.status') }}</span>
          <span>{{ t('dashboard.table.startedAt') }}</span>
        </div>
        <RouterLink v-for="run in latest" :key="run.id" class="run-row run-row-dashboard" :to="`/runs/${run.id}`">
          <span>{{ run.name }}</span>
          <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
          <span>{{ new Date(run.started_at).toLocaleString() }}</span>
        </RouterLink>
      </div>
    </section>
  </section>
</template>
