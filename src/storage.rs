#![allow(dead_code)]

mod backfill;

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{
    Annotation, AnnotationCategory, AuthConfig, BenchConfig, BenchMode, BenchReport, BenchRun,
    BenchSpecimen, BenchStatus, BenchTemplate, BrokerProfile, BrokerProtocol, MetricSnapshot,
    PayloadKind, PayloadProfile, QosLevel, Run, RunStats, RunStatus, RunWorkload, Scenario,
    SpecimenUpdate, TemplateDraft, TlsConfig, Workload,
};

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial",
        include_str!("storage/migrations/0001_initial.sql"),
    ),
    (
        "0002_scenarios",
        include_str!("storage/migrations/0002_scenarios.sql"),
    ),
    (
        "0003_runs_v2",
        include_str!("storage/migrations/0003_runs_v2.sql"),
    ),
    (
        "0004_annotations",
        include_str!("storage/migrations/0004_annotations.sql"),
    ),
    (
        "0005_broker_protocol",
        include_str!("storage/migrations/0005_broker_protocol.sql"),
    ),
];

#[derive(Debug, Clone)]
pub struct Storage {
    path: Arc<PathBuf>,
}

impl Storage {
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let storage = Self {
            path: Arc::new(path.into()),
        };
        storage.init().await?;
        Ok(storage)
    }

    async fn init(&self) -> Result<()> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }

            let mut conn = Connection::open(path.as_ref())?;
            conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                "#,
            )?;
            run_migrations(&conn)?;
            ensure_column(
                &conn,
                "broker_profiles",
                "protocol",
                "TEXT NOT NULL DEFAULT 'mqtt'",
            )?;
            ensure_column(&conn, "broker_profiles", "websocket_path", "TEXT")?;
            seed_default_templates(&conn)?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_count",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_avg_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_min_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_max_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_p50_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_p90_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_p95_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_p99_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                &conn,
                "metric_snapshots",
                "latency_p999_ms",
                "REAL NOT NULL DEFAULT 0",
            )?;
            ensure_column(&conn, "metric_snapshots", "run_workload_id", "TEXT")?;
            backfill::backfill_legacy_runs(&mut conn)?;
            Ok(())
        })
        .await?
    }

    pub async fn start_run(
        &self,
        run_id: &str,
        config: &BenchConfig,
        started_at: DateTime<Utc>,
        specimen: &BenchSpecimen,
    ) -> Result<()> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        let config = config.clone();
        let specimen = specimen.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = Connection::open(path.as_ref())?;
            let config_json = serde_json::to_string(&config)?;
            let tags_json = serde_json::to_string(&specimen.tags)?;
            let specimen_config_json = serde_json::to_string(&specimen.config)?;
            let tx = conn.transaction()?;
            tx.execute(
                r#"
                INSERT INTO runs (id, status, mode, config_json, started_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    run_id,
                    "running",
                    config.mode.as_str(),
                    config_json,
                    started_at.to_rfc3339()
                ],
            )?;
            tx.execute(
                r#"
                INSERT INTO bench_specimens (
                    id, run_id, name, description, tags_json, config_json, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    specimen.id,
                    specimen.run_id,
                    specimen.name,
                    specimen.description,
                    tags_json,
                    specimen_config_json,
                    specimen.created_at.to_rfc3339()
                ],
            )?;
            backfill::insert_legacy_run_v2(
                &tx,
                &run_id,
                &config,
                "running",
                started_at,
                None,
                Some(&specimen),
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn finish_run(
        &self,
        run_id: &str,
        status: BenchStatus,
        stopped_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        let status = format!("{status:?}").to_lowercase();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = Connection::open(path.as_ref())?;
            conn.execute(
                r#"
                UPDATE runs
                   SET status = ?1,
                       stopped_at = ?2
                 WHERE id = ?3
                "#,
                params![status, stopped_at.to_rfc3339(), run_id],
            )?;
            conn.execute(
                r#"
                UPDATE runs_v2
                   SET status = ?1,
                       stopped_at = ?2
                 WHERE legacy_run_id = ?3 OR id = ?3
                "#,
                params![status, stopped_at.to_rfc3339(), run_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn insert_snapshot(&self, snapshot: MetricSnapshot) -> Result<()> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = Connection::open(path.as_ref())?;
            conn.execute(
                r#"
                INSERT INTO metric_snapshots (
                    run_id, ts, elapsed_ms, connected, published, received, errors,
                    publish_rate, receive_rate, connect_rate, error_rate,
                    latency_count, latency_avg_ms, latency_min_ms, latency_p50_ms,
                    latency_p90_ms, latency_p95_ms, latency_p99_ms, latency_p999_ms,
                    latency_max_ms, run_workload_id
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                "#,
                params![
                    snapshot.run_id,
                    snapshot.ts.to_rfc3339(),
                    snapshot.elapsed_ms as i64,
                    snapshot.connected as i64,
                    snapshot.published as i64,
                    snapshot.received as i64,
                    snapshot.errors as i64,
                    snapshot.publish_rate,
                    snapshot.receive_rate,
                    snapshot.connect_rate,
                    snapshot.error_rate,
                    snapshot.latency_count as i64,
                    snapshot.latency_avg_ms,
                    snapshot.latency_min_ms,
                    snapshot.latency_p50_ms,
                    snapshot.latency_p90_ms,
                    snapshot.latency_p95_ms,
                    snapshot.latency_p99_ms,
                    snapshot.latency_p999_ms,
                    snapshot.latency_max_ms,
                    snapshot
                        .run_workload_id
                        .clone()
                        .unwrap_or_else(|| backfill::legacy_run_workload_id(&snapshot.run_id)),
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn recent_snapshots(
        &self,
        run_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<MetricSnapshot>> {
        let path = Arc::clone(&self.path);
        let limit = limit.clamp(1, 10_000);
        tokio::task::spawn_blocking(move || -> Result<Vec<MetricSnapshot>> {
            let conn = Connection::open(path.as_ref())?;
            let mut snapshots = if let Some(run_id) = run_id {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT run_id, run_workload_id, ts, elapsed_ms, connected, published, received, errors,
                           publish_rate, receive_rate, connect_rate, error_rate,
                           latency_count, latency_avg_ms, latency_min_ms,
                           latency_p50_ms, latency_p90_ms, latency_p95_ms,
                           latency_p99_ms, latency_p999_ms, latency_max_ms
                      FROM metric_snapshots
                     WHERE run_id = ?1
                     ORDER BY id DESC
                     LIMIT ?2
                    "#,
                )?;
                rows_to_snapshots(stmt.query_map(params![run_id, limit as i64], map_snapshot)?)?
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT run_id, run_workload_id, ts, elapsed_ms, connected, published, received, errors,
                           publish_rate, receive_rate, connect_rate, error_rate,
                           latency_count, latency_avg_ms, latency_min_ms,
                           latency_p50_ms, latency_p90_ms, latency_p95_ms,
                           latency_p99_ms, latency_p999_ms, latency_max_ms
                      FROM metric_snapshots
                     ORDER BY id DESC
                     LIMIT ?1
                    "#,
                )?;
                rows_to_snapshots(stmt.query_map(params![limit as i64], map_snapshot)?)?
            };
            snapshots.reverse();
            Ok(snapshots)
        })
        .await?
    }

    pub async fn list_runs(&self, limit: usize) -> Result<Vec<BenchRun>> {
        let path = Arc::clone(&self.path);
        let limit = limit.clamp(1, 500);
        tokio::task::spawn_blocking(move || -> Result<Vec<BenchRun>> {
            let conn = Connection::open(path.as_ref())?;
            let mut stmt = conn.prepare(
                r#"
                SELECT r.id, r.status, r.mode, r.config_json, r.started_at, r.stopped_at,
                       COUNT(m.id) AS sample_count,
                       s.id, s.run_id, s.name, s.description, s.tags_json, s.config_json, s.created_at
                  FROM runs r
             LEFT JOIN metric_snapshots m ON m.run_id = r.id
             LEFT JOIN bench_specimens s ON s.run_id = r.id
              GROUP BY r.id
              ORDER BY r.started_at DESC
                 LIMIT ?1
                "#,
            )?;

            let rows = stmt.query_map(params![limit as i64], map_run)?;
            let mut runs = Vec::new();
            for row in rows {
                runs.push(row?);
            }
            Ok(runs)
        })
        .await?
    }

    pub async fn report(&self, run_id: &str) -> Result<Option<BenchReport>> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<BenchReport>> {
            let conn = Connection::open(path.as_ref())?;
            let mut run_stmt = conn.prepare(
                r#"
                SELECT r.id, r.status, r.mode, r.config_json, r.started_at, r.stopped_at,
                       COUNT(m.id) AS sample_count,
                       s.id, s.run_id, s.name, s.description, s.tags_json, s.config_json, s.created_at
                  FROM runs r
             LEFT JOIN metric_snapshots m ON m.run_id = r.id
             LEFT JOIN bench_specimens s ON s.run_id = r.id
                 WHERE r.id = ?1
              GROUP BY r.id
                "#,
            )?;

            let mut runs = run_stmt.query_map(params![run_id], map_run)?;
            let Some(run) = runs.next().transpose()? else {
                return Ok(None);
            };

            let mut snapshot_stmt = conn.prepare(
                r#"
                SELECT run_id, run_workload_id, ts, elapsed_ms, connected, published, received, errors,
                       publish_rate, receive_rate, connect_rate, error_rate,
                       latency_count, latency_avg_ms, latency_min_ms,
                       latency_p50_ms, latency_p90_ms, latency_p95_ms,
                       latency_p99_ms, latency_p999_ms, latency_max_ms
                  FROM metric_snapshots
                 WHERE run_id = ?1
                 ORDER BY id ASC
                "#,
            )?;
            let snapshots =
                rows_to_snapshots(snapshot_stmt.query_map(params![run.id.clone()], map_snapshot)?)?;
            let stats = summarize_snapshots(&snapshots);

            Ok(Some(BenchReport {
                run,
                stats,
                snapshots,
            }))
        })
        .await?
    }

    pub async fn list_specimens(&self, limit: usize) -> Result<Vec<BenchSpecimen>> {
        let path = Arc::clone(&self.path);
        let limit = limit.clamp(1, 500);
        tokio::task::spawn_blocking(move || -> Result<Vec<BenchSpecimen>> {
            let conn = Connection::open(path.as_ref())?;
            let mut stmt = conn.prepare(
                r#"
                SELECT id, run_id, name, description, tags_json, config_json, created_at
                  FROM bench_specimens
              ORDER BY created_at DESC
                 LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![limit as i64], map_specimen)?;
            let mut specimens = Vec::new();
            for row in rows {
                specimens.push(row?);
            }
            Ok(specimens)
        })
        .await?
    }

    pub async fn update_specimen(
        &self,
        specimen_id: &str,
        update: SpecimenUpdate,
    ) -> Result<Option<BenchSpecimen>> {
        let path = Arc::clone(&self.path);
        let specimen_id = specimen_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<BenchSpecimen>> {
            let conn = Connection::open(path.as_ref())?;
            let Some(mut specimen) = query_specimen_by_id(&conn, &specimen_id)? else {
                return Ok(None);
            };

            if let Some(name) = update.name {
                let name = name.trim();
                if !name.is_empty() {
                    specimen.name = name.to_string();
                }
            }
            if let Some(description) = update.description {
                specimen.description = description.trim().to_string();
            }
            if let Some(tags) = update.tags {
                specimen.tags = normalize_tags(tags);
            }

            let tags_json = serde_json::to_string(&specimen.tags)?;
            conn.execute(
                r#"
                UPDATE bench_specimens
                   SET name = ?1,
                       description = ?2,
                       tags_json = ?3
                 WHERE id = ?4
                "#,
                params![specimen.name, specimen.description, tags_json, specimen_id],
            )?;

            query_specimen_by_id(&conn, &specimen_id)
        })
        .await?
    }

    pub async fn delete_specimen(&self, specimen_id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let specimen_id = specimen_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = Connection::open(path.as_ref())?;
            let changed = conn.execute(
                "DELETE FROM bench_specimens WHERE id = ?1",
                params![specimen_id],
            )?;
            Ok(changed > 0)
        })
        .await?
    }

    pub async fn list_templates(&self, limit: usize) -> Result<Vec<BenchTemplate>> {
        let path = Arc::clone(&self.path);
        let limit = limit.clamp(1, 500);
        tokio::task::spawn_blocking(move || -> Result<Vec<BenchTemplate>> {
            let conn = Connection::open(path.as_ref())?;
            let mut stmt = conn.prepare(
                r#"
                SELECT id, name, description, tags_json, config_json, created_at, updated_at
                  FROM bench_templates
              ORDER BY updated_at DESC
                 LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![limit as i64], map_template)?;
            let mut templates = Vec::new();
            for row in rows {
                templates.push(row?);
            }
            Ok(templates)
        })
        .await?
    }

    pub async fn create_template(&self, template: BenchTemplate) -> Result<BenchTemplate> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<BenchTemplate> {
            let conn = Connection::open(path.as_ref())?;
            let tags_json = serde_json::to_string(&template.tags)?;
            let config_json = serde_json::to_string(&template.config)?;
            conn.execute(
                r#"
                INSERT INTO bench_templates (
                    id, name, description, tags_json, config_json, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    template.id,
                    template.name,
                    template.description,
                    tags_json,
                    config_json,
                    template.created_at.to_rfc3339(),
                    template.updated_at.to_rfc3339()
                ],
            )?;
            Ok(template)
        })
        .await?
    }

    pub async fn update_template(
        &self,
        template_id: &str,
        draft: TemplateDraft,
    ) -> Result<Option<BenchTemplate>> {
        let path = Arc::clone(&self.path);
        let template_id = template_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<BenchTemplate>> {
            let conn = Connection::open(path.as_ref())?;
            if query_template_by_id(&conn, &template_id)?.is_none() {
                return Ok(None);
            }

            let name = draft
                .name
                .unwrap_or_else(|| "benchmark template".to_string());
            let description = draft.description.unwrap_or_default();
            let tags_json = serde_json::to_string(&draft.tags)?;
            let config_json = serde_json::to_string(&draft.config)?;
            let updated_at = Utc::now();
            conn.execute(
                r#"
                UPDATE bench_templates
                   SET name = ?1,
                       description = ?2,
                       tags_json = ?3,
                       config_json = ?4,
                       updated_at = ?5
                 WHERE id = ?6
                "#,
                params![
                    name,
                    description,
                    tags_json,
                    config_json,
                    updated_at.to_rfc3339(),
                    template_id
                ],
            )?;

            query_template_by_id(&conn, &template_id)
        })
        .await?
    }

    pub async fn delete_template(&self, template_id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let template_id = template_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = Connection::open(path.as_ref())?;
            let changed = conn.execute(
                "DELETE FROM bench_templates WHERE id = ?1",
                params![template_id],
            )?;
            Ok(changed > 0)
        })
        .await?
    }

    pub async fn list_broker_profiles(&self) -> Result<Vec<BrokerProfile>> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Vec<BrokerProfile>> {
            let conn = Connection::open(path.as_ref())?;
            BrokerProfileRepo::new(&conn).list()
        })
        .await?
    }

    pub async fn get_broker_profile(&self, id: &str) -> Result<Option<BrokerProfile>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<BrokerProfile>> {
            let conn = Connection::open(path.as_ref())?;
            BrokerProfileRepo::new(&conn).get(&id)
        })
        .await?
    }

    pub async fn upsert_broker_profile(&self, profile: BrokerProfile) -> Result<BrokerProfile> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<BrokerProfile> {
            let conn = Connection::open(path.as_ref())?;
            BrokerProfileRepo::new(&conn).upsert(&profile)?;
            Ok(profile)
        })
        .await?
    }

    pub async fn delete_broker_profile(&self, id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = Connection::open(path.as_ref())?;
            BrokerProfileRepo::new(&conn).delete(&id)
        })
        .await?
    }

    pub async fn list_payload_profiles(&self) -> Result<Vec<PayloadProfile>> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Vec<PayloadProfile>> {
            let conn = Connection::open(path.as_ref())?;
            PayloadProfileRepo::new(&conn).list()
        })
        .await?
    }

    pub async fn get_payload_profile(&self, id: &str) -> Result<Option<PayloadProfile>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<PayloadProfile>> {
            let conn = Connection::open(path.as_ref())?;
            PayloadProfileRepo::new(&conn).get(&id)
        })
        .await?
    }

    pub async fn upsert_payload_profile(&self, profile: PayloadProfile) -> Result<PayloadProfile> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<PayloadProfile> {
            let conn = Connection::open(path.as_ref())?;
            PayloadProfileRepo::new(&conn).upsert(&profile)?;
            Ok(profile)
        })
        .await?
    }

    pub async fn delete_payload_profile(&self, id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = Connection::open(path.as_ref())?;
            PayloadProfileRepo::new(&conn).delete(&id)
        })
        .await?
    }

    pub async fn list_scenarios(&self) -> Result<Vec<Scenario>> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Vec<Scenario>> {
            let conn = Connection::open(path.as_ref())?;
            ScenarioRepo::new(&conn).list()
        })
        .await?
    }

    pub async fn get_scenario(&self, id: &str) -> Result<Option<Scenario>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Scenario>> {
            let conn = Connection::open(path.as_ref())?;
            ScenarioRepo::new(&conn).get(&id)
        })
        .await?
    }

    pub async fn upsert_scenario(&self, scenario: Scenario) -> Result<Scenario> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Scenario> {
            let conn = Connection::open(path.as_ref())?;
            ScenarioRepo::new(&conn).upsert(&scenario)?;
            Ok(scenario)
        })
        .await?
    }

    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = Connection::open(path.as_ref())?;
            ScenarioRepo::new(&conn).delete(&id)
        })
        .await?
    }

    pub async fn set_scenario_baseline(
        &self,
        scenario_id: &str,
        run_id: Option<String>,
    ) -> Result<Option<Scenario>> {
        let path = Arc::clone(&self.path);
        let scenario_id = scenario_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Scenario>> {
            let conn = Connection::open(path.as_ref())?;
            if ScenarioRepo::new(&conn).get(&scenario_id)?.is_none() {
                return Ok(None);
            }
            conn.execute(
                r#"
                UPDATE scenarios
                   SET baseline_run_id = ?1,
                       updated_at = ?2
                 WHERE id = ?3
                "#,
                params![run_id, Utc::now().to_rfc3339(), scenario_id],
            )?;
            ScenarioRepo::new(&conn).get(&scenario_id)
        })
        .await?
    }

    pub async fn list_runs_v2(
        &self,
        scenario_id: Option<String>,
        status: Option<String>,
        limit: usize,
    ) -> Result<Vec<Run>> {
        let path = Arc::clone(&self.path);
        let limit = limit.clamp(1, 500);
        tokio::task::spawn_blocking(move || -> Result<Vec<Run>> {
            let conn = Connection::open(path.as_ref())?;
            list_runs_v2(&conn, scenario_id.as_deref(), status.as_deref(), limit)
        })
        .await?
    }

    pub async fn get_run_v2(&self, id: &str) -> Result<Option<Run>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Run>> {
            let conn = Connection::open(path.as_ref())?;
            RunRepo::new(&conn).get(&id)
        })
        .await?
    }

    pub async fn report_v2(&self, id: &str) -> Result<Option<BenchReport>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<BenchReport>> {
            let conn = Connection::open(path.as_ref())?;
            let Some(run) = RunRepo::new(&conn).get(&id)? else {
                return Ok(None);
            };
            let snapshots = list_snapshots_v2(&conn, &id, None, None, 20_000)?;
            let config = run
                .workloads
                .first()
                .and_then(|workload| {
                    serde_json::from_str::<Workload>(&workload.config_snapshot_json).ok()
                })
                .and_then(|workload| workload.flatten_to_legacy())
                .unwrap_or_default();
            let specimen = BenchSpecimen {
                id: format!("v2-specimen-{id}"),
                run_id: id.clone(),
                name: run.name.clone(),
                description: run.description.clone(),
                tags: run.tags.clone(),
                config: config.clone(),
                created_at: run.started_at,
            };
            let report_run = BenchRun {
                id: id.clone(),
                status: run_status_as_str(&run.status).to_string(),
                mode: config.mode.as_str().to_string(),
                config,
                started_at: run.started_at,
                stopped_at: run.stopped_at,
                sample_count: snapshots.len() as u64,
                specimen: Some(specimen),
            };
            let stats = summarize_snapshots(&snapshots);
            Ok(Some(BenchReport {
                run: report_run,
                stats,
                snapshots,
            }))
        })
        .await?
    }

    pub async fn upsert_run_v2(&self, run: Run) -> Result<Run> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Run> {
            let conn = Connection::open(path.as_ref())?;
            RunRepo::new(&conn).upsert(&run)?;
            Ok(run)
        })
        .await?
    }

    pub async fn finish_run_v2(
        &self,
        run_id: &str,
        status: RunStatus,
        stopped_at: DateTime<Utc>,
    ) -> Result<()> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = Connection::open(path.as_ref())?;
            conn.execute(
                r#"
                UPDATE runs_v2
                   SET status = ?1,
                       stopped_at = ?2
                 WHERE id = ?3
                "#,
                params![run_status_as_str(&status), stopped_at.to_rfc3339(), run_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_run_v2_metadata(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<Option<Run>> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Run>> {
            let conn = Connection::open(path.as_ref())?;
            let Some(mut run) = RunRepo::new(&conn).get(&id)? else {
                return Ok(None);
            };
            if let Some(name) = name {
                let name = name.trim();
                if !name.is_empty() {
                    run.name = name.to_string();
                }
            }
            if let Some(description) = description {
                run.description = description.trim().to_string();
            }
            if let Some(tags) = tags {
                run.tags = normalize_tags(tags);
            }
            let tags_json = serde_json::to_string(&run.tags)?;
            conn.execute(
                r#"
                UPDATE runs_v2
                   SET name = ?1,
                       description = ?2,
                       tags_json = ?3
                 WHERE id = ?4
                "#,
                params![run.name, run.description, tags_json, id],
            )?;
            RunRepo::new(&conn).get(&id)
        })
        .await?
    }

    pub async fn delete_run_v2(&self, id: &str) -> Result<bool> {
        let path = Arc::clone(&self.path);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let mut conn = Connection::open(path.as_ref())?;
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM metric_snapshots WHERE run_id = ?1",
                params![id],
            )?;
            tx.execute("DELETE FROM bench_specimens WHERE run_id = ?1", params![id])?;
            tx.execute("DELETE FROM runs WHERE id = ?1", params![id])?;
            tx.execute("DELETE FROM run_workloads WHERE run_id = ?1", params![id])?;
            tx.execute("DELETE FROM annotations WHERE run_id = ?1", params![id])?;
            let changed = tx.execute("DELETE FROM runs_v2 WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(changed > 0)
        })
        .await?
    }

    pub async fn snapshots_v2(
        &self,
        run_id: &str,
        run_workload_id: Option<String>,
        since_ms: Option<u64>,
        limit: usize,
    ) -> Result<Vec<MetricSnapshot>> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        let limit = limit.clamp(1, 20_000);
        tokio::task::spawn_blocking(move || -> Result<Vec<MetricSnapshot>> {
            let conn = Connection::open(path.as_ref())?;
            list_snapshots_v2(&conn, &run_id, run_workload_id.as_deref(), since_ms, limit)
        })
        .await?
    }

    pub async fn list_annotations(&self, run_id: &str) -> Result<Vec<Annotation>> {
        let path = Arc::clone(&self.path);
        let run_id = run_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<Vec<Annotation>> {
            let conn = Connection::open(path.as_ref())?;
            AnnotationRepo::new(&conn).list_for_run(&run_id)
        })
        .await?
    }

    pub async fn upsert_annotation(&self, annotation: Annotation) -> Result<Annotation> {
        let path = Arc::clone(&self.path);
        tokio::task::spawn_blocking(move || -> Result<Annotation> {
            let conn = Connection::open(path.as_ref())?;
            AnnotationRepo::new(&conn).upsert(&annotation)?;
            Ok(annotation)
        })
        .await?
    }
}

