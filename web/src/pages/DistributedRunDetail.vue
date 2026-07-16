<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { Activity, CircleStop, Cloud, Download, RefreshCw, Server } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppError from '@/components/feedback/AppError.vue';
import ConnectionsChart from '@/components/charts/ConnectionsChart.vue';
import ThroughputChart from '@/components/charts/ThroughputChart.vue';
import LatencyChart from '@/components/charts/LatencyChart.vue';
import type { AgentNode, DistributedMetrics, DistributedRun } from '@/api/types';
import { useToast } from '@/composables/useToast';

const props = defineProps<{ id: string }>();
const { t } = useI18n();
const toast = useToast();
const run = ref<DistributedRun | null>(null);
const metrics = ref<DistributedMetrics>({ run_id: props.id, summary: [], nodes: [] });
const agents = ref<AgentNode[]>([]);
const error = ref('');
const loading = ref(false);
let timer: number | undefined;

const nodeMap = computed(() => Object.fromEntries(agents.value.map((node) => [node.id, node])));
const latest = computed(() => metrics.value.summary.at(-1));
const terminal = computed(() => run.value && ['completed', 'partial', 'failed', 'stopped'].includes(run.value.status));

async function load() {
  loading.value = true;
  try {
    [run.value, metrics.value, agents.value] = await Promise.all([
      api.distributedRun(props.id), api.distributedMetrics(props.id), api.agents(),
    ]);
    error.value = '';
    if (terminal.value) window.clearInterval(timer);
  } catch (err) { error.value = err instanceof Error ? err.message : String(err); }
  finally { loading.value = false; }
}

async function stop() {
  run.value = await api.stopDistributedRun(props.id);
  await load();
}

async function exportCsv() {
  try {
    const blob = await api.exportDistributedRunCsv(props.id);
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `velamq-distributed-${props.id}.csv`;
    anchor.click();
    URL.revokeObjectURL(url);
    toast.success(t('distributed.exported'));
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}

onMounted(async () => { await load(); timer = window.setInterval(load, 2000); });
onUnmounted(() => window.clearInterval(timer));
</script>

<template>
  <section v-if="run" class="page-stack distributed-detail">
    <div class="page-title">
      <div><h1>{{ run.name }}</h1><p>{{ t('distributed.detailHint', { count: run.tasks.length }) }}</p></div>
      <div class="topbar-actions"><span class="status-chip" :data-status="run.status">{{ t(`distributed.status.${run.status}`) }}</span><button class="secondary-action" type="button" @click="exportCsv"><Download :size="15" />{{ t('distributed.exportCsv') }}</button><button class="secondary-action" type="button" :disabled="loading" @click="load"><RefreshCw :size="15" :class="{ spinning: loading }" />{{ t('common.refresh') }}</button><button v-if="!terminal" class="secondary-action danger" type="button" @click="stop"><CircleStop :size="15" />{{ t('distributed.stop') }}</button></div>
    </div>
    <AppError :message="error" />

    <div class="node-kpi-grid">
      <article class="node-kpi"><Server :size="20" /><span>{{ t('distributed.nodes') }}</span><strong>{{ run.tasks.length }}</strong></article>
      <article class="node-kpi online"><Activity :size="20" /><span>{{ t('distributed.connected') }}</span><strong>{{ latest?.connected ?? 0 }}</strong></article>
      <article class="node-kpi"><Cloud :size="20" /><span>{{ t('distributed.throughput') }}</span><strong>{{ Math.round((latest?.publish_rate ?? 0) + (latest?.receive_rate ?? 0)).toLocaleString() }}/s</strong></article>
      <article class="node-kpi"><Activity :size="20" /><span>P99</span><strong>{{ (latest?.latency_p99_ms ?? 0).toFixed(2) }} ms</strong></article>
    </div>

    <div class="compare-chart-grid">
      <section class="panel"><div class="panel-head"><h2>{{ t('distributed.connectionSummary') }}</h2></div><ConnectionsChart :snapshots="metrics.summary" /></section>
      <section class="panel"><div class="panel-head"><h2>{{ t('distributed.throughputSummary') }}</h2></div><ThroughputChart :snapshots="metrics.summary" aggregate /></section>
      <section class="panel distributed-latency-chart"><div class="panel-head"><h2>{{ t('distributed.latencySummary') }}</h2></div><LatencyChart :snapshots="metrics.summary" /></section>
    </div>

    <section class="panel"><div class="panel-head"><h2>{{ t('distributed.nodeDetails') }}</h2></div><div class="node-grid"><article v-for="task in run.tasks" :key="task.id" class="node-card" :data-status="task.status"><header><span class="node-status-dot" /><div><strong>{{ nodeMap[task.node_id]?.name ?? task.node_id }}</strong><span>{{ task.spec.scenario.stages.flatMap((stage) => 'parallel' in stage ? stage.parallel.workloads : stage.sequential.workloads).reduce((sum, workload) => sum + workload.clients, 0).toLocaleString() }} clients · attempt {{ task.attempt }}</span></div><span class="status-chip fit" :data-status="task.status">{{ t(`distributed.taskStatus.${task.status}`) }}</span></header><p v-if="task.error" class="task-error">{{ task.error }}</p><ThroughputChart :snapshots="metrics.nodes.find((node) => node.task_id === task.id)?.snapshots ?? []" aggregate height="220px" /></article></div></section>
  </section>
</template>
