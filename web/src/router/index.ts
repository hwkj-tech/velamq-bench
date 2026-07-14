import { createRouter, createWebHistory } from 'vue-router';
import Dashboard from '@/pages/Dashboard.vue';
import Runs from '@/pages/Runs.vue';
import Scenarios from '@/pages/Scenarios.vue';
import ScenarioDetail from '@/pages/ScenarioDetail.vue';
import ScenarioBuilder from '@/pages/ScenarioBuilder.vue';
import Templates from '@/pages/Templates.vue';
import SettingsLayout from '@/pages/settings/SettingsLayout.vue';
import SettingsBrokers from '@/pages/settings/SettingsBrokers.vue';
import SettingsPayloads from '@/pages/settings/SettingsPayloads.vue';
import SettingsNetwork from '@/pages/settings/SettingsNetwork.vue';
import SettingsPreferences from '@/pages/settings/SettingsPreferences.vue';
import SettingsImport from '@/pages/settings/SettingsImport.vue';

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/dashboard' },
    { path: '/dashboard', component: Dashboard },
    { path: '/runs', component: Runs },
    { path: '/runs/:id/:tab?', component: () => import('@/pages/RunDetail.vue'), props: true },
    { path: '/compare', redirect: (to) => ({ path: '/runs', query: to.query }) },
    { path: '/scenarios', component: Scenarios },
    { path: '/scenarios/new', component: ScenarioBuilder },
    { path: '/scenarios/:id', component: ScenarioDetail, props: true },
    { path: '/scenarios/:id/edit', component: ScenarioBuilder, props: true },
    { path: '/scenarios/:id/run', component: ScenarioBuilder, props: true },
    { path: '/templates', component: Templates },
    {
      path: '/settings',
      component: SettingsLayout,
      redirect: '/settings/brokers',
      children: [
        { path: 'brokers', component: SettingsBrokers },
        { path: 'payloads', component: SettingsPayloads },
        { path: 'network', component: SettingsNetwork },
        { path: 'preferences', component: SettingsPreferences },
        { path: 'import', component: SettingsImport },
      ],
    },
  ],
});