pub(crate) fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_versions (
            id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );
        "#,
    )?;

    for (id, sql) in MIGRATIONS {
        let already = conn
            .query_row(
                "SELECT 1 FROM schema_versions WHERE id = ?1",
                params![id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if already {
            continue;
        }

        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_versions (id, applied_at) VALUES (?1, ?2)",
            params![id, Utc::now().to_rfc3339()],
        )?;
    }

    Ok(())
}

pub struct BrokerProfileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> BrokerProfileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<BrokerProfile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, protocol, host, port, websocket_path, tls_json, auth_json,
                   keepalive_secs, clean_session, created_at, updated_at
              FROM broker_profiles
          ORDER BY updated_at DESC
            "#,
        )?;
        collect_rows(stmt.query_map([], map_broker_profile)?)
    }

    pub fn get(&self, id: &str) -> Result<Option<BrokerProfile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, protocol, host, port, websocket_path, tls_json, auth_json,
                   keepalive_secs, clean_session, created_at, updated_at
              FROM broker_profiles
             WHERE id = ?1
            "#,
        )?;
        stmt.query_row(params![id], map_broker_profile)
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert(&self, profile: &BrokerProfile) -> Result<()> {
        let tls_json = profile
            .tls
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let auth_json = profile
            .auth
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn.execute(
            r#"
            INSERT INTO broker_profiles (
                id, name, protocol, host, port, websocket_path, tls_json, auth_json,
                keepalive_secs, clean_session, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                protocol = excluded.protocol,
                host = excluded.host,
                port = excluded.port,
                websocket_path = excluded.websocket_path,
                tls_json = excluded.tls_json,
                auth_json = excluded.auth_json,
                keepalive_secs = excluded.keepalive_secs,
                clean_session = excluded.clean_session,
                updated_at = excluded.updated_at
            "#,
            params![
                profile.id,
                profile.name,
                profile.protocol.as_str(),
                profile.host,
                profile.port as i64,
                profile.websocket_path,
                tls_json,
                auth_json,
                profile.keepalive_secs as i64,
                if profile.clean_session { 1 } else { 0 },
                profile.created_at.to_rfc3339(),
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM broker_profiles WHERE id = ?1", params![id])?
            > 0)
    }
}

pub struct PayloadProfileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PayloadProfileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<PayloadProfile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, kind_json, created_at, updated_at
              FROM payload_profiles
          ORDER BY updated_at DESC
            "#,
        )?;
        collect_rows(stmt.query_map([], map_payload_profile)?)
    }

    pub fn get(&self, id: &str) -> Result<Option<PayloadProfile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, kind_json, created_at, updated_at
              FROM payload_profiles
             WHERE id = ?1
            "#,
        )?;
        stmt.query_row(params![id], map_payload_profile)
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert(&self, profile: &PayloadProfile) -> Result<()> {
        let kind_json = serde_json::to_string(&profile.kind)?;
        self.conn.execute(
            r#"
            INSERT INTO payload_profiles (id, name, kind_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind_json = excluded.kind_json,
                updated_at = excluded.updated_at
            "#,
            params![
                profile.id,
                profile.name,
                kind_json,
                profile.created_at.to_rfc3339(),
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM payload_profiles WHERE id = ?1", params![id])?
            > 0)
    }
}

pub struct ScenarioRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ScenarioRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<Scenario>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, description, tags_json, stages_json, baseline_run_id,
                   created_at, updated_at
              FROM scenarios
          ORDER BY updated_at DESC
            "#,
        )?;
        collect_rows(stmt.query_map([], map_scenario)?)
    }

    pub fn get(&self, id: &str) -> Result<Option<Scenario>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, description, tags_json, stages_json, baseline_run_id,
                   created_at, updated_at
              FROM scenarios
             WHERE id = ?1
            "#,
        )?;
        stmt.query_row(params![id], map_scenario)
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert(&self, scenario: &Scenario) -> Result<()> {
        let tags_json = serde_json::to_string(&scenario.tags)?;
        let stages_json = serde_json::to_string(&scenario.stages)?;
        self.conn.execute(
            r#"
            INSERT INTO scenarios (
                id, name, description, tags_json, stages_json, baseline_run_id,
                created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                tags_json = excluded.tags_json,
                stages_json = excluded.stages_json,
                baseline_run_id = excluded.baseline_run_id,
                updated_at = excluded.updated_at
            "#,
            params![
                scenario.id,
                scenario.name,
                scenario.description,
                tags_json,
                stages_json,
                scenario.baseline_run_id,
                scenario.created_at.to_rfc3339(),
                scenario.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM scenarios WHERE id = ?1", params![id])?
            > 0)
    }
}

pub struct RunRepo<'a> {
    conn: &'a Connection,
}

