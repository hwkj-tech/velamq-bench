<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { RouterLink, RouterView } from 'vue-router';

const { t } = useI18n();
</script>

<template>
  <section class="settings-layout">
    <nav class="settings-tabs">
      <RouterLink to="/settings/brokers">{{ t('settings.tabs.brokers') }}</RouterLink>
      <RouterLink to="/settings/payloads">{{ t('settings.tabs.payloads') }}</RouterLink>
      <RouterLink to="/settings/network">{{ t('settings.tabs.network') }}</RouterLink>
      <RouterLink to="/settings/preferences">{{ t('settings.tabs.preferences') }}</RouterLink>
      <RouterLink to="/settings/import">{{ t('settings.tabs.import') }}</RouterLink>
    </nav>
    <RouterView />
  </section>
</template>
