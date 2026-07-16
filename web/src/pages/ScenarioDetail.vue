<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { RouterLink, useRouter } from 'vue-router';
import { Cloud, Edit, Play, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import type { AgentNode, Run, Scenario, SchedulingStrategy, Workload } from '@/api/types';

const props = defineProps<{ id: string }>();
const router = useRouter();
const { t } = useI18n();
const scenario = ref<Scenario | null>(null);
const runs = ref<Run[]>([]);
const agents = ref<AgentNode[]>([]);
const distributedOpen = ref(false);
const distributedBusy = ref(false);
const strategy = ref<SchedulingStrategy>('even');
const selectedNodes = ref<string[]>([]);
const requiredLabels = ref('');

onMounted(async () => {
  scenario.value = await api.scenario(props.id);
  runs.value = await api.runs(80);
  runs.value = runs.value.filter((run) => run.scenario_id === props.id);
  agents.value = (await api.agents()).filter((node) => node.enabled && !node.draining && ['online', 'busy'].includes(node.status));
});

async function runAgain() {
  const response = await api.runScenario(props.id);
  await router.push(`/runs/${response.run_id}`);
}

async function runDistributed() {
  distributedBusy.value = true;
  try {
    const run = await api.startDistributedRun({
      scenario_id: props.id,
      node_ids: selectedNodes.value,
      required_labels: requiredLabels.value.split(',').map((label) => label.trim()).filter(Boolean),
      strategy: strategy.value,
    });
    await router.push(`/distributed-runs/${run.id}`);
  } finally {
    distributedBusy.value = false;
  }
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
        <button class="secondary-action" type="button" @click="distributedOpen = true">
          <Cloud :size="15" />
          {{ t('distributed.run') }}
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

    <section v-if="distributedOpen" class="panel distributed-launcher">
      <div class="panel-head split"><div><h2>{{ t('distributed.launchTitle') }}</h2><p>{{ t('distributed.launchHint') }}</p></div><button class="icon-button" type="button" :aria-label="t('common.close')" @click="distributedOpen = false"><X :size="16" /></button></div>
      <div class="sheet-grid">
        <label><span>{{ t('distributed.strategy') }}</span><select v-model="strategy" class="control"><option value="even">{{ t('distributed.strategies.even') }}</option><option value="capacity_weighted">{{ t('distributed.strategies.capacity_weighted') }}</option><option value="selected">{{ t('distributed.strategies.selected') }}</option></select></label>
        <label><span>{{ t('distributed.requiredLabels') }}</span><input v-model="requiredLabels" class="control" :placeholder="t('distributed.labelsHint')" /></label>
      </div>
      <div class="distributed-node-picker">
        <label v-for="agent in agents" :key="agent.id" class="node-pick"><input v-model="selectedNodes" type="checkbox" :value="agent.id" /><span><strong>{{ agent.name }}</strong><small>{{ agent.capabilities.max_clients.toLocaleString() }} clients · {{ agent.labels.join(', ') || t('nodes.noLabels') }}</small></span></label>
        <AppEmpty v-if="agents.length === 0" :title="t('distributed.noNodes')" compact />
      </div>
      <div class="topbar-actions"><button class="primary-action" type="button" :disabled="distributedBusy || agents.length === 0 || (strategy === 'selected' && selectedNodes.length === 0)" @click="runDistributed"><Cloud :size="15" />{{ distributedBusy ? t('distributed.scheduling') : t('distributed.start') }}</button></div>
    </section>
  </section>
</template>
