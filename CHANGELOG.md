# Changelog

## 0.3.0

- Added MQTT 3.1.1 and MQTT 5 broker profiles over TCP, TLS, WebSocket, and secure WebSocket, including mTLS and MQTT 5 connection properties.
- Added the `velamq-bench-agent` remote executor, authenticated node registration, heartbeat, leases, task control, telemetry, and logs.
- Added multi-node distributed scheduling with selected, even, and capacity-weighted client allocation.
- Added global/per-node distributed charts, mergeable latency histograms, and distributed CSV export.
- Added Nodes and Distributed Run UI, deployment documentation, and cross-platform Agent release packaging.
- Split every platform Release into a `velamq-bench` service package and a separate `velamq-bench-agent` package.

## 0.2.0

- Added v2 domain models for profiles, scenarios, workloads, runs, snapshots, and annotations.
- Added v2 runtime APIs, scenario execution, SSE, and non-flat load profiles.
- Replaced the legacy UI with a Vite + Vue application shell.
- Added Scenario Builder with multi-workload orchestration and load-shape previews.
- Added Run Detail tabs with ECharts-based throughput, latency, connection, error, log, config, and notes views.
- Added Compare view with run picker, KPI delta table, and overlay charts.
- Added Broker and Payload profile management in Settings.
- Added Network and Preferences settings, i18n key checking, a11y smoke checks, and v2 API documentation.
- Added common load-test templates and moved template usage into Scenario Builder as a save-only preset flow.
- Added bundle export/import endpoints and Settings import flow for reproducible run migration.
- Removed the legacy static UI fallback; the server now serves the built Vite app from `web/dist`.
- Marked legacy `/api/bench/*` compatibility endpoints with `Deprecation`, `Sunset`, and successor `Link` headers.
