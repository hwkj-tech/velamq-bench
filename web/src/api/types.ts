export type RunStatus = 'pending' | 'running' | 'completed' | 'stopped' | 'failed';
export type WorkloadKind = 'pub' | 'sub' | 'conn';
export type QosLevel = 'qos0' | 'qos1' | 'qos2';
export type NetworkBindMode = 'system' | 'auto_random' | 'auto_round_robin' | 'manual_random' | 'manual_round_robin';
export type BrokerProtocol = 'mqtt' | 'mqtts' | 'ws' | 'wss';
export type AgentStatus = 'online' | 'busy' | 'draining' | 'offline' | 'disabled';

export interface AgentCapabilities {
  os: string;
  arch: string;
  version: string;
  cpu_cores: number;
  memory_bytes: number;
  max_clients: number;
  features: string[];
}

export interface AgentNode {
  id: string;
  instance_id: string;
  name: string;
  status: AgentStatus;
  enabled: boolean;
  draining: boolean;
  labels: string[];
  capabilities: AgentCapabilities;
  current_task_id?: string | null;
  last_seen_at: string;
  created_at: string;
  updated_at: string;
}
export type MqttVersion = 'v3_1_1' | 'v5_0';
export type AuthConfig =
  | { kind: 'user_password'; username: string; password: string }
  | { kind: 'client_cert'; cert_pem: string; key_pem: string }
  | { kind: 'none' };

export interface TlsConfig {
  enabled: boolean;
  ca_pem?: string | null;
  client_cert_pem?: string | null;
  client_key_pem?: string | null;
  server_name?: string | null;
  insecure_skip_verify: boolean;
  alpn_protocols: string[];
}

export interface Mqtt5Config {
  session_expiry_interval_secs?: number | null;
  receive_maximum?: number | null;
  maximum_packet_size?: number | null;
  topic_alias_maximum?: number | null;
  request_problem_information: boolean;
}

export interface MetricSnapshot {
  run_id: string;
  run_workload_id?: string | null;
  ts: string;
  elapsed_ms: number;
  connected: number;
  published: number;
  received: number;
  errors: number;
  publish_rate: number;
  receive_rate: number;
  connect_rate: number;
  error_rate: number;
  latency_count: number;
  latency_window_count?: number;
  latency_window_sum_us?: number;
  latency_histogram?: Array<{ upper_bound_us: number; count: number }>;
  latency_avg_ms: number;
  latency_min_ms: number;
  latency_p50_ms: number;
  latency_p90_ms: number;
  latency_p95_ms: number;
  latency_p99_ms: number;
  latency_p999_ms: number;
  latency_max_ms: number;
}

export type AgentTaskStatus = 'queued' | 'leased' | 'running' | 'completed' | 'failed' | 'stopped' | 'expired';
export type SchedulingStrategy = 'selected' | 'even' | 'capacity_weighted';
export type DistributedRunStatus = 'pending' | 'running' | 'completed' | 'partial' | 'failed' | 'stopped';

export interface AgentTask {
  id: string;
  distributed_run_id?: string | null;
  node_id: string;
  attempt: number;
  status: AgentTaskStatus;
  stop_requested: boolean;
  started_at?: string | null;
  finished_at?: string | null;
  error?: string | null;
  spec: { scenario: Scenario };
}

export interface DistributedRun {
  id: string;
  scenario_id: string;
  name: string;
  strategy: SchedulingStrategy;
  node_ids: string[];
  required_labels: string[];
  status: DistributedRunStatus;
  tasks: AgentTask[];
  created_at: string;
  started_at?: string | null;
  stopped_at?: string | null;
}

export interface DistributedMetrics {
  run_id: string;
  summary: MetricSnapshot[];
  nodes: Array<{ task_id: string; node_id: string; snapshots: MetricSnapshot[] }>;
}

export interface RuntimeView {
  status: string;
  run_id?: string | null;
  started_at?: string | null;
  stopped_at?: string | null;
  latest?: MetricSnapshot | null;
  logs: LogLine[];
}

export interface RuntimeSummary {
  active_run_id?: string | null;
  state: RuntimeView;
}

