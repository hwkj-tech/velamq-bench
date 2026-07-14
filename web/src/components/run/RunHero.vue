<script setup lang="ts">
import { FileDown, Flag, Square, Tag, Timer, MessageSquarePlus } from 'lucide-vue-next';
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import type { Run } from '@/api/types';

const { t } = useI18n();
const exportFormat = ref<'svg' | 'pdf' | 'csv'>('pdf');

defineProps<{
  run: Run | null;
  status: string;
  durationMs: number;
  canMarkBaseline?: boolean;
  isBaseline?: boolean;
}>();

defineEmits<{
  stop: [];
  addAnnotation: [];
  markBaseline: [];
  exportChart: [format: 'svg' | 'pdf' | 'csv'];
}>();
</script>

<template>
  <section class="panel run-hero">
    <div>
      <span class="status-chip" :data-status="status">{{ t(`status.${status}`) }}</span>
      <h1>{{ run?.name ?? t('runDetail.title') }}</h1>
      <p>{{ run?.description || run?.id }}</p>
    </div>
    <div class="run-hero-meta">
      <span><Timer :size="15" /> {{ Math.round(durationMs / 1000) }}s</span>
      <span><Tag :size="15" /> {{ t('runs.workloadCount', { count: run?.workloads.length ?? 0 }) }}</span>
    </div>
    <div class="topbar-actions">
      <button class="secondary-action" type="button" @click="$emit('addAnnotation')">
        <MessageSquarePlus :size="15" />
        {{ t('runDetail.addNote') }}
      </button>
      <button v-if="canMarkBaseline" class="secondary-action" type="button" @click="$emit('markBaseline')">
        <Flag :size="15" />
        {{ isBaseline ? t('runDetail.baseline') : t('runDetail.markBaseline') }}
      </button>
      <select v-model="exportFormat" class="control export-format" :aria-label="t('runDetail.exportFormat')">
        <option value="pdf">{{ t('runDetail.exportPdf') }}</option>
        <option value="svg">{{ t('runDetail.exportSvg') }}</option>
        <option value="csv">{{ t('runDetail.exportCsv') }}</option>
      </select>
      <button class="secondary-action" type="button" :disabled="!run" @click="$emit('exportChart', exportFormat)">
        <FileDown :size="15" />
        {{ t('runDetail.export') }}
      </button>
      <button v-if="status === 'running'" class="secondary-action danger" type="button" @click="$emit('stop')">
        <Square :size="15" />
        {{ t('runDetail.stop') }}
      </button>
    </div>
  </section>
</template>
