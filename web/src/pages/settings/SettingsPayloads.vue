<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Edit, Plus, Save, Trash2, X } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import { useProfilesStore } from '@/stores/profiles';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import type { PayloadKind, PayloadProfile } from '@/api/types';

const profiles = useProfilesStore();
const { t } = useI18n();
const editing = ref(false);
const busy = ref(false);
const error = ref('');
const form = reactive<Partial<PayloadProfile>>({ id: '', name: '', kind: fixedKind(), created_at: '', updated_at: '' });
const kindName = computed({
  get: () => form.kind?.kind ?? 'fixed_bytes',
  set: (value: PayloadKind['kind']) => {
    form.kind =
      value === 'fixed_bytes'
        ? fixedKind()
        : value === 'json_template'
          ? { kind: 'json_template', template: '{\"id\":\"{{id}}\",\"ts\":{{ts}}}', vars: { id: 'client_id', ts: 'now_ms' } }
          : value === 'csv_replay'
            ? { kind: 'csv_replay', path: 'data/samples/sample.csv', column: 'payload', loop_when_done: true }
            : { kind: 'counter', width: 8 };
  },
});

onMounted(() => profiles.load());

function fixedKind(): PayloadKind {
  return { kind: 'fixed_bytes', size: 256, with_timestamp: true };
}

function startNew() {
  Object.assign(form, { id: '', name: '', kind: fixedKind(), created_at: '', updated_at: '' });
  editing.value = true;
}

function edit(profile: PayloadProfile) {
  Object.assign(form, JSON.parse(JSON.stringify(profile)) as PayloadProfile);
  editing.value = true;
}

async function save() {
  busy.value = true;
  error.value = '';
  try {
    if (form.id) await api.updatePayload({ ...form, id: form.id });
    else await api.createPayload(form);
    editing.value = false;
    await profiles.load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function remove(id: string) {
  if (!window.confirm(t('settings.payloads.confirmDelete'))) return;
  await api.deletePayload(id);
  await profiles.load();
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('settings.payloads.title') }}</h1>
        <p>{{ t('settings.payloads.subtitle') }}</p>
      </div>
      <button class="primary-action" type="button" @click="startNew">
        <Plus :size="16" />
        {{ t('common.new') }}
      </button>
    </div>

    <AppError :message="error" />

    <section v-if="editing" class="panel profile-editor">
      <div class="panel-head split">
        <h2>{{ form.id ? t('settings.payloads.edit') : t('settings.payloads.new') }}</h2>
        <button class="icon-button" type="button" :aria-label="t('common.close')" @click="editing = false"><X :size="16" /></button>
      </div>
      <div class="sheet-grid">
        <label><span>{{ t('fields.name') }}</span><input v-model="form.name" class="control" /></label>
        <label>
          <span>{{ t('settings.payloads.generator') }}</span>
          <select v-model="kindName" class="control">
            <option value="fixed_bytes">{{ t('payloadKind.fixedBytes') }}</option>
            <option value="json_template">{{ t('payloadKind.jsonTemplate') }}</option>
            <option value="csv_replay">{{ t('payloadKind.csvReplay') }}</option>
            <option value="counter">{{ t('payloadKind.counter') }}</option>
          </select>
        </label>
      </div>

      <div v-if="form.kind?.kind === 'fixed_bytes'" class="sheet-grid">
        <label><span>{{ t('fields.size') }}</span><input v-model.number="form.kind.size" class="control" type="number" min="1" /></label>
        <label class="checkbox-row"><input v-model="form.kind.with_timestamp" type="checkbox" /> {{ t('settings.payloads.withTimestamp') }}</label>
      </div>
      <div v-else-if="form.kind?.kind === 'json_template'" class="builder-section">
        <label><span>{{ t('settings.payloads.template') }}</span><input v-model="form.kind.template" class="control" /></label>
        <label><span>{{ t('settings.payloads.varsJson') }}</span><input :value="JSON.stringify(form.kind.vars)" class="control" @change="form.kind.vars = JSON.parse(($event.target as HTMLInputElement).value)" /></label>
      </div>
      <div v-else-if="form.kind?.kind === 'csv_replay'" class="sheet-grid">
        <label><span>{{ t('fields.path') }}</span><input v-model="form.kind.path" class="control" /></label>
        <label><span>{{ t('fields.column') }}</span><input v-model="form.kind.column" class="control" /></label>
        <label class="checkbox-row"><input v-model="form.kind.loop_when_done" type="checkbox" /> {{ t('settings.payloads.loopWhenDone') }}</label>
      </div>
      <div v-else-if="form.kind?.kind === 'counter'" class="sheet-grid">
        <label><span>{{ t('fields.width') }}</span><input v-model.number="form.kind.width" class="control" type="number" min="1" /></label>
      </div>

      <button class="primary-action fit" type="button" :disabled="busy" @click="save"><Save :size="15" />{{ t('common.save') }}</button>
    </section>

    <section class="panel">
      <div class="profile-table">
        <div v-for="payload in profiles.payloads" :key="payload.id" class="profile-row">
          <strong>{{ payload.name }}</strong>
          <span>{{ payload.kind.kind }}</span>
          <span>{{ payload.updated_at ? new Date(payload.updated_at).toLocaleString() : '' }}</span>
          <div class="scenario-actions">
            <button class="secondary-action" type="button" @click="edit(payload)"><Edit :size="15" />{{ t('common.edit') }}</button>
            <button class="secondary-action danger" type="button" @click="remove(payload.id)"><Trash2 :size="15" />{{ t('common.delete') }}</button>
          </div>
        </div>
        <AppEmpty v-if="profiles.payloads.length === 0" :title="t('settings.payloads.empty')" compact />
      </div>
    </section>
  </section>
</template>
