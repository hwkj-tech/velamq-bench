<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Eye, RefreshCw, Wand2 } from 'lucide-vue-next';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { api } from '@/api/client';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';
import type { BenchTemplate } from '@/api/types';

const router = useRouter();
const { t } = useI18n();
const templates = ref<BenchTemplate[]>([]);
const loading = ref(false);
const error = ref('');
const expandedId = ref('');

const sortedTemplates = computed(() =>
  [...templates.value].sort((a, b) => a.name.localeCompare(b.name)),
);

onMounted(loadTemplates);

async function loadTemplates() {
  loading.value = true;
  error.value = '';
  try {
    templates.value = await api.templates();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

function useTemplate(template: BenchTemplate) {
  router.push({ path: '/scenarios/new', query: { template: template.id } });
}

function templateMeta(template: BenchTemplate) {
  const config = template.config;
  return t('templates.meta', {
    mode: t(`builder.modes.${config.mode}`),
    clients: config.clients,
    seconds: config.duration_secs,
  });
}

function displayName(template: BenchTemplate) {
  const key = `templates.defaults.${template.id}.name`;
  const value = t(key);
  return value === key ? template.name : value;
}

function displayDescription(template: BenchTemplate) {
  const key = `templates.defaults.${template.id}.description`;
  const value = t(key);
  return value === key ? template.description : value;
}

function togglePreview(id: string) {
  expandedId.value = expandedId.value === id ? '' : id;
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('templates.title') }}</h1>
        <p>{{ t('templates.note') }}</p>
      </div>
      <button class="secondary-action" type="button" @click="loadTemplates">
        <RefreshCw :size="16" />
        {{ t('common.refresh') }}
      </button>
    </div>

    <AppError :message="error" />

    <section class="panel">
      <div class="template-grid">
        <article v-for="template in sortedTemplates" :key="template.id" class="template-card">
          <div class="template-card-main">
            <strong>{{ displayName(template) }}</strong>
            <span>{{ displayDescription(template) || t('templates.noDescription') }}</span>
            <small>{{ templateMeta(template) }}</small>
          </div>
          <div class="template-stats">
            <span>{{ t(`protocol.${template.config.protocol ?? 'mqtt'}`) }} · {{ template.config.host }}:{{ template.config.port }}{{ template.config.websocket_path || '' }}</span>
            <span>{{ template.config.topic }}</span>
            <span>{{ template.config.payload_size }} B</span>
          </div>
          <dl v-if="expandedId === template.id" class="template-preview">
            <div><dt>{{ t('fields.mode') }}</dt><dd>{{ t(`builder.modes.${template.config.mode}`) }}</dd></div>
            <div><dt>{{ t('fields.protocol') }}</dt><dd>{{ t(`protocol.${template.config.protocol ?? 'mqtt'}`) }}</dd></div>
            <div><dt>{{ t('fields.clients') }}</dt><dd>{{ template.config.clients }}</dd></div>
            <div><dt>{{ t('fields.ratePerSec') }}</dt><dd>{{ (1000 / template.config.message_interval_ms).toFixed(1) }}</dd></div>
            <div><dt>{{ t('builder.totalDurationMs') }}</dt><dd>{{ template.config.duration_secs }}s</dd></div>
            <div><dt>QoS</dt><dd>{{ template.config.qos.replace('qos', '') }}</dd></div>
            <div><dt>{{ t('fields.payload') }}</dt><dd>{{ template.config.payload_size }} B</dd></div>
          </dl>
          <div class="scenario-actions">
            <button class="primary-action" type="button" @click="useTemplate(template)">
              <Wand2 :size="15" />
              {{ t('templates.useTemplate') }}
            </button>
            <button class="secondary-action" type="button" @click="togglePreview(template.id)">
              <Eye :size="15" />
              {{ expandedId === template.id ? t('templates.hidePreview') : t('templates.preview') }}
            </button>
          </div>
        </article>
        <AppEmpty v-if="!loading && sortedTemplates.length === 0" :title="t('templates.empty')" compact />
        <AppLoading v-if="loading" :label="t('common.loading')" compact />
      </div>
    </section>
  </section>
</template>
