<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterView, useRoute, useRouter } from 'vue-router';
import {
  PhCheckCircle,
  PhGauge,
  PhGearSix,
  PhMoon,
  PhPauseCircle,
  PhPlayCircle,
  PhPower,
  PhPulse,
  PhStack,
  PhSun,
  PhTable,
  PhTranslate,
  PhWarningCircle,
  PhX,
} from '@phosphor-icons/vue';
import { api } from '@/api/client';
import { useRuntimeStore } from '@/stores/runtime';
import { useScenariosStore } from '@/stores/scenarios';
import { useUIStore } from '@/stores/ui';
import AppError from '@/components/feedback/AppError.vue';
import AppToast from '@/components/feedback/AppToast.vue';
import type { BrokerProtocol, Scenario } from '@/api/types';
import { useToast } from '@/composables/useToast';

const runtime = useRuntimeStore();
const scenarios = useScenariosStore();
const ui = useUIStore();
const router = useRouter();
const route = useRoute();
const { t, locale } = useI18n();
const toast = useToast();
const quickOpen = ref(false);
const quickHost = ref('127.0.0.1');
const quickPort = ref(1883);
const quickProtocol = ref<BrokerProtocol>('mqtt');
const quickWebsocketPath = ref('/mqtt');
const quickClients = ref(100);
const quickMode = ref<'pub' | 'sub' | 'conn'>('pub');
const selectedScenario = ref('');
const quickBusy = ref(false);
const quickError = ref('');
const protocolOptions: BrokerProtocol[] = ['mqtt', 'mqtts', 'ws', 'wss'];

const navItems = computed(() => [
  { to: '/dashboard', label: t('nav.dashboard'), icon: PhGauge },
  { to: '/runs', label: t('nav.runs'), icon: PhPulse },
  { to: '/scenarios', label: t('nav.scenarios'), icon: PhStack },
  { to: '/templates', label: t('nav.templates'), icon: PhTable },
  { to: '/settings/brokers', label: t('nav.settings'), icon: PhGearSix },
]);

const activeNav = computed(() => {
  const currentPath = route.path;
  if (currentPath.startsWith('/settings')) {
    return '/settings/brokers';
  }
  const active = navItems.value
    .filter((item) => item.to !== '/dashboard')
    .find((item) => currentPath.startsWith(item.to));
  return active?.to ?? '/dashboard';
});

const runtimeIcon = computed(() => {
  if (runtime.status === 'running') {
    return PhPulse;
  }
  if (runtime.status === 'failed') {
    return PhWarningCircle;
  }
  if (runtime.status === 'stopped') {
    return PhPauseCircle;
  }
  if (runtime.status === 'completed') {
    return PhCheckCircle;
  }
  return PhPower;
});

onMounted(async () => {
  await Promise.allSettled([runtime.load(), scenarios.load()]);
  if (runtime.activeRunId) {
    runtime.attach(runtime.activeRunId);
  }
});

function closeQuickSheet() {
  if (!quickBusy.value) {
    quickOpen.value = false;
  }
}

function setLanguage(value: string) {
  locale.value = value;
  localStorage.setItem('velamq.lang', value);
}

function navigateNav(path: string) {
  if (path !== route.path) {
    void router.push(path);
  }
}

function defaultPort(protocol: BrokerProtocol) {
  return { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 }[protocol];
}

function isWebSocket(protocol: BrokerProtocol) {
  return protocol === 'ws' || protocol === 'wss';
}

function applyQuickProtocol(protocol: BrokerProtocol) {
  quickProtocol.value = protocol;
  quickPort.value = defaultPort(protocol);
  if (!isWebSocket(protocol)) {
    quickWebsocketPath.value = '/mqtt';
  }
}