impl<'a> RunRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> Result<Option<Run>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, scenario_id, name, tags_json, description, status,
                   started_at, stopped_at
              FROM runs_v2
             WHERE id = ?1
            "#,
        )?;
        let Some(mut run) = stmt.query_row(params![id], map_run_v2).optional()? else {
            return Ok(None);
        };
        run.workloads = self.list_workloads(&run.id)?;
        Ok(Some(run))
    }

    pub fn upsert(&self, run: &Run) -> Result<()> {
        let tags_json = serde_json::to_string(&run.tags)?;
        self.conn.execute(
            r#"
            INSERT INTO runs_v2 (
                id, scenario_id, name, tags_json, description, status,
                started_at, stopped_at, legacy_run_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            ON CONFLICT(id) DO UPDATE SET
                scenario_id = excluded.scenario_id,
                name = excluded.name,
                tags_json = excluded.tags_json,
                description = excluded.description,
                status = excluded.status,
                stopped_at = excluded.stopped_at
            "#,
            params![
                run.id,
                run.scenario_id,
                run.name,
                tags_json,
                run.description,
                run_status_as_str(&run.status),
                run.started_at.to_rfc3339(),
                run.stopped_at.map(|value| value.to_rfc3339()),
            ],
        )?;

        let config = run
            .workloads
            .first()
            .and_then(|workload| {
                serde_json::from_str::<Workload>(&workload.config_snapshot_json).ok()
            })
            .and_then(|workload| workload.flatten_to_legacy())
            .unwrap_or_default();
        let config_json = serde_json::to_string(&config)?;
        self.conn.execute(
            r#"
            INSERT INTO runs (id, status, mode, config_json, started_at, stopped_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                mode = excluded.mode,
                config_json = excluded.config_json,
                stopped_at = excluded.stopped_at
            "#,
            params![
                run.id,
                run_status_as_str(&run.status),
                config.mode.as_str(),
                config_json,
                run.started_at.to_rfc3339(),
                run.stopped_at.map(|value| value.to_rfc3339()),
            ],
        )?;

        for workload in &run.workloads {
            self.upsert_workload(workload)?;
        }
        Ok(())
    }

    pub fn upsert_workload(&self, workload: &RunWorkload) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO run_workloads (
                id, run_id, workload_id, kind, config_snapshot_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                workload_id = excluded.workload_id,
                kind = excluded.kind,
                config_snapshot_json = excluded.config_snapshot_json
            "#,
            params![
                workload.id,
                workload.run_id,
                workload.workload_id,
                workload_kind_as_str(&workload.kind),
                workload.config_snapshot_json,
            ],
        )?;
        Ok(())
    }

    fn list_workloads(&self, run_id: &str) -> Result<Vec<RunWorkload>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, run_id, workload_id, kind, config_snapshot_json
              FROM run_workloads
             WHERE run_id = ?1
          ORDER BY id ASC
            "#,
        )?;
        collect_rows(stmt.query_map(params![run_id], map_run_workload)?)
    }
}

