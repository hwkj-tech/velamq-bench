<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { RouterLink, useRouter } from 'vue-router';
import { Edit, Play } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import type { Run, Scenario, Workload } from '@/api/types';

const props = defineProps<{ id: string }>();
const router = useRouter();
const { t } = useI18n();
const scenario = ref<Scenario | null>(null);
const runs = ref<Run[]>([]);

onMounted(async () => {
  scenario.value = await api.scenario(props.id);
  runs.value = await api.runs(80);
  runs.value = runs.value.filter((run) => run.scenario_id === props.id);
});

async function runAgain() {
  const response = await api.runScenario(props.id);
  await router.push(`/runs/${response.run_id}`);
}

function workloadsOf(scenario: Scenario): Workload[] {
  return scenario.stages.flatMap((stage) =>
    'parallel' in stage ? stage.parallel.workloads : stage.sequential.workloads,
  );
}
</script>

<template>
  <section v-if="scenario" class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ scenario.name }}</h1>
        <p>{{ scenario.description || t('scenarios.detail') }}</p>
      </div>
      <div class="topbar-actions">
        <RouterLink class="secondary-action" :to="`/scenarios/${scenario.id}/edit`">
          <Edit :size="15" />
          {{ t('common.edit') }}
        </RouterLink>
        <button class="primary-action" type="button" @click="runAgain">
          <Play :size="15" />
          {{ t('scenarios.runAgain') }}
        </button>
      </div>
    </div>

    <div class="detail-grid">
      <section class="panel">
        <div class="panel-head">
          <h2>{{ t('scenarios.workloads') }}</h2>
        </div>
        <div class="run-table">
          <div v-for="workload in workloadsOf(scenario)" :key="workload.id || workload.name" class="run-row run-row-workload static">
            <strong>{{ workload.name || workload.kind }}</strong>
            <span>{{ workload.kind }}</span>
            <span>{{ t('scenarios.clientCount', { count: workload.clients }) }}</span>
            <span>{{ workload.topics.topic_template }}</span>
          </div>
        </div>
      </section>

      <section class="panel">
        <div class="panel-head">
          <h2>{{ t('scenarios.runTimeline') }}</h2>
        </div>
        <div class="run-table">
          <RouterLink v-for="run in runs" :key="run.id" class="run-row run-row-dashboard" :to="`/runs/${run.id}`">
            <strong>{{ run.name }}</strong>
            <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
            <span>{{ new Date(run.started_at).toLocaleString() }}</span>
          </RouterLink>
          <AppEmpty v-if="runs.length === 0" :title="t('scenarios.noRuns')" compact />
        </div>
      </section>
    </div>
  </section>
</template>
