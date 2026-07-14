use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::model::{AuthConfig, BenchConfig, BenchSpecimen, PayloadKind};

const BACKFILL_ID: &str = "_backfill_runs_v2";

#[derive(Debug)]
struct LegacyRunRow {
    id: String,
    status: String,
    config: BenchConfig,
    started_at: DateTime<Utc>,
    stopped_at: Option<DateTime<Utc>>,
    specimen: Option<LegacySpecimen>,
}

#[derive(Debug)]
struct LegacySpecimen {
    name: String,
    description: String,
    tags: Vec<String>,
}

pub fn backfill_legacy_runs(conn: &mut Connection) -> Result<()> {
    let already: bool = conn
        .query_row(
            "SELECT 1 FROM schema_versions WHERE id = ?1",
            params![BACKFILL_ID],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if already {
        return Ok(());
    }

    let rows = load_legacy_runs(conn)?;
    let tx = conn.transaction()?;
    for row in rows {
        insert_legacy_run_v2(
            &tx,
            &row.id,
            &row.config,
            &row.status,
            row.started_at,
            row.stopped_at,
            row.specimen.as_ref(),
        )?;
        tx.execute(
            r#"
            UPDATE metric_snapshots
               SET run_workload_id = ?1
             WHERE run_id = ?2
               AND run_workload_id IS NULL
            "#,
            params![legacy_run_workload_id(&row.id), row.id],
        )?;
    }
    tx.execute(
        "INSERT INTO schema_versions (id, applied_at) VALUES (?1, ?2)",
        params![BACKFILL_ID, Utc::now().to_rfc3339()],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn insert_legacy_run_v2<M: LegacyMetadata + ?Sized>(
    tx: &Transaction<'_>,
    run_id: &str,
    config: &BenchConfig,
    status: &str,
    started_at: DateTime<Utc>,
    stopped_at: Option<DateTime<Utc>>,
    specimen: Option<&M>,
) -> Result<()> {
    let now = Utc::now();
    let broker_id = ensure_broker_profile(tx, config, now)?;
    let payload_id = ensure_payload_profile(tx, config, now)?;
    let mut workload = config.to_workload(&broker_id, payload_id.as_deref());
    workload.id = legacy_workload_id(run_id);
    let config_snapshot_json = serde_json::to_string(&workload)?;

    let (name, description, tags) = specimen
        .map(|specimen| {
            (
                specimen.legacy_name(),
                specimen.legacy_description(),
                specimen.legacy_tags(),
            )
        })
        .unwrap_or_else(|| (format!("Legacy run {run_id}"), String::new(), Vec::new()));
    let tags_json = serde_json::to_string(&tags)?;

    tx.execute(
        r#"
        INSERT OR IGNORE INTO runs_v2 (
            id, scenario_id, name, tags_json, description, status,
            started_at, stopped_at, legacy_run_id
        )
        VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            run_id,
            name,
            tags_json,
            description,
            status,
            started_at.to_rfc3339(),
            stopped_at.map(|value| value.to_rfc3339()),
            run_id,
        ],
    )?;
    tx.execute(
        r#"
        INSERT OR IGNORE INTO run_workloads (
            id, run_id, workload_id, kind, config_snapshot_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            legacy_run_workload_id(run_id),
            run_id,
            workload.id,
            config.mode.as_str(),
            config_snapshot_json,
        ],
    )?;
    Ok(())
}

pub(super) fn legacy_run_workload_id(run_id: &str) -> String {
    format!("legacy-run-workload-{run_id}")
}

fn legacy_workload_id(run_id: &str) -> String {
    format!("legacy-workload-{run_id}")
}

pub(super) trait LegacyMetadata {
    fn legacy_name(&self) -> String;
    fn legacy_description(&self) -> String;
    fn legacy_tags(&self) -> Vec<String>;
}

impl LegacyMetadata for BenchSpecimen {
    fn legacy_name(&self) -> String {
        self.name.clone()
    }

    fn legacy_description(&self) -> String {
        self.description.clone()
    }

    fn legacy_tags(&self) -> Vec<String> {
        self.tags.clone()
    }
}

impl LegacyMetadata for LegacySpecimen {
    fn legacy_name(&self) -> String {
        self.name.clone()
    }

    fn legacy_description(&self) -> String {
        self.description.clone()
    }

    fn legacy_tags(&self) -> Vec<String> {
        self.tags.clone()
    }
}

fn load_legacy_runs(conn: &Connection) -> Result<Vec<LegacyRunRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT r.id, r.status, r.config_json, r.started_at, r.stopped_at,
               s.name, s.description, s.tags_json
          FROM runs r
     LEFT JOIN bench_specimens s ON s.run_id = r.id
      ORDER BY r.started_at ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        let config_json: String = row.get(2)?;
        let started_at: String = row.get(3)?;
        let stopped_at: Option<String> = row.get(4)?;
        let specimen_name: Option<String> = row.get(5)?;
        let specimen = specimen_name.map(|name| {
            let tags_json = row
                .get::<_, Option<String>>(7)
                .ok()
                .flatten()
                .unwrap_or_else(|| "[]".to_string());
            LegacySpecimen {
                name,
                description: row
                    .get::<_, Option<String>>(6)
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            }
        });

        Ok(LegacyRunRow {
            id: row.get(0)?,
            status: row.get(1)?,
            config: serde_json::from_str(&config_json).unwrap_or_default(),
            started_at: parse_utc_lossy(&started_at),
            stopped_at: stopped_at.as_deref().map(parse_utc_lossy),
            specimen,
        })
    })?;

    let mut runs = Vec::new();
    for row in rows {
        runs.push(row?);
    }
    Ok(runs)
}

