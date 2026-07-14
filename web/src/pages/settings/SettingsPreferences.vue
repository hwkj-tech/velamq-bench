<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useUIStore, type ThemeChoice } from '@/stores/ui';

const ui = useUIStore();
const { locale, t } = useI18n();
const p95Threshold = ref(Number(localStorage.getItem('velamq.threshold.p95') ?? 10));
const p99Threshold = ref(Number(localStorage.getItem('velamq.threshold.p99') ?? 10));
const rateThreshold = ref(Number(localStorage.getItem('velamq.threshold.rate') ?? 10));
const errorThreshold = ref(Number(localStorage.getItem('velamq.threshold.error') ?? 0.05));
const themeChoice = computed({
  get: () => ui.theme,
  set: (value: string) => {
    ui.setTheme(value as ThemeChoice);
  },
});

function save() {
  localStorage.setItem('velamq.lang', locale.value);
  localStorage.setItem('velamq.threshold.p95', String(p95Threshold.value));
  localStorage.setItem('velamq.threshold.p99', String(p99Threshold.value));
  localStorage.setItem('velamq.threshold.rate', String(rateThreshold.value));
  localStorage.setItem('velamq.threshold.error', String(errorThreshold.value));
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('settings.preferences.title') }}</h1>
        <p>{{ t('settings.preferences.subtitle') }}</p>
      </div>
      <button class="primary-action" type="button" @click="save">{{ t('common.save') }}</button>
    </div>

    <section class="panel profile-editor">
      <div class="sheet-grid">
        <label>
          <span>{{ t('settings.preferences.language') }}</span>
          <select v-model="locale" class="control">
            <option value="en">{{ t('settings.preferences.english') }}</option>
            <option value="zh-CN">{{ t('settings.preferences.chinese') }}</option>
          </select>
        </label>
        <label>
          <span>{{ t('settings.preferences.theme') }}</span>
          <select v-model="themeChoice" class="control">
            <option value="light">{{ t('settings.preferences.light') }}</option>
            <option value="dark">{{ t('settings.preferences.dark') }}</option>
            <option value="system">{{ t('settings.preferences.system') }}</option>
          </select>
        </label>
      </div>
    </section>

    <section class="panel profile-editor">
      <div class="panel-head"><h2>{{ t('settings.preferences.thresholds') }}</h2></div>
      <div class="sheet-grid">
        <label><span>{{ t('settings.preferences.p95') }}</span><input v-model.number="p95Threshold" class="control" type="number" min="1" /></label>
        <label><span>{{ t('settings.preferences.p99') }}</span><input v-model.number="p99Threshold" class="control" type="number" min="1" /></label>
        <label><span>{{ t('settings.preferences.rate') }}</span><input v-model.number="rateThreshold" class="control" type="number" min="1" /></label>
        <label><span>{{ t('settings.preferences.error') }}</span><input v-model.number="errorThreshold" class="control" type="number" min="0" step="0.01" /></label>
      </div>
    </section>
  </section>
</template>
