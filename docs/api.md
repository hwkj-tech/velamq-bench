# VelaMQ Bench v2 API

Base URL defaults to `http://127.0.0.1:8088`.

## Runtime

- `GET /api/v2/runtime/state` returns active run state.
- `GET /api/v2/network-interfaces` lists local network interfaces.

## Templates

- `GET /api/v2/templates?limit=80`

Templates are read-only scenario presets for the UI. They are applied in `Scenarios -> New` and cannot be run directly.

## Profiles

- `GET /api/v2/broker-profiles`
- `POST /api/v2/broker-profiles`
- `GET /api/v2/broker-profiles/{id}`
- `PATCH /api/v2/broker-profiles/{id}`
- `DELETE /api/v2/broker-profiles/{id}`
- `POST /api/v2/broker-profiles/{id}/test-connection`
- `GET /api/v2/payload-profiles`
- `POST /api/v2/payload-profiles`
- `GET /api/v2/payload-profiles/{id}`
- `PATCH /api/v2/payload-profiles/{id}`
- `DELETE /api/v2/payload-profiles/{id}`

## Scenarios

- `GET /api/v2/scenarios`
- `POST /api/v2/scenarios`
- `GET /api/v2/scenarios/{id}`
- `PATCH /api/v2/scenarios/{id}`
- `DELETE /api/v2/scenarios/{id}`
- `POST /api/v2/scenarios/{id}/run`
- `POST /api/v2/scenarios/{id}/baseline` with `{ "run_id": "..." }`

## Runs

- `GET /api/v2/runs?limit=50`
- `POST /api/v2/runs` starts an ad-hoc scenario.
- `GET /api/v2/runs/{id}`
- `PATCH /api/v2/runs/{id}`
- `DELETE /api/v2/runs/{id}`
- `POST /api/v2/runs/{id}/stop`
- `GET /api/v2/runs/{id}/snapshots?since_ms=0&limit=600`
- `GET /api/v2/runs/{id}/report`
- `GET /api/v2/runs/{id}/report.svg?lang=zh-CN` downloads an SVG chart report.
- `GET /api/v2/runs/{id}/report.pdf?lang=zh-CN` downloads a PDF chart report.
- `GET /api/v2/runs/{id}/report.csv` downloads summary metrics and time-series samples as CSV.
- `GET /api/v2/runs/{id}/events`
- `GET /api/v2/runs/{id}/annotations`
- `POST /api/v2/runs/{id}/annotations`

## Bundles

- `POST /api/v2/bundles/export` with `{ "run_ids": ["..."], "include_snapshots": true, "format": "json" | "zip" }`
- `POST /api/v2/bundles/import?conflict=rename` with a JSON wrapper body or an `application/zip` body containing `bundle.json`

Legacy `/api/bench/*` endpoints remain available for compatibility while new work should target `/api/v2/*`. Compatibility responses include:

- `Deprecation: true`
- `Sunset: Sat, 01 Aug 2026 00:00:00 GMT`
- `Link: </api/v2/runs>; rel="successor-version"`