pub struct AnnotationRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AnnotationRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_for_run(&self, run_id: &str) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, run_id, run_workload_id, ts, category, title, detail
              FROM annotations
             WHERE run_id = ?1
          ORDER BY ts ASC
            "#,
        )?;
        collect_rows(stmt.query_map(params![run_id], map_annotation)?)
    }

    pub fn upsert(&self, annotation: &Annotation) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO annotations (
                id, run_id, run_workload_id, ts, category, title, detail
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                run_workload_id = excluded.run_workload_id,
                ts = excluded.ts,
                category = excluded.category,
                title = excluded.title,
                detail = excluded.detail
            "#,
            params![
                annotation.id,
                annotation.run_id,
                annotation.run_workload_id,
                annotation.ts.to_rfc3339(),
                annotation_category_as_str(&annotation.category),
                annotation.title,
                annotation.detail,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM annotations WHERE id = ?1", params![id])?
            > 0)
    }
}

fn seed_default_templates(conn: &Connection) -> Result<()> {
    let now = Utc::now();
    let mut publish = BenchConfig::default();
    publish.mode = BenchMode::Pub;
    publish.clients = 100;
    publish.payload_size = 256;
    publish.message_interval_ms = 1000;
    publish.duration_secs = 60;

    let mut subscribe = BenchConfig::default();
    subscribe.mode = BenchMode::Sub;
    subscribe.clients = 200;
    subscribe.topic = "velamq/bench/#".to_string();
    subscribe.duration_secs = 120;

    let mut connection = BenchConfig::default();
    connection.mode = BenchMode::Conn;
    connection.clients = 1000;
    connection.connect_rate = 200;
    connection.duration_secs = 30;
    connection.qos = QosLevel::Qos0;

    let mut throughput = BenchConfig::default();
    throughput.mode = BenchMode::Pub;
    throughput.clients = 500;
    throughput.connect_rate = 200;
    throughput.payload_size = 512;
    throughput.message_interval_ms = 20;
    throughput.duration_secs = 300;

    let mut latency = BenchConfig::default();
    latency.mode = BenchMode::Pub;
    latency.clients = 100;
    latency.payload_size = 128;
    latency.payload_timestamp = true;
    latency.message_interval_ms = 100;
    latency.duration_secs = 180;
    latency.sample_interval_ms = 500;

    let mut soak_pub = BenchConfig::default();
    soak_pub.mode = BenchMode::Pub;
    soak_pub.clients = 200;
    soak_pub.payload_size = 256;
    soak_pub.message_interval_ms = 1000;
    soak_pub.duration_secs = 1800;

    let mut soak_sub = BenchConfig::default();
    soak_sub.mode = BenchMode::Sub;
    soak_sub.clients = 1000;
    soak_sub.topic = "velamq/bench/#".to_string();
    soak_sub.duration_secs = 1800;

    let mut connect_storm = BenchConfig::default();
    connect_storm.mode = BenchMode::Conn;
    connect_storm.clients = 5000;
    connect_storm.connect_rate = 500;
    connect_storm.duration_secs = 60;

    let mut large_payload = BenchConfig::default();
    large_payload.mode = BenchMode::Pub;
    large_payload.clients = 50;
    large_payload.payload_size = 64 * 1024;
    large_payload.payload_timestamp = false;
    large_payload.message_interval_ms = 200;
    large_payload.duration_secs = 300;

    let mut qos1 = BenchConfig::default();
    qos1.mode = BenchMode::Pub;
    qos1.clients = 200;
    qos1.qos = QosLevel::Qos1;
    qos1.payload_size = 256;
    qos1.message_interval_ms = 50;
    qos1.duration_secs = 300;

    let mut iot_low_rate = BenchConfig::default();
    iot_low_rate.mode = BenchMode::Pub;
    iot_low_rate.clients = 1000;
    iot_low_rate.connect_rate = 100;
    iot_low_rate.payload_size = 128;
    iot_low_rate.message_interval_ms = 5000;
    iot_low_rate.duration_secs = 900;

    let mut mqtts_smoke = publish.clone();
    mqtts_smoke.protocol = BrokerProtocol::Mqtts;
    mqtts_smoke.port = BrokerProtocol::Mqtts.default_port();

    let mut websocket_smoke = publish.clone();
    websocket_smoke.protocol = BrokerProtocol::Ws;
    websocket_smoke.port = BrokerProtocol::Ws.default_port();
    websocket_smoke.websocket_path = Some("/mqtt".to_string());

    let mut websocket_sub = subscribe.clone();
    websocket_sub.protocol = BrokerProtocol::Ws;
    websocket_sub.port = BrokerProtocol::Ws.default_port();
    websocket_sub.websocket_path = Some("/mqtt".to_string());

    let mut wss_smoke = publish.clone();
    wss_smoke.protocol = BrokerProtocol::Wss;
    wss_smoke.port = BrokerProtocol::Wss.default_port();
    wss_smoke.websocket_path = Some("/mqtt".to_string());

    let templates = [
        BenchTemplate {
            id: "tpl-default-pub-smoke".to_string(),
            name: "Publish Smoke".to_string(),
            description: "Small publish baseline with timestamp payloads.".to_string(),
            tags: vec!["publish".to_string(), "smoke".to_string()],
            config: publish,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-sub-fanout".to_string(),
            name: "Subscriber Fanout".to_string(),
            description: "Subscriber-side fanout baseline.".to_string(),
            tags: vec!["subscribe".to_string(), "fanout".to_string()],
            config: subscribe,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-conn-ramp".to_string(),
            name: "Connection Ramp".to_string(),
            description: "Connection ramp for broker session capacity checks.".to_string(),
            tags: vec!["connection".to_string(), "ramp".to_string()],
            config: connection,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-pub-throughput".to_string(),
            name: "Publish Throughput".to_string(),
            description: "Sustained publish throughput baseline with medium payloads.".to_string(),
            tags: vec!["publish".to_string(), "throughput".to_string()],
            config: throughput,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-latency-probe".to_string(),
            name: "Latency Probe".to_string(),
            description: "Timestamp payload probe for latency percentile checks.".to_string(),
            tags: vec!["publish".to_string(), "latency".to_string()],
            config: latency,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-pub-soak".to_string(),
            name: "Publish Soak".to_string(),
            description: "Long-running steady publish test for stability checks.".to_string(),
            tags: vec!["publish".to_string(), "soak".to_string()],
            config: soak_pub,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-sub-soak".to_string(),
            name: "Subscriber Soak".to_string(),
            description: "Long-running subscriber fanout stability test.".to_string(),
            tags: vec!["subscribe".to_string(), "soak".to_string()],
            config: soak_sub,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-conn-storm".to_string(),
            name: "Connection Storm".to_string(),
            description: "Short high-rate connection churn test for session admission limits."
                .to_string(),
            tags: vec!["connection".to_string(), "churn".to_string()],
            config: connect_storm,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-large-payload".to_string(),
            name: "Large Payload Publish".to_string(),
            description: "Publish baseline for bandwidth and packet-size pressure.".to_string(),
            tags: vec!["publish".to_string(), "payload".to_string()],
            config: large_payload,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-qos1-publish".to_string(),
            name: "QoS1 Publish".to_string(),
            description: "QoS1 publish baseline for acknowledgement overhead checks.".to_string(),
            tags: vec!["publish".to_string(), "qos1".to_string()],
            config: qos1,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-iot-low-rate".to_string(),
            name: "IoT Low Rate".to_string(),
            description: "Many low-frequency publishers for IoT-style workloads.".to_string(),
            tags: vec!["publish".to_string(), "iot".to_string()],
            config: iot_low_rate,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-mqtts-pub-smoke".to_string(),
            name: "MQTTS Publish Smoke".to_string(),
            description: "Secure MQTT/TLS publish smoke test on the standard 8883 port."
                .to_string(),
            tags: vec![
                "publish".to_string(),
                "mqtts".to_string(),
                "tls".to_string(),
            ],
            config: mqtts_smoke,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-ws-pub-smoke".to_string(),
            name: "WebSocket Publish Smoke".to_string(),
            description: "MQTT over WebSocket publish smoke test using the /mqtt path.".to_string(),
            tags: vec!["publish".to_string(), "websocket".to_string()],
            config: websocket_smoke,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-ws-sub-fanout".to_string(),
            name: "WebSocket Subscriber Fanout".to_string(),
            description: "Subscriber fanout baseline over MQTT WebSocket transport.".to_string(),
            tags: vec!["subscribe".to_string(), "websocket".to_string()],
            config: websocket_sub,
            created_at: now,
            updated_at: now,
        },
        BenchTemplate {
            id: "tpl-default-wss-pub-smoke".to_string(),
            name: "WSS Publish Smoke".to_string(),
            description: "Secure MQTT over WebSocket publish smoke test on the 8084 port."
                .to_string(),
            tags: vec![
                "publish".to_string(),
                "websocket".to_string(),
                "tls".to_string(),
            ],
            config: wss_smoke,
            created_at: now,
            updated_at: now,
        },
    ];

    for template in templates {
        let tags_json = serde_json::to_string(&template.tags)?;
        let config_json = serde_json::to_string(&template.config)?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO bench_templates (
                id, name, description, tags_json, config_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                template.id,
                template.name,
                template.description,
                tags_json,
                config_json,
                template.created_at.to_rfc3339(),
                template.updated_at.to_rfc3339()
            ],
        )?;
    }

    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in columns {
        if name? == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .take(32)
        .collect()
}

fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn list_runs_v2(
    conn: &Connection,
    scenario_id: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<Run>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, scenario_id, name, tags_json, description, status,
               started_at, stopped_at
          FROM runs_v2
         WHERE (?1 IS NULL OR scenario_id = ?1)
           AND (?2 IS NULL OR status = ?2)
      ORDER BY started_at DESC
         LIMIT ?3
        "#,
    )?;
    let mut runs =
        collect_rows(stmt.query_map(params![scenario_id, status, limit as i64], map_run_v2)?)?;
    let repo = RunRepo::new(conn);
    for run in &mut runs {
        run.workloads = repo.list_workloads(&run.id)?;
    }
    Ok(runs)
}

fn list_snapshots_v2(
    conn: &Connection,
    run_id: &str,
    run_workload_id: Option<&str>,
    since_ms: Option<u64>,
    limit: usize,
) -> Result<Vec<MetricSnapshot>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT run_id, run_workload_id, ts, elapsed_ms, connected, published, received, errors,
               publish_rate, receive_rate, connect_rate, error_rate,
               latency_count, latency_avg_ms, latency_min_ms,
               latency_p50_ms, latency_p90_ms, latency_p95_ms,
               latency_p99_ms, latency_p999_ms, latency_max_ms
          FROM metric_snapshots
         WHERE run_id = ?1
           AND (?2 IS NULL OR run_workload_id = ?2)
           AND (?3 IS NULL OR elapsed_ms >= ?3)
      ORDER BY id ASC
         LIMIT ?4
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            run_id,
            run_workload_id,
            since_ms.map(|value| value as i64),
            limit as i64
        ],
        map_snapshot,
    )?;
    Ok(rows_to_snapshots(rows)?)
}