async function startQuickBench() {
  quickBusy.value = true;
  quickError.value = '';
  try {
    let runId: string;
    if (selectedScenario.value) {
      runId = (await api.runScenario(selectedScenario.value)).run_id;
    } else {
      const broker = await api.createBroker({
        id: '',
        name: `${quickProtocol.value}://${quickHost.value}:${quickPort.value}`,
        protocol: quickProtocol.value,
        host: quickHost.value,
        port: quickPort.value,
        websocket_path: isWebSocket(quickProtocol.value) ? quickWebsocketPath.value : null,
        keepalive_secs: 30,
        clean_session: true,
      });
      const scenario = buildAdHocScenario(broker.id);
      runId = (await api.startAdHoc(scenario)).run_id;
    }
    runtime.attach(runId);
    quickOpen.value = false;
    await router.push(`/runs/${runId}`);
  } catch (err) {
    quickError.value = err instanceof Error ? err.message : String(err);
    toast.error(quickError.value);
  } finally {
    quickBusy.value = false;
  }
}

function buildAdHocScenario(brokerProfileId: string): Scenario {
  const now = new Date().toISOString();
  return {
    id: '',
    name: `Quick ${quickMode.value.toUpperCase()} ${quickProtocol.value}://${quickHost.value}:${quickPort.value}`,
    description: '',
    tags: ['quick'],
    baseline_run_id: null,
    created_at: now,
    updated_at: now,
    stages: [
      {
        parallel: {
          workloads: [
            {
              id: '',
              name: quickMode.value,
              kind: quickMode.value,
              broker_profile_id: brokerProfileId,
              payload_profile_id: null,
              clients: quickClients.value,
              start_number: 1,
              client_id_template: 'velamq-{mode}-{i}',
              topics: {
                topic_template: 'velamq/bench/{i}',
                partitions: 1,
                group_strategy: 'client_id',
              },
              qos: 'qos0',
              retain: false,
              load: {
                connect_shape: { shape: 'flat', rate: 100 },
                message_shape: { shape: 'flat', rate: 1 },
                total_duration_ms: 60000,
              },
              network_bind_mode: 'system',
              bind_interfaces: [],
              sample_interval_ms: 1000,
            },
          ],
        },
      },
    ],
  };
}
</script>

