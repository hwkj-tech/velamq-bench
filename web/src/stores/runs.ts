import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api/client';
import type { Run } from '@/api/types';

export const useRunsStore = defineStore('runs', () => {
  const list = ref<Run[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load(limit = 50) {
    loading.value = true;
    error.value = null;
    try {
      list.value = await api.runs(limit);
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  async function loadOne(id: string) {
    const current = list.value.find((run) => run.id === id);
    if (current) return current;
    const run = await api.run(id);
    list.value = [run, ...list.value.filter((item) => item.id !== id)];
    return run;
  }

  async function remove(id: string) {
    await api.deleteRun(id);
    list.value = list.value.filter((run) => run.id !== id);
  }

  return { list, loading, error, load, loadOne, remove };
});
