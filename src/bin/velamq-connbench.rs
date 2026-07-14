use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use rumqttc::{AsyncClient, Event as MqttEvent, MqttOptions, Packet, QoS};
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior, sleep_until, timeout};

#[derive(Debug, Clone)]
struct Config {
    host: String,
    port: u16,
    clients: u64,
    start_index: u64,
    client_prefix: String,
    connect_rate_per_sec: f64,
    connect_timeout_ms: u64,
    hold_secs: u64,
    keepalive_secs: u64,
    clean_session: bool,
    username: Option<String>,
    password: Option<String>,
    max_in_flight_spawns: usize,
    publish_interval_ms: u64,
    payload_bytes: usize,
    topic: String,
    qos: u8,
    progress_secs: u64,
    report_json: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct LatencySummary {
    count: usize,
    min_ms: Option<u64>,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    p99_ms: Option<u64>,
    max_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ErrorSummary {
    message: String,
    count: u64,
}

#[derive(Debug, Serialize)]
struct Report {
    target: String,
    clients: u64,
    attempted: u64,
    connected: u64,
    failed: u64,
    disconnected: u64,
    active_at_end: u64,
    published: u64,
    publish_failed: u64,
    connect_rate_per_sec: f64,
    effective_connects_per_sec: f64,
    connect_window_ms: u64,
    elapsed_ms: u64,
    hold_secs: u64,
    connect_latency: LatencySummary,
    errors: Vec<ErrorSummary>,
    started_at_unix_ms: u128,
}

#[derive(Default)]
struct Shared {
    attempted: AtomicU64,
    connected: AtomicU64,
    failed: AtomicU64,
    disconnected: AtomicU64,
    active: AtomicU64,
    published: AtomicU64,
    publish_failed: AtomicU64,
    latencies_ms: Mutex<Vec<u64>>,
    errors: Mutex<BTreeMap<String, u64>>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let config = Config::parse()?;
    let started_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = Instant::now();
    let shared = Arc::new(Shared::default());
    let spawn_limit = Arc::new(Semaphore::new(config.max_in_flight_spawns.max(1)));
    let progress_handle = spawn_progress_logger(config.clone(), Arc::clone(&shared));

    let mut joins = JoinSet::new();
    let connect_started = Instant::now();
    for offset in 0..config.clients {
        sleep_until(next_spawn_deadline(
            connect_started,
            offset,
            config.connect_rate_per_sec,
        ))
        .await;
        let permit = Arc::clone(&spawn_limit)
            .acquire_owned()
            .await
            .context("acquire spawn permit")?;
        let cfg = config.clone();
        let shared = Arc::clone(&shared);
        joins.spawn(async move {
            let _permit = permit;
            run_connection(cfg.start_index + offset, cfg, shared).await;
        });
    }
    let connect_window_ms = connect_started.elapsed().as_millis() as u64;

    while let Some(result) = joins.join_next().await {
        if let Err(error) = result {
            shared.failed.fetch_add(1, Ordering::Relaxed);
            record_error(&shared, format!("task join error: {error}")).await;
        }
    }
    progress_handle.abort();

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let connected = shared.connected.load(Ordering::Relaxed);
    let report = Report {
        target: format!("{}:{}", config.host, config.port),
        clients: config.clients,
        attempted: shared.attempted.load(Ordering::Relaxed),
        connected,
        failed: shared.failed.load(Ordering::Relaxed),
        disconnected: shared.disconnected.load(Ordering::Relaxed),
        active_at_end: shared.active.load(Ordering::Relaxed),
        published: shared.published.load(Ordering::Relaxed),
        publish_failed: shared.publish_failed.load(Ordering::Relaxed),
        connect_rate_per_sec: config.connect_rate_per_sec,
        effective_connects_per_sec: if connect_window_ms == 0 {
            connected as f64
        } else {
            connected as f64 / (connect_window_ms as f64 / 1000.0)
        },
        connect_window_ms,
        elapsed_ms,
        hold_secs: config.hold_secs,
        connect_latency: latency_summary(shared.latencies_ms.lock().await.clone()),
        errors: shared
            .errors
            .lock()
            .await
            .iter()
            .map(|(message, count)| ErrorSummary {
                message: message.clone(),
                count: *count,
            })
            .collect(),
        started_at_unix_ms,
    };

    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = &config.report_json {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create report dir {}", parent.display()))?;
        }
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("write report {}", path.display()))?;
    }
    println!("{json}");
    Ok(())
}

