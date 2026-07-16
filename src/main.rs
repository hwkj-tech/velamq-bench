use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    io::{Cursor, Read, Write},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{FromRef, Path, Query, Request, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
    routing::{any, get, patch, post, put},
};
use chrono::Utc;
use futures_util::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

use velamq_bench::{
    bench::BenchManager,
    cluster::{AGENT_PROTOCOL_VERSION, ClusterManager},
    export::{report_to_pdf, report_to_svg},
    model::{
        AgentHeartbeat, AgentLogBatch, AgentMetricBatch, AgentNodeUpdate, AgentRegistration,
        AgentTaskAck, AgentTaskComplete, AgentTaskControl, AgentTaskCreate, Annotation, ApiError,
        BenchEvent, BenchReport, BrokerProfile, DistributedMetrics, DistributedRunCreate,
        MetricSnapshot, PayloadProfile, Scenario, ScenarioStage, SpecimenUpdate, StartBenchRequest,
        TemplateDraft, Workload, normalize_websocket_path,
    },
    runtime::sse::RunEvent,
    storage::Storage,
};

#[derive(Clone)]
struct AppState {
    manager: Arc<BenchManager>,
    cluster: Arc<ClusterManager>,
}

impl FromRef<AppState> for Arc<BenchManager> {
    fn from_ref(state: &AppState) -> Self {
        Arc::clone(&state.manager)
    }
}

