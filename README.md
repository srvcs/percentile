# srvcs-percentile

The percentile orchestrator of the srvcs.cloud distributed standard library.

Its single concern: the **`p`-th percentile of a list**, computed by linear
interpolation between order statistics. It does no arithmetic of its own. It
asks [`srvcs-sortascending`](https://github.com/srvcs/sortascending) to sort the
list, computes the interpolation rank locally, then delegates the real-valued
steps to the float primitives:

1. `sorted = sortascending(values)`
2. `idx = (p / 100) * (n - 1)`, `lo = floor(idx)`, `frac = idx - lo` (local)
3. if `lo + 1 >= n`: `result = sorted[lo]`
4. otherwise:
   - `diff = floatsubtract(sorted[lo+1], sorted[lo])`
   - `scaled = floatmultiply(frac, diff)`
   - `result = floatadd(sorted[lo], scaled)`

For example, `percentile([1, 2, 3, 4], 50) = 2.5`.

Input validation propagates from the leaves: if the list is empty this service
rejects it with `422`; if an element is not a valid integer, a dependency
rejects it with `422` and this service forwards that rejection unchanged.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute the `p`-th percentile of `values` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' \
  -d '{"values": [1, 2, 3, 4], "p": 50}'
# {"values":[1,2,3,4],"p":50.0,"result":2.5}
```

Responses:

- `200 {"values": [...], "p": p, "result": r}` — evaluated (`result` is a float).
- `422` — empty list, or an invalid element forwarded from a leaf dependency.
- `500` — a dependency returned an unusable response.
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-sortascending`](https://github.com/srvcs/sortascending)
- [`srvcs-floatadd`](https://github.com/srvcs/floatadd)
- [`srvcs-floatmultiply`](https://github.com/srvcs/floatmultiply)
- [`srvcs-floatsubtract`](https://github.com/srvcs/floatsubtract)

A single request here fans out across the dependency graph: percentile asks
`srvcs-sortascending` for the sorted list, then `srvcs-floatsubtract`,
`srvcs-floatmultiply`, and `srvcs-floatadd` to interpolate between the bracketing
order statistics.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_SORTASCENDING_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-sortascending` |
| `SRVCS_FLOATADD_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-floatadd` |
| `SRVCS_FLOATMULTIPLY_URL` | `http://127.0.0.1:8092` | Base URL of `srvcs-floatmultiply` |
| `SRVCS_FLOATSUBTRACT_URL` | `http://127.0.0.1:8093` | Base URL of `srvcs-floatsubtract` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up mock dependency services in-process that actually
compute (sortascending sorts, floatsubtract subtracts, floatmultiply multiplies,
floatadd adds), so the interpolation is genuinely exercised against the asserted
cases with a `1e-9` tolerance. They also cover an empty-list `422`, a forwarded
`422`, and a degraded dependency (`503`). See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