async fn run_connection(index: u64, config: Config, shared: Arc<Shared>) {
    shared.attempted.fetch_add(1, Ordering::Relaxed);
    let client_id = format!("{}-{}", config.client_prefix, index);
    let mut options = MqttOptions::new(client_id.clone(), config.host.clone(), config.port);
    options.set_keep_alive(Duration::from_secs(config.keepalive_secs.max(1)));
    options.set_clean_session(config.clean_session);
    if let Some(username) = config.username.as_deref().filter(|value| !value.is_empty()) {
        options.set_credentials(username, config.password.clone().unwrap_or_default());
    }

    let (client, mut eventloop) = AsyncClient::new(options, 10);
    let connect_started = Instant::now();
    let connect_timeout = Duration::from_millis(config.connect_timeout_ms.max(1));
    loop {
        match timeout(connect_timeout, eventloop.poll()).await {
            Ok(Ok(MqttEvent::Incoming(Packet::ConnAck(_)))) => {
                shared.connected.fetch_add(1, Ordering::Relaxed);
                shared.active.fetch_add(1, Ordering::Relaxed);
                shared
                    .latencies_ms
                    .lock()
                    .await
                    .push(connect_started.elapsed().as_millis() as u64);
                break;
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                shared.failed.fetch_add(1, Ordering::Relaxed);
                record_error(&shared, normalize_error(error.to_string())).await;
                return;
            }
            Err(_) => {
                shared.failed.fetch_add(1, Ordering::Relaxed);
                record_error(
                    &shared,
                    format!("connect timeout after {}ms", config.connect_timeout_ms),
                )
                .await;
                return;
            }
        }
    }

    let hold_deadline = Instant::now() + Duration::from_secs(config.hold_secs);
    if config.publish_interval_ms == 0 {
        poll_until_deadline(&mut eventloop, hold_deadline, &shared).await;
    } else {
        publish_until_deadline(
            &client,
            &mut eventloop,
            hold_deadline,
            &config,
            index,
            &shared,
        )
        .await;
    }

    let _ = client.disconnect().await;
    shared.active.fetch_sub(1, Ordering::Relaxed);
    shared.disconnected.fetch_add(1, Ordering::Relaxed);
}

async fn poll_until_deadline(
    eventloop: &mut rumqttc::EventLoop,
    deadline: Instant,
    shared: &Arc<Shared>,
) {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        match timeout(deadline - now, eventloop.poll()).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                record_error(shared, normalize_error(error.to_string())).await;
                return;
            }
            Err(_) => return,
        }
    }
}

async fn publish_until_deadline(
    client: &AsyncClient,
    eventloop: &mut rumqttc::EventLoop,
    deadline: Instant,
    config: &Config,
    index: u64,
    shared: &Arc<Shared>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(config.publish_interval_ms));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let qos = match config.qos {
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    };
    let payload = vec![b'x'; config.payload_bytes];
    let topic = config
        .topic
        .replace("{client}", &index.to_string())
        .replace(
            "{client_id}",
            &format!("{}-{}", config.client_prefix, index),
        );

    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        tokio::select! {
            event = eventloop.poll() => {
                if let Err(error) = event {
                    record_error(shared, normalize_error(error.to_string())).await;
                    return;
                }
            }
            _ = interval.tick() => {
                match client.publish(topic.clone(), qos, false, payload.clone()).await {
                    Ok(()) => {
                        shared.published.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        shared.publish_failed.fetch_add(1, Ordering::Relaxed);
                        record_error(shared, normalize_error(format!("publish: {error}"))).await;
                    }
                }
            }
            _ = sleep_until(deadline) => return,
        }
    }
}

