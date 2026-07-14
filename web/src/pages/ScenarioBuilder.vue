<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight, Play, Plus, Save, Trash2, TestTube2 } from 'lucide-vue-next';
import { api } from '@/api/client';
import { useProfilesStore } from '@/stores/profiles';
import AppError from '@/components/feedback/AppError.vue';
import type { BenchConfig, BenchTemplate, BrokerProfile, BrokerProtocol, LoadShape, PayloadProfile, Scenario, ScenarioStage, Workload, WorkloadKind } from '@/api/types';

const props = defineProps<{ id?: string }>();
const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const profiles = useProfilesStore();
const step = ref(1);
const busy = ref(false);
const error = ref('');
const connectionState = ref<'idle' | 'testing' | 'ok' | 'failed'>('idle');
const connectionMessage = ref('');
const brokerMode = ref<'saved' | 'adhoc'>('saved');
const adhocBroker = reactive<{ host: string; port: number; name: string; protocol: BrokerProtocol; websocket_path: string | null }>({
  host: '127.0.0.1',
  port: 1883,
  name: t('settings.brokers.local'),
  protocol: 'mqtt',
  websocket_path: null,
});
const stageStrategy = ref<'parallel' | 'sequential'>('parallel');
const sourceTemplateName = ref('');
const pendingPayloadProfile = ref<Partial<PayloadProfile> | null>(null);
const templateStartMode = ref<'blank' | 'template'>('blank');
const templates = ref<BenchTemplate[]>([]);
const selectedTemplateId = ref('');

const form = reactive<Scenario>({
  id: '',
  name: t('scenarios.newName'),
  description: '',
  tags: [],
  stages: [{ parallel: { workloads: [newWorkload()] } }],
  baseline_run_id: null,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});

const workloads = computed(() => stageWorkloads(form.stages[0]));
const firstWorkload = computed(() => workloads.value[0] ?? null);
const selectedTemplate = computed(() => templates.value.find((template) => template.id === selectedTemplateId.value) ?? null);
const brokerProfileId = computed({
  get: () => workloads.value[0]?.broker_profile_id ?? '',
  set: (value: string) => workloads.value.forEach((workload) => (workload.broker_profile_id = value)),
});
const durationMs = computed({
  get: () => firstWorkload.value?.load.total_duration_ms ?? 0,
  set: (value: number) => workloads.value.forEach((workload) => (workload.load.total_duration_ms = value)),
});
const sampleIntervalMs = computed({
  get: () => firstWorkload.value?.sample_interval_ms ?? 1000,
  set: (value: number) => workloads.value.forEach((workload) => (workload.sample_interval_ms = value)),
});
const networkBindMode = computed({
  get: () => firstWorkload.value?.network_bind_mode ?? 'system',
  set: (value: string) => workloads.value.forEach((workload) => (workload.network_bind_mode = value)),
});
const bindInterfacesText = computed({
  get: () => firstWorkload.value?.bind_interfaces.join(', ') ?? '',
  set: (value: string) => {
    const interfaces = value
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);
    workloads.value.forEach((workload) => (workload.bind_interfaces = interfaces));
  },
});
const canPrev = computed(() => step.value > 1);
const canNext = computed(() => step.value < 3);
const draftKey = computed(() => {
  const templateId = typeof route.query.template === 'string' ? route.query.template : '';
  return `velamq:scenario-builder:${props.id ?? (templateId || 'new')}`;
});
const protocolOptions: BrokerProtocol[] = ['mqtt', 'mqtts', 'ws', 'wss'];

onMounted(async () => {
  await Promise.allSettled([profiles.load(), loadTemplates()]);
  if (props.id && route.path !== '/scenarios/new') {
    Object.assign(form, await api.scenario(props.id));
    stageStrategy.value = form.stages[0] && 'parallel' in form.stages[0] ? 'parallel' : 'sequential';
    if (route.path.endsWith('/run')) {
      await runSavedScenario();
      return;
    }
  } else if (typeof route.query.template === 'string' && route.query.template) {
    templateStartMode.value = 'template';
    selectedTemplateId.value = route.query.template;
    await applyTemplateDraft(route.query.template);
  } else {
    hydrateDraft();
  }
  if (brokerMode.value === 'saved' && !brokerProfileId.value && profiles.brokers[0]) {
    brokerProfileId.value = profiles.brokers[0].id;
  }
  window.addEventListener('keydown', saveDraftShortcut);
});

