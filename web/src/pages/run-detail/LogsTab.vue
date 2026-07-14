<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import type { LogLine } from '@/api/types';

const props = defineProps<{ logs: LogLine[] }>();
const level = ref('all');
const query = ref('');
const paused = ref(false);
const { t } = useI18n();

const filtered = computed(() => {
  if (paused.value) return [];
  return props.logs
    .filter((log) => level.value === 'all' || log.level === level.value)
    .filter((log) => log.message.toLowerCase().includes(query.value.toLowerCase()))
    .slice(-500);
});
</script>

<template>
  <section class="panel run-tab-panel">
    <div class="logs-toolbar">
      <select v-model="level" class="control compact">
        <option value="all">{{ t('runDetail.logLevels.all') }}</option>
        <option value="info">{{ t('runDetail.logLevels.info') }}</option>
        <option value="warn">{{ t('runDetail.logLevels.warn') }}</option>
        <option value="error">{{ t('runDetail.logLevels.error') }}</option>
      </select>
      <input v-model="query" class="control" :placeholder="t('runDetail.searchLogs')" />
      <button class="secondary-action" type="button" @click="paused = !paused">{{ paused ? t('runDetail.resume') : t('runDetail.pause') }}</button>
    </div>
    <div class="log-list">
      <div v-for="log in filtered" :key="`${log.ts}-${log.message}`" class="log-row">
        <span class="run-chip">{{ log.level }}</span>
        <span>{{ new Date(log.ts).toLocaleTimeString() }}</span>
        <code>{{ log.message }}</code>
      </div>
    </div>
  </section>
</template>
