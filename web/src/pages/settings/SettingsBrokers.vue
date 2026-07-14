<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { Edit, PlugZap, Plus, Save, Trash2, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import { useProfilesStore } from '@/stores/profiles';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import type { BrokerProfile, BrokerProtocol } from '@/api/types';

const profiles = useProfilesStore();
const { t } = useI18n();
const editing = ref(false);
const busy = ref(false);
const error = ref('');
const testResult = ref('');
const form = reactive<Partial<BrokerProfile>>({
  id: '',
  name: '',
  protocol: 'mqtt',
  host: '127.0.0.1',
  port: 1883,
  websocket_path: null,
  keepalive_secs: 30,
  clean_session: true,
});
const protocolOptions: BrokerProtocol[] = ['mqtt', 'mqtts', 'ws', 'wss'];

function defaultPort(protocol: BrokerProtocol) {
  return { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 }[protocol];
}

function isWebSocket(protocol?: BrokerProtocol) {
  return protocol === 'ws' || protocol === 'wss';
}

function applyProtocol(protocol: BrokerProtocol) {
  form.protocol = protocol;
  form.port = defaultPort(protocol);
  form.websocket_path = isWebSocket(protocol) ? (form.websocket_path || '/mqtt') : null;
}

onMounted(() => profiles.load());

function startNew() {
  Object.assign(form, { id: '', name: '', protocol: 'mqtt', host: '127.0.0.1', port: 1883, websocket_path: null, keepalive_secs: 30, clean_session: true });
  testResult.value = '';
  editing.value = true;
}

function edit(profile: BrokerProfile) {
  Object.assign(form, profile);
  testResult.value = '';
  editing.value = true;
}

async function save() {
  busy.value = true;
  error.value = '';
  try {
    if (form.id) await api.updateBroker({ ...form, id: form.id });
    else await api.createBroker(form);
    editing.value = false;
    await profiles.load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function remove(id: string) {
  if (!window.confirm(t('settings.brokers.confirmDelete'))) return;
  await api.deleteBroker(id);
  await profiles.load();
}

async function test(id?: string) {
  if (!id) {
    testResult.value = t('settings.brokers.saveBeforeTesting');
    return;
  }
  const result = await api.testBroker(id);
  testResult.value = result.ok ? t('settings.brokers.connectedIn', { ms: result.elapsed_ms }) : (result.error ?? t('settings.brokers.connectionFailed'));
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('settings.brokers.title') }}</h1>
        <p>{{ t('settings.brokers.subtitle') }}</p>
      </div>
      <button class="primary-action" type="button" @click="startNew">
        <Plus :size="16" />
        {{ t('common.new') }}
      </button>
    </div>

    <AppError :message="error" />

    <section v-if="editing" class="panel profile-editor">
      <div class="panel-head split">
        <h2>{{ form.id ? t('settings.brokers.edit') : t('settings.brokers.new') }}</h2>
        <button class="icon-button" type="button" :aria-label="t('common.close')" @click="editing = false"><X :size="16" /></button>
      </div>
      <div class="sheet-grid">
        <label><span>{{ t('fields.name') }}</span><input v-model="form.name" class="control" /></label>
        <label>
          <span>{{ t('fields.protocol') }}</span>
          <select class="control" :value="form.protocol" @change="applyProtocol(($event.target as HTMLSelectElement).value as BrokerProtocol)">
            <option v-for="protocol in protocolOptions" :key="protocol" :value="protocol">{{ t(`protocol.${protocol}`) }}</option>
          </select>
        </label>
        <label><span>{{ t('fields.host') }}</span><input v-model="form.host" class="control" /></label>
        <label><span>{{ t('fields.port') }}</span><input v-model.number="form.port" class="control" type="number" min="1" /></label>
        <label v-if="isWebSocket(form.protocol)">
          <span>{{ t('fields.websocketPath') }}</span>
          <input v-model="form.websocket_path" class="control" placeholder="/mqtt" />
        </label>
        <label><span>{{ t('settings.brokers.keepalive') }}</span><input v-model.number="form.keepalive_secs" class="control" type="number" min="1" /></label>
      </div>
      <label class="checkbox-row"><input v-model="form.clean_session" type="checkbox" /> {{ t('settings.brokers.cleanSession') }}</label>
      <div class="topbar-actions">
        <button class="primary-action" type="button" :disabled="busy" @click="save"><Save :size="15" />{{ t('common.save') }}</button>
        <button class="secondary-action" type="button" @click="test(form.id)"><PlugZap :size="15" />{{ t('common.test') }}</button>
        <span v-if="testResult" class="status-chip fit">{{ testResult }}</span>
      </div>
    </section>

    <section class="panel">
      <div class="profile-table">
        <div v-for="broker in profiles.brokers" :key="broker.id" class="profile-row">
          <strong>{{ broker.name || `${broker.host}:${broker.port}` }}</strong>
          <span>{{ t(`protocol.${broker.protocol}`) }} · {{ broker.host }}:{{ broker.port }}{{ broker.websocket_path || '' }}</span>
          <span>{{ broker.clean_session ? t('settings.brokers.clean') : t('settings.brokers.persistent') }}</span>
          <div class="scenario-actions">
            <button class="secondary-action" type="button" @click="test(broker.id)"><PlugZap :size="15" />{{ t('common.test') }}</button>
            <button class="secondary-action" type="button" @click="edit(broker)"><Edit :size="15" />{{ t('common.edit') }}</button>
            <button class="secondary-action danger" type="button" @click="remove(broker.id)"><Trash2 :size="15" />{{ t('common.delete') }}</button>
          </div>
        </div>
        <AppEmpty v-if="profiles.brokers.length === 0" :title="t('settings.brokers.empty')" compact />
      </div>
    </section>
  </section>
</template>
