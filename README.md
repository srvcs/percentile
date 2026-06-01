# srvcs-percentile

## Name

| Field | Value |
| --- | --- |
| Service | `srvcs-percentile` |
| Slug | `percentile` |
| Repository | `srvcs/percentile` |
| Package | `srvcs-percentile` |
| Kind | `orchestrator` |

## Function

comparison: p-th percentile of a list (linear interpolation)

## Dependencies

| Dependency | Repository |
| --- | --- |
| `srvcs-sortascending` | [srvcs/sortascending](https://github.com/srvcs/sortascending) |
| `srvcs-floatadd` | [srvcs/floatadd](https://github.com/srvcs/floatadd) |
| `srvcs-floatmultiply` | [srvcs/floatmultiply](https://github.com/srvcs/floatmultiply) |
| `srvcs-floatsubtract` | [srvcs/floatsubtract](https://github.com/srvcs/floatsubtract) |

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity |
| `POST` | `/` | Evaluate the service function |
| `GET` | `/healthz` | Liveness probe |
| `GET` | `/readyz` | Readiness probe |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/openapi.json` | OpenAPI document |

## Inputs

| Name | Type | Required |
| --- | --- | --- |
| `values` | `json[]` | yes |
| `p` | `number` | yes |

## Outputs

| Name | Type |
| --- | --- |
| `values` | `json[]` |
| `p` | `number` |
| `result` | `number` |

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |
| `SRVCS_FLOATADD_URL` | `http://127.0.0.1:8091` | Base URL for srvcs-floatadd |
| `SRVCS_FLOATMULTIPLY_URL` | `` | Base URL for srvcs-floatmultiply |
| `SRVCS_FLOATSUBTRACT_URL` | `` | Base URL for srvcs-floatsubtract |
| `SRVCS_SORTASCENDING_URL` | `` | Base URL for srvcs-sortascending |

## Error Behavior

- `422` means the request could not be evaluated for the documented input shape.
- `503` means a required dependency was unavailable or returned an unexpected response.
- Dependency validation errors are forwarded when this service delegates validation.

## Local Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See the [srvcs service standard](https://github.com/srvcs/platform/blob/main/STANDARD.md) for the full operational contract.

## Metadata

Machine-readable service metadata lives in `srvcs.yaml`. Keep it aligned with this README when the service contract changes.
