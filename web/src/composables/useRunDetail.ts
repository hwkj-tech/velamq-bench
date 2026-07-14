import { computed, onBeforeUnmount, reactive, ref } from 'vue';
import { api } from '@/api/client';
import type { Annotation, LogLine, MetricSnapshot, Run, RuntimeView } from '@/api/types';

export function useRunDetail(runId: string) {
  const run = ref<Run | null>(null);
  const runtime = ref<RuntimeView | null>(null);
  const annotations = ref<Annotation[]>([]);
  const logs = ref<LogLine[]>([]);
  const snapshotsByWorkload = reactive<Record<string, MetricSnapshot[]>>({});
  const loading = ref(false);
  const error = ref('');
  let source: EventSource | null = null;

  const snapshots = computed(() =>
    Object.values(snapshotsByWorkload)
      .flat()
      .sort((a, b) => a.elapsed_ms - b.elapsed_ms),
  );
  const latest = computed(() => snapshots.value.at(-1) ?? null);
  const status = computed(() => runtime.value?.status ?? run.value?.status ?? 'pending');

  async function load() {
    loading.value = true;
    error.value = '';
    try {
      const [runData, snapshotData, annotationData] = await Promise.all([
        api.run(runId),
        api.runSnapshots(runId, 600),
        api.annotations(runId),
      ]);
      run.value = runData;
      annotations.value = annotationData;
      replaceSnapshots(snapshotData);
      if (runData.status === 'running') attach();
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  function replaceSnapshots(snapshotData: MetricSnapshot[]) {
    for (const key of Object.keys(snapshotsByWorkload)) delete snapshotsByWorkload[key];
    for (const snapshot of snapshotData) appendSnapshot(snapshot);
  }

  function appendSnapshot(snapshot: MetricSnapshot) {
    const key = snapshot.run_workload_id ?? 'aggregate';
    const existing = snapshotsByWorkload[key] ?? [];
    if (!existing.some((item) => item.elapsed_ms === snapshot.elapsed_ms)) {
      snapshotsByWorkload[key] = [...existing, snapshot].slice(-1200);
    }
  }

  function attach() {
    detach();
    const sinceMs = snapshots.value.at(-1)?.elapsed_ms ?? 0;
    source = new EventSource(`/api/v2/runs/${runId}/events?since_ms=${sinceMs}`);
    source.addEventListener('run_state', (event) => {
      const data = JSON.parse(event.data) as { run: RuntimeView };
      runtime.value = data.run;
      if (run.value && ['completed', 'failed', 'stopped'].includes(data.run.status)) {
        run.value = { ...run.value, status: data.run.status as Run['status'], stopped_at: data.run.stopped_at ?? null };
        detach();
      }
    });
    source.addEventListener('workload_metric', (event) => {
      const data = JSON.parse(event.data) as { snapshot: MetricSnapshot };
      appendSnapshot(data.snapshot);
    });
    source.addEventListener('workload_log', (event) => {
      const data = JSON.parse(event.data) as { log: LogLine };
      logs.value = [...logs.value, data.log].slice(-5000);
    });
    source.addEventListener('annotation', (event) => {
      const data = JSON.parse(event.data) as { annotation: Annotation };
      annotations.value = [...annotations.value.filter((item) => item.id !== data.annotation.id), data.annotation];
    });
  }

  function detach() {
    source?.close();
    source = null;
  }

  async function stop() {
    await api.stopRun(runId);
  }

  async function addAnnotation(input: { title: string; detail: string; ts?: string }) {
    const annotation = await api.createAnnotation(runId, {
      id: '',
      run_id: runId,
      run_workload_id: null,
      ts: input.ts ?? new Date().toISOString(),
      category: 'manual',
      title: input.title,
      detail: input.detail,
    });
    annotations.value = [...annotations.value, annotation];
    return annotation;
  }

  onBeforeUnmount(detach);

  return { run, runtime, annotations, logs, snapshotsByWorkload, snapshots, latest, status, loading, error, load, stop, addAnnotation };
}