fn map_broker_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<BrokerProfile> {
    let protocol: String = row.get(2)?;
    let tls_json: Option<String> = row.get(6)?;
    let auth_json: Option<String> = row.get(7)?;
    let created_at: String = row.get(10)?;
    let updated_at: String = row.get(11)?;
    Ok(BrokerProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        protocol: BrokerProtocol::from_storage(&protocol),
        host: row.get(3)?,
        port: row.get::<_, i64>(4)? as u16,
        websocket_path: row.get(5)?,
        tls: parse_json_option::<TlsConfig>(tls_json)?,
        auth: parse_json_option::<AuthConfig>(auth_json)?,
        keepalive_secs: row.get::<_, i64>(8)? as u16,
        clean_session: row.get::<_, i64>(9)? != 0,
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
        updated_at: parse_utc(&updated_at).map_err(to_sql_error)?,
    })
}

fn map_payload_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<PayloadProfile> {
    let kind_json: String = row.get(2)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;
    Ok(PayloadProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: parse_json::<PayloadKind>(&kind_json)?,
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
        updated_at: parse_utc(&updated_at).map_err(to_sql_error)?,
    })
}

fn map_scenario(row: &rusqlite::Row<'_>) -> rusqlite::Result<Scenario> {
    let tags_json: String = row.get(3)?;
    let stages_json: String = row.get(4)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    Ok(Scenario {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        tags: parse_json::<Vec<String>>(&tags_json)?,
        stages: parse_json(&stages_json)?,
        baseline_run_id: row.get(5)?,
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
        updated_at: parse_utc(&updated_at).map_err(to_sql_error)?,
    })
}