fn ensure_broker_profile(
    tx: &Transaction<'_>,
    config: &BenchConfig,
    now: DateTime<Utc>,
) -> Result<String> {
    let auth = config
        .username
        .as_ref()
        .map(|username| AuthConfig::UserPassword {
            username: username.clone(),
            password: config.password.clone().unwrap_or_default(),
        });
    let auth_json = auth.as_ref().map(serde_json::to_string).transpose()?;
    let key = format!(
        "{}:{}:{}:{}",
        config.host,
        config.port,
        config.keepalive_secs,
        auth_json.as_deref().unwrap_or("none")
    );
    let id = stable_id("broker", &key);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO broker_profiles (
            id, name, protocol, host, port, websocket_path, tls_json, auth_json,
            keepalive_secs, clean_session, created_at, updated_at
        )
        VALUES (?1, ?2, 'mqtt', ?3, ?4, NULL, NULL, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            id,
            format!("{}:{}", config.host, config.port),
            config.host,
            config.port as i64,
            auth_json,
            config.keepalive_secs as i64,
            if config.clean_session { 1 } else { 0 },
            now.to_rfc3339(),
            now.to_rfc3339(),
        ],
    )?;
    Ok(id)
}

fn ensure_payload_profile(
    tx: &Transaction<'_>,
    config: &BenchConfig,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    if !matches!(config.mode, crate::model::BenchMode::Pub) {
        return Ok(None);
    }

    let kind = PayloadKind::FixedBytes {
        size: config.payload_size,
        with_timestamp: config.payload_timestamp,
    };
    let kind_json = serde_json::to_string(&kind)?;
    let id = stable_id("payload", &kind_json);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO payload_profiles (
            id, name, kind_json, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            id,
            format!(
                "{} bytes{}",
                config.payload_size,
                if config.payload_timestamp {
                    " + timestamp"
                } else {
                    ""
                }
            ),
            kind_json,
            now.to_rfc3339(),
            now.to_rfc3339(),
        ],
    )?;
    Ok(Some(id))
}

fn stable_id(prefix: &str, value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

fn parse_utc_lossy(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::run_migrations;

    #[test]
    fn backfills_legacy_runs_into_v2_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let now = Utc::now();

        let mut pub_config = BenchConfig::default();
        pub_config.host = "127.0.0.1".to_string();
        pub_config.payload_size = 512;
        let mut sub_config = BenchConfig::default();
        sub_config.mode = crate::model::BenchMode::Sub;
        let conn_config = BenchConfig {
            mode: crate::model::BenchMode::Conn,
            ..BenchConfig::default()
        };

        for (idx, config) in [pub_config, sub_config, conn_config]
            .into_iter()
            .enumerate()
        {
            let run_id = format!("run-{idx}");
            conn.execute(
                r#"
                INSERT INTO runs (id, status, mode, config_json, started_at)
                VALUES (?1, 'completed', ?2, ?3, ?4)
                "#,
                params![
                    run_id,
                    config.mode.as_str(),
                    serde_json::to_string(&config).unwrap(),
                    now.to_rfc3339(),
                ],
            )
            .unwrap();
            conn.execute(
                r#"
                INSERT INTO metric_snapshots (
                    run_id, ts, elapsed_ms, connected, published, received, errors,
                    publish_rate, receive_rate, connect_rate, error_rate
                )
                VALUES (?1, ?2, 1, 1, 1, 1, 0, 1, 1, 1, 0)
                "#,
                params![run_id, now.to_rfc3339()],
            )
            .unwrap();
        }

        backfill_legacy_runs(&mut conn).unwrap();

        let runs_v2: i64 = conn
            .query_row("SELECT COUNT(*) FROM runs_v2", [], |row| row.get(0))
            .unwrap();
        let run_workloads: i64 = conn
            .query_row("SELECT COUNT(*) FROM run_workloads", [], |row| row.get(0))
            .unwrap();
        let broker_profiles: i64 = conn
            .query_row("SELECT COUNT(*) FROM broker_profiles", [], |row| row.get(0))
            .unwrap();
        let payload_profiles: i64 = conn
            .query_row("SELECT COUNT(*) FROM payload_profiles", [], |row| {
                row.get(0)
            })
            .unwrap();
        let snapshots_with_workload: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM metric_snapshots WHERE run_workload_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(runs_v2, 3);
        assert_eq!(run_workloads, 3);
        assert_eq!(broker_profiles, 1);
        assert_eq!(payload_profiles, 1);
        assert_eq!(snapshots_with_workload, 3);
    }
}
