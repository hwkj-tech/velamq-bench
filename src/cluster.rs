use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    model::{
        AgentHeartbeat, AgentLogBatch, AgentLogPoint, AgentMetricBatch, AgentNode, AgentNodeUpdate,
        AgentRegistration, AgentRegistrationResponse, AgentTask, AgentTaskComplete,
        AgentTaskCreate, AgentTaskLease, AgentTaskSpec, DistributedMetrics, DistributedRun,
        DistributedRunCreate, LatencyBucket, LoadShape, MetricSnapshot, Scenario, ScenarioStage,
        SchedulingStrategy, normalize_labels, validate_labels,
    },
    storage::Storage,
};

pub const AGENT_PROTOCOL_VERSION: u16 = 1;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 5;

#[derive(Debug)]
pub struct ClusterManager {
    storage: Storage,
    bootstrap_token: Option<String>,
}

impl ClusterManager {
    pub fn new(storage: Storage) -> Arc<Self> {
        Arc::new(Self {
            storage,
            bootstrap_token: std::env::var("VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }

    pub fn registration_enabled(&self) -> bool {
        self.bootstrap_token.is_some()
    }

    pub async fn register(
        &self,
        bearer_token: &str,
        mut registration: AgentRegistration,
    ) -> Result<AgentRegistrationResponse> {
        let expected = self.bootstrap_token.as_deref().ok_or_else(|| {
            anyhow!("agent registration is disabled; configure VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN")
        })?;
        if !constant_time_equal(expected.as_bytes(), bearer_token.as_bytes()) {
            return Err(anyhow!("invalid agent bootstrap token"));
        }
        registration.validate().map_err(|err| anyhow!(err))?;
        registration.instance_id = registration.instance_id.trim().to_string();
        registration.name = registration.name.trim().to_string();
        let token = format!("vma_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let node = self
            .storage
            .register_agent(registration, token_hash(&token))
            .await?;
        Ok(AgentRegistrationResponse {
            node,
            token,
            heartbeat_interval_secs: HEARTBEAT_INTERVAL_SECS,
            protocol_version: AGENT_PROTOCOL_VERSION,
        })
    }

    pub async fn authenticate(&self, node_id: &str, bearer_token: &str) -> Result<()> {
        let expected = self
            .storage
            .agent_token_hash(node_id)
            .await?
            .ok_or_else(|| anyhow!("agent node not found"))?;
        let actual = token_hash(bearer_token);
        if !constant_time_equal(expected.as_bytes(), actual.as_bytes()) {
            return Err(anyhow!("invalid agent token"));
        }
        Ok(())
    }

    pub async fn heartbeat(
        &self,
        node_id: &str,
        heartbeat: AgentHeartbeat,
    ) -> Result<Option<AgentNode>> {
        self.storage
            .heartbeat_agent(
                node_id,
                heartbeat.capabilities,
                heartbeat.current_task_id,
                heartbeat.lease_id,
            )
            .await
    }

    pub async fn nodes(&self) -> Result<Vec<AgentNode>> {
        self.storage.list_agent_nodes().await
    }

    pub async fn node(&self, id: &str) -> Result<Option<AgentNode>> {
        self.storage.get_agent_node(id).await
    }

    pub async fn update_node(
        &self,
        id: &str,
        update: AgentNodeUpdate,
    ) -> Result<Option<AgentNode>> {
        if let Some(name) = &update.name {
            if name.trim().is_empty() || name.len() > 120 {
                return Err(anyhow!(
                    "agent name is required and must be <= 120 characters"
                ));
            }
        }
        if let Some(labels) = &update.labels {
            validate_labels(labels).map_err(|err| anyhow!(err))?;
        }
        self.storage.update_agent_node(id, update).await
    }

    pub async fn delete_node(&self, id: &str) -> Result<bool> {
        self.storage.delete_agent_node(id).await
    }

    pub async fn create_task(&self, request: AgentTaskCreate) -> Result<AgentTask> {
        let node = self
            .storage
            .get_agent_node(&request.node_id)
            .await?
            .ok_or_else(|| anyhow!("agent node not found"))?;
        if !node.enabled || node.draining {
            return Err(anyhow!("agent node is not available for scheduling"));
        }
        if request.spec.scenario.stages.is_empty() {
            return Err(anyhow!(
                "agent task scenario must contain at least one stage"
            ));
        }
        self.storage.create_agent_task(request).await
    }

    pub async fn tasks(&self, node_id: Option<String>) -> Result<Vec<AgentTask>> {
        self.storage.list_agent_tasks(node_id).await
    }

    pub async fn lease_next_task(&self, node_id: &str) -> Result<Option<AgentTaskLease>> {
        let Some(task) = self.storage.lease_next_agent_task(node_id).await? else {
            return Ok(None);
        };
        Ok(Some(AgentTaskLease {
            lease_id: task.lease_id.clone().unwrap_or_default(),
            lease_expires_at: task.lease_expires_at.unwrap_or_else(chrono::Utc::now),
            task,
        }))
    }

    pub async fn ack_task(
        &self,
        task_id: &str,
        node_id: &str,
        lease_id: &str,
    ) -> Result<Option<AgentTask>> {
        self.storage
            .ack_agent_task(task_id, node_id, lease_id)
            .await
    }

    pub async fn complete_task(
        &self,
        task_id: &str,
        node_id: &str,
        complete: AgentTaskComplete,
    ) -> Result<Option<AgentTask>> {
        self.storage
            .complete_agent_task(
                task_id,
                node_id,
                &complete.lease_id,
                complete.status,
                complete.error,
            )
            .await
    }

    pub async fn task(&self, task_id: &str) -> Result<Option<AgentTask>> {
        self.storage.get_agent_task(task_id).await
    }

    pub async fn stop_task(&self, task_id: &str) -> Result<Option<AgentTask>> {
        self.storage.request_agent_task_stop(task_id).await
    }

    pub async fn start_distributed_run(
        &self,
        mut request: DistributedRunCreate,
    ) -> Result<DistributedRun> {
        request.required_labels = normalize_labels(request.required_labels);
        validate_labels(&request.required_labels).map_err(|err| anyhow!(err))?;
        let scenario = self
            .storage
            .get_scenario(&request.scenario_id)
            .await?
            .ok_or_else(|| anyhow!("scenario not found"))?;
        let requested: HashSet<_> = request.node_ids.iter().cloned().collect();
        let mut nodes: Vec<_> = self
            .storage
            .list_agent_nodes()
            .await?
            .into_iter()
            .filter(|node| {
                node.enabled
                    && !node.draining
                    && !matches!(
                        node.status,
                        crate::model::AgentStatus::Offline | crate::model::AgentStatus::Disabled
                    )
                    && (requested.is_empty() || requested.contains(&node.id))
                    && request
                        .required_labels
                        .iter()
                        .all(|label| node.labels.contains(label))
            })
            .collect();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        if nodes.is_empty() {
            return Err(anyhow!(
                "no eligible agent nodes match this distributed run"
            ));
        }
        if request.strategy == SchedulingStrategy::Selected && request.node_ids.is_empty() {
            return Err(anyhow!("selected scheduling requires node_ids"));
        }
        if !requested.is_empty() && nodes.len() != requested.len() {
            return Err(anyhow!(
                "one or more selected nodes are offline, disabled, draining, or label-incompatible"
            ));
        }

        let broker_ids: HashSet<String> = scenario
            .stages
            .iter()
            .flat_map(stage_workloads)
            .map(|workload| workload.broker_profile_id.clone())
            .collect();
        let payload_ids: HashSet<String> = scenario
            .stages
            .iter()
            .flat_map(stage_workloads)
            .filter_map(|workload| workload.payload_profile_id.clone())
            .collect();
        let brokers = self
            .storage
            .list_broker_profiles()
            .await?
            .into_iter()
            .filter(|profile| broker_ids.contains(&profile.id))
            .collect::<Vec<_>>();
        if brokers.len() != broker_ids.len() {
            return Err(anyhow!("scenario references a missing broker profile"));
        }
        let payloads = self
            .storage
            .list_payload_profiles()
            .await?
            .into_iter()
            .filter(|profile| payload_ids.contains(&profile.id))
            .collect::<Vec<_>>();
        if payloads.len() != payload_ids.len() {
            return Err(anyhow!("scenario references a missing payload profile"));
        }

        let run_id = Uuid::new_v4().to_string();
        let node_ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
        self.storage
            .create_distributed_run(
                run_id.clone(),
                scenario.clone(),
                request.strategy.clone(),
                node_ids.clone(),
                request.required_labels,
            )
            .await?;
        for (index, node) in nodes.iter().enumerate() {
            let scenario_slice = slice_scenario(&scenario, &nodes, &request.strategy, index);
            self.storage
                .create_agent_task(AgentTaskCreate {
                    node_id: node.id.clone(),
                    distributed_run_id: Some(run_id.clone()),
                    idempotency_key: Some(format!("{run_id}/{}/1", node.id)),
                    spec: AgentTaskSpec {
                        scenario: scenario_slice,
                        broker_profiles: brokers.clone(),
                        payload_profiles: payloads.clone(),
                    },
                })
                .await?;
        }
        self.storage
            .distributed_run(&run_id)
            .await?
            .ok_or_else(|| anyhow!("distributed run was not found after scheduling"))
    }

    pub async fn distributed_runs(&self) -> Result<Vec<DistributedRun>> {
        self.storage.distributed_runs().await
    }

    pub async fn distributed_run(&self, id: &str) -> Result<Option<DistributedRun>> {
        self.storage.distributed_run(id).await
    }

    pub async fn stop_distributed_run(&self, id: &str) -> Result<Option<DistributedRun>> {
        self.storage.stop_distributed_run(id).await
    }

    pub async fn upload_metrics(
        &self,
        task_id: &str,
        node_id: &str,
        batch: AgentMetricBatch,
    ) -> Result<usize> {
        if batch.points.len() > 500 {
            return Err(anyhow!("metric batch must contain <= 500 points"));
        }
        self.storage
            .insert_agent_metrics(task_id, node_id, &batch.lease_id, batch.points)
            .await
    }

    pub async fn upload_logs(
        &self,
        task_id: &str,
        node_id: &str,
        batch: AgentLogBatch,
    ) -> Result<usize> {
        if batch.points.len() > 1000 {
            return Err(anyhow!("log batch must contain <= 1000 points"));
        }
        self.storage
            .insert_agent_logs(task_id, node_id, &batch.lease_id, batch.points)
            .await
    }

    pub async fn distributed_metrics(&self, run_id: &str) -> Result<DistributedMetrics> {
        let nodes = self.storage.distributed_task_metrics(run_id).await?;
        let summary = aggregate_metrics(run_id, &nodes);
        Ok(DistributedMetrics {
            run_id: run_id.to_string(),
            summary,
            nodes,
        })
    }

    pub async fn task_logs(&self, task_id: &str) -> Result<Vec<AgentLogPoint>> {
        self.storage.agent_task_logs(task_id).await
    }
}

fn stage_workloads(stage: &ScenarioStage) -> &Vec<crate::model::Workload> {
    match stage {
        ScenarioStage::Parallel { workloads } | ScenarioStage::Sequential { workloads } => {
            workloads
        }
    }
}

fn slice_scenario(
    scenario: &Scenario,
    nodes: &[AgentNode],
    strategy: &SchedulingStrategy,
    node_index: usize,
) -> Scenario {
    let mut output = scenario.clone();
    output.id = format!("{}-node-{}", scenario.id, node_index + 1);
    output.name = format!("{} · {}", scenario.name, nodes[node_index].name);
    output.stages = scenario
        .stages
        .iter()
        .map(|stage| {
            let workloads = stage_workloads(stage)
                .iter()
                .filter_map(|workload| {
                    let allocations = allocate_clients(workload.clients, nodes, strategy);
                    let clients = allocations[node_index];
                    if clients == 0 {
                        return None;
                    }
                    let mut slice = workload.clone();
                    slice.clients = clients;
                    slice.start_number =
                        workload.start_number + allocations[..node_index].iter().sum::<u64>();
                    let ratio = clients as f64 / workload.clients.max(1) as f64;
                    slice.load.connect_shape = scale_shape(&workload.load.connect_shape, ratio);
                    slice.load.message_shape = scale_shape(&workload.load.message_shape, ratio);
                    Some(slice)
                })
                .collect();
            match stage {
                ScenarioStage::Parallel { .. } => ScenarioStage::Parallel { workloads },
                ScenarioStage::Sequential { .. } => ScenarioStage::Sequential { workloads },
            }
        })
        .collect();
    output
}

fn allocate_clients(total: u64, nodes: &[AgentNode], strategy: &SchedulingStrategy) -> Vec<u64> {
    if nodes.is_empty() {
        return Vec::new();
    }
    if *strategy != SchedulingStrategy::CapacityWeighted {
        let base = total / nodes.len() as u64;
        let remainder = total % nodes.len() as u64;
        return (0..nodes.len())
            .map(|index| base + u64::from(index < remainder as usize))
            .collect();
    }
    let weights = nodes
        .iter()
        .map(|node| node.capabilities.max_clients.max(1))
        .collect::<Vec<_>>();
    let sum = weights.iter().sum::<u64>();
    let mut shares = weights
        .iter()
        .map(|weight| total.saturating_mul(*weight) / sum)
        .collect::<Vec<_>>();
    let mut remaining = total.saturating_sub(shares.iter().sum());
    let mut remainders = weights
        .iter()
        .enumerate()
        .map(|(index, weight)| (index, total.saturating_mul(*weight) % sum))
        .collect::<Vec<_>>();
    remainders.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    for (index, _) in remainders {
        if remaining == 0 {
            break;
        }
        shares[index] += 1;
        remaining -= 1;
    }
    shares
}

fn scale_shape(shape: &LoadShape, ratio: f64) -> LoadShape {
    match shape {
        LoadShape::Flat { rate } => LoadShape::Flat { rate: rate * ratio },
        LoadShape::Ramp {
            from,
            to,
            duration_ms,
        } => LoadShape::Ramp {
            from: from * ratio,
            to: to * ratio,
            duration_ms: *duration_ms,
        },
        LoadShape::Step { stages } => LoadShape::Step {
            stages: stages
                .iter()
                .map(|stage| crate::model::LoadStage {
                    rate: stage.rate * ratio,
                    duration_ms: stage.duration_ms,
                })
                .collect(),
        },
        LoadShape::Soak { rate, duration_ms } => LoadShape::Soak {
            rate: rate * ratio,
            duration_ms: *duration_ms,
        },
        LoadShape::Spike {
            baseline,
            peak,
            peak_duration_ms,
            period_ms,
        } => LoadShape::Spike {
            baseline: baseline * ratio,
            peak: peak * ratio,
            peak_duration_ms: *peak_duration_ms,
            period_ms: *period_ms,
        },
    }
}

fn aggregate_metrics(
    run_id: &str,
    nodes: &[crate::model::AgentTaskMetrics],
) -> Vec<MetricSnapshot> {
    let mut buckets = BTreeMap::<u64, (MetricSnapshot, BTreeMap<u64, u64>)>::new();
    for node in nodes {
        for source in &node.snapshots {
            let elapsed_ms = source.elapsed_ms / 1000 * 1000;
            let entry = buckets.entry(elapsed_ms).or_insert_with(|| {
                let mut snapshot = source.clone();
                snapshot.run_id = run_id.to_string();
                snapshot.run_workload_id = None;
                snapshot.elapsed_ms = elapsed_ms;
                snapshot.connected = 0;
                snapshot.published = 0;
                snapshot.received = 0;
                snapshot.errors = 0;
                snapshot.publish_rate = 0.0;
                snapshot.receive_rate = 0.0;
                snapshot.connect_rate = 0.0;
                snapshot.error_rate = 0.0;
                snapshot.latency_count = 0;
                snapshot.latency_window_count = 0;
                snapshot.latency_window_sum_us = 0;
                snapshot.latency_histogram.clear();
                snapshot.latency_min_ms = 0.0;
                snapshot.latency_max_ms = 0.0;
                (snapshot, BTreeMap::new())
            });
            let target = &mut entry.0;
            target.ts = target.ts.min(source.ts);
            target.connected = target.connected.saturating_add(source.connected);
            target.published = target.published.saturating_add(source.published);
            target.received = target.received.saturating_add(source.received);
            target.errors = target.errors.saturating_add(source.errors);
            target.publish_rate += source.publish_rate;
            target.receive_rate += source.receive_rate;
            target.connect_rate += source.connect_rate;
            target.error_rate += source.error_rate;
            target.latency_count = target.latency_count.saturating_add(source.latency_count);
            target.latency_window_count = target
                .latency_window_count
                .saturating_add(source.latency_window_count);
            target.latency_window_sum_us = target
                .latency_window_sum_us
                .saturating_add(source.latency_window_sum_us);
            if source.latency_window_count > 0 {
                target.latency_min_ms = if target.latency_min_ms == 0.0 {
                    source.latency_min_ms
                } else {
                    target.latency_min_ms.min(source.latency_min_ms)
                };
                target.latency_max_ms = target.latency_max_ms.max(source.latency_max_ms);
            }
            for bucket in &source.latency_histogram {
                *entry.1.entry(bucket.upper_bound_us).or_default() += bucket.count;
            }
        }
    }
    buckets
        .into_values()
        .map(|(mut snapshot, histogram)| {
            snapshot.latency_avg_ms = if snapshot.latency_window_count == 0 {
                0.0
            } else {
                snapshot.latency_window_sum_us as f64
                    / snapshot.latency_window_count as f64
                    / 1000.0
            };
            snapshot.latency_histogram = histogram
                .iter()
                .map(|(upper_bound_us, count)| LatencyBucket {
                    upper_bound_us: *upper_bound_us,
                    count: *count,
                })
                .collect();
            snapshot.latency_p50_ms = histogram_percentile(&histogram, 0.50);
            snapshot.latency_p90_ms = histogram_percentile(&histogram, 0.90);
            snapshot.latency_p95_ms = histogram_percentile(&histogram, 0.95);
            snapshot.latency_p99_ms = histogram_percentile(&histogram, 0.99);
            snapshot.latency_p999_ms = histogram_percentile(&histogram, 0.999);
            snapshot
        })
        .collect()
}

fn histogram_percentile(histogram: &BTreeMap<u64, u64>, quantile: f64) -> f64 {
    let total = histogram.values().sum::<u64>();
    if total == 0 {
        return 0.0;
    }
    let target = (total as f64 * quantile).ceil() as u64;
    let mut seen = 0;
    for (upper, count) in histogram {
        seen += count;
        if seen >= target {
            return *upper as f64 / 1000.0;
        }
    }
    0.0
}

fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, capacity: u64) -> AgentNode {
        let now = chrono::Utc::now();
        AgentNode {
            id: id.to_string(),
            instance_id: format!("instance-{id}"),
            name: id.to_string(),
            status: crate::model::AgentStatus::Online,
            enabled: true,
            draining: false,
            labels: Vec::new(),
            capabilities: crate::model::AgentCapabilities {
                max_clients: capacity,
                ..Default::default()
            },
            current_task_id: None,
            last_seen_at: now,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn token_hash_is_stable_and_not_plaintext() {
        assert_eq!(token_hash("secret"), token_hash("secret"));
        assert_ne!(token_hash("secret"), "secret");
        assert_ne!(token_hash("secret"), token_hash("other"));
    }

    #[test]
    fn constant_time_comparison_checks_length_and_content() {
        assert!(constant_time_equal(b"abc", b"abc"));
        assert!(!constant_time_equal(b"abc", b"abd"));
        assert!(!constant_time_equal(b"abc", b"ab"));
    }

    #[test]
    fn even_allocation_preserves_total_and_is_deterministic() {
        let nodes = vec![node("a", 1), node("b", 1), node("c", 1)];
        assert_eq!(
            allocate_clients(1001, &nodes, &SchedulingStrategy::Even),
            vec![334, 334, 333]
        );
    }

    #[test]
    fn capacity_allocation_uses_largest_remainder() {
        let nodes = vec![node("a", 1), node("b", 3), node("c", 6)];
        let shares = allocate_clients(1001, &nodes, &SchedulingStrategy::CapacityWeighted);
        assert_eq!(shares.iter().sum::<u64>(), 1001);
        assert_eq!(shares, vec![100, 300, 601]);
    }

    #[test]
    fn scenario_slices_use_non_overlapping_client_ranges() {
        let nodes = vec![node("a", 1), node("b", 1), node("c", 1)];
        let mut scenario = Scenario::default();
        scenario.id = "scenario".to_string();
        scenario.name = "distributed".to_string();
        let mut workload = crate::model::Workload::default();
        workload.clients = 10;
        workload.start_number = 50;
        workload.load.connect_shape = LoadShape::Flat { rate: 100.0 };
        scenario.stages = vec![ScenarioStage::Parallel {
            workloads: vec![workload],
        }];
        let slices = (0..3)
            .map(|index| slice_scenario(&scenario, &nodes, &SchedulingStrategy::Even, index))
            .collect::<Vec<_>>();
        let workloads = slices
            .iter()
            .map(|scenario| &stage_workloads(&scenario.stages[0])[0])
            .collect::<Vec<_>>();
        assert_eq!(
            workloads
                .iter()
                .map(|workload| workload.clients)
                .sum::<u64>(),
            10
        );
        assert_eq!(
            workloads
                .iter()
                .map(|workload| workload.start_number)
                .collect::<Vec<_>>(),
            vec![50, 54, 57]
        );
        let rates = workloads
            .iter()
            .map(|workload| workload.load.connect_shape.instant_rate(0))
            .sum::<f64>();
        assert!((rates - 100.0).abs() < 0.0001);
    }

    #[test]
    fn aggregation_sums_counters_and_merges_latency_histograms() {
        fn snapshot(run_id: &str, connected: u64, histogram: Vec<LatencyBucket>) -> MetricSnapshot {
            let latency_window_count = histogram.iter().map(|bucket| bucket.count).sum();
            MetricSnapshot {
                run_id: run_id.to_string(),
                run_workload_id: Some("workload".to_string()),
                ts: chrono::Utc::now(),
                elapsed_ms: 1_500,
                connected,
                published: connected * 10,
                received: connected * 8,
                errors: connected,
                publish_rate: connected as f64 * 2.0,
                receive_rate: connected as f64,
                connect_rate: connected as f64 / 2.0,
                error_rate: connected as f64 / 10.0,
                latency_count: latency_window_count,
                latency_window_count,
                latency_window_sum_us: connected * 1_000,
                latency_histogram: histogram,
                latency_avg_ms: 0.0,
                latency_min_ms: 1.0,
                latency_p50_ms: 0.0,
                latency_p90_ms: 0.0,
                latency_p95_ms: 0.0,
                latency_p99_ms: 0.0,
                latency_p999_ms: 0.0,
                latency_max_ms: 8.0,
            }
        }

        let nodes = vec![
            crate::model::AgentTaskMetrics {
                task_id: "task-a".to_string(),
                node_id: "node-a".to_string(),
                snapshots: vec![snapshot(
                    "local-a",
                    2,
                    vec![
                        LatencyBucket {
                            upper_bound_us: 1_000,
                            count: 2,
                        },
                        LatencyBucket {
                            upper_bound_us: 4_000,
                            count: 1,
                        },
                    ],
                )],
            },
            crate::model::AgentTaskMetrics {
                task_id: "task-b".to_string(),
                node_id: "node-b".to_string(),
                snapshots: vec![snapshot(
                    "local-b",
                    3,
                    vec![
                        LatencyBucket {
                            upper_bound_us: 2_000,
                            count: 2,
                        },
                        LatencyBucket {
                            upper_bound_us: 8_000,
                            count: 1,
                        },
                    ],
                )],
            },
        ];

        let aggregate = aggregate_metrics("distributed", &nodes);
        assert_eq!(aggregate.len(), 1);
        let point = &aggregate[0];
        assert_eq!(point.run_id, "distributed");
        assert_eq!(point.elapsed_ms, 1_000);
        assert_eq!(point.connected, 5);
        assert_eq!(point.published, 50);
        assert_eq!(point.latency_window_count, 6);
        assert_eq!(
            point
                .latency_histogram
                .iter()
                .map(|bucket| bucket.count)
                .sum::<u64>(),
            6
        );
        assert_eq!(point.latency_p50_ms, 2.0);
        assert_eq!(point.latency_p99_ms, 8.0);
    }
}
