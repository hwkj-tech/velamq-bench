<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { apiGet } from '@/api/client';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';

interface NetworkInterface {
  name: string;
  addr?: string;
  ip?: string;
  is_loopback?: boolean;
}

const interfaces = ref<NetworkInterface[]>([]);
const { t } = useI18n();
const loading = ref(false);
const error = ref('');
const defaultMode = ref(localStorage.getItem('velamq.defaultBindMode') ?? 'system');

onMounted(load);

async function load() {
  loading.value = true;
  error.value = '';
  try {
    interfaces.value = await apiGet<NetworkInterface[]>('/api/v2/network-interfaces');
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

function saveMode() {
  localStorage.setItem('velamq.defaultBindMode', defaultMode.value);
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('settings.network.title') }}</h1>
        <p>{{ t('settings.network.subtitle') }}</p>
      </div>
      <button class="secondary-action" type="button" :disabled="loading" @click="load">
        <RefreshCw :size="15" />
        {{ t('common.refresh') }}
      </button>
    </div>

    <AppError :message="error" />

    <section class="panel profile-editor">
      <label>
        <span>{{ t('settings.network.defaultMode') }}</span>
        <select v-model="defaultMode" class="control" @change="saveMode">
          <option value="system">{{ t('bind.system') }}</option>
          <option value="auto_random">{{ t('bind.autoRandom') }}</option>
          <option value="auto_round_robin">{{ t('bind.autoRoundRobin') }}</option>
          <option value="manual_random">{{ t('bind.manualRandom') }}</option>
          <option value="manual_round_robin">{{ t('bind.manualRoundRobin') }}</option>
        </select>
      </label>
    </section>

    <section class="panel">
      <div class="profile-table">
        <div v-for="nic in interfaces" :key="`${nic.name}-${nic.addr ?? nic.ip ?? ''}`" class="profile-row">
          <strong>{{ nic.name }}</strong>
          <span>{{ nic.addr ?? nic.ip ?? t('common.unknown') }}</span>
          <span class="status-chip" :data-status="nic.is_loopback ? 'failed' : 'ok'">{{ nic.is_loopback ? t('settings.network.loopback') : t('settings.network.usable') }}</span>
        </div>
        <AppLoading v-if="loading && interfaces.length === 0" :label="t('settings.network.loading')" compact />
        <AppEmpty v-else-if="interfaces.length === 0" :title="t('settings.network.empty')" compact />
      </div>
    </section>
  </section>
</template>
