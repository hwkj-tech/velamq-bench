<script setup lang="ts">
import { reactive } from 'vue';
import { useI18n } from 'vue-i18n';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import type { Annotation, Run } from '@/api/types';

defineProps<{ run: Run | null; annotations: Annotation[] }>();
const emit = defineEmits<{ add: [{ title: string; detail: string }] }>();
const { t } = useI18n();

const form = reactive({ title: '', detail: '' });

function submit() {
  if (!form.title.trim()) return;
  emit('add', { title: form.title, detail: form.detail });
  form.title = '';
  form.detail = '';
}
</script>

<template>
  <section class="run-tab-grid">
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.runNotes') }}</h2></div>
      <div class="builder-section">
        <label>
          <span>{{ t('fields.title') }}</span>
          <input v-model="form.title" class="control" />
        </label>
        <label>
          <span>{{ t('fields.detail') }}</span>
          <input v-model="form.detail" class="control" />
        </label>
        <button class="primary-action fit" type="button" @click="submit">{{ t('runDetail.addAnnotation') }}</button>
      </div>
    </section>
    <section class="panel">
      <div class="panel-head"><h2>{{ t('runDetail.timeline') }}</h2></div>
      <div class="annotation-list">
        <article v-for="annotation in annotations" :key="annotation.id" class="annotation-row" :data-category="annotation.category">
          <strong>{{ annotation.title }}</strong>
          <span>{{ annotation.category }} · {{ new Date(annotation.ts).toLocaleString() }}</span>
          <p>{{ annotation.detail }}</p>
        </article>
        <AppEmpty v-if="annotations.length === 0" :title="t('runDetail.noAnnotations')" compact />
      </div>
    </section>
  </section>
</template>