fn map_run_v2(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let tags_json: String = row.get(3)?;
    let started_at: String = row.get(6)?;
    let stopped_at: Option<String> = row.get(7)?;
    Ok(Run {
        id: row.get(0)?,
        scenario_id: row.get(1)?,
        name: row.get(2)?,
        tags: parse_json::<Vec<String>>(&tags_json)?,
        description: row.get(4)?,
        status: parse_run_status(&row.get::<_, String>(5)?),
        started_at: parse_utc(&started_at).map_err(to_sql_error)?,
        stopped_at: stopped_at
            .as_deref()
            .map(parse_utc)
            .transpose()
            .map_err(to_sql_error)?,
        workloads: Vec::new(),
        baseline_of_scenario_id: None,
    })
}

fn map_run_workload(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunWorkload> {
    let kind: String = row.get(3)?;
    Ok(RunWorkload {
        id: row.get(0)?,
        run_id: row.get(1)?,
        workload_id: row.get(2)?,
        kind: parse_workload_kind(&kind),
        config_snapshot_json: row.get(4)?,
    })
}

fn map_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    let ts: String = row.get(3)?;
    let category: String = row.get(4)?;
    Ok(Annotation {
        id: row.get(0)?,
        run_id: row.get(1)?,
        run_workload_id: row.get(2)?,
        ts: parse_utc(&ts).map_err(to_sql_error)?,
        category: parse_annotation_category(&category),
        title: row.get(5)?,
        detail: row.get(6)?,
    })
}

fn parse_json<T: serde::de::DeserializeOwned>(json: &str) -> rusqlite::Result<T> {
    serde_json::from_str(json).map_err(|err| {
        to_sql_error(anyhow!(
            "failed to decode json stored in velamq-bench sqlite: {err}"
        ))
    })
}

fn parse_json_option<T: serde::de::DeserializeOwned>(
    json: Option<String>,
) -> rusqlite::Result<Option<T>> {
    json.as_deref().map(parse_json).transpose()
}

fn run_status_as_str(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Stopped => "stopped",
        RunStatus::Failed => "failed",
    }
}

fn parse_run_status(status: &str) -> RunStatus {
    match status {
        "pending" => RunStatus::Pending,
        "running" => RunStatus::Running,
        "completed" => RunStatus::Completed,
        "stopped" => RunStatus::Stopped,
        "failed" => RunStatus::Failed,
        _ => RunStatus::Failed,
    }
}

fn workload_kind_as_str(kind: &crate::model::WorkloadKind) -> &'static str {
    match kind {
        crate::model::WorkloadKind::Pub => "pub",
        crate::model::WorkloadKind::Sub => "sub",
        crate::model::WorkloadKind::Conn => "conn",
    }
}

fn parse_workload_kind(kind: &str) -> crate::model::WorkloadKind {
    match kind {
        "sub" => crate::model::WorkloadKind::Sub,
        "conn" => crate::model::WorkloadKind::Conn,
        _ => crate::model::WorkloadKind::Pub,
    }
}

fn annotation_category_as_str(category: &AnnotationCategory) -> &'static str {
    match category {
        AnnotationCategory::Manual => "manual",
        AnnotationCategory::BrokerEvent => "broker_event",
        AnnotationCategory::SlaBreach => "sla_breach",
        AnnotationCategory::ConfigChange => "config_change",
    }
}

fn parse_annotation_category(category: &str) -> AnnotationCategory {
    match category {
        "broker_event" => AnnotationCategory::BrokerEvent,
        "sla_breach" => AnnotationCategory::SlaBreach,
        "config_change" => AnnotationCategory::ConfigChange,
        _ => AnnotationCategory::Manual,
    }
}

fn query_specimen_by_id(conn: &Connection, specimen_id: &str) -> Result<Option<BenchSpecimen>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, run_id, name, description, tags_json, config_json, created_at
          FROM bench_specimens
         WHERE id = ?1
        "#,
    )?;
    let mut rows = stmt.query_map(params![specimen_id], map_specimen)?;
    rows.next().transpose().map_err(Into::into)
}