async fn record_error(shared: &Arc<Shared>, message: String) {
    let mut errors = shared.errors.lock().await;
    *errors.entry(message).or_default() += 1;
}

fn normalize_error(message: String) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("connection refused") {
        "connection refused".to_string()
    } else if lower.contains("quota exceeded") || lower.contains("limit reached") {
        "quota exceeded".to_string()
    } else if lower.contains("i/o") || lower.contains("broken pipe") {
        "io error".to_string()
    } else {
        message
    }
}

fn next_spawn_deadline(start: Instant, offset: u64, rate: f64) -> Instant {
    if rate <= 0.0 {
        return start;
    }
    start + Duration::from_secs_f64(offset as f64 / rate)
}

fn latency_summary(mut values: Vec<u64>) -> LatencySummary {
    values.sort_unstable();
    let count = values.len();
    LatencySummary {
        count,
        min_ms: values.first().copied(),
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        p99_ms: percentile(&values, 0.99),
        max_ms: values.last().copied(),
    }
}

fn percentile(values: &[u64], q: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values.get(idx).copied()
}

fn spawn_progress_logger(config: Config, shared: Arc<Shared>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if config.progress_secs == 0 {
            return;
        }
        let mut interval = tokio::time::interval(Duration::from_secs(config.progress_secs));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            eprintln!(
                "progress target={}:{} attempted={} connected={} failed={} active={} published={}",
                config.host,
                config.port,
                shared.attempted.load(Ordering::Relaxed),
                shared.connected.load(Ordering::Relaxed),
                shared.failed.load(Ordering::Relaxed),
                shared.active.load(Ordering::Relaxed),
                shared.published.load(Ordering::Relaxed),
            );
        }
    })
}

impl Config {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1).collect::<Vec<_>>();
        if args.iter().any(|arg| arg == "-h" || arg == "--help") {
            print_help();
            std::process::exit(0);
        }

        let mut config = Self {
            host: "127.0.0.1".to_string(),
            port: 1883,
            clients: 1000,
            start_index: 0,
            client_prefix: "velamq-connbench".to_string(),
            connect_rate_per_sec: 500.0,
            connect_timeout_ms: 10_000,
            hold_secs: 30,
            keepalive_secs: 30,
            clean_session: true,
            username: None,
            password: None,
            max_in_flight_spawns: 4096,
            publish_interval_ms: 0,
            payload_bytes: 0,
            topic: "velamq/bench/{client_id}".to_string(),
            qos: 0,
            progress_secs: 5,
            report_json: None,
        };

        while !args.is_empty() {
            let key = args.remove(0);
            match key.as_str() {
                "--target" => {
                    let value = take_value(&mut args, "--target")?;
                    let (host, port) = parse_target(&value)?;
                    config.host = host;
                    config.port = port;
                }
                "--host" => config.host = take_value(&mut args, "--host")?,
                "--port" => config.port = parse_value(&take_value(&mut args, "--port")?, "--port")?,
                "--clients" => {
                    config.clients = parse_value(&take_value(&mut args, "--clients")?, "--clients")?
                }
                "--start-index" => {
                    config.start_index =
                        parse_value(&take_value(&mut args, "--start-index")?, "--start-index")?
                }
                "--client-prefix" => {
                    config.client_prefix = take_value(&mut args, "--client-prefix")?
                }
                "--connect-rate" | "--connect-rate-per-sec" => {
                    config.connect_rate_per_sec =
                        parse_value(&take_value(&mut args, "--connect-rate")?, "--connect-rate")?
                }
                "--connect-timeout-ms" => {
                    config.connect_timeout_ms = parse_value(
                        &take_value(&mut args, "--connect-timeout-ms")?,
                        "--connect-timeout-ms",
                    )?
                }
                "--hold-secs" => {
                    config.hold_secs =
                        parse_value(&take_value(&mut args, "--hold-secs")?, "--hold-secs")?
                }
                "--keepalive-secs" => {
                    config.keepalive_secs = parse_value(
                        &take_value(&mut args, "--keepalive-secs")?,
                        "--keepalive-secs",
                    )?
                }
                "--clean-session" => {
                    config.clean_session = parse_bool(
                        &take_value(&mut args, "--clean-session")?,
                        "--clean-session",
                    )?
                }
                "--username" => config.username = Some(take_value(&mut args, "--username")?),
                "--password" => config.password = Some(take_value(&mut args, "--password")?),
                "--max-in-flight-spawns" => {
                    config.max_in_flight_spawns = parse_value(
                        &take_value(&mut args, "--max-in-flight-spawns")?,
                        "--max-in-flight-spawns",
                    )?
                }
                "--publish-interval-ms" => {
                    config.publish_interval_ms = parse_value(
                        &take_value(&mut args, "--publish-interval-ms")?,
                        "--publish-interval-ms",
                    )?
                }
                "--payload-bytes" => {
                    config.payload_bytes = parse_value(
                        &take_value(&mut args, "--payload-bytes")?,
                        "--payload-bytes",
                    )?
                }
                "--topic" => config.topic = take_value(&mut args, "--topic")?,
                "--qos" => config.qos = parse_value(&take_value(&mut args, "--qos")?, "--qos")?,
                "--progress-secs" => {
                    config.progress_secs = parse_value(
                        &take_value(&mut args, "--progress-secs")?,
                        "--progress-secs",
                    )?
                }
                "--report-json" => {
                    config.report_json =
                        Some(PathBuf::from(take_value(&mut args, "--report-json")?))
                }
                other => bail!("unknown argument {other}; use --help"),
            }
        }

        if config.clients == 0 {
            bail!("--clients must be greater than 0");
        }
        if config.connect_rate_per_sec < 0.0 {
            bail!("--connect-rate must be >= 0");
        }
        if config.qos > 2 {
            bail!("--qos must be 0, 1, or 2");
        }
        Ok(config)
    }
}

