<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import type { Run } from '@/api/types';

defineProps<{ run: Run | null }>();
const { t } = useI18n();

function parseConfig(json: string) {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}
</script>

<template>
  <section class="panel run-tab-panel">
    <div class="panel-head"><h2>{{ t('runDetail.frozenConfig') }}</h2></div>
    <div class="config-list">
      <details v-for="workload in run?.workloads ?? []" :key="workload.id" open>
        <summary>{{ workload.kind }} · {{ workload.workload_id }}</summary>
        <pre>{{ parseConfig(workload.config_snapshot_json) }}</pre>
      </details>
    </div>
  </section>
</template>