fn query_template_by_id(conn: &Connection, template_id: &str) -> Result<Option<BenchTemplate>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, tags_json, config_json, created_at, updated_at
          FROM bench_templates
         WHERE id = ?1
        "#,
    )?;
    let mut rows = stmt.query_map(params![template_id], map_template)?;
    rows.next().transpose().map_err(Into::into)
}

fn rows_to_snapshots<F>(rows: rusqlite::MappedRows<'_, F>) -> rusqlite::Result<Vec<MetricSnapshot>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<MetricSnapshot>,
{
    let mut snapshots = Vec::new();
    for row in rows {
        snapshots.push(row?);
    }
    Ok(snapshots)
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<BenchRun> {
    let config_json: String = row.get(3)?;
    let started_at: String = row.get(4)?;
    let stopped_at: Option<String> = row.get(5)?;
    let config = serde_json::from_str(&config_json).unwrap_or_default();

    Ok(BenchRun {
        id: row.get(0)?,
        status: row.get(1)?,
        mode: row.get(2)?,
        config,
        started_at: parse_utc(&started_at).map_err(to_sql_error)?,
        stopped_at: stopped_at
            .as_deref()
            .map(parse_utc)
            .transpose()
            .map_err(to_sql_error)?,
        sample_count: row.get::<_, i64>(6)? as u64,
        specimen: map_optional_specimen(row, 7)?,
    })
}

fn map_optional_specimen(
    row: &rusqlite::Row<'_>,
    start: usize,
) -> rusqlite::Result<Option<BenchSpecimen>> {
    let id: Option<String> = row.get(start)?;
    let Some(id) = id else {
        return Ok(None);
    };

    let tags_json: String = row.get(start + 4)?;
    let config_json: String = row.get(start + 5)?;
    let created_at: String = row.get(start + 6)?;
    Ok(Some(BenchSpecimen {
        id,
        run_id: row.get(start + 1)?,
        name: row.get(start + 2)?,
        description: row.get(start + 3)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        config: serde_json::from_str(&config_json).unwrap_or_default(),
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
    }))
}

fn map_specimen(row: &rusqlite::Row<'_>) -> rusqlite::Result<BenchSpecimen> {
    let tags_json: String = row.get(4)?;
    let config_json: String = row.get(5)?;
    let created_at: String = row.get(6)?;

    Ok(BenchSpecimen {
        id: row.get(0)?,
        run_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        config: serde_json::from_str(&config_json).unwrap_or_default(),
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
    })
}

fn map_template(row: &rusqlite::Row<'_>) -> rusqlite::Result<BenchTemplate> {
    let tags_json: String = row.get(3)?;
    let config_json: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    Ok(BenchTemplate {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        config: serde_json::from_str(&config_json).unwrap_or_default(),
        created_at: parse_utc(&created_at).map_err(to_sql_error)?,
        updated_at: parse_utc(&updated_at).map_err(to_sql_error)?,
    })
}

fn map_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<MetricSnapshot> {
    let ts: String = row.get(2)?;
    let ts = chrono::DateTime::parse_from_rfc3339(&ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(MetricSnapshot {
        run_id: row.get(0)?,
        run_workload_id: row.get(1)?,
        ts,
        elapsed_ms: row.get::<_, i64>(3)? as u64,
        connected: row.get::<_, i64>(4)? as u64,
        published: row.get::<_, i64>(5)? as u64,
        received: row.get::<_, i64>(6)? as u64,
        errors: row.get::<_, i64>(7)? as u64,
        publish_rate: row.get(8)?,
        receive_rate: row.get(9)?,
        connect_rate: row.get(10)?,
        error_rate: row.get(11)?,
        latency_count: row.get::<_, i64>(12)? as u64,
        latency_avg_ms: row.get(13)?,
        latency_min_ms: row.get(14)?,
        latency_p50_ms: row.get(15)?,
        latency_p90_ms: row.get(16)?,
        latency_p95_ms: row.get(17)?,
        latency_p99_ms: row.get(18)?,
        latency_p999_ms: row.get(19)?,
        latency_max_ms: row.get(20)?,
    })
}

fn summarize_snapshots(snapshots: &[MetricSnapshot]) -> RunStats {
    let Some(last) = snapshots.last() else {
        return RunStats::default();
    };

    let duration_secs = (last.elapsed_ms as f64 / 1000.0).max(0.001);
    let mut stats = RunStats {
        duration_ms: last.elapsed_ms,
        sample_count: snapshots.len() as u64,
        max_connected: snapshots
            .iter()
            .map(|snapshot| snapshot.connected)
            .max()
            .unwrap_or(0),
        total_published: last.published,
        total_received: last.received,
        total_errors: last.errors,
        avg_publish_rate: last.published as f64 / duration_secs,
        avg_receive_rate: last.received as f64 / duration_secs,
        avg_connect_rate: average(snapshots.iter().map(|snapshot| snapshot.connect_rate)),
        avg_error_rate: last.errors as f64 / duration_secs,
        max_publish_rate: max_f64(snapshots.iter().map(|snapshot| snapshot.publish_rate)),
        max_receive_rate: max_f64(snapshots.iter().map(|snapshot| snapshot.receive_rate)),
        max_connect_rate: max_f64(snapshots.iter().map(|snapshot| snapshot.connect_rate)),
        max_error_rate: max_f64(snapshots.iter().map(|snapshot| snapshot.error_rate)),
        latency_count: last.latency_count,
        ..RunStats::default()
    };

    let mut previous_latency_count = 0_u64;
    let mut weighted_latency_sum = 0.0;
    let mut weighted_latency_count = 0_u64;
    for snapshot in snapshots {
        let delta_count = snapshot
            .latency_count
            .saturating_sub(previous_latency_count);
        previous_latency_count = snapshot.latency_count;

        if delta_count > 0 {
            weighted_latency_sum += snapshot.latency_avg_ms * delta_count as f64;
            weighted_latency_count += delta_count;
            if stats.latency_min_ms == 0.0
                || (snapshot.latency_min_ms > 0.0 && snapshot.latency_min_ms < stats.latency_min_ms)
            {
                stats.latency_min_ms = snapshot.latency_min_ms;
            }
            if snapshot.latency_max_ms > stats.latency_max_ms {
                stats.latency_max_ms = snapshot.latency_max_ms;
            }
            stats.latency_p50_ms = stats.latency_p50_ms.max(snapshot.latency_p50_ms);
            stats.latency_p90_ms = stats.latency_p90_ms.max(snapshot.latency_p90_ms);
            stats.latency_p95_ms = stats.latency_p95_ms.max(snapshot.latency_p95_ms);
            stats.latency_p99_ms = stats.latency_p99_ms.max(snapshot.latency_p99_ms);
            stats.latency_p999_ms = stats.latency_p999_ms.max(snapshot.latency_p999_ms);
        }
    }
    if weighted_latency_count > 0 {
        stats.latency_avg_ms = weighted_latency_sum / weighted_latency_count as f64;
    }

    stats
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    let mut count = 0_u64;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn max_f64(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0, f64::max)
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|err| anyhow!("invalid timestamp {value}: {err}"))?
        .with_timezone(&Utc))
}

fn to_sql_error(error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}