export interface LogLine {
  ts: string;
  level: string;
  message: string;
}

export interface BrokerProfile {
  id: string;
  name: string;
  protocol: BrokerProtocol;
  mqtt_version: MqttVersion;
  host: string;
  port: number;
  websocket_path?: string | null;
  tls?: TlsConfig | null;
  auth?: AuthConfig | null;
  keepalive_secs: number;
  connection_timeout_secs: number;
  clean_session: boolean;
  mqtt5?: Mqtt5Config | null;
  created_at: string;
  updated_at: string;
}

export interface BrokerConnectionTest {
  ok: boolean;
  profile_id: string;
  host: string;
  port: number;
  elapsed_ms: number;
  error?: string | null;
}

export interface PayloadProfile {
  id: string;
  name: string;
  kind: PayloadKind;
  created_at: string;
  updated_at: string;
}

export type PayloadKind =
  | { kind: 'fixed_bytes'; size: number; with_timestamp: boolean }
  | { kind: 'json_template'; template: string; vars: Record<string, string> }
  | { kind: 'csv_replay'; path: string; column: string; loop_when_done: boolean }
  | { kind: 'counter'; width: number };

export interface LoadProfile {
  connect_shape: LoadShape;
  message_shape: LoadShape;
  total_duration_ms: number;
}

export type LoadShape =
  | { shape: 'flat'; rate: number }
  | { shape: 'ramp'; from: number; to: number; duration_ms: number }
  | { shape: 'step'; stages: Array<{ rate: number; duration_ms: number }> }
  | { shape: 'soak'; rate: number; duration_ms: number }
  | { shape: 'spike'; baseline: number; peak: number; peak_duration_ms: number; period_ms: number };

export interface Workload {
  id: string;
  name: string;
  kind: WorkloadKind;
  broker_profile_id: string;
  payload_profile_id?: string | null;
  clients: number;
  start_number: number;
  client_id_template: string;
  topics: {
    topic_template: string;
    partitions: number;
    group_strategy: string;
  };
  qos: QosLevel;
  retain: boolean;
  load: LoadProfile;
  network_bind_mode: string;
  bind_interfaces: string[];
  sample_interval_ms: number;
}

export interface Scenario {
  id: string;
  name: string;
  description: string;
  tags: string[];
  stages: ScenarioStage[];
  baseline_run_id?: string | null;
  created_at: string;
  updated_at: string;
}

export type ScenarioStage =
  | { parallel: { workloads: Workload[] } }
  | { sequential: { workloads: Workload[] } };

export interface RunWorkload {
  id: string;
  run_id: string;
  workload_id: string;
  kind: WorkloadKind;
  config_snapshot_json: string;
}

export interface Run {
  id: string;
  scenario_id?: string | null;
  name: string;
  tags: string[];
  description: string;
  status: RunStatus;
  started_at: string;
  stopped_at?: string | null;
  workloads: RunWorkload[];
  baseline_of_scenario_id?: string | null;
}

export interface Annotation {
  id: string;
  run_id: string;
  run_workload_id?: string | null;
  ts: string;
  category: 'manual' | 'broker_event' | 'sla_breach' | 'config_change' | string;
  title: string;
  detail: string;
}

export interface StartResponse {
  run_id: string;
}

export interface BenchConfig {
  mode: WorkloadKind;
  protocol: BrokerProtocol;
  host: string;
  port: number;
  websocket_path?: string | null;
  clients: number;
  start_number: number;
  connect_rate: number;
  client_interval_ms: number;
  username?: string | null;
  password?: string | null;
  keepalive_secs: number;
  clean_session: boolean;
  client_id_template: string;
  topic: string;
  qos: QosLevel;
  retain: boolean;
  payload_size: number;
  payload_timestamp: boolean;
  message_interval_ms: number;
  duration_secs: number;
  sample_interval_ms: number;
  network_bind_mode: NetworkBindMode;
  bind_interfaces: string[];
}

export interface BenchTemplate {
  id: string;
  name: string;
  description: string;
  tags: string[];
  config: BenchConfig;
  created_at: string;
  updated_at: string;
}
