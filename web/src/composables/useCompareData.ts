import { computed, ref } from 'vue';
import { api } from '@/api/client';
import type { MetricSnapshot, Run } from '@/api/types';

export interface CompareRunData {
  run: Run;
  snapshots: MetricSnapshot[];
}

export function useCompareData() {
  const items = ref<CompareRunData[]>([]);
  const loading = ref(false);
  const error = ref('');

  const stats = computed(() =>
    items.value.map((item) => {
      const latest = item.snapshots.at(-1);
      const avg = average(item.snapshots);
      return {
        run: item.run,
        published: latest?.published ?? 0,
        received: latest?.received ?? 0,
        p95: avg.p95,
        p99: avg.p99,
        errorRate: avg.errorRate,
        publishRate: avg.publishRate,
      };
    }),
  );

  async function load(ids: string[]) {
    if (ids.length === 0) {
      items.value = [];
      return;
    }
    loading.value = true;
    error.value = '';
    try {
      items.value = await Promise.all(
        ids.map(async (id) => ({
          run: await api.run(id),
          snapshots: await api.runSnapshots(id, 1200),
        })),
      );
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  return { items, stats, loading, error, load };
}

function average(snapshots: MetricSnapshot[]) {
  const count = Math.max(snapshots.length, 1);
  return snapshots.reduce(
    (acc, snapshot) => ({
      p95: acc.p95 + snapshot.latency_p95_ms / count,
      p99: acc.p99 + snapshot.latency_p99_ms / count,
      errorRate: acc.errorRate + snapshot.error_rate / count,
      publishRate: acc.publishRate + snapshot.publish_rate / count,
    }),
    { p95: 0, p99: 0, errorRate: 0, publishRate: 0 },
  );
}
