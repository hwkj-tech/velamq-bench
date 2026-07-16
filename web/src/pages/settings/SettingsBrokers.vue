<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Edit, LockKeyhole, PlugZap, Plus, Save, ShieldCheck, Trash2, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import { useProfilesStore } from '@/stores/profiles';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import type { BrokerProfile, BrokerProtocol, MqttVersion } from '@/api/types';

type AuthMode = 'none' | 'user_password';
interface BrokerForm {
  id: string;
  name: string;
  protocol: BrokerProtocol;
  mqtt_version: MqttVersion;
  host: string;
  port: number;
  websocket_path: string | null;
  keepalive_secs: number;
  connection_timeout_secs: number;
  clean_session: boolean;
  auth_mode: AuthMode;
  username: string;
  password: string;
  ca_pem: string;
  client_cert_pem: string;
  client_key_pem: string;
  insecure_skip_verify: boolean;
  alpn_protocols: string;
  session_expiry_interval_secs: number | null;
  receive_maximum: number | null;
  maximum_packet_size: number | null;
  topic_alias_maximum: number | null;
  request_problem_information: boolean;
}

const profiles = useProfilesStore();
const { t } = useI18n();
const editing = ref(false);
const busy = ref(false);
const error = ref('');
const testResult = ref('');
const protocolOptions: BrokerProtocol[] = ['mqtt', 'mqtts', 'ws', 'wss'];
const versionOptions: MqttVersion[] = ['v3_1_1', 'v5_0'];

function blankForm(): BrokerForm {
  return {
    id: '', name: '', protocol: 'mqtt', mqtt_version: 'v3_1_1', host: '127.0.0.1', port: 1883,
    websocket_path: null, keepalive_secs: 30, connection_timeout_secs: 10, clean_session: true,
    auth_mode: 'none', username: '', password: '', ca_pem: '', client_cert_pem: '', client_key_pem: '',
    insecure_skip_verify: false, alpn_protocols: '', session_expiry_interval_secs: null,
    receive_maximum: null, maximum_packet_size: null, topic_alias_maximum: null,
    request_problem_information: true,
  };
}

const form = reactive<BrokerForm>(blankForm());
const isWebSocket = computed(() => form.protocol === 'ws' || form.protocol === 'wss');
const isTls = computed(() => form.protocol === 'mqtts' || form.protocol === 'wss');
const isMqtt5 = computed(() => form.mqtt_version === 'v5_0');

function defaultPort(protocol: BrokerProtocol) {
  return { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 }[protocol];
}

function applyProtocol(protocol: BrokerProtocol) {
  form.protocol = protocol;
  form.port = defaultPort(protocol);
  form.websocket_path = protocol === 'ws' || protocol === 'wss' ? (form.websocket_path || '/mqtt') : null;
}

function applyVersion(version: MqttVersion) {
  form.mqtt_version = version;
  if (version === 'v3_1_1') form.session_expiry_interval_secs = null;
}

onMounted(() => profiles.load());

function startNew() {
  Object.assign(form, blankForm());
  testResult.value = '';
  editing.value = true;
}

function edit(profile: BrokerProfile) {
  const auth = profile.auth?.kind === 'user_password' ? profile.auth : null;
  Object.assign(form, blankForm(), {
    ...profile,
    auth_mode: auth ? 'user_password' : 'none',
    username: auth?.username ?? '',
    password: auth?.password ?? '',
    ca_pem: profile.tls?.ca_pem ?? '',
    client_cert_pem: profile.tls?.client_cert_pem ?? (profile.auth?.kind === 'client_cert' ? profile.auth.cert_pem : ''),
    client_key_pem: profile.tls?.client_key_pem ?? (profile.auth?.kind === 'client_cert' ? profile.auth.key_pem : ''),
    insecure_skip_verify: profile.tls?.insecure_skip_verify ?? false,
    alpn_protocols: profile.tls?.alpn_protocols?.join(', ') ?? '',
    session_expiry_interval_secs: profile.mqtt5?.session_expiry_interval_secs ?? null,
    receive_maximum: profile.mqtt5?.receive_maximum ?? null,
    maximum_packet_size: profile.mqtt5?.maximum_packet_size ?? null,
    topic_alias_maximum: profile.mqtt5?.topic_alias_maximum ?? null,
    request_problem_information: profile.mqtt5?.request_problem_information ?? true,
  });
  testResult.value = '';
  editing.value = true;
}