onBeforeUnmount(() => window.removeEventListener('keydown', saveDraftShortcut));

async function loadTemplates() {
  try {
    templates.value = await api.templates();
    if (!selectedTemplateId.value && templates.value[0]) {
      selectedTemplateId.value = templates.value[0].id;
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

function stageWorkloads(stage: ScenarioStage | undefined): Workload[] {
  if (!stage) return [];
  return 'parallel' in stage ? stage.parallel.workloads : stage.sequential.workloads;
}

function newWorkload(kind: WorkloadKind = 'pub'): Workload {
  return {
    id: crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    name: t('builder.workloadName', { kind }),
    kind,
    broker_profile_id: '',
    payload_profile_id: null,
    clients: 100,
    start_number: 1,
    client_id_template: 'velamq-{mode}-{i}',
    topics: { topic_template: 'velamq/bench/{i}', partitions: 1, group_strategy: 'client_id' },
    qos: 'qos0',
    retain: false,
    load: {
      connect_shape: { shape: 'flat', rate: 100 },
      message_shape: { shape: 'flat', rate: 1 },
      total_duration_ms: 60000,
    },
    network_bind_mode: 'system',
    bind_interfaces: [],
    sample_interval_ms: 1000,
  };
}

function blankScenario(): Scenario {
  const now = new Date().toISOString();
  return {
    id: '',
    name: t('scenarios.newName'),
    description: '',
    tags: [],
    stages: [{ parallel: { workloads: [newWorkload()] } }],
    baseline_run_id: null,
    created_at: now,
    updated_at: now,
  };
}

function useBlankStart() {
  templateStartMode.value = 'blank';
  sourceTemplateName.value = '';
  pendingPayloadProfile.value = null;
  brokerMode.value = 'saved';
  Object.assign(form, blankScenario());
  if (profiles.brokers[0]) {
    brokerProfileId.value = profiles.brokers[0].id;
  }
}

async function applySelectedTemplate() {
  if (!selectedTemplateId.value) return;
  templateStartMode.value = 'template';
  await applyTemplateDraft(selectedTemplateId.value);
}

function addWorkload() {
  const workload = newWorkload('pub');
  workload.broker_profile_id = brokerProfileId.value;
  workloads.value.push(workload);
}

function removeWorkload(id: string) {
  const index = workloads.value.findIndex((workload) => workload.id === id);
  if (index >= 0 && workloads.value.length > 1) workloads.value.splice(index, 1);
}

function applyStageStrategy() {
  const current = [...workloads.value];
  form.stages = stageStrategy.value === 'parallel' ? [{ parallel: { workloads: current } }] : [{ sequential: { workloads: current } }];
}

async function ensureBroker() {
  if (brokerMode.value === 'saved') return;
  const broker = await api.createBroker({
    id: '',
    name: adhocBroker.name,
    protocol: adhocBroker.protocol,
    host: adhocBroker.host,
    port: adhocBroker.port,
    websocket_path: adhocBroker.websocket_path,
    keepalive_secs: 30,
    clean_session: true,
  } as Partial<BrokerProfile>);
  brokerProfileId.value = broker.id;
  await profiles.load();
  brokerMode.value = 'saved';
}

async function ensureTemplatePayload() {
  if (!pendingPayloadProfile.value) return;
  const targetWorkloads = workloads.value.filter((workload) => workload.kind === 'pub' && !workload.payload_profile_id);
  if (targetWorkloads.length === 0) {
    pendingPayloadProfile.value = null;
    return;
  }
  const payload = await api.createPayload(pendingPayloadProfile.value);
  targetWorkloads.forEach((workload) => (workload.payload_profile_id = payload.id));
  await profiles.load();
  pendingPayloadProfile.value = null;
}

async function testConnection() {
  connectionState.value = 'testing';
  connectionMessage.value = '';
  try {
    await ensureBroker();
    const response = await api.testBroker(brokerProfileId.value);
    connectionState.value = response.ok ? 'ok' : 'failed';
    connectionMessage.value = response.ok
      ? `Connected to ${response.host}:${response.port} in ${response.elapsed_ms}ms`
      : (response.error ?? `Could not connect to ${response.host}:${response.port}`);
  } catch (err) {
    connectionState.value = 'failed';
    connectionMessage.value = err instanceof Error ? err.message : String(err);
  }
}

async function saveScenario() {
  busy.value = true;
  error.value = '';
  try {
    await ensureBroker();
    await ensureTemplatePayload();
    applyStageStrategy();
    const saved = form.id ? await api.updateScenario(form) : await api.createScenario(form);
    localStorage.removeItem(draftKey.value);
    Object.assign(form, saved);
    await router.push(`/scenarios/${saved.id}`);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function saveAndRun() {
  busy.value = true;
  error.value = '';
  try {
    await ensureBroker();
    await ensureTemplatePayload();
    applyStageStrategy();
    const saved = form.id ? await api.updateScenario(form) : await api.createScenario(form);
    localStorage.removeItem(draftKey.value);
    const response = await api.runScenario(saved.id);
    await router.push(`/runs/${response.run_id}`);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function runOnce() {
  busy.value = true;
  error.value = '';
  try {
    await ensureBroker();
    await ensureTemplatePayload();
    applyStageStrategy();
    const response = await api.startAdHoc(form);
    localStorage.removeItem(draftKey.value);
    await router.push(`/runs/${response.run_id}`);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function runSavedScenario() {
  busy.value = true;
  error.value = '';
  try {
    const response = await api.runScenario(props.id ?? form.id);
    await router.push(`/runs/${response.run_id}`);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

function previewPoints(shape: LoadShape) {
  const values = Array.from({ length: 24 }, (_, index) => instantRate(shape, index * 1000));
  const max = Math.max(...values, 1);
  return values.map((value, index) => `${(index / 23) * 100},${36 - (value / max) * 32}`).join(' ');
}

function instantRate(shape: LoadShape, elapsedMs: number): number {
  if (shape.shape === 'flat') return shape.rate;
  if (shape.shape === 'ramp') return elapsedMs >= shape.duration_ms ? shape.to : shape.from + (shape.to - shape.from) * (elapsedMs / shape.duration_ms);
  if (shape.shape === 'soak') return elapsedMs <= shape.duration_ms ? shape.rate : 0;
  if (shape.shape === 'spike') return elapsedMs % shape.period_ms < shape.peak_duration_ms ? shape.peak : shape.baseline;
  let cursor = 0;
  for (const stage of shape.stages) {
    cursor += stage.duration_ms;
    if (elapsedMs < cursor) return stage.rate;
  }
  return shape.stages.at(-1)?.rate ?? 0;
}

function onShapeChange(event: Event, workload: Workload, target: 'connect_shape' | 'message_shape') {
  const shape = (event.target as HTMLSelectElement).value as LoadShape['shape'];
  setShape(workload, target, shape);
}

function setShape(workload: Workload, target: 'connect_shape' | 'message_shape', shape: LoadShape['shape']) {
  const current = workload.load[target];
  if (current.shape === shape) return;
  workload.load[target] =
    shape === 'flat'
      ? { shape: 'flat', rate: 1 }
      : shape === 'ramp'
        ? { shape: 'ramp', from: 1, to: 100, duration_ms: 60000 }
        : shape === 'step'
          ? { shape: 'step', stages: [{ rate: 20, duration_ms: 30000 }, { rate: 100, duration_ms: 30000 }] }
          : shape === 'soak'
            ? { shape: 'soak', rate: 50, duration_ms: 300000 }
            : { shape: 'spike', baseline: 20, peak: 200, peak_duration_ms: 5000, period_ms: 60000 };
}

function appendStep(shape: LoadShape) {
  if (shape.shape === 'step') {
    shape.stages.push({ rate: shape.stages.at(-1)?.rate ?? 50, duration_ms: 30000 });
  }
}

function removeStep(shape: LoadShape, index: number) {
  if (shape.shape === 'step' && shape.stages.length > 1) {
    shape.stages.splice(index, 1);
  }
}

function saveDraftShortcut(event: KeyboardEvent) {
  if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 's') return;
  event.preventDefault();
  localStorage.setItem(draftKey.value, JSON.stringify(form));
}

function hydrateDraft() {
  const draft = localStorage.getItem(draftKey.value);
  if (!draft) return;
  try {
    Object.assign(form, JSON.parse(draft) as Scenario);
  } catch {
    localStorage.removeItem(draftKey.value);
  }
}

async function applyTemplateDraft(templateId: string) {
  if (templates.value.length === 0) {
    await loadTemplates();
  }
  const template = templates.value.find((item) => item.id === templateId);
  if (!template) {
    error.value = t('templates.notFound');
    hydrateDraft();
    return;
  }
  selectedTemplateId.value = template.id;
  templateStartMode.value = 'template';
  const now = new Date().toISOString();
  sourceTemplateName.value = displayTemplateName(template);
  brokerMode.value = 'adhoc';
  adhocBroker.name = `${displayTemplateName(template)} ${t('templates.brokerSuffix')}`;
  adhocBroker.host = template.config.host;
  adhocBroker.port = template.config.port;
  adhocBroker.protocol = template.config.protocol ?? 'mqtt';
  adhocBroker.websocket_path = template.config.websocket_path ?? (isWebSocket(adhocBroker.protocol) ? '/mqtt' : null);
  pendingPayloadProfile.value = template.config.mode === 'pub' ? toPendingPayload(template) : null;
  Object.assign(form, {
    id: '',
    name: displayTemplateName(template),
    description: displayTemplateDescription(template),
    tags: [...template.tags],
    baseline_run_id: null,
    created_at: now,
    updated_at: now,
    stages: [{ parallel: { workloads: [workloadFromTemplate(template.config)] } }],
  } satisfies Scenario);
  stageStrategy.value = 'parallel';
}

function workloadFromTemplate(config: BenchConfig): Workload {
  return {
    id: crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    name: t('templates.workloadName', { mode: t(`builder.modes.${config.mode}`) }),
    kind: config.mode,
    broker_profile_id: '',
    payload_profile_id: null,
    clients: config.clients,
    start_number: config.start_number,
    client_id_template: config.client_id_template,
    topics: {
      topic_template: config.topic,
      partitions: 1,
      group_strategy: 'client_id',
    },
    qos: config.qos,
    retain: config.retain,
    load: {
      connect_shape: { shape: 'flat', rate: config.connect_rate },
      message_shape: { shape: 'flat', rate: intervalToRate(config.message_interval_ms) },
      total_duration_ms: config.duration_secs * 1000,
    },
    network_bind_mode: config.network_bind_mode,
    bind_interfaces: [...config.bind_interfaces],
    sample_interval_ms: config.sample_interval_ms,
  };
}

function toPendingPayload(template: BenchTemplate): Partial<PayloadProfile> {
  return {
    name: `${displayTemplateName(template)} ${t('templates.payloadSuffix')}`,
    kind: {
      kind: 'fixed_bytes',
      size: template.config.payload_size,
      with_timestamp: template.config.payload_timestamp,
    },
  };
}

function defaultPort(protocol: BrokerProtocol) {
  return { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 }[protocol];
}

function isWebSocket(protocol?: BrokerProtocol) {
  return protocol === 'ws' || protocol === 'wss';
}

function applyAdhocProtocol(protocol: BrokerProtocol) {
  adhocBroker.protocol = protocol;
  adhocBroker.port = defaultPort(protocol);
  adhocBroker.websocket_path = isWebSocket(protocol) ? (adhocBroker.websocket_path || '/mqtt') : null;
}

function intervalToRate(intervalMs: number) {
  return intervalMs > 0 ? 1000 / intervalMs : 1;
}

function displayTemplateName(template: BenchTemplate) {
  const key = `templates.defaults.${template.id}.name`;
  const value = t(key);
  return value === key ? template.name : value;
}

function displayTemplateDescription(template: BenchTemplate) {
  const key = `templates.defaults.${template.id}.description`;
  const value = t(key);
  return value === key ? template.description : value;
}
</script>

<template>
  <section class="page-stack builder-page">
    <div class="page-title">
      <div>
        <h1>{{ t('builder.title') }}</h1>
        <p>{{ t('builder.subtitle') }}</p>
      </div>
    </div>

    <div v-if="sourceTemplateName" class="builder-source-banner">
      {{ t('builder.fromTemplate', { name: sourceTemplateName }) }}
    </div>

    <section v-if="!props.id" class="panel builder-start-panel">
      <div class="panel-head">
        <h2>{{ t('builder.startFrom') }}</h2>
      </div>
      <div class="segmented">
        <button :class="{ active: templateStartMode === 'blank' }" type="button" @click="useBlankStart">
          {{ t('builder.blankScenario') }}
        </button>
        <button :class="{ active: templateStartMode === 'template' }" type="button" @click="templateStartMode = 'template'">
          {{ t('builder.fromTemplateOption') }}
        </button>
      </div>
      <div v-if="templateStartMode === 'template'" class="builder-template-picker">
        <label>
          <span>{{ t('builder.template') }}</span>
          <select v-model="selectedTemplateId" class="control">
            <option v-for="template in templates" :key="template.id" :value="template.id">
              {{ displayTemplateName(template) }}
            </option>
          </select>
        </label>
        <button class="primary-action fit" type="button" :disabled="!selectedTemplateId" @click="applySelectedTemplate">
          {{ t('builder.applyTemplate') }}
        </button>
        <p v-if="selectedTemplate" class="template-picker-note">
          {{ displayTemplateDescription(selectedTemplate) || t('templates.noDescription') }}
        </p>
      </div>
    </section>

    <nav class="builder-stepper" :aria-label="t('builder.stepsLabel')">
      <button :aria-current="step === 1 ? 'step' : undefined" type="button" @click="step = 1">1 {{ t('builder.steps.broker') }}</button>
      <button :aria-current="step === 2 ? 'step' : undefined" type="button" @click="step = 2">2 {{ t('builder.steps.workloads') }}</button>
      <button :aria-current="step === 3 ? 'step' : undefined" type="button" @click="step = 3">3 {{ t('builder.steps.profile') }}</button>
    </nav>

    <section class="panel builder-panel">
      <div v-if="step === 1" class="builder-section">
        <label>
          <span>{{ t('fields.name') }}</span>
          <input v-model="form.name" class="control" />
        </label>
        <label>
          <span>{{ t('fields.description') }}</span>
          <input v-model="form.description" class="control" />
        </label>
        <div class="segmented">
          <button :class="{ active: brokerMode === 'saved' }" type="button" @click="brokerMode = 'saved'">{{ t('builder.savedBroker') }}</button>
          <button :class="{ active: brokerMode === 'adhoc' }" type="button" @click="brokerMode = 'adhoc'">{{ t('builder.oneOffBroker') }}</button>
        </div>
        <div v-if="brokerMode === 'saved'" class="builder-section nested">
          <label>
            <span>{{ t('builder.brokerProfile') }}</span>
            <select v-model="brokerProfileId" class="control">
              <option v-for="broker in profiles.brokers" :key="broker.id" :value="broker.id">
                {{ broker.name }} · {{ broker.host }}:{{ broker.port }}
              </option>
            </select>
          </label>
          <button class="secondary-action fit" type="button" :disabled="connectionState === 'testing' || !brokerProfileId" @click="testConnection">
            <TestTube2 :size="15" />
            {{ t('builder.testConnection') }}
          </button>
          <span v-if="connectionMessage" class="status-chip fit" :data-status="connectionState">{{ connectionMessage }}</span>
        </div>
        <div v-else class="sheet-grid">
          <label>
            <span>{{ t('fields.name') }}</span>
            <input v-model="adhocBroker.name" class="control" />
          </label>
          <label>
            <span>{{ t('fields.protocol') }}</span>
            <select class="control" :value="adhocBroker.protocol" @change="applyAdhocProtocol(($event.target as HTMLSelectElement).value as BrokerProtocol)">
              <option v-for="protocol in protocolOptions" :key="protocol" :value="protocol">{{ t(`protocol.${protocol}`) }}</option>
            </select>
          </label>
          <label>
            <span>{{ t('fields.host') }}</span>
            <input v-model="adhocBroker.host" class="control" />
          </label>
          <label>
            <span>{{ t('fields.port') }}</span>
            <input v-model.number="adhocBroker.port" class="control" type="number" min="1" />
          </label>
          <label v-if="isWebSocket(adhocBroker.protocol)">
            <span>{{ t('fields.websocketPath') }}</span>
            <input v-model="adhocBroker.websocket_path" class="control" placeholder="/mqtt" />
          </label>
        </div>
      </div>

      <div v-if="step === 2" class="builder-section">
        <div v-for="workload in workloads" :key="workload.id" class="workload-card">
          <div class="workload-head">
            <input v-model="workload.name" class="control" />
            <button class="icon-button" type="button" :aria-label="t('a11y.removeWorkload')" :disabled="workloads.length === 1" @click="removeWorkload(workload.id)">
              <Trash2 :size="16" />
            </button>
          </div>
          <div class="sheet-grid">
            <label>
              <span>{{ t('fields.mode') }}</span>
              <select v-model="workload.kind" class="control">
                <option value="pub">{{ t('builder.modes.pub') }}</option>
                <option value="sub">{{ t('builder.modes.sub') }}</option>
                <option value="conn">{{ t('builder.modes.conn') }}</option>
              </select>
            </label>
            <label>
              <span>{{ t('fields.clients') }}</span>
              <input v-model.number="workload.clients" class="control" type="number" min="1" />
            </label>
            <label>
              <span>{{ t('fields.topic') }}</span>
              <input v-model="workload.topics.topic_template" class="control" />
            </label>
            <label>
              <span>QoS</span>
              <select v-model="workload.qos" class="control">
                <option value="qos0">0</option>
                <option value="qos1">1</option>
                <option value="qos2">2</option>
              </select>
            </label>
            <label>
              <span>{{ t('fields.payload') }}</span>
              <select v-model="workload.payload_profile_id" class="control">
                <option :value="null">{{ t('common.none') }}</option>
                <option v-for="payload in profiles.payloads" :key="payload.id" :value="payload.id">{{ payload.name }}</option>
              </select>
            </label>
            <label>
              <span>{{ t('fields.clientId') }}</span>
              <input v-model="workload.client_id_template" class="control" />
            </label>
            <label>
              <span>{{ t('fields.partitions') }}</span>
              <input v-model.number="workload.topics.partitions" class="control" type="number" min="1" />
            </label>
            <label>
              <span>{{ t('fields.distribution') }}</span>
              <select v-model="workload.topics.group_strategy" class="control">
                <option value="client_id">{{ t('builder.distribution.client_id') }}</option>
                <option value="round_robin">{{ t('builder.distribution.round_robin') }}</option>
                <option value="random">{{ t('builder.distribution.random') }}</option>
              </select>
            </label>
          </div>
          <div class="load-editors">
            <div class="load-editor">
              <strong>{{ t('builder.connectProfile') }}</strong>
              <select class="control" :value="workload.load.connect_shape.shape" @change="onShapeChange($event, workload, 'connect_shape')">
                <option value="flat">{{ t('loadShape.flat') }}</option>
                <option value="ramp">{{ t('loadShape.ramp') }}</option>
                <option value="step">{{ t('loadShape.step') }}</option>
                <option value="soak">{{ t('loadShape.soak') }}</option>
                <option value="spike">{{ t('loadShape.spike') }}</option>
              </select>
              <svg class="load-preview" viewBox="0 0 100 40" preserveAspectRatio="none">
                <polyline :points="previewPoints(workload.load.connect_shape)" />
              </svg>
              <div class="shape-fields">
                <template v-if="workload.load.connect_shape.shape === 'flat'">
                  <label><span>{{ t('fields.ratePerSec') }}</span><input v-model.number="workload.load.connect_shape.rate" class="control" type="number" min="0" /></label>
                </template>
                <template v-else-if="workload.load.connect_shape.shape === 'ramp'">
                  <label><span>{{ t('fields.from') }}</span><input v-model.number="workload.load.connect_shape.from" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.to') }}</span><input v-model.number="workload.load.connect_shape.to" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.durationMs') }}</span><input v-model.number="workload.load.connect_shape.duration_ms" class="control" type="number" min="1" /></label>
                </template>
                <template v-else-if="workload.load.connect_shape.shape === 'step'">
                  <div v-for="(stage, index) in workload.load.connect_shape.stages" :key="index" class="step-row">
                    <input v-model.number="stage.rate" class="control" type="number" min="0" />
                    <input v-model.number="stage.duration_ms" class="control" type="number" min="1" />
                    <button class="icon-button" type="button" :aria-label="t('a11y.removeStep')" :disabled="workload.load.connect_shape.stages.length === 1" @click="removeStep(workload.load.connect_shape, index)">
                      <Trash2 :size="14" />
                    </button>
                  </div>
                  <button class="secondary-action fit" type="button" @click="appendStep(workload.load.connect_shape)">{{ t('builder.addStep') }}</button>
                </template>
                <template v-else-if="workload.load.connect_shape.shape === 'soak'">
                  <label><span>{{ t('fields.ratePerSec') }}</span><input v-model.number="workload.load.connect_shape.rate" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.durationMs') }}</span><input v-model.number="workload.load.connect_shape.duration_ms" class="control" type="number" min="1" /></label>
                </template>
                <template v-else>
                  <label><span>{{ t('fields.baseline') }}</span><input v-model.number="workload.load.connect_shape.baseline" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.peak') }}</span><input v-model.number="workload.load.connect_shape.peak" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.peakMs') }}</span><input v-model.number="workload.load.connect_shape.peak_duration_ms" class="control" type="number" min="1" /></label>
                  <label><span>{{ t('fields.periodMs') }}</span><input v-model.number="workload.load.connect_shape.period_ms" class="control" type="number" min="1" /></label>
                </template>
              </div>
            </div>
            <div class="load-editor">
              <strong>{{ t('builder.messageProfile') }}</strong>
              <select class="control" :value="workload.load.message_shape.shape" @change="onShapeChange($event, workload, 'message_shape')">
                <option value="flat">{{ t('loadShape.flat') }}</option>
                <option value="ramp">{{ t('loadShape.ramp') }}</option>
                <option value="step">{{ t('loadShape.step') }}</option>
                <option value="soak">{{ t('loadShape.soak') }}</option>
                <option value="spike">{{ t('loadShape.spike') }}</option>
              </select>
              <svg class="load-preview" viewBox="0 0 100 40" preserveAspectRatio="none">
                <polyline :points="previewPoints(workload.load.message_shape)" />
              </svg>
              <div class="shape-fields">
                <template v-if="workload.load.message_shape.shape === 'flat'">
                  <label><span>{{ t('fields.ratePerSec') }}</span><input v-model.number="workload.load.message_shape.rate" class="control" type="number" min="0" /></label>
                </template>
                <template v-else-if="workload.load.message_shape.shape === 'ramp'">
                  <label><span>{{ t('fields.from') }}</span><input v-model.number="workload.load.message_shape.from" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.to') }}</span><input v-model.number="workload.load.message_shape.to" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.durationMs') }}</span><input v-model.number="workload.load.message_shape.duration_ms" class="control" type="number" min="1" /></label>
                </template>
                <template v-else-if="workload.load.message_shape.shape === 'step'">
                  <div v-for="(stage, index) in workload.load.message_shape.stages" :key="index" class="step-row">
                    <input v-model.number="stage.rate" class="control" type="number" min="0" />
                    <input v-model.number="stage.duration_ms" class="control" type="number" min="1" />
                    <button class="icon-button" type="button" :aria-label="t('a11y.removeStep')" :disabled="workload.load.message_shape.stages.length === 1" @click="removeStep(workload.load.message_shape, index)">
                      <Trash2 :size="14" />
                    </button>
                  </div>
                  <button class="secondary-action fit" type="button" @click="appendStep(workload.load.message_shape)">{{ t('builder.addStep') }}</button>
                </template>
                <template v-else-if="workload.load.message_shape.shape === 'soak'">
                  <label><span>{{ t('fields.ratePerSec') }}</span><input v-model.number="workload.load.message_shape.rate" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.durationMs') }}</span><input v-model.number="workload.load.message_shape.duration_ms" class="control" type="number" min="1" /></label>
                </template>
                <template v-else>
                  <label><span>{{ t('fields.baseline') }}</span><input v-model.number="workload.load.message_shape.baseline" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.peak') }}</span><input v-model.number="workload.load.message_shape.peak" class="control" type="number" min="0" /></label>
                  <label><span>{{ t('fields.peakMs') }}</span><input v-model.number="workload.load.message_shape.peak_duration_ms" class="control" type="number" min="1" /></label>
                  <label><span>{{ t('fields.periodMs') }}</span><input v-model.number="workload.load.message_shape.period_ms" class="control" type="number" min="1" /></label>
                </template>
              </div>
            </div>
          </div>
        </div>
        <button class="secondary-action" type="button" @click="addWorkload">
          <Plus :size="15" />
          {{ t('builder.addWorkload') }}
        </button>
      </div>

      <div v-if="step === 3 && workloads.length > 0" class="builder-section">
        <label>
          <span>{{ t('builder.stageStrategy') }}</span>
          <select v-model="stageStrategy" class="control">
            <option value="parallel">{{ t('builder.parallel') }}</option>
            <option value="sequential">{{ t('builder.sequential') }}</option>
          </select>
        </label>
        <label>
          <span>{{ t('builder.totalDurationMs') }}</span>
          <input v-model.number="durationMs" class="control" type="number" min="0" />
        </label>
        <label>
          <span>{{ t('builder.sampleIntervalMs') }}</span>
          <input v-model.number="sampleIntervalMs" class="control" type="number" min="250" />
        </label>
        <label>
          <span>{{ t('builder.nicBind') }}</span>
          <select v-model="networkBindMode" class="control">
            <option value="system">{{ t('bind.system') }}</option>
            <option value="auto_random">{{ t('bind.autoRandom') }}</option>
            <option value="auto_round_robin">{{ t('bind.autoRoundRobin') }}</option>
            <option value="manual_random">{{ t('bind.manualRandom') }}</option>
            <option value="manual_round_robin">{{ t('bind.manualRoundRobin') }}</option>
          </select>
        </label>
        <label>
          <span>{{ t('builder.bindInterfaces') }}</span>
          <input v-model="bindInterfacesText" class="control" :placeholder="t('builder.bindInterfacesPlaceholder')" />
        </label>
      </div>
    </section>

    <AppError :message="error" />
    <footer class="builder-footer">
      <button class="secondary-action" type="button" :disabled="!canPrev" @click="step--">
        <ChevronLeft :size="15" />
        {{ t('common.previous') }}
      </button>
      <button v-if="canNext" class="primary-action" type="button" @click="step++">
        {{ t('common.next') }}
        <ChevronRight :size="15" />
      </button>
      <template v-else>
        <button class="secondary-action" type="button" :disabled="busy" @click="saveScenario">
          <Save :size="15" />
          {{ t('common.save') }}
        </button>
        <button v-if="!sourceTemplateName" class="primary-action" type="button" :disabled="busy" @click="saveAndRun">
          <Play :size="15" />
          {{ t('builder.saveAndRun') }}
        </button>
        <button v-if="!sourceTemplateName" class="secondary-action" type="button" :disabled="busy" @click="runOnce">{{ t('builder.runOnce') }}</button>
      </template>
    </footer>
  </section>
</template>
