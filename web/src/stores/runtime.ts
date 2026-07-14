import { defineStore } from 'pinia';
import { computed, reactive, ref } from 'vue';
import { api } from '@/api/client';
import type { LogLine, MetricSnapshot, RuntimeView } from '@/api/types';

export const useRuntimeStore = defineStore('runtime', () => {
  const activeRunId = ref<string | null>(null);
  const state = ref<RuntimeView | null>(null);
  const lastSnapshotByWorkload = reactive<Record<string, MetricSnapshot>>({});
  const logs = ref<LogLine[]>([]);
  const source = ref<EventSource | null>(null);

  const status = computed(() => state.value?.status ?? 'idle');

  async function load() {
    const summary = await api.runtime();
    activeRunId.value = summary.active_run_id ?? summary.state.run_id ?? null;
    state.value = summary.state;
    if (summary.state.latest?.run_workload_id) {
      lastSnapshotByWorkload[summary.state.latest.run_workload_id] = summary.state.latest;
    }
    logs.value = summary.state.logs ?? [];
  }

  function attach(runId: string) {
    detach();
    activeRunId.value = runId;
    source.value = new EventSource(`/api/v2/runs/${runId}/events`);
    source.value.addEventListener('run_state', (event) => {
      state.value = JSON.parse(event.data).run;
    });
    source.value.addEventListener('workload_metric', (event) => {
      const data = JSON.parse(event.data) as { run_workload_id: string; snapshot: MetricSnapshot };
      lastSnapshotByWorkload[data.run_workload_id] = data.snapshot;
    });
    source.value.addEventListener('workload_log', (event) => {
      const data = JSON.parse(event.data) as { log: LogLine };
      logs.value = [...logs.value.slice(-199), data.log];
    });
  }

  function detach() {
    source.value?.close();
    source.value = null;
  }

  return { activeRunId, state, status, lastSnapshotByWorkload, logs, load, attach, detach };
});
