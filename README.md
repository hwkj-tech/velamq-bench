# VelaMQ Bench

VelaMQ Bench is a local MQTT load console for building reusable scenarios, running broker workloads, and comparing run history.

## Run

```bash
cargo run
```

By default the app binds to `127.0.0.1:8088`. Override it with:

```bash
VELAMQ_BIND=127.0.0.1:8090 cargo run
```

The server serves the built Vite app from `web/dist`.

## Frontend

```bash
cd web
npm install
npm run lint
npm run lint:i18n
npm run lint:a11y
npm run typecheck
npm run build
```

## Concepts

- Scenario: saved multi-workload plan.
- Workload: pub, sub, or connection leg inside a scenario.
- Broker profile: reusable MQTT endpoint.
- Payload profile: reusable payload generator.
- Run: execution record with snapshots, logs, annotations, and reports.
- Template: read-only scenario preset. Templates are applied from the Scenario Builder and are not runnable by themselves.

## Templates And Scenarios

Open `Templates` to browse common MQTT load presets. Click `Use template` to open `Scenarios -> New` with that preset prefilled, then review broker, workload, payload, and network settings before saving the scenario.

When a scenario draft starts from a template, the builder only shows `Save`. Run actions are available after the scenario is saved from the Scenarios page or Scenario Detail.

## Chart Export And Bundles

Run Detail exports the whole load-test result as a PDF report, SVG chart, or CSV dataset:

```bash
curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.svg?lang=zh-CN' \
  -o velamq-chart.svg

curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.pdf?lang=zh-CN' \
  -o velamq-report.pdf

curl 'http://127.0.0.1:8088/api/v2/runs/run-id/report.csv' \
  -o velamq-data.csv
```

Runs can be exported as a reproducible ZIP bundle with `bundle.json`, scenario, profiles, snapshots, and annotations:

```bash
curl -X POST http://127.0.0.1:8088/api/v2/bundles/export \
  -H 'Content-Type: application/json' \
  -d '{ "run_ids": ["run-id"], "include_snapshots": true, "format": "zip" }' \
  -o velamq-bundle.zip
```

Import the bundle through `Settings -> Import` or `POST /api/v2/bundles/import?conflict=rename`. Conflict strategies are `skip`, `rename`, and `overwrite`.

## i18n And Accessibility

New UI copy must be added to both `web/locales/en.json` and `web/locales/zh-CN.json`. Run `npm run lint:i18n` before submitting UI changes.

Run `npm run lint:a11y` for the local accessibility smoke checks. It catches missing image alt text, empty anchor placeholders, icon-only buttons without labels, and tab roles without selected state.

## API

See [docs/api.md](docs/api.md) for the v2 endpoint summary. Legacy `/api/bench/*` endpoints remain available for compatibility and include `Deprecation`, `Sunset`, and successor `Link` response headers.
