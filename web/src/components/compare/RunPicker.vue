<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import type { Run } from '@/api/types';

const props = defineProps<{ runs: Run[]; selectedIds: string[] }>();
const emit = defineEmits<{ toggle: [id: string] }>();
const { t } = useI18n();

function checked(id: string) {
  return props.selectedIds.includes(id);
}
</script>

<template>
  <section class="panel compare-picker">
    <div class="panel-head"><h2>{{ t('compare.selectRuns') }}</h2></div>
    <div class="compare-run-list">
      <label v-for="run in runs" :key="run.id" class="compare-run-row">
        <input type="checkbox" :checked="checked(run.id)" :disabled="!checked(run.id) && selectedIds.length >= 4" @change="emit('toggle', run.id)" />
        <strong>{{ run.name }}</strong>
        <span class="status-chip" :data-status="run.status">{{ t(`status.${run.status}`) }}</span>
        <span>{{ new Date(run.started_at).toLocaleString() }}</span>
      </label>
    </div>
  </section>
</template>