function payload(): Partial<BrokerProfile> {
  return {
    id: form.id, name: form.name, protocol: form.protocol, mqtt_version: form.mqtt_version,
    host: form.host, port: form.port, websocket_path: form.websocket_path,
    keepalive_secs: form.keepalive_secs, connection_timeout_secs: form.connection_timeout_secs,
    clean_session: form.clean_session,
    auth: form.auth_mode === 'user_password'
      ? { kind: 'user_password', username: form.username, password: form.password }
      : { kind: 'none' },
    tls: isTls.value ? {
      enabled: true,
      ca_pem: form.ca_pem || null,
      client_cert_pem: form.client_cert_pem || null,
      client_key_pem: form.client_key_pem || null,
      server_name: null,
      insecure_skip_verify: form.insecure_skip_verify,
      alpn_protocols: form.alpn_protocols.split(',').map((item) => item.trim()).filter(Boolean),
    } : null,
    mqtt5: isMqtt5.value ? {
      session_expiry_interval_secs: form.session_expiry_interval_secs,
      receive_maximum: form.receive_maximum,
      maximum_packet_size: form.maximum_packet_size,
      topic_alias_maximum: form.topic_alias_maximum,
      request_problem_information: form.request_problem_information,
    } : null,
  };
}

async function save() {
  busy.value = true;
  error.value = '';
  try {
    const data = payload();
    if (form.id) await api.updateBroker({ ...data, id: form.id });
    else await api.createBroker(data);
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
  if (!id) { testResult.value = t('settings.brokers.saveBeforeTesting'); return; }
  const result = await api.testBroker(id);
  testResult.value = result.ok ? t('settings.brokers.connectedIn', { ms: result.elapsed_ms }) : (result.error ?? t('settings.brokers.connectionFailed'));
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div><h1>{{ t('settings.brokers.title') }}</h1><p>{{ t('settings.brokers.subtitle') }}</p></div>
      <button class="primary-action" type="button" @click="startNew"><Plus :size="16" />{{ t('common.new') }}</button>
    </div>
    <AppError :message="error" />

    <section v-if="editing" class="panel profile-editor broker-editor">
      <div class="panel-head split">
        <div><h2>{{ form.id ? t('settings.brokers.edit') : t('settings.brokers.new') }}</h2><p>{{ t('settings.brokers.editorHint') }}</p></div>
        <button class="icon-button" type="button" :aria-label="t('common.close')" @click="editing = false"><X :size="16" /></button>
      </div>

      <div class="broker-section">
        <div class="broker-section-title"><PlugZap :size="17" /><div><strong>{{ t('settings.brokers.endpoint') }}</strong><span>{{ t('settings.brokers.endpointHint') }}</span></div></div>
        <div class="sheet-grid">
          <label><span>{{ t('fields.name') }}</span><input v-model="form.name" class="control" /></label>
          <label><span>{{ t('settings.brokers.mqttVersion') }}</span><select class="control" :value="form.mqtt_version" @change="applyVersion(($event.target as HTMLSelectElement).value as MqttVersion)"><option v-for="version in versionOptions" :key="version" :value="version">{{ t(`settings.brokers.versions.${version}`) }}</option></select></label>
          <label><span>{{ t('fields.protocol') }}</span><select class="control" :value="form.protocol" @change="applyProtocol(($event.target as HTMLSelectElement).value as BrokerProtocol)"><option v-for="protocol in protocolOptions" :key="protocol" :value="protocol">{{ t(`protocol.${protocol}`) }}</option></select></label>
          <label><span>{{ t('fields.host') }}</span><input v-model.trim="form.host" class="control" placeholder="broker.example.com" /></label>
          <label><span>{{ t('fields.port') }}</span><input v-model.number="form.port" class="control" type="number" min="1" max="65535" /></label>
          <label v-if="isWebSocket"><span>{{ t('fields.websocketPath') }}</span><input v-model="form.websocket_path" class="control" placeholder="/mqtt" /></label>
          <label><span>{{ t('settings.brokers.keepalive') }}</span><input v-model.number="form.keepalive_secs" class="control" type="number" min="1" max="3600" /></label>
          <label><span>{{ t('settings.brokers.connectionTimeout') }}</span><input v-model.number="form.connection_timeout_secs" class="control" type="number" min="1" max="300" /></label>
        </div>
        <label class="checkbox-row"><input v-model="form.clean_session" type="checkbox" /> {{ isMqtt5 ? t('settings.brokers.cleanStart') : t('settings.brokers.cleanSession') }}</label>
      </div>

      <div class="broker-section">
        <div class="broker-section-title"><LockKeyhole :size="17" /><div><strong>{{ t('settings.brokers.authentication') }}</strong><span>{{ t('settings.brokers.authenticationHint') }}</span></div></div>
        <div class="sheet-grid">
          <label><span>{{ t('settings.brokers.authMode') }}</span><select v-model="form.auth_mode" class="control"><option value="none">{{ t('settings.brokers.noAuth') }}</option><option value="user_password">{{ t('settings.brokers.userPassword') }}</option></select></label>
          <template v-if="form.auth_mode === 'user_password'">
            <label><span>{{ t('settings.brokers.username') }}</span><input v-model="form.username" class="control" autocomplete="username" /></label>
            <label><span>{{ t('settings.brokers.password') }}</span><input v-model="form.password" class="control" type="password" autocomplete="new-password" /></label>
          </template>
        </div>
      </div>

      <div v-if="isTls" class="broker-section tls-section">
        <div class="broker-section-title"><ShieldCheck :size="17" /><div><strong>{{ t('settings.brokers.tls') }}</strong><span>{{ t('settings.brokers.tlsHint') }}</span></div></div>
        <div class="certificate-grid">
          <label><span>{{ t('settings.brokers.caCertificate') }}</span><textarea v-model="form.ca_pem" class="control code-control" rows="7" placeholder="-----BEGIN CERTIFICATE-----" /></label>
          <label><span>{{ t('settings.brokers.clientCertificate') }}</span><textarea v-model="form.client_cert_pem" class="control code-control" rows="7" placeholder="-----BEGIN CERTIFICATE-----" /></label>
          <label><span>{{ t('settings.brokers.clientKey') }}</span><textarea v-model="form.client_key_pem" class="control code-control" rows="7" placeholder="-----BEGIN PRIVATE KEY-----" /></label>
        </div>
        <div class="sheet-grid compact-grid"><label><span>{{ t('settings.brokers.alpn') }}</span><input v-model="form.alpn_protocols" class="control" placeholder="mqtt" /></label></div>
        <label class="checkbox-row danger-check"><input v-model="form.insecure_skip_verify" type="checkbox" /> {{ t('settings.brokers.skipVerify') }}</label>
      </div>

      <div v-if="isMqtt5" class="broker-section">
        <div class="broker-section-title"><span class="protocol-orb">5</span><div><strong>{{ t('settings.brokers.mqtt5Options') }}</strong><span>{{ t('settings.brokers.mqtt5Hint') }}</span></div></div>
        <div class="sheet-grid">
          <label><span>{{ t('settings.brokers.sessionExpiry') }}</span><input v-model.number="form.session_expiry_interval_secs" class="control" type="number" min="0" /></label>
          <label><span>{{ t('settings.brokers.receiveMaximum') }}</span><input v-model.number="form.receive_maximum" class="control" type="number" min="1" max="65535" /></label>
          <label><span>{{ t('settings.brokers.maximumPacketSize') }}</span><input v-model.number="form.maximum_packet_size" class="control" type="number" min="1" /></label>
          <label><span>{{ t('settings.brokers.topicAliasMaximum') }}</span><input v-model.number="form.topic_alias_maximum" class="control" type="number" min="0" max="65535" /></label>
        </div>
        <label class="checkbox-row"><input v-model="form.request_problem_information" type="checkbox" /> {{ t('settings.brokers.problemInformation') }}</label>
      </div>

      <div class="topbar-actions broker-actions"><button class="primary-action" type="button" :disabled="busy" @click="save"><Save :size="15" />{{ t('common.save') }}</button><button class="secondary-action" type="button" @click="test(form.id)"><PlugZap :size="15" />{{ t('common.test') }}</button><span v-if="testResult" class="status-chip fit">{{ testResult }}</span></div>
    </section>

    <section class="panel"><div class="profile-table"><div v-for="broker in profiles.brokers" :key="broker.id" class="profile-row"><strong>{{ broker.name || `${broker.host}:${broker.port}` }}</strong><span>{{ t(`settings.brokers.versions.${broker.mqtt_version}`) }} · {{ t(`protocol.${broker.protocol}`) }} · {{ broker.host }}:{{ broker.port }}{{ broker.websocket_path || '' }}</span><span>{{ broker.clean_session ? t('settings.brokers.clean') : t('settings.brokers.persistent') }}</span><div class="scenario-actions"><button class="secondary-action" type="button" @click="test(broker.id)"><PlugZap :size="15" />{{ t('common.test') }}</button><button class="secondary-action" type="button" @click="edit(broker)"><Edit :size="15" />{{ t('common.edit') }}</button><button class="secondary-action danger" type="button" @click="remove(broker.id)"><Trash2 :size="15" />{{ t('common.delete') }}</button></div></div><AppEmpty v-if="profiles.brokers.length === 0" :title="t('settings.brokers.empty')" compact /></div></section>
  </section>
</template>