<template>
  <div class="app-shell" :data-theme="ui.resolvedTheme">
    <header class="topbar">
      <div class="brand-lockup">
        <div class="brand-mark" aria-hidden="true">
          <svg viewBox="0 0 48 48" role="img">
            <defs>
              <linearGradient id="fluxLogoGradient" x1="8" x2="38" y1="8" y2="40" gradientUnits="userSpaceOnUse">
                <stop stop-color="#37c8ff" />
                <stop offset="0.58" stop-color="#1479ff" />
                <stop offset="1" stop-color="#16f2c2" />
              </linearGradient>
            </defs>
            <path class="logo-mark" d="M10 8h27.5l-2.6 7.2H18.2l-1.7 5.3h15.8l-2.5 7H14.2L10.4 40H2.8L10 8Z" />
            <path class="logo-flow" d="M15.5 34.5c7.4-5.7 16-5.7 25.8 0" />
            <path class="logo-pulse" d="M11.5 25h6.2l2.8-7.8 5.1 16.3 4.3-11.2h6.6" />
            <circle class="logo-dot" cx="39" cy="12" r="3.2" />
          </svg>
        </div>
        <div>
          <strong>VelaMQ Bench</strong>
          <span>{{ t('app.subtitle') }}</span>
        </div>
      </div>
      <div class="topbar-actions">
        <el-button class="quick-bench-button" type="primary" size="large" @click="quickOpen = true">
          <span class="toolbar-button-icon">
            <PhPlayCircle :size="18" weight="duotone" />
          </span>
          {{ t('quick.title') }}
        </el-button>
        <span class="runtime-tag" :data-status="runtime.status">
          <component :is="runtimeIcon" :size="15" weight="duotone" />
          <span>{{ t(`status.${runtime.status}`) }}</span>
        </span>
        <el-tag v-if="runtime.activeRunId" class="run-id-tag" effect="plain" size="large" round>{{ runtime.activeRunId }}</el-tag>
        <div class="language-control">
          <PhTranslate class="language-icon" :size="17" weight="duotone" />
          <el-select
            class="language-select"
            :aria-label="t('a11y.language')"
            :model-value="locale"
            size="large"
            @change="setLanguage"
          >
            <el-option label="English" value="en" />
            <el-option label="简体中文" value="zh-CN" />
          </el-select>
        </div>
        <el-tooltip :content="t('app.theme')" placement="bottom">
          <el-button class="theme-button toolbar-square-button" circle size="large" :aria-label="t('app.theme')" @click="ui.toggleTheme">
            <PhMoon v-if="!ui.isDark" :size="19" weight="duotone" />
            <PhSun v-else :size="19" weight="duotone" />
          </el-button>
        </el-tooltip>
      </div>
    </header>

    <aside class="sidebar">
      <nav :aria-label="t('a11y.primaryNavigation')">
        <el-menu class="app-menu" :default-active="activeNav" @select="navigateNav">
          <el-menu-item v-for="item in navItems" :key="item.to" :index="item.to">
            <component :is="item.icon" :size="19" weight="duotone" />
            <span>{{ item.label }}</span>
          </el-menu-item>
        </el-menu>
      </nav>
    </aside>

    <main class="workspace">
      <RouterView />
    </main>

    <section v-if="quickOpen" class="sheet-backdrop" @click.self="closeQuickSheet" @keydown.esc="closeQuickSheet">
      <form class="quick-sheet" @submit.prevent="startQuickBench">
        <div class="sheet-head">
          <div>
            <h2>{{ t('quick.title') }}</h2>
            <span>{{ selectedScenario ? t('quick.scenario') : t('quick.adHoc') }}</span>
          </div>
          <button class="icon-button" type="button" :aria-label="t('common.close')" @click="closeQuickSheet">
            <PhX :size="16" weight="bold" />
          </button>
        </div>
        <label>
          <span>{{ t('quick.scenario') }}</span>
          <select v-model="selectedScenario" class="control">
            <option value="">{{ t('quick.adHoc') }}</option>
            <option v-for="scenario in scenarios.list" :key="scenario.id" :value="scenario.id">{{ scenario.name }}</option>
          </select>
        </label>
        <div v-if="!selectedScenario" class="sheet-grid">
          <label>
            <span>{{ t('fields.protocol') }}</span>
            <select class="control" :value="quickProtocol" @change="applyQuickProtocol(($event.target as HTMLSelectElement).value as BrokerProtocol)">
              <option v-for="protocol in protocolOptions" :key="protocol" :value="protocol">{{ t(`protocol.${protocol}`) }}</option>
            </select>
          </label>
          <label>
            <span>{{ t('quick.host') }}</span>
            <input v-model="quickHost" class="control" />
          </label>
          <label>
            <span>{{ t('quick.port') }}</span>
            <input v-model.number="quickPort" class="control" type="number" min="1" max="65535" />
          </label>
          <label v-if="isWebSocket(quickProtocol)">
            <span>{{ t('fields.websocketPath') }}</span>
            <input v-model="quickWebsocketPath" class="control" placeholder="/mqtt" />
          </label>
          <div class="mode-field">
            <span>{{ t('quick.mode') }}</span>
            <div class="quick-mode-picker">
              <button type="button" :class="{ active: quickMode === 'pub' }" :aria-pressed="quickMode === 'pub'" @click="quickMode = 'pub'">
                {{ t('builder.modes.pub') }}
              </button>
              <button type="button" :class="{ active: quickMode === 'sub' }" :aria-pressed="quickMode === 'sub'" @click="quickMode = 'sub'">
                {{ t('builder.modes.sub') }}
              </button>
              <button type="button" :class="{ active: quickMode === 'conn' }" :aria-pressed="quickMode === 'conn'" @click="quickMode = 'conn'">
                {{ t('builder.modes.conn') }}
              </button>
            </div>
          </div>
          <label>
            <span>{{ t('quick.clients') }}</span>
            <input v-model.number="quickClients" class="control" type="number" min="1" />
          </label>
        </div>
        <AppError :message="quickError" />
        <button class="primary-action full" type="submit" :disabled="quickBusy" :aria-busy="quickBusy">
          <span v-if="quickBusy" class="action-spinner" aria-hidden="true" />
          <PhPlayCircle :size="17" weight="duotone" />
          {{ quickBusy ? t('quick.starting') : t('quick.start') }}
        </button>
      </form>
    </section>

    <AppToast />
  </div>
</template>
