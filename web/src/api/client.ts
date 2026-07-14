import type {
  Annotation,
  BenchTemplate,
  BrokerConnectionTest,
  BrokerProfile,
  MetricSnapshot,
  PayloadProfile,
  Run,
  RuntimeSummary,
  Scenario,
  StartResponse,
} from './types';

const JSON_HEADERS = { 'Content-Type': 'application/json' };

export async function apiGet<T>(path: string): Promise<T> {
  return request<T>(path);
}

export async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const init: RequestInit = {
    method: 'POST',
    headers: JSON_HEADERS,
  };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }
  return request<T>(path, init);
}

export async function apiPatch<T>(path: string, body?: unknown): Promise<T> {
  const init: RequestInit = {
    method: 'PATCH',
    headers: JSON_HEADERS,
  };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }
  return request<T>(path, init);
}

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, init);
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const data = (await response.json()) as { error?: string };
      message = data.error ?? message;
    } catch {
      // Keep status text.
    }
    throw new Error(message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export async function requestBlob(path: string, init?: RequestInit, expectedContentType?: string): Promise<Blob> {
  const response = await fetch(path, init);
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const data = (await response.json()) as { error?: string };
      message = data.error ?? message;
    } catch {
      // Keep status text.
    }
    throw new Error(message);
  }
  if (expectedContentType) {
    const contentType = response.headers.get('content-type') ?? '';
    if (!contentType.toLowerCase().includes(expectedContentType.toLowerCase())) {
      let message = `unexpected export content type: ${contentType || 'unknown'}`;
      try {
        const text = await response.text();
        if (text.trim().startsWith('<!doctype html') || text.trim().startsWith('<html')) {
          message = 'export endpoint returned the application shell instead of the chart file';
        }
      } catch {
        // Keep the content-type message.
      }
      throw new Error(message);
    }
  }
  return response.blob();
}

export async function requestUpload<T>(path: string, file: File, init?: RequestInit): Promise<T> {
  return request<T>(path, {
    method: 'POST',
    body: file,
    headers: { 'Content-Type': file.type || (file.name.endsWith('.zip') ? 'application/zip' : 'application/json') },
    ...init,
  });
}

export interface BundleImportCounts {
  scenarios: number;
  broker_profiles: number;
  payload_profiles: number;
  runs: number;
  snapshots: number;
  annotations: number;
}

export const api = {
  runtime: () => apiGet<RuntimeSummary>('/api/v2/runtime/state'),
  templates: (limit = 80) => apiGet<BenchTemplate[]>(`/api/v2/templates?limit=${limit}`),
  runs: (limit = 50) => apiGet<Run[]>(`/api/v2/runs?limit=${limit}`),
  run: (id: string) => apiGet<Run>(`/api/v2/runs/${id}`),
  runSnapshots: (id: string, limit = 600, sinceMs?: number) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (sinceMs !== undefined) params.set('since_ms', String(sinceMs));
    return apiGet<MetricSnapshot[]>(`/api/v2/runs/${id}/snapshots?${params.toString()}`);
  },
  annotations: (id: string) => apiGet<Annotation[]>(`/api/v2/runs/${id}/annotations`),
  createAnnotation: (id: string, annotation: Partial<Annotation>) =>
    apiPost<Annotation>(`/api/v2/runs/${id}/annotations`, annotation),
  scenarios: () => apiGet<Scenario[]>('/api/v2/scenarios'),
  scenario: (id: string) => apiGet<Scenario>(`/api/v2/scenarios/${id}`),
  createScenario: (scenario: Scenario) => apiPost<Scenario>('/api/v2/scenarios', scenario),
  updateScenario: (scenario: Scenario) => apiPatch<Scenario>(`/api/v2/scenarios/${scenario.id}`, scenario),
  deleteScenario: (id: string) => request<void>(`/api/v2/scenarios/${id}`, { method: 'DELETE' }),
  setScenarioBaseline: (scenarioId: string, runId: string | null) =>
    apiPost<Scenario>(`/api/v2/scenarios/${scenarioId}/baseline`, { run_id: runId }),
  brokers: () => apiGet<BrokerProfile[]>('/api/v2/broker-profiles'),
  payloads: () => apiGet<PayloadProfile[]>('/api/v2/payload-profiles'),
  createBroker: (profile: Partial<BrokerProfile>) => apiPost<BrokerProfile>('/api/v2/broker-profiles', profile),
  updateBroker: (profile: Partial<BrokerProfile> & { id: string }) =>
    apiPatch<BrokerProfile>(`/api/v2/broker-profiles/${profile.id}`, profile),
  deleteBroker: (id: string) => request<void>(`/api/v2/broker-profiles/${id}`, { method: 'DELETE' }),
  testBroker: (id: string) => apiPost<BrokerConnectionTest>(`/api/v2/broker-profiles/${id}/test-connection`),
  createPayload: (profile: Partial<PayloadProfile>) => apiPost<PayloadProfile>('/api/v2/payload-profiles', profile),
  updatePayload: (profile: Partial<PayloadProfile> & { id: string }) =>
    apiPatch<PayloadProfile>(`/api/v2/payload-profiles/${profile.id}`, profile),
  deletePayload: (id: string) => request<void>(`/api/v2/payload-profiles/${id}`, { method: 'DELETE' }),
  runScenario: (id: string) => apiPost<StartResponse>(`/api/v2/scenarios/${id}/run`),
  startAdHoc: (scenario: Scenario) => apiPost<StartResponse>('/api/v2/runs', scenario),
  stopRun: (id: string) => apiPost<RuntimeSummary['state']>(`/api/v2/runs/${id}/stop`),
  deleteRun: (id: string) => request<void>(`/api/v2/runs/${id}`, { method: 'DELETE' }),
  exportRunChart: (id: string, lang: string) =>
    requestBlob(`/api/v2/runs/${id}/report.svg?lang=${encodeURIComponent(lang)}`, { cache: 'no-store' }, 'image/svg+xml'),
  exportRunPdf: (id: string, lang: string) =>
    requestBlob(`/api/v2/runs/${id}/report.pdf?lang=${encodeURIComponent(lang)}`, { cache: 'no-store' }, 'application/pdf'),
  exportRunCsv: (id: string) =>
    requestBlob(`/api/v2/runs/${id}/report.csv`, { cache: 'no-store' }, 'text/csv'),
  exportBundle: (runIds: string[]) =>
    requestBlob('/api/v2/bundles/export', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify({ run_ids: runIds, include_snapshots: true, format: 'zip' }),
    }),
  importBundle: (file: File, conflict: 'skip' | 'rename' | 'overwrite') =>
    requestUpload<BundleImportCounts>(`/api/v2/bundles/import?conflict=${conflict}`, file),
};
