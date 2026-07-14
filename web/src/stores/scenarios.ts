import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api/client';
import type { Scenario } from '@/api/types';

export const useScenariosStore = defineStore('scenarios', () => {
  const list = ref<Scenario[]>([]);
  async function load() {
    list.value = await api.scenarios();
  }
  async function save(scenario: Scenario) {
    const saved = scenario.id ? await api.updateScenario(scenario) : await api.createScenario(scenario);
    await load();
    return saved;
  }
  async function remove(id: string) {
    await api.deleteScenario(id);
    list.value = list.value.filter((scenario) => scenario.id !== id);
  }
  return { list, load, save, remove };
});