impl FromRef<AppState> for Arc<ClusterManager> {
    fn from_ref(state: &AppState) -> Self {
        Arc::clone(&state.cluster)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    velamq_bench::install_crypto_provider();
    tracing_subscriber::fmt::init();

    let runtime_root = runtime_root();
    let data_path = std::env::var_os("VELAMQ_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_root.join("data"))
        .join("velamq-bench.sqlite3");
    let storage = Storage::new(data_path).await?;
    let manager = BenchManager::new(storage.clone());
    let cluster = ClusterManager::new(storage);
    let app_state = AppState { manager, cluster };

    let packaged_web_root = runtime_root.join("web");
    let web_root = std::env::var_os("VELAMQ_WEB_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            packaged_web_root
                .join("dist/index.html")
                .exists()
                .then_some(packaged_web_root)
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web"));
    let dist_dir = web_root.join("dist");
    let index = dist_dir.join("index.html");
    let static_files = ServeDir::new(dist_dir).fallback(ServeFile::new(index));

    let legacy_routes = Router::new()
        .route("/api/bench/start", post(start_bench))
        .route("/api/bench/stop", post(stop_bench))
        .route("/api/bench/state", get(bench_state))
        .route("/api/bench/interfaces", get(bench_interfaces))
        .route("/api/bench/history", get(bench_history))
        .route("/api/bench/runs", get(bench_runs))
        .route("/api/bench/report", get(bench_report))
        .route("/api/bench/report.csv", get(bench_report_csv))
        .route("/api/bench/report.svg", get(bench_report_svg))
        .route("/api/bench/report.pdf", get(bench_report_pdf))
        .route("/api/bench/specimens", get(bench_specimens))
        .route(
            "/api/bench/specimens/{id}",
            patch(bench_specimen_update).delete(bench_specimen_delete),
        )
        .route(
            "/api/bench/templates",
            get(bench_templates).post(bench_template_create),
        )
        .route(
            "/api/bench/templates/{id}",
            put(bench_template_update).delete(bench_template_delete),
        )
        .route("/api/bench/events", get(bench_events))
        .route_layer(middleware::from_fn(add_deprecation_headers));

    let app = Router::new()
        .merge(legacy_routes)
        .route(
            "/api/v2/broker-profiles",
            get(v2_brokers).post(v2_broker_create),
        )
        .route(
            "/api/v2/broker-profiles/{id}",
            get(v2_broker_get)
                .patch(v2_broker_update)
                .delete(v2_broker_delete),
        )
        .route(
            "/api/v2/broker-profiles/{id}/test-connection",
            post(v2_broker_test_connection),
        )
        .route("/api/v2/agents", get(v2_agents))
        .route("/api/v2/agents/register", post(v2_agent_register))
        .route(
            "/api/v2/agents/{id}",
            get(v2_agent_get)
                .patch(v2_agent_update)
                .delete(v2_agent_delete),
        )
        .route("/api/v2/agents/{id}/heartbeat", post(v2_agent_heartbeat))
        .route("/api/v2/agents/{id}/tasks/next", get(v2_agent_next_task))
        .route(
            "/api/v2/agent-tasks",
            get(v2_agent_tasks).post(v2_agent_task_create),
        )
        .route("/api/v2/agent-tasks/{id}", get(v2_agent_task_get))
        .route("/api/v2/agent-tasks/{id}/ack", post(v2_agent_task_ack))
        .route(
            "/api/v2/agent-tasks/{id}/metrics",
            post(v2_agent_task_metrics),
        )
        .route(
            "/api/v2/agent-tasks/{id}/logs",
            get(v2_agent_task_logs).post(v2_agent_task_log_upload),
        )
        .route(
            "/api/v2/agent-tasks/{id}/complete",
            post(v2_agent_task_complete),
        )
        .route(
            "/api/v2/agent-tasks/{id}/control",
            get(v2_agent_task_control),
        )
        .route("/api/v2/agent-tasks/{id}/stop", post(v2_agent_task_stop))
        .route(
            "/api/v2/distributed-runs",
            get(v2_distributed_runs).post(v2_distributed_run_create),
        )
        .route("/api/v2/distributed-runs/{id}", get(v2_distributed_run_get))
        .route(
            "/api/v2/distributed-runs/{id}/stop",
            post(v2_distributed_run_stop),
        )
        .route(
            "/api/v2/distributed-runs/{id}/metrics",
            get(v2_distributed_run_metrics),
        )
        .route(
            "/api/v2/distributed-runs/{id}/report.csv",
            get(v2_distributed_run_report_csv),
        )
        .route(
            "/api/v2/payload-profiles",
            get(v2_payloads).post(v2_payload_create),
        )
        .route(
            "/api/v2/payload-profiles/{id}",
            get(v2_payload_get)
                .patch(v2_payload_update)
                .delete(v2_payload_delete),
        )
        .route(
            "/api/v2/scenarios",
            get(v2_scenarios).post(v2_scenario_create),
        )
        .route(
            "/api/v2/scenarios/{id}",
            get(v2_scenario_get)
                .patch(v2_scenario_update)
                .delete(v2_scenario_delete),
        )
        .route("/api/v2/scenarios/{id}/run", post(v2_scenario_run))
        .route(
            "/api/v2/scenarios/{id}/baseline",
            post(v2_scenario_baseline),
        )
        .route("/api/v2/runs", get(v2_runs).post(v2_run_create))
        .route(
            "/api/v2/runs/{id}",
            get(v2_run_get).patch(v2_run_update).delete(v2_run_delete),
        )
        .route("/api/v2/runs/{id}/stop", post(v2_run_stop))
        .route("/api/v2/runs/{id}/snapshots", get(v2_run_snapshots))
        .route("/api/v2/runs/{id}/report", get(v2_run_report))
        .route("/api/v2/runs/{id}/report.svg", get(v2_run_report_svg))
        .route("/api/v2/runs/{id}/report.pdf", get(v2_run_report_pdf))
        .route("/api/v2/runs/{id}/report.csv", get(v2_run_report_csv))
        .route("/api/v2/runs/{id}/events", get(v2_run_events))
        .route(
            "/api/v2/runs/{id}/annotations",
            get(v2_annotations).post(v2_annotation_create),
        )
        .route("/api/v2/network-interfaces", get(v2_network_interfaces))
        .route("/api/v2/runtime/state", get(v2_runtime_state))
        .route("/api/v2/templates", get(v2_templates))
        .route("/api/v2/bundles/export", post(v2_bundle_export))
        .route("/api/v2/bundles/import", post(v2_bundle_import))
        .route("/api/{*path}", any(api_not_found))
        .fallback_service(static_files)
        .with_state(app_state);

    let bind = std::env::var("VELAMQ_BIND").unwrap_or_else(|_| "127.0.0.1:8088".to_string());
    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("velamq-bench listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn runtime_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

async fn add_deprecation_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("deprecation"),
        HeaderValue::from_static("true"),
    );
    headers.insert(
        HeaderName::from_static("sunset"),
        HeaderValue::from_static("Sat, 01 Aug 2026 00:00:00 GMT"),
    );
    headers.insert(
        HeaderName::from_static("link"),
        HeaderValue::from_static("</api/v2/runs>; rel=\"successor-version\""),
    );
    response
}

async fn start_bench(
    State(manager): State<Arc<BenchManager>>,
    Json(request): Json<StartBenchRequest>,
) -> impl IntoResponse {
    match manager.start(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

async fn stop_bench(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    match manager.stop().await {
        Ok(state) => (StatusCode::OK, Json(state)).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

async fn bench_state(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    Json(manager.state().await)
}

async fn bench_interfaces(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    match manager.interfaces().await {
        Ok(interfaces) => (StatusCode::OK, Json(interfaces)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load interfaces: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    run_id: Option<String>,
    limit: Option<usize>,
}

async fn bench_history(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    match manager
        .history(query.run_id, query.limit.unwrap_or(600))
        .await
    {
        Ok(history) => (StatusCode::OK, Json(history)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load history: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    limit: Option<usize>,
}

async fn bench_runs(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    match manager.runs(query.limit.unwrap_or(50)).await {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load runs: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ReportQuery {
    run_id: String,
    lang: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReportImageQuery {
    lang: Option<String>,
}

async fn bench_report(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<ReportQuery>,
) -> impl IntoResponse {
    match manager.report(&query.run_id).await {
        Ok(Some(report)) => (StatusCode::OK, Json(report)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load report: {err:#}"),
        ),
    }
}

async fn bench_specimens(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    match manager.specimens(query.limit.unwrap_or(80)).await {
        Ok(specimens) => (StatusCode::OK, Json(specimens)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load specimens: {err:#}"),
        ),
    }
}

async fn bench_specimen_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(update): Json<SpecimenUpdate>,
) -> impl IntoResponse {
    match manager.update_specimen(&id, update).await {
        Ok(Some(specimen)) => (StatusCode::OK, Json(specimen)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "specimen not found".to_string()),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

async fn bench_specimen_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_specimen(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "specimen not found".to_string()),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn bench_templates(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    match manager.templates(query.limit.unwrap_or(80)).await {
        Ok(templates) => (StatusCode::OK, Json(templates)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load templates: {err:#}"),
        ),
    }
}

async fn v2_templates(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<RunsQuery>,
) -> impl IntoResponse {
    match manager.templates(query.limit.unwrap_or(80)).await {
        Ok(templates) => (StatusCode::OK, Json(templates)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load templates: {err:#}"),
        ),
    }
}

async fn bench_template_create(
    State(manager): State<Arc<BenchManager>>,
    Json(draft): Json<TemplateDraft>,
) -> impl IntoResponse {
    match manager.create_template(draft).await {
        Ok(template) => (StatusCode::CREATED, Json(template)).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

async fn bench_template_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(draft): Json<TemplateDraft>,
) -> impl IntoResponse {
    match manager.update_template(&id, draft).await {
        Ok(Some(template)) => (StatusCode::OK, Json(template)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "template not found".to_string()),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

async fn bench_template_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_template(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "template not found".to_string()),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn bench_report_csv(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<ReportQuery>,
) -> impl IntoResponse {
    match manager.report(&query.run_id).await {
        Ok(Some(report)) => {
            let csv = report_to_csv(&report);
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/csv; charset=utf-8"),
            );
            let disposition = format!(
                "attachment; filename=\"velamq-bench-{}.csv\"",
                report.run.id
            );
            if let Ok(value) = HeaderValue::from_str(&disposition) {
                headers.insert(CONTENT_DISPOSITION, value);
            }
            (headers, csv).into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to export report: {err:#}"),
        ),
    }
}

async fn bench_report_svg(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<ReportQuery>,
) -> impl IntoResponse {
    match manager.report(&query.run_id).await {
        Ok(Some(report)) => {
            let svg = report_to_svg(&report, query.lang.as_deref().unwrap_or("en"));
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            );
            let disposition = format!(
                "attachment; filename=\"velamq-bench-{}.svg\"",
                report.run.id
            );
            if let Ok(value) = HeaderValue::from_str(&disposition) {
                headers.insert(CONTENT_DISPOSITION, value);
            }
            (headers, svg).into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to export svg: {err:#}"),
        ),
    }
}

async fn bench_report_pdf(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<ReportQuery>,
) -> impl IntoResponse {
    match manager.report(&query.run_id).await {
        Ok(Some(report)) => match report_to_pdf(&report, query.lang.as_deref().unwrap_or("en")) {
            Ok(pdf) => {
                let mut headers = HeaderMap::new();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
                let disposition = format!(
                    "attachment; filename=\"velamq-bench-{}.pdf\"",
                    report.run.id
                );
                if let Ok(value) = HeaderValue::from_str(&disposition) {
                    headers.insert(CONTENT_DISPOSITION, value);
                }
                (headers, pdf).into_response()
            }
            Err(err) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to render pdf: {err:#}"),
            ),
        },
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load report for pdf: {err:#}"),
        ),
    }
}

async fn bench_events(
    State(manager): State<Arc<BenchManager>>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = BroadcastStream::new(manager.subscribe()).filter_map(|event| async move {
        match event {
            Ok(event) => Some(Ok(sse_event(event))),
            Err(BroadcastStreamRecvError::Lagged(skipped)) => Some(Ok(SseEvent::default()
                .event("lagged")
                .data(format!(r#"{{"skipped":{skipped}}}"#)))),
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn v2_brokers(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    match manager.broker_profiles().await {
        Ok(profiles) => (StatusCode::OK, Json(profiles)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list broker profiles: {err:#}"),
        ),
    }
}

async fn v2_agents(State(cluster): State<Arc<ClusterManager>>) -> impl IntoResponse {
    match cluster.nodes().await {
        Ok(nodes) => (StatusCode::OK, Json(nodes)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list agent nodes: {err:#}"),
        ),
    }
}

async fn v2_agent_get(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.node(&id).await {
        Ok(Some(node)) => (StatusCode::OK, Json(node)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent node not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load agent node: {err:#}"),
        ),
    }
}

async fn v2_agent_register(
    State(cluster): State<Arc<ClusterManager>>,
    headers: HeaderMap,
    Json(registration): Json<AgentRegistration>,
) -> impl IntoResponse {
    if !cluster.registration_enabled() {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "agent registration is disabled; configure VELAMQ_BENCH_AGENT_BOOTSTRAP_TOKEN"
                .to_string(),
        );
    }
    let Some(token) = bearer_token(&headers) else {
        return api_error(StatusCode::UNAUTHORIZED, "missing bearer token".to_string());
    };
    match cluster.register(token, registration).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(err) if err.to_string().contains("bootstrap token") => {
            api_error(StatusCode::UNAUTHORIZED, err.to_string())
        }
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_agent_heartbeat(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(heartbeat): Json<AgentHeartbeat>,
) -> impl IntoResponse {
    if !agent_protocol_supported(&headers) {
        return api_error(
            StatusCode::UPGRADE_REQUIRED,
            format!("unsupported agent protocol; expected {AGENT_PROTOCOL_VERSION}"),
        );
    }
    let Some(token) = bearer_token(&headers) else {
        return api_error(StatusCode::UNAUTHORIZED, "missing agent token".to_string());
    };
    if let Err(err) = cluster.authenticate(&id, token).await {
        return api_error(StatusCode::UNAUTHORIZED, err.to_string());
    }
    match cluster.heartbeat(&id, heartbeat).await {
        Ok(Some(node)) => (StatusCode::OK, Json(node)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent node not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to record agent heartbeat: {err:#}"),
        ),
    }
}

async fn v2_agent_update(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
    Json(update): Json<AgentNodeUpdate>,
) -> impl IntoResponse {
    match cluster.update_node(&id, update).await {
        Ok(Some(node)) => (StatusCode::OK, Json(node)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent node not found".to_string()),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_agent_delete(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.delete_node(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "agent node not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete agent node: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct AgentTaskQuery {
    node_id: Option<String>,
}

async fn v2_agent_tasks(
    State(cluster): State<Arc<ClusterManager>>,
    Query(query): Query<AgentTaskQuery>,
) -> impl IntoResponse {
    match cluster.tasks(query.node_id).await {
        Ok(tasks) => (StatusCode::OK, Json(tasks)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list agent tasks: {err:#}"),
        ),
    }
}

async fn v2_agent_task_create(
    State(cluster): State<Arc<ClusterManager>>,
    Json(request): Json<AgentTaskCreate>,
) -> impl IntoResponse {
    match cluster.create_task(request).await {
        Ok(task) => (StatusCode::CREATED, Json(task)).into_response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_agent_task_get(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.task(&id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent task not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load agent task: {err:#}"),
        ),
    }
}

async fn v2_agent_next_task(
    State(cluster): State<Arc<ClusterManager>>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = authenticate_agent_headers(&cluster, &node_id, &headers).await {
        return response;
    }
    match cluster.lease_next_task(&node_id).await {
        Ok(Some(lease)) => (StatusCode::OK, Json(lease)).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to lease agent task: {err:#}"),
        ),
    }
}

async fn v2_agent_task_ack(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(ack): Json<AgentTaskAck>,
) -> impl IntoResponse {
    let Some(node_id) = agent_node_id(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent node id".to_string(),
        );
    };
    if let Err(response) = authenticate_agent_headers(&cluster, node_id, &headers).await {
        return response;
    }
    match cluster.ack_task(&task_id, node_id, &ack.lease_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => api_error(
            StatusCode::CONFLICT,
            "agent task lease is invalid or expired".to_string(),
        ),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_agent_task_complete(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(complete): Json<AgentTaskComplete>,
) -> impl IntoResponse {
    let Some(node_id) = agent_node_id(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent node id".to_string(),
        );
    };
    if let Err(response) = authenticate_agent_headers(&cluster, node_id, &headers).await {
        return response;
    }
    match cluster.complete_task(&task_id, node_id, complete).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => api_error(
            StatusCode::CONFLICT,
            "agent task is not running or its lease expired".to_string(),
        ),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_agent_task_metrics(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(batch): Json<AgentMetricBatch>,
) -> impl IntoResponse {
    let Some(node_id) = agent_node_id(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent node id".to_string(),
        );
    };
    if let Err(response) = authenticate_agent_headers(&cluster, node_id, &headers).await {
        return response;
    }
    match cluster.upload_metrics(&task_id, node_id, batch).await {
        Ok(inserted) => {
            (StatusCode::ACCEPTED, Json(json!({ "inserted": inserted }))).into_response()
        }
        Err(err) => api_error(StatusCode::CONFLICT, err.to_string()),
    }
}

async fn v2_agent_task_log_upload(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(batch): Json<AgentLogBatch>,
) -> impl IntoResponse {
    let Some(node_id) = agent_node_id(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent node id".to_string(),
        );
    };
    if let Err(response) = authenticate_agent_headers(&cluster, node_id, &headers).await {
        return response;
    }
    match cluster.upload_logs(&task_id, node_id, batch).await {
        Ok(inserted) => {
            (StatusCode::ACCEPTED, Json(json!({ "inserted": inserted }))).into_response()
        }
        Err(err) => api_error(StatusCode::CONFLICT, err.to_string()),
    }
}

async fn v2_agent_task_logs(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match cluster.task_logs(&task_id).await {
        Ok(logs) => (StatusCode::OK, Json(logs)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load agent task logs: {err:#}"),
        ),
    }
}

async fn v2_agent_task_control(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(node_id) = agent_node_id(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent node id".to_string(),
        );
    };
    if let Err(response) = authenticate_agent_headers(&cluster, node_id, &headers).await {
        return response;
    }
    match cluster.task(&task_id).await {
        Ok(Some(task)) if task.node_id == node_id => (
            StatusCode::OK,
            Json(AgentTaskControl {
                stop_requested: task.stop_requested,
            }),
        )
            .into_response(),
        Ok(Some(_)) => api_error(
            StatusCode::FORBIDDEN,
            "task belongs to another node".to_string(),
        ),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent task not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load agent task control: {err:#}"),
        ),
    }
}

async fn v2_agent_task_stop(
    State(cluster): State<Arc<ClusterManager>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match cluster.stop_task(&task_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => api_error(
            StatusCode::CONFLICT,
            "agent task is not active or was not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to stop agent task: {err:#}"),
        ),
    }
}

async fn v2_distributed_runs(State(cluster): State<Arc<ClusterManager>>) -> impl IntoResponse {
    match cluster.distributed_runs().await {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list distributed runs: {err:#}"),
        ),
    }
}

async fn v2_distributed_run_create(
    State(cluster): State<Arc<ClusterManager>>,
    Json(request): Json<DistributedRunCreate>,
) -> impl IntoResponse {
    match cluster.start_distributed_run(request).await {
        Ok(run) => (StatusCode::CREATED, Json(run)).into_response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn v2_distributed_run_get(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.distributed_run(&id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(run)).into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "distributed run not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load distributed run: {err:#}"),
        ),
    }
}

async fn v2_distributed_run_stop(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.stop_distributed_run(&id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(run)).into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "distributed run not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to stop distributed run: {err:#}"),
        ),
    }
}

async fn v2_distributed_run_metrics(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.distributed_metrics(&id).await {
        Ok(metrics) => (StatusCode::OK, Json(metrics)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to aggregate distributed metrics: {err:#}"),
        ),
    }
}

async fn v2_distributed_run_report_csv(
    State(cluster): State<Arc<ClusterManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cluster.distributed_run(&id).await {
        Ok(Some(run)) => match cluster.distributed_metrics(&id).await {
            Ok(metrics) => download_response(
                "text/csv; charset=utf-8",
                &format!("velamq-bench-distributed-{}.csv", run.id),
                distributed_metrics_to_csv(&metrics),
            ),
            Err(err) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to aggregate distributed metrics: {err:#}"),
            ),
        },
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "distributed run not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load distributed run: {err:#}"),
        ),
    }
}

async fn authenticate_agent_headers(
    cluster: &ClusterManager,
    node_id: &str,
    headers: &HeaderMap,
) -> Result<(), Response> {
    if !agent_protocol_supported(headers) {
        return Err(api_error(
            StatusCode::UPGRADE_REQUIRED,
            format!("unsupported agent protocol; expected {AGENT_PROTOCOL_VERSION}"),
        ));
    }
    let Some(token) = bearer_token(headers) else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "missing agent token".to_string(),
        ));
    };
    cluster
        .authenticate(node_id, token)
        .await
        .map_err(|err| api_error(StatusCode::UNAUTHORIZED, err.to_string()))
}

fn agent_node_id(headers: &HeaderMap) -> Option<&str> {
    headers.get("x-velamq-agent-id")?.to_str().ok()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
}

fn agent_protocol_supported(headers: &HeaderMap) -> bool {
    headers
        .get("x-velamq-protocol-version")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u16>().ok())
        == Some(AGENT_PROTOCOL_VERSION)
}

async fn v2_broker_create(
    State(manager): State<Arc<BenchManager>>,
    Json(mut profile): Json<BrokerProfile>,
) -> impl IntoResponse {
    normalize_broker_profile(&mut profile, None);
    if let Err(err) = profile.validate() {
        return api_error(StatusCode::BAD_REQUEST, err);
    }
    match manager.upsert_broker_profile(profile).await {
        Ok(profile) => (StatusCode::CREATED, Json(profile)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save broker profile: {err:#}"),
        ),
    }
}

async fn v2_broker_get(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.broker_profile(&id).await {
        Ok(Some(profile)) => (StatusCode::OK, Json(profile)).into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "broker profile not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load broker profile: {err:#}"),
        ),
    }
}

async fn v2_broker_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(mut profile): Json<BrokerProfile>,
) -> impl IntoResponse {
    normalize_broker_profile(&mut profile, Some(id));
    if let Err(err) = profile.validate() {
        return api_error(StatusCode::BAD_REQUEST, err);
    }
    match manager.upsert_broker_profile(profile).await {
        Ok(profile) => (StatusCode::OK, Json(profile)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save broker profile: {err:#}"),
        ),
    }
}

async fn v2_broker_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_broker_profile(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(
            StatusCode::NOT_FOUND,
            "broker profile not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete broker profile: {err:#}"),
        ),
    }
}

async fn v2_broker_test_connection(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.test_broker_connection(&id).await {
        Ok(Some(result)) => (StatusCode::OK, Json(result)).into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "broker profile not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load broker profile: {err:#}"),
        ),
    }
}

async fn v2_payloads(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    match manager.payload_profiles().await {
        Ok(profiles) => (StatusCode::OK, Json(profiles)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list payload profiles: {err:#}"),
        ),
    }
}

async fn v2_payload_create(
    State(manager): State<Arc<BenchManager>>,
    Json(mut profile): Json<PayloadProfile>,
) -> impl IntoResponse {
    normalize_payload_profile(&mut profile, None);
    match manager.upsert_payload_profile(profile).await {
        Ok(profile) => (StatusCode::CREATED, Json(profile)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save payload profile: {err:#}"),
        ),
    }
}

async fn v2_payload_get(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.payload_profile(&id).await {
        Ok(Some(profile)) => (StatusCode::OK, Json(profile)).into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            "payload profile not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load payload profile: {err:#}"),
        ),
    }
}

async fn v2_payload_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(mut profile): Json<PayloadProfile>,
) -> impl IntoResponse {
    normalize_payload_profile(&mut profile, Some(id));
    match manager.upsert_payload_profile(profile).await {
        Ok(profile) => (StatusCode::OK, Json(profile)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save payload profile: {err:#}"),
        ),
    }
}

async fn v2_payload_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_payload_profile(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(
            StatusCode::NOT_FOUND,
            "payload profile not found".to_string(),
        ),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete payload profile: {err:#}"),
        ),
    }
}

async fn v2_scenarios(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    match manager.scenarios().await {
        Ok(scenarios) => (StatusCode::OK, Json(scenarios)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list scenarios: {err:#}"),
        ),
    }
}

async fn v2_scenario_create(
    State(manager): State<Arc<BenchManager>>,
    Json(mut scenario): Json<Scenario>,
) -> impl IntoResponse {
    normalize_scenario(&mut scenario, None);
    match manager.upsert_scenario(scenario).await {
        Ok(scenario) => (StatusCode::CREATED, Json(scenario)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save scenario: {err:#}"),
        ),
    }
}

async fn v2_scenario_get(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.scenario(&id).await {
        Ok(Some(scenario)) => (StatusCode::OK, Json(scenario)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "scenario not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load scenario: {err:#}"),
        ),
    }
}

async fn v2_scenario_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(mut scenario): Json<Scenario>,
) -> impl IntoResponse {
    normalize_scenario(&mut scenario, Some(id));
    match manager.upsert_scenario(scenario).await {
        Ok(scenario) => (StatusCode::OK, Json(scenario)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save scenario: {err:#}"),
        ),
    }
}

async fn v2_scenario_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_scenario(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "scenario not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete scenario: {err:#}"),
        ),
    }
}

async fn v2_scenario_run(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.scenario(&id).await {
        Ok(Some(scenario)) => match manager.start_scenario(scenario).await {
            Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
            Err(err) => api_error(StatusCode::BAD_REQUEST, err),
        },
        Ok(None) => api_error(StatusCode::NOT_FOUND, "scenario not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load scenario: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct BaselineRequest {
    run_id: Option<String>,
}

async fn v2_scenario_baseline(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(request): Json<BaselineRequest>,
) -> impl IntoResponse {
    match manager.set_scenario_baseline(&id, request.run_id).await {
        Ok(Some(scenario)) => (StatusCode::OK, Json(scenario)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "scenario not found".to_string()),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to update baseline: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct V2RunsQuery {
    scenario_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

async fn v2_runs(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<V2RunsQuery>,
) -> impl IntoResponse {
    match manager
        .runs_v2(query.scenario_id, query.status, query.limit.unwrap_or(50))
        .await
    {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list runs: {err:#}"),
        ),
    }
}

async fn v2_run_create(
    State(manager): State<Arc<BenchManager>>,
    Json(mut scenario): Json<Scenario>,
) -> impl IntoResponse {
    normalize_scenario(&mut scenario, None);
    match manager.start_ad_hoc_scenario(scenario).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err),
    }
}

async fn v2_run_get(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.run_v2(&id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(run)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load run: {err:#}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct RunPatch {
    name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
}

async fn v2_run_update(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(patch): Json<RunPatch>,
) -> impl IntoResponse {
    match manager
        .update_run_v2_metadata(&id, patch.name, patch.description, patch.tags)
        .await
    {
        Ok(Some(run)) => (StatusCode::OK, Json(run)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to update run: {err:#}"),
        ),
    }
}

async fn v2_run_delete(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.delete_run_v2(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete run: {err:#}"),
        ),
    }
}

async fn v2_run_stop(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.stop_run(&id).await {
        Ok(state) => (StatusCode::OK, Json(state)).into_response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err),
    }
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    run_workload_id: Option<String>,
    since_ms: Option<u64>,
    limit: Option<usize>,
}

async fn v2_run_snapshots(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Query(query): Query<SnapshotQuery>,
) -> impl IntoResponse {
    match manager
        .snapshots_v2(
            &id,
            query.run_workload_id,
            query.since_ms,
            query.limit.unwrap_or(1000),
        )
        .await
    {
        Ok(snapshots) => (StatusCode::OK, Json(snapshots)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load snapshots: {err:#}"),
        ),
    }
}

async fn v2_run_report(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.report_v2(&id).await {
        Ok(Some(report)) => (StatusCode::OK, Json(report)).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load report: {err:#}"),
        ),
    }
}

async fn v2_run_report_svg(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Query(query): Query<ReportImageQuery>,
) -> impl IntoResponse {
    match manager.report_v2(&id).await {
        Ok(Some(report)) => {
            let svg = report_to_svg(&report, query.lang.as_deref().unwrap_or("en"));
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            );
            let disposition = format!(
                "attachment; filename=\"velamq-bench-{}.svg\"",
                report.run.id
            );
            if let Ok(value) = HeaderValue::from_str(&disposition) {
                headers.insert(CONTENT_DISPOSITION, value);
            }
            (headers, svg).into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to export svg: {err:#}"),
        ),
    }
}

async fn v2_run_report_pdf(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Query(query): Query<ReportImageQuery>,
) -> impl IntoResponse {
    match manager.report_v2(&id).await {
        Ok(Some(report)) => match report_to_pdf(&report, query.lang.as_deref().unwrap_or("en")) {
            Ok(pdf) => download_response(
                "application/pdf",
                &format!("velamq-bench-{}.pdf", report.run.id),
                pdf,
            ),
            Err(err) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to render pdf: {err:#}"),
            ),
        },
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load report for pdf: {err:#}"),
        ),
    }
}

async fn v2_run_report_csv(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.report_v2(&id).await {
        Ok(Some(report)) => download_response(
            "text/csv; charset=utf-8",
            &format!("velamq-bench-{}.csv", report.run.id),
            report_to_csv(&report),
        ),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "run not found".to_string()),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to export csv: {err:#}"),
        ),
    }
}

fn download_response(
    content_type: &'static str,
    filename: &str,
    body: impl IntoResponse,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    (headers, body).into_response()
}

async fn v2_run_events(
    State(manager): State<Arc<BenchManager>>,
    Path(run_id): Path<String>,
    Query(query): Query<SnapshotQuery>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let replay = manager
        .snapshots_v2(
            &run_id,
            query.run_workload_id,
            query.since_ms,
            query.limit.unwrap_or(1000),
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|snapshot| {
            let run_workload_id = snapshot
                .run_workload_id
                .clone()
                .unwrap_or_else(|| format!("legacy-run-workload-{}", snapshot.run_id));
            run_sse_event(RunEvent::WorkloadMetric {
                run_id: snapshot.run_id.clone(),
                run_workload_id,
                snapshot,
            })
            .map(Ok)
        });
    let replay = stream::iter(replay);

    let live = BroadcastStream::new(manager.subscribe_run_events()).filter_map(move |event| {
        let run_id = run_id.clone();
        async move {
            match event {
                Ok(event) => {
                    if event.run_id() == run_id {
                        run_sse_event(event).map(Ok)
                    } else {
                        None
                    }
                }
                Err(BroadcastStreamRecvError::Lagged(skipped)) => Some(Ok(SseEvent::default()
                    .event("lagged")
                    .data(format!(r#"{{"skipped":{skipped}}}"#)))),
            }
        }
    });

    Sse::new(replay.chain(live)).keep_alive(KeepAlive::default())
}

async fn v2_annotations(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match manager.annotations(&id).await {
        Ok(annotations) => (StatusCode::OK, Json(annotations)).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load annotations: {err:#}"),
        ),
    }
}

async fn v2_annotation_create(
    State(manager): State<Arc<BenchManager>>,
    Path(id): Path<String>,
    Json(mut annotation): Json<Annotation>,
) -> impl IntoResponse {
    if annotation.id.trim().is_empty() {
        annotation.id = Uuid::new_v4().to_string();
    }
    annotation.run_id = id;
    if annotation.title.trim().is_empty() {
        annotation.title = "Annotation".to_string();
    }
    match manager.upsert_annotation(annotation).await {
        Ok(annotation) => (StatusCode::CREATED, Json(annotation)).into_response(),
        Err(err) => api_error(
            StatusCode::BAD_REQUEST,
            format!("failed to save annotation: {err:#}"),
        ),
    }
}

async fn v2_network_interfaces(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    bench_interfaces(State(manager)).await
}

async fn v2_runtime_state(State(manager): State<Arc<BenchManager>>) -> impl IntoResponse {
    Json(manager.runtime_state_v2().await)
}

#[derive(Debug, Deserialize)]
struct BundleExportRequest {
    run_ids: Vec<String>,
    include_snapshots: Option<bool>,
    format: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BundleExport {
    version: String,
    generated_at: chrono::DateTime<Utc>,
    scenarios: Vec<Scenario>,
    broker_profiles: Vec<BrokerProfile>,
    payload_profiles: Vec<PayloadProfile>,
    runs: Vec<BundleRun>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BundleRun {
    run: velamq_bench::model::Run,
    snapshots: Vec<velamq_bench::model::MetricSnapshot>,
    annotations: Vec<Annotation>,
}

#[derive(Debug, Deserialize)]
struct BundleImportRequest {
    bundle: BundleExport,
    conflict: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BundleImportQuery {
    conflict: Option<String>,
}

#[derive(Debug, Serialize, Default)]
struct BundleImportCounts {
    scenarios: usize,
    broker_profiles: usize,
    payload_profiles: usize,
    runs: usize,
    snapshots: usize,
    annotations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundleConflict {
    Skip,
    Rename,
    Overwrite,
}

impl BundleConflict {
    fn parse(value: Option<&str>) -> Option<Self> {
        match value.unwrap_or("skip") {
            "skip" => Some(Self::Skip),
            "rename" => Some(Self::Rename),
            "overwrite" => Some(Self::Overwrite),
            _ => None,
        }
    }
}

#[derive(Default)]
struct BundleIdMap {
    brokers: HashMap<String, String>,
    payloads: HashMap<String, String>,
    scenarios: HashMap<String, String>,
    runs: HashMap<String, String>,
    run_workloads: HashMap<String, String>,
}

async fn v2_bundle_export(
    State(manager): State<Arc<BenchManager>>,
    Json(request): Json<BundleExportRequest>,
) -> impl IntoResponse {
    if request.run_ids.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "run_ids is required".to_string());
    }
    let format = request.format.as_deref().unwrap_or("json");
    if !matches!(format, "json" | "zip") {
        return api_error(
            StatusCode::BAD_REQUEST,
            "format must be json or zip".to_string(),
        );
    }

    let requested_run_ids: HashSet<String> = request.run_ids.into_iter().collect();
    let mut scenario_ids = HashSet::new();
    let mut broker_ids = HashSet::new();
    let mut payload_ids = HashSet::new();

    let mut runs = Vec::with_capacity(requested_run_ids.len());
    for run_id in &requested_run_ids {
        let run = match manager.run_v2(&run_id).await {
            Ok(Some(run)) => run,
            Ok(None) => return api_error(StatusCode::NOT_FOUND, format!("run {run_id} not found")),
            Err(err) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to load run {run_id}: {err:#}"),
                );
            }
        };
        if let Some(scenario_id) = &run.scenario_id {
            scenario_ids.insert(scenario_id.clone());
        }
        collect_profile_ids_from_run(&run, &mut broker_ids, &mut payload_ids);
        let snapshots = if request.include_snapshots.unwrap_or(true) {
            match manager.snapshots_v2(&run_id, None, None, 20_000).await {
                Ok(snapshots) => snapshots,
                Err(err) => {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to load snapshots for {run_id}: {err:#}"),
                    );
                }
            }
        } else {
            Vec::new()
        };
        let annotations = match manager.annotations(&run_id).await {
            Ok(annotations) => annotations,
            Err(err) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to load annotations for {run_id}: {err:#}"),
                );
            }
        };
        runs.push(BundleRun {
            run,
            snapshots,
            annotations,
        });
    }

    let all_scenarios = match manager.scenarios().await {
        Ok(scenarios) => scenarios,
        Err(err) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load scenarios: {err:#}"),
            );
        }
    };
    let scenarios: Vec<_> = all_scenarios
        .into_iter()
        .filter(|scenario| scenario_ids.contains(&scenario.id))
        .inspect(|scenario| {
            collect_profile_ids_from_scenario(scenario, &mut broker_ids, &mut payload_ids)
        })
        .collect();

    let all_broker_profiles = match manager.broker_profiles().await {
        Ok(profiles) => profiles,
        Err(err) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load broker profiles: {err:#}"),
            );
        }
    };
    let broker_profiles = all_broker_profiles
        .into_iter()
        .filter(|profile| broker_ids.contains(&profile.id))
        .collect();

    let all_payload_profiles = match manager.payload_profiles().await {
        Ok(profiles) => profiles,
        Err(err) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load payload profiles: {err:#}"),
            );
        }
    };
    let payload_profiles = all_payload_profiles
        .into_iter()
        .filter(|profile| payload_ids.contains(&profile.id))
        .collect();

    let bundle = BundleExport {
        version: "1.0".to_string(),
        generated_at: Utc::now(),
        scenarios,
        broker_profiles,
        payload_profiles,
        runs,
    };
    match format {
        "zip" => match bundle_to_zip(&bundle) {
            Ok(body) => {
                let mut headers = HeaderMap::new();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
                headers.insert(
                    CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment; filename=\"velamq-bundle.zip\""),
                );
                (headers, body).into_response()
            }
            Err(err) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to zip bundle: {err:#}"),
            ),
        },
        _ => match serde_json::to_string(&bundle) {
            Ok(body) => {
                let mut headers = HeaderMap::new();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                headers.insert(
                    CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment; filename=\"velamq-bundle.json\""),
                );
                (headers, body).into_response()
            }
            Err(err) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to serialize bundle: {err:#}"),
            ),
        },
    }
}

async fn v2_bundle_import(
    State(manager): State<Arc<BenchManager>>,
    Query(query): Query<BundleImportQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let request = match parse_bundle_import(&headers, &body, query.conflict) {
        Ok(request) => request,
        Err(err) => return api_error(StatusCode::BAD_REQUEST, err),
    };
    import_bundle_request(manager, request).await
}

fn bundle_to_zip(bundle: &BundleExport) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("bundle.json", options)?;
        zip.write_all(serde_json::to_string(bundle)?.as_bytes())?;
        zip.finish()?;
    }
    Ok(buffer.into_inner())
}

fn parse_bundle_import(
    headers: &HeaderMap,
    body: &[u8],
    query_conflict: Option<String>,
) -> Result<BundleImportRequest, String> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .to_ascii_lowercase();

    if content_type.contains("application/zip")
        || content_type.contains("application/octet-stream")
        || body.starts_with(b"PK")
    {
        let bundle = bundle_from_zip(body)?;
        return Ok(BundleImportRequest {
            bundle,
            conflict: query_conflict,
        });
    }

    let mut request: BundleImportRequest = serde_json::from_slice(body)
        .map_err(|err| format!("failed to parse bundle JSON: {err}"))?;
    if request.conflict.is_none() {
        request.conflict = query_conflict;
    }
    Ok(request)
}

fn bundle_from_zip(body: &[u8]) -> Result<BundleExport, String> {
    let reader = Cursor::new(body);
    let mut archive =
        ZipArchive::new(reader).map_err(|err| format!("failed to read zip: {err}"))?;
    let mut file = archive
        .by_name("bundle.json")
        .map_err(|_| "zip bundle must contain bundle.json".to_string())?;
    let mut json = String::new();
    file.read_to_string(&mut json)
        .map_err(|err| format!("failed to read bundle.json: {err}"))?;
    serde_json::from_str(&json).map_err(|err| format!("failed to parse bundle.json: {err}"))
}

async fn import_bundle_request(
    manager: Arc<BenchManager>,
    request: BundleImportRequest,
) -> Response {
    if request.bundle.version != "1.0" {
        return api_error(
            StatusCode::BAD_REQUEST,
            format!("unsupported bundle version {}", request.bundle.version),
        );
    }
    let Some(conflict) = BundleConflict::parse(request.conflict.as_deref()) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "conflict must be skip, rename, or overwrite".to_string(),
        );
    };

    let mut counts = BundleImportCounts::default();

    let mut id_map = BundleIdMap::default();

    for mut profile in request.bundle.broker_profiles {
        let original_id = profile.id.clone();
        let exists = matches!(manager.broker_profile(&original_id).await, Ok(Some(_)));
        if conflict == BundleConflict::Skip && exists {
            id_map.brokers.insert(original_id.clone(), original_id);
            continue;
        }
        if conflict == BundleConflict::Rename {
            rename_broker_profile(&mut profile, &original_id);
        }
        id_map.brokers.insert(original_id, profile.id.clone());
        if let Err(err) = manager.upsert_broker_profile(profile).await {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("failed to import broker profile: {err:#}"),
            );
        }
        counts.broker_profiles += 1;
    }

    for mut profile in request.bundle.payload_profiles {
        let original_id = profile.id.clone();
        let exists = matches!(manager.payload_profile(&original_id).await, Ok(Some(_)));
        if conflict == BundleConflict::Skip && exists {
            id_map.payloads.insert(original_id.clone(), original_id);
            continue;
        }
        if conflict == BundleConflict::Rename {
            rename_payload_profile(&mut profile, &original_id);
        }
        id_map.payloads.insert(original_id, profile.id.clone());
        if let Err(err) = manager.upsert_payload_profile(profile).await {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("failed to import payload profile: {err:#}"),
            );
        }
        counts.payload_profiles += 1;
    }

    for mut scenario in request.bundle.scenarios {
        let original_id = scenario.id.clone();
        let exists = matches!(manager.scenario(&original_id).await, Ok(Some(_)));
        if conflict == BundleConflict::Skip && exists {
            id_map.scenarios.insert(original_id.clone(), original_id);
            continue;
        }
        if conflict == BundleConflict::Rename {
            rename_scenario(&mut scenario, &original_id);
        }
        id_map.scenarios.insert(original_id, scenario.id.clone());
        remap_scenario(&mut scenario, &id_map);
        if let Err(err) = manager.upsert_scenario(scenario).await {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("failed to import scenario: {err:#}"),
            );
        }
        counts.scenarios += 1;
    }

    for mut run_package in request.bundle.runs {
        let original_run_id = run_package.run.id.clone();
        let mut run_id = original_run_id.clone();
        if matches!(manager.run_v2(&run_id).await, Ok(Some(_))) {
            if conflict == BundleConflict::Skip {
                id_map.runs.insert(original_run_id.clone(), original_run_id);
                continue;
            }
            if conflict == BundleConflict::Overwrite {
                if let Err(err) = manager.delete_run_v2(&run_id).await {
                    return api_error(
                        StatusCode::BAD_REQUEST,
                        format!("failed to overwrite run {run_id}: {err:#}"),
                    );
                }
            } else {
                run_id = format!("run-{}", Uuid::new_v4());
            }
        } else if conflict == BundleConflict::Rename {
            run_id = format!("run-{}", Uuid::new_v4());
        }
        id_map.runs.insert(original_run_id, run_id.clone());
        remap_run_package(&mut run_package, &mut id_map, &run_id);

        if let Err(err) = manager.upsert_run_v2(run_package.run).await {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("failed to import run {run_id}: {err:#}"),
            );
        }
        counts.runs += 1;

        for mut snapshot in run_package.snapshots {
            remap_snapshot(&mut snapshot, &id_map);
            if let Err(err) = manager.import_snapshot(snapshot).await {
                let _ = manager.delete_run_v2(&run_id).await;
                return api_error(
                    StatusCode::BAD_REQUEST,
                    format!("failed to import snapshot for {run_id}: {err:#}"),
                );
            }
            counts.snapshots += 1;
        }

        for mut annotation in run_package.annotations {
            remap_annotation(&mut annotation, &id_map, conflict);
            if let Err(err) = manager.upsert_annotation(annotation).await {
                let _ = manager.delete_run_v2(&run_id).await;
                return api_error(
                    StatusCode::BAD_REQUEST,
                    format!("failed to import annotation for {run_id}: {err:#}"),
                );
            }
            counts.annotations += 1;
        }
    }

    Json(counts).into_response()
}

fn collect_profile_ids_from_run(
    run: &velamq_bench::model::Run,
    broker_ids: &mut HashSet<String>,
    payload_ids: &mut HashSet<String>,
) {
    for workload in &run.workloads {
        if let Ok(config) = serde_json::from_str::<Workload>(&workload.config_snapshot_json) {
            broker_ids.insert(config.broker_profile_id);
            if let Some(payload_id) = config.payload_profile_id {
                payload_ids.insert(payload_id);
            }
        }
    }
}

fn collect_profile_ids_from_scenario(
    scenario: &Scenario,
    broker_ids: &mut HashSet<String>,
    payload_ids: &mut HashSet<String>,
) {
    for workload in scenario_workloads(scenario) {
        broker_ids.insert(workload.broker_profile_id.clone());
        if let Some(payload_id) = &workload.payload_profile_id {
            payload_ids.insert(payload_id.clone());
        }
    }
}

fn scenario_workloads(scenario: &Scenario) -> impl Iterator<Item = &Workload> {
    scenario.stages.iter().flat_map(|stage| match stage {
        ScenarioStage::Parallel { workloads } | ScenarioStage::Sequential { workloads } => {
            workloads.iter()
        }
    })
}

fn rename_broker_profile(profile: &mut BrokerProfile, original_id: &str) {
    profile.id = format!("broker-{}", Uuid::new_v4());
    profile.name = imported_name(&profile.name, original_id);
    let now = Utc::now();
    profile.created_at = now;
    profile.updated_at = now;
}

fn rename_payload_profile(profile: &mut PayloadProfile, original_id: &str) {
    profile.id = format!("payload-{}", Uuid::new_v4());
    profile.name = imported_name(&profile.name, original_id);
    let now = Utc::now();
    profile.created_at = now;
    profile.updated_at = now;
}

fn rename_scenario(scenario: &mut Scenario, original_id: &str) {
    scenario.id = format!("scenario-{}", Uuid::new_v4());
    scenario.name = imported_name(&scenario.name, original_id);
    let now = Utc::now();
    scenario.created_at = now;
    scenario.updated_at = now;
}

fn imported_name(name: &str, original_id: &str) -> String {
    let suffix = &original_id[..original_id.len().min(8)];
    format!("{name} (imported {suffix})")
}

fn remap_scenario(scenario: &mut Scenario, id_map: &BundleIdMap) {
    if let Some(run_id) = &scenario.baseline_run_id {
        scenario.baseline_run_id = id_map
            .runs
            .get(run_id)
            .cloned()
            .or_else(|| Some(run_id.clone()));
    }
    for stage in &mut scenario.stages {
        let workloads = match stage {
            ScenarioStage::Parallel { workloads } | ScenarioStage::Sequential { workloads } => {
                workloads
            }
        };
        for workload in workloads {
            remap_workload(workload, id_map);
        }
    }
}

fn remap_run_package(package: &mut BundleRun, id_map: &mut BundleIdMap, run_id: &str) {
    package.run.id = run_id.to_string();
    if let Some(scenario_id) = &package.run.scenario_id {
        package.run.scenario_id = id_map
            .scenarios
            .get(scenario_id)
            .cloned()
            .or_else(|| Some(scenario_id.clone()));
    }
    package.run.baseline_of_scenario_id =
        package
            .run
            .baseline_of_scenario_id
            .as_ref()
            .and_then(|scenario_id| {
                id_map
                    .scenarios
                    .get(scenario_id)
                    .cloned()
                    .or_else(|| Some(scenario_id.clone()))
            });

    for workload in &mut package.run.workloads {
        let original_id = workload.id.clone();
        let new_id = if package.run.id == workload.run_id {
            original_id.clone()
        } else {
            format!("rw-{}", Uuid::new_v4())
        };
        id_map.run_workloads.insert(original_id, new_id.clone());
        workload.id = new_id;
        workload.run_id = run_id.to_string();
        if let Ok(mut config) = serde_json::from_str::<Workload>(&workload.config_snapshot_json) {
            remap_workload(&mut config, id_map);
            if let Ok(json) = serde_json::to_string(&config) {
                workload.config_snapshot_json = json;
            }
        }
    }
}

fn remap_workload(workload: &mut Workload, id_map: &BundleIdMap) {
    if let Some(id) = id_map.brokers.get(&workload.broker_profile_id) {
        workload.broker_profile_id = id.clone();
    }
    if let Some(payload_id) = &workload.payload_profile_id {
        workload.payload_profile_id = id_map
            .payloads
            .get(payload_id)
            .cloned()
            .or_else(|| Some(payload_id.clone()));
    }
}

fn remap_snapshot(snapshot: &mut velamq_bench::model::MetricSnapshot, id_map: &BundleIdMap) {
    if let Some(run_id) = id_map.runs.get(&snapshot.run_id) {
        snapshot.run_id = run_id.clone();
    }
    if let Some(workload_id) = &snapshot.run_workload_id {
        snapshot.run_workload_id = id_map
            .run_workloads
            .get(workload_id)
            .cloned()
            .or_else(|| Some(workload_id.clone()));
    }
}

fn remap_annotation(annotation: &mut Annotation, id_map: &BundleIdMap, conflict: BundleConflict) {
    if conflict == BundleConflict::Rename {
        annotation.id = format!("annotation-{}", Uuid::new_v4());
    }
    if let Some(run_id) = id_map.runs.get(&annotation.run_id) {
        annotation.run_id = run_id.clone();
    }
    if let Some(workload_id) = &annotation.run_workload_id {
        annotation.run_workload_id = id_map
            .run_workloads
            .get(workload_id)
            .cloned()
            .or_else(|| Some(workload_id.clone()));
    }
}

fn sse_event(event: BenchEvent) -> SseEvent {
    let name = match &event {
        BenchEvent::State(_) => "state",
        BenchEvent::Metrics(_) => "metrics",
        BenchEvent::Log(_) => "log",
    };
    SseEvent::default()
        .event(name)
        .data(serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string()))
}

fn run_sse_event(event: RunEvent) -> Option<SseEvent> {
    let name = event.event_name();
    let data = match event {
        RunEvent::RunStateChanged { run, .. } => json!({ "run": run }),
        RunEvent::WorkloadMetric {
            run_workload_id,
            snapshot,
            ..
        } => json!({ "run_workload_id": run_workload_id, "snapshot": snapshot }),
        RunEvent::WorkloadLog {
            run_workload_id,
            log,
            ..
        } => json!({ "run_workload_id": run_workload_id, "log": log }),
        RunEvent::RunAnnotation { annotation, .. } => json!({ "annotation": annotation }),
    };
    Some(
        SseEvent::default()
            .event(name)
            .data(serde_json::to_string(&data).ok()?),
    )
}

fn normalize_broker_profile(profile: &mut BrokerProfile, id: Option<String>) {
    if let Some(id) = id {
        profile.id = id;
    }
    if profile.id.trim().is_empty() {
        profile.id = Uuid::new_v4().to_string();
    }
    if profile.host.trim().is_empty() {
        profile.host = "127.0.0.1".to_string();
    }
    if profile.port == 0 {
        profile.port = profile.protocol.default_port();
    }
    if profile.protocol.is_websocket() {
        profile.websocket_path = Some(normalize_websocket_path(profile.websocket_path.as_deref()));
    } else {
        profile.websocket_path = None;
    }
    if profile.name.trim().is_empty() {
        profile.name = format!(
            "{}://{}:{}",
            profile.protocol.as_str(),
            profile.host,
            profile.port
        );
    }
    if profile.keepalive_secs == 0 {
        profile.keepalive_secs = 30;
    }
    if profile.connection_timeout_secs == 0 {
        profile.connection_timeout_secs = 10;
    }
    if profile.mqtt_version == velamq_bench::model::MqttVersion::V5_0 && profile.mqtt5.is_none() {
        profile.mqtt5 = Some(velamq_bench::model::Mqtt5Config::default());
    }
    if !matches!(
        profile.protocol,
        velamq_bench::model::BrokerProtocol::Mqtts | velamq_bench::model::BrokerProtocol::Wss
    ) {
        profile.tls = None;
    }
    let now = Utc::now();
    if profile.created_at.timestamp() == 0 {
        profile.created_at = now;
    }
    profile.updated_at = now;
}

fn normalize_payload_profile(profile: &mut PayloadProfile, id: Option<String>) {
    if let Some(id) = id {
        profile.id = id;
    }
    if profile.id.trim().is_empty() {
        profile.id = Uuid::new_v4().to_string();
    }
    if profile.name.trim().is_empty() {
        profile.name = "Payload Profile".to_string();
    }
    let now = Utc::now();
    if profile.created_at.timestamp() == 0 {
        profile.created_at = now;
    }
    profile.updated_at = now;
}

fn normalize_scenario(scenario: &mut Scenario, id: Option<String>) {
    if let Some(id) = id {
        scenario.id = id;
    }
    if scenario.id.trim().is_empty() {
        scenario.id = Uuid::new_v4().to_string();
    }
    if scenario.name.trim().is_empty() {
        scenario.name = "Scenario".to_string();
    }
    let now = Utc::now();
    if scenario.created_at.timestamp() == 0 {
        scenario.created_at = now;
    }
    scenario.updated_at = now;
}

fn api_error(status: StatusCode, error: String) -> axum::response::Response {
    (status, Json(ApiError { error })).into_response()
}

async fn api_not_found() -> axum::response::Response {
    api_error(StatusCode::NOT_FOUND, "api endpoint not found".to_string())
}

fn report_to_csv(report: &BenchReport) -> String {
    let mut csv = String::from(
        "row_type,metric,value,run_id,ts,elapsed_ms,connected,published,received,errors,publish_rate,receive_rate,connect_rate,error_rate,latency_count,latency_avg_ms,latency_min_ms,latency_p50_ms,latency_p90_ms,latency_p95_ms,latency_p99_ms,latency_p999_ms,latency_max_ms\n",
    );

    push_summary_row(&mut csv, &report.run.id, "status", &report.run.status);
    push_summary_row(&mut csv, &report.run.id, "mode", &report.run.mode);
    if let Some(specimen) = &report.run.specimen {
        push_summary_row(&mut csv, &report.run.id, "specimen_id", &specimen.id);
        push_summary_row(&mut csv, &report.run.id, "specimen_name", &specimen.name);
        push_summary_row(
            &mut csv,
            &report.run.id,
            "specimen_description",
            &specimen.description,
        );
        push_summary_row(
            &mut csv,
            &report.run.id,
            "specimen_tags",
            specimen.tags.join("|"),
        );
        push_summary_row(
            &mut csv,
            &report.run.id,
            "specimen_created_at",
            specimen.created_at.to_rfc3339(),
        );
    }
    push_summary_row(
        &mut csv,
        &report.run.id,
        "started_at",
        report.run.started_at.to_rfc3339(),
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "stopped_at",
        report
            .run
            .stopped_at
            .map(|ts| ts.to_rfc3339())
            .unwrap_or_default(),
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "duration_ms",
        report.stats.duration_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "sample_count",
        report.stats.sample_count,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "max_connected",
        report.stats.max_connected,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "total_published",
        report.stats.total_published,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "total_received",
        report.stats.total_received,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "total_errors",
        report.stats.total_errors,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "avg_publish_rate",
        report.stats.avg_publish_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "avg_receive_rate",
        report.stats.avg_receive_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "avg_connect_rate",
        report.stats.avg_connect_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "avg_error_rate",
        report.stats.avg_error_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "max_publish_rate",
        report.stats.max_publish_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "max_receive_rate",
        report.stats.max_receive_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "max_connect_rate",
        report.stats.max_connect_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "max_error_rate",
        report.stats.max_error_rate,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_count",
        report.stats.latency_count,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_avg_ms",
        report.stats.latency_avg_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_min_ms",
        report.stats.latency_min_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_p50_ms",
        report.stats.latency_p50_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_p90_ms",
        report.stats.latency_p90_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_p95_ms",
        report.stats.latency_p95_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_p99_ms",
        report.stats.latency_p99_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_p999_ms",
        report.stats.latency_p999_ms,
    );
    push_summary_row(
        &mut csv,
        &report.run.id,
        "latency_max_ms",
        report.stats.latency_max_ms,
    );

    for snapshot in &report.snapshots {
        let row = [
            "snapshot".to_string(),
            String::new(),
            String::new(),
            csv_escape(&snapshot.run_id),
            csv_escape(&snapshot.ts.to_rfc3339()),
            snapshot.elapsed_ms.to_string(),
            snapshot.connected.to_string(),
            snapshot.published.to_string(),
            snapshot.received.to_string(),
            snapshot.errors.to_string(),
            snapshot.publish_rate.to_string(),
            snapshot.receive_rate.to_string(),
            snapshot.connect_rate.to_string(),
            snapshot.error_rate.to_string(),
            snapshot.latency_count.to_string(),
            snapshot.latency_avg_ms.to_string(),
            snapshot.latency_min_ms.to_string(),
            snapshot.latency_p50_ms.to_string(),
            snapshot.latency_p90_ms.to_string(),
            snapshot.latency_p95_ms.to_string(),
            snapshot.latency_p99_ms.to_string(),
            snapshot.latency_p999_ms.to_string(),
            snapshot.latency_max_ms.to_string(),
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }

    csv
}

fn distributed_metrics_to_csv(metrics: &DistributedMetrics) -> String {
    let mut csv = String::from(
        "series,node_id,task_id,run_id,ts,elapsed_ms,connected,published,received,errors,publish_rate,receive_rate,connect_rate,error_rate,latency_count,latency_avg_ms,latency_min_ms,latency_p50_ms,latency_p90_ms,latency_p95_ms,latency_p99_ms,latency_p999_ms,latency_max_ms\n",
    );
    for snapshot in &metrics.summary {
        push_distributed_snapshot_row(&mut csv, "global", "", "", snapshot);
    }
    for node in &metrics.nodes {
        for snapshot in &node.snapshots {
            push_distributed_snapshot_row(&mut csv, "node", &node.node_id, &node.task_id, snapshot);
        }
    }
    csv
}

fn push_distributed_snapshot_row(
    csv: &mut String,
    series: &str,
    node_id: &str,
    task_id: &str,
    snapshot: &MetricSnapshot,
) {
    csv.push_str(&format!(
        "{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}\n",
        csv_escape(series),
        csv_escape(node_id),
        csv_escape(task_id),
        csv_escape(&snapshot.run_id),
        csv_escape(&snapshot.ts.to_rfc3339()),
        snapshot.elapsed_ms,
        snapshot.connected,
        snapshot.published,
        snapshot.received,
        snapshot.errors,
        snapshot.publish_rate,
        snapshot.receive_rate,
        snapshot.connect_rate,
        snapshot.error_rate,
        snapshot.latency_count,
        snapshot.latency_avg_ms,
        snapshot.latency_min_ms,
        snapshot.latency_p50_ms,
        snapshot.latency_p90_ms,
        snapshot.latency_p95_ms,
        snapshot.latency_p99_ms,
        snapshot.latency_p999_ms,
        snapshot.latency_max_ms,
    ));
}

fn push_summary_row(csv: &mut String, run_id: &str, metric: &str, value: impl ToString) {
    csv.push_str(&format!(
        "summary,{},{},{}",
        csv_escape(metric),
        csv_escape(&value.to_string()),
        csv_escape(run_id)
    ));
    csv.push_str(&",".repeat(19));
    csv.push('\n');
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|ch| matches!(ch, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
