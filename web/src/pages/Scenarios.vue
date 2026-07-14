<script setup lang="ts">
import { onMounted } from 'vue';
import { Edit, Play, Plus, Trash2 } from 'lucide-vue-next';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useScenariosStore } from '@/stores/scenarios';
import { api } from '@/api/client';
import { useToast } from '@/composables/useToast';
import AppEmpty from '@/components/feedback/AppEmpty.vue';

const scenarios = useScenariosStore();
const router = useRouter();
const { t } = useI18n();
const toast = useToast();
onMounted(() => scenarios.load());

async function runScenario(id: string) {
  const response = await api.runScenario(id);
  await router.push(`/runs/${response.run_id}`);
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
    <section class="panel">
      <div class="scenario-grid">
        <article v-for="scenario in scenarios.list" :key="scenario.id" class="scenario-card">
          <RouterLink class="scenario-card-main" :to="`/scenarios/${scenario.id}`">
            <strong>{{ scenario.name }}</strong>
            <span>{{ scenario.description || t('scenarios.noDescription') }}</span>
            <small>{{ scenario.tags.join(', ') || t('scenarios.untagged') }}</small>
          </RouterLink>
          <div class="scenario-actions">
            <button class="secondary-action" type="button" @click="runScenario(scenario.id)">
              <Play :size="15" />
              {{ t('common.run') }}
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
        <AppEmpty v-if="scenarios.list.length === 0" :title="t('scenarios.empty')" compact />
      </div>
    </section>
  </section>
</template>
