<script setup lang="ts">
import { ref } from 'vue';
import { Upload } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppError from '@/components/feedback/AppError.vue';
import { useToast } from '@/composables/useToast';

const { t } = useI18n();
const toast = useToast();
const conflict = ref<'skip' | 'rename' | 'overwrite'>('skip');
const busy = ref(false);
const error = ref('');
const fileName = ref('');
const counts = ref<Awaited<ReturnType<typeof api.importBundle>> | null>(null);
const bundleFile = ref<File | null>(null);
const preview = ref<{
  runs: number;
  snapshots: number;
  scenarios: number;
  profiles: number;
  scenarioNames: string[];
} | null>(null);
const zipSelected = ref(false);

async function onFile(event: Event) {
  error.value = '';
  counts.value = null;
  preview.value = null;
  zipSelected.value = false;
  const file = (event.target as HTMLInputElement).files?.[0];
  if (!file) return;
  fileName.value = file.name;
  bundleFile.value = file;
  if (file.name.endsWith('.zip') || file.type === 'application/zip') {
    zipSelected.value = true;
    return;
  }
  try {
    const bundle = JSON.parse(await file.text()) as {
      scenarios?: Array<{ name?: string }>;
      broker_profiles?: unknown[];
      payload_profiles?: unknown[];
      runs?: Array<{ snapshots?: unknown[] }>;
    };
    preview.value = {
      runs: bundle.runs?.length ?? 0,
      snapshots: bundle.runs?.reduce((total, run) => total + (run.snapshots?.length ?? 0), 0) ?? 0,
      scenarios: bundle.scenarios?.length ?? 0,
      profiles: (bundle.broker_profiles?.length ?? 0) + (bundle.payload_profiles?.length ?? 0),
      scenarioNames: (bundle.scenarios?.map((scenario) => scenario.name).filter(Boolean).slice(0, 5) ?? []) as string[],
    };
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function importBundle() {
  if (!bundleFile.value) return;
  busy.value = true;
  error.value = '';
  try {
    counts.value = await api.importBundle(bundleFile.value, conflict.value);
    toast.success(t('settings.import.imported'));
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    toast.error(error.value);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('settings.import.title') }}</h1>
        <p>{{ t('settings.import.subtitle') }}</p>
      </div>
    </div>

    <section class="panel profile-editor">
      <label>
        <span>{{ t('settings.import.bundleJson') }}</span>
        <input class="control" type="file" accept="application/json,application/zip,.json,.zip" @change="onFile" />
      </label>
      <label>
        <span>{{ t('settings.import.conflict') }}</span>
        <select v-model="conflict" class="control">
          <option value="skip">{{ t('settings.import.skip') }}</option>
          <option value="rename">{{ t('settings.import.rename') }}</option>
          <option value="overwrite">{{ t('settings.import.overwrite') }}</option>
        </select>
      </label>
      <button class="primary-action fit" type="button" :disabled="busy || !bundleFile" @click="importBundle">
        <Upload :size="15" />
        {{ t('settings.import.action') }}
      </button>
      <p v-if="fileName" class="status-chip fit">{{ fileName }}</p>
      <AppError :message="error" />
    </section>

    <section v-if="preview || zipSelected" class="panel">
      <div class="panel-head"><h2>{{ t('settings.import.preview') }}</h2></div>
      <p v-if="zipSelected" class="muted-copy">{{ t('settings.import.zipPreview') }}</p>
      <template v-else-if="preview">
        <div class="kpi-strip">
          <article class="kpi-card"><span>{{ t('settings.import.runs') }}</span><strong>{{ preview.runs }}</strong></article>
          <article class="kpi-card"><span>{{ t('settings.import.snapshots') }}</span><strong>{{ preview.snapshots }}</strong></article>
          <article class="kpi-card"><span>{{ t('settings.import.scenarios') }}</span><strong>{{ preview.scenarios }}</strong></article>
          <article class="kpi-card"><span>{{ t('settings.import.profiles') }}</span><strong>{{ preview.profiles }}</strong></article>
        </div>
        <div v-if="preview.scenarioNames.length" class="tag-row">
          <span v-for="name in preview.scenarioNames" :key="name" class="tag-pill">{{ name }}</span>
        </div>
      </template>
    </section>

    <section v-if="counts" class="panel">
      <div class="kpi-strip">
        <article class="kpi-card"><span>{{ t('settings.import.runs') }}</span><strong>{{ counts.runs }}</strong></article>
        <article class="kpi-card"><span>{{ t('settings.import.snapshots') }}</span><strong>{{ counts.snapshots }}</strong></article>
        <article class="kpi-card"><span>{{ t('settings.import.scenarios') }}</span><strong>{{ counts.scenarios }}</strong></article>
        <article class="kpi-card"><span>{{ t('settings.import.profiles') }}</span><strong>{{ counts.broker_profiles + counts.payload_profiles }}</strong></article>
      </div>
    </section>
  </section>
</template>
