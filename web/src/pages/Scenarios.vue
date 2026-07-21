<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Edit, Layers3, Play, Plus, Search, Trash2, Users } from 'lucide-vue-next';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useScenariosStore } from '@/stores/scenarios';
import { api } from '@/api/client';
import { useToast } from '@/composables/useToast';
import AppEmpty from '@/components/feedback/AppEmpty.vue';
import AppError from '@/components/feedback/AppError.vue';
import AppLoading from '@/components/feedback/AppLoading.vue';

const scenarios = useScenariosStore();
const router = useRouter();
const { t } = useI18n();
const toast = useToast();
const query = ref('');
const loading = ref(true);
const error = ref('');
const runningIds = ref(new Set<string>());
const filteredScenarios = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase();
  return scenarios.list.filter((scenario) => {
    const searchable = `${scenario.name} ${scenario.description} ${scenario.tags.join(' ')}`.toLocaleLowerCase();
    return !needle || searchable.includes(needle);
  });
});

onMounted(async () => {
  try {
    await scenarios.load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
});

async function runScenario(id: string) {
  if (runningIds.value.has(id)) return;
  runningIds.value.add(id);
  try {
    const response = await api.runScenario(id);
    await router.push(`/runs/${response.run_id}`);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  } finally {
    runningIds.value.delete(id);
  }
}

function workloads(scenario: (typeof scenarios.list)[number]) {
  return scenario.stages.reduce((sum, stage) => sum + ('parallel' in stage ? stage.parallel.workloads.length : stage.sequential.workloads.length), 0);
}

function clients(scenario: (typeof scenarios.list)[number]) {
  return scenario.stages.reduce(
    (sum, stage) => sum + ('parallel' in stage ? stage.parallel.workloads : stage.sequential.workloads).reduce((count, workload) => count + workload.clients, 0),
    0,
  );
}

async function removeScenario(id: string, name: string) {
  if (!window.confirm(t('scenarios.confirmDelete', { name }))) return;
  try {
    await scenarios.remove(id);
    toast.success(t('scenarios.deleted'));
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}
</script>

<template>
  <section class="page-stack">
    <div class="page-title">
      <div>
        <h1>{{ t('scenarios.title') }}</h1>
        <p>{{ t('scenarios.subtitle') }}</p>
      </div>
      <RouterLink class="primary-action" to="/scenarios/new">
        <Plus :size="16" />
        {{ t('scenarios.new') }}
      </RouterLink>
    </div>
    <AppError :message="error" />
    <section class="panel">
      <div class="list-toolbar">
        <label class="search-control search-control--wide">
          <Search :size="16" aria-hidden="true" />
          <span class="sr-only">{{ t('scenarios.search') }}</span>
          <input v-model="query" type="search" :placeholder="t('scenarios.searchPlaceholder')" />
        </label>
        <span class="result-count">{{ t('scenarios.resultCount', { count: filteredScenarios.length }) }}</span>
        <button v-if="query" class="text-action" type="button" @click="query = ''">{{ t('runs.clearFilters') }}</button>
      </div>
      <AppLoading v-if="loading" :label="t('common.loading')" compact />
      <div class="scenario-grid">
        <article v-for="scenario in filteredScenarios" :key="scenario.id" class="scenario-card">
          <RouterLink class="scenario-card-main" :to="`/scenarios/${scenario.id}`">
            <strong>{{ scenario.name }}</strong>
            <span>{{ scenario.description || t('scenarios.noDescription') }}</span>
            <div class="scenario-card__stats">
              <span><Layers3 :size="14" />{{ t('scenarios.workloadCount', { count: workloads(scenario) }) }}</span>
              <span><Users :size="14" />{{ t('scenarios.clientCount', { count: clients(scenario) }) }}</span>
            </div>
            <div class="scenario-card__tags"><small v-for="tag in scenario.tags" :key="tag">{{ tag }}</small><small v-if="!scenario.tags.length">{{ t('scenarios.untagged') }}</small></div>
          </RouterLink>
          <div class="scenario-actions">
            <button class="primary-action" type="button" :disabled="runningIds.has(scenario.id)" @click="runScenario(scenario.id)">
              <Play :size="15" />
              {{ runningIds.has(scenario.id) ? t('scenarios.starting') : t('common.run') }}
            </button>
            <RouterLink class="secondary-action" :to="`/scenarios/${scenario.id}/edit`">
              <Edit :size="15" />
              {{ t('common.edit') }}
            </RouterLink>
            <button class="secondary-action danger" type="button" @click="removeScenario(scenario.id, scenario.name)">
              <Trash2 :size="15" />
              {{ t('common.delete') }}
            </button>
          </div>
        </article>
        <AppEmpty
          v-if="!loading && filteredScenarios.length === 0"
          :title="query ? t('scenarios.noMatches') : t('scenarios.empty')"
          :hint="query ? t('scenarios.noMatchesHint') : t('scenarios.emptyHint')"
          compact
        >
          <button v-if="query" class="secondary-action" type="button" @click="query = ''">{{ t('runs.clearFilters') }}</button>
        </AppEmpty>
      </div>
    </section>
  </section>
</template>