fn take_value(args: &mut Vec<String>, key: &str) -> Result<String> {
    if args.is_empty() {
        bail!("{key} requires a value");
    }
    Ok(args.remove(0))
}

fn parse_value<T>(value: &str, key: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| anyhow!("{key} invalid value {value:?}: {error}"))
}

fn parse_bool(value: &str, key: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("{key} expects true/false"),
    }
}

fn parse_target(value: &str) -> Result<(String, u16)> {
    let Some((host, port)) = value.rsplit_once(':') else {
        bail!("--target must be host:port");
    };
    Ok((
        host.trim_matches(['[', ']']).to_string(),
        parse_value(port, "--target port")?,
    ))
}

fn print_help() {
    println!(
        r#"Usage:
  velamq-connbench --target 127.0.0.1:1883 --clients 10000 --connect-rate 2000 --hold-secs 120 --report-json report.json

Options:
  --target HOST:PORT              MQTT target. Overrides --host and --port.
  --host HOST                     MQTT host. Default: 127.0.0.1
  --port PORT                     MQTT port. Default: 1883
  --clients N                     MQTT clients in this process.
  --start-index N                 First client index for unique IDs.
  --client-prefix TEXT            Client ID prefix.
  --connect-rate N                Connection attempts per second for this process. 0 means no throttle.
  --connect-timeout-ms N          Per-client CONNECT timeout.
  --hold-secs N                   Hold connected sessions before disconnecting.
  --keepalive-secs N              MQTT keepalive.
  --clean-session true|false      MQTT clean session flag.
  --username USER                 Optional username.
  --password PASS                 Optional password.
  --max-in-flight-spawns N        Backpressure for active task spawning.
  --publish-interval-ms N         Optional publish interval after connect. 0 disables publish.
  --payload-bytes N               Payload bytes for optional publish.
  --topic TEMPLATE                Publish topic. Supports {{client}} and {{client_id}}.
  --qos 0|1|2                     Publish QoS.
  --progress-secs N               stderr progress interval. 0 disables progress.
  --report-json FILE              Write final JSON report to file.
"#
    );
}
