<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { Activity, Ban, CheckCircle2, Cpu, Edit3, HardDrive, RefreshCw, Server, Trash2, XCircle } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import type { AgentNode } from '@/api/types';

const { t, locale } = useI18n();
const nodes = ref<AgentNode[]>([]);
const loading = ref(false);
const error = ref('');
const editingId = ref('');
const editName = ref('');
const editLabels = ref('');
let refreshTimer: number | undefined;

const online = computed(() => nodes.value.filter((node) => ['online', 'busy', 'draining'].includes(node.status)).length);
const capacity = computed(() => nodes.value.reduce((sum, node) => sum + node.capabilities.max_clients, 0));
const busy = computed(() => nodes.value.filter((node) => node.status === 'busy').length);

async function load(silent = false) {
  if (!silent) loading.value = true;
  error.value = '';
  try { nodes.value = await api.agents(); }
  catch (err) { error.value = err instanceof Error ? err.message : String(err); }
  finally { loading.value = false; }
}

function startEdit(node: AgentNode) {
  editingId.value = node.id;
  editName.value = node.name;
  editLabels.value = node.labels.join(', ');
}

async function saveEdit(node: AgentNode) {
  await api.updateAgent(node.id, {
    name: editName.value,
    labels: editLabels.value.split(',').map((label) => label.trim()).filter(Boolean),
  });
  editingId.value = '';
  await load(true);
}

async function toggleEnabled(node: AgentNode) {
  await api.updateAgent(node.id, { enabled: !node.enabled });
  await load(true);
}

async function toggleDrain(node: AgentNode) {
  await api.updateAgent(node.id, { draining: !node.draining });
  await load(true);
}

async function remove(node: AgentNode) {
  if (!window.confirm(t('nodes.confirmDelete', { name: node.name }))) return;
  await api.deleteAgent(node.id);
  await load(true);
}

function formatBytes(value: number) {
  if (!value) return '—';
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatSeen(value: string) {
  return new Intl.DateTimeFormat(locale.value, { dateStyle: 'short', timeStyle: 'medium' }).format(new Date(value));
}

onMounted(async () => {
  await load();
  refreshTimer = window.setInterval(() => load(true), 5000);
});
onUnmounted(() => window.clearInterval(refreshTimer));
</script>

<template>
  <section class="page-stack nodes-page">
    <div class="page-title">
      <div><h1>{{ t('nodes.title') }}</h1><p>{{ t('nodes.subtitle') }}</p></div>
      <button class="secondary-action" type="button" :disabled="loading" @click="load()"><RefreshCw :size="15" :class="{ spinning: loading }" />{{ t('common.refresh') }}</button>
    </div>
    <AppError :message="error" />

    <div class="node-kpi-grid">
      <article class="node-kpi"><Server :size="20" /><span>{{ t('nodes.total') }}</span><strong>{{ nodes.length }}</strong></article>
      <article class="node-kpi online"><Activity :size="20" /><span>{{ t('nodes.online') }}</span><strong>{{ online }}</strong></article>
      <article class="node-kpi"><Cpu :size="20" /><span>{{ t('nodes.busy') }}</span><strong>{{ busy }}</strong></article>
      <article class="node-kpi"><HardDrive :size="20" /><span>{{ t('nodes.capacity') }}</span><strong>{{ capacity.toLocaleString() }}</strong></article>
    </div>

    <section class="panel node-panel">
      <div class="panel-head split"><div><h2>{{ t('nodes.registry') }}</h2><p>{{ t('nodes.registryHint') }}</p></div><span class="status-chip fit">{{ t('nodes.autoRefresh') }}</span></div>
      <div v-if="nodes.length" class="node-grid">
        <article v-for="node in nodes" :key="node.id" class="node-card" :data-status="node.status">
          <header>
            <span class="node-status-dot" />
            <div><strong>{{ node.name }}</strong><span>{{ node.capabilities.os }} / {{ node.capabilities.arch }} · v{{ node.capabilities.version || '—' }}</span></div>
            <span class="status-chip fit">{{ t(`nodes.status.${node.status}`) }}</span>
          </header>
          <div v-if="editingId === node.id" class="node-edit">
            <label><span>{{ t('fields.name') }}</span><input v-model="editName" class="control" /></label>
            <label><span>{{ t('nodes.labels') }}</span><input v-model="editLabels" class="control" :placeholder="t('nodes.labelsHint')" /></label>
            <div class="scenario-actions"><button class="primary-action" type="button" @click="saveEdit(node)">{{ t('common.save') }}</button><button class="secondary-action" type="button" @click="editingId = ''">{{ t('common.cancel') }}</button></div>
          </div>
          <template v-else>
            <dl class="node-facts">
              <div><dt>{{ t('nodes.cores') }}</dt><dd>{{ node.capabilities.cpu_cores || '—' }}</dd></div>
              <div><dt>{{ t('nodes.memory') }}</dt><dd>{{ formatBytes(node.capabilities.memory_bytes) }}</dd></div>
              <div><dt>{{ t('nodes.maxClients') }}</dt><dd>{{ node.capabilities.max_clients.toLocaleString() }}</dd></div>
              <div><dt>{{ t('nodes.lastSeen') }}</dt><dd>{{ formatSeen(node.last_seen_at) }}</dd></div>
            </dl>
            <div class="node-labels"><span v-for="label in node.labels" :key="label">{{ label }}</span><em v-if="!node.labels.length">{{ t('nodes.noLabels') }}</em></div>
            <div class="node-actions">
              <button class="secondary-action" type="button" @click="startEdit(node)"><Edit3 :size="14" />{{ t('common.edit') }}</button>
              <button class="secondary-action" type="button" @click="toggleDrain(node)"><Ban :size="14" />{{ node.draining ? t('nodes.resume') : t('nodes.drain') }}</button>
              <button class="secondary-action" type="button" @click="toggleEnabled(node)"><component :is="node.enabled ? XCircle : CheckCircle2" :size="14" />{{ node.enabled ? t('nodes.disable') : t('nodes.enable') }}</button>
              <button class="secondary-action danger" type="button" @click="remove(node)"><Trash2 :size="14" />{{ t('common.delete') }}</button>
            </div>
          </template>
        </article>
      </div>
      <AppEmpty v-else :title="t('nodes.empty')" :hint="t('nodes.emptyHint')" />
    </section>
  </section>
</template>
