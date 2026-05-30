use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-percentile";
pub const CONCERN: &str = "comparison: p-th percentile of a list (linear interpolation)";
pub const DEPENDS_ON: &[&str] = &[
    "srvcs-sortascending",
    "srvcs-floatadd",
    "srvcs-floatmultiply",
    "srvcs-floatsubtract",
];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub sortascending_url: String,
    pub floatadd_url: String,
    pub floatmultiply_url: String,
    pub floatsubtract_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    /// The list of integers to take the percentile of. Must be non-empty.
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    /// The percentile to compute, in `0..=100`.
    pub p: f64,
}

#[derive(Serialize, ToSchema)]
pub struct PercentileResponse {
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    pub p: f64,
    pub result: f64,
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for invalid
/// input, so percentile reports the same rejection a leaf dependency did).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

fn no_result(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": format!("{dependency} returned no usable result") })),
    )
        .into_response()
}

/// Ask `srvcs-sortascending` to sort `values`, returning the sorted JSON array.
async fn ask_sort(url: &str, values: &[Value]) -> Result<Vec<Value>, Response> {
    let payload = json!({ "values": values });
    match client::call(url, &payload).await {
        Err(DepError::Unreachable) => Err(degraded("srvcs-sortascending")),
        Ok((200, body)) => match body.get("result").and_then(Value::as_array) {
            Some(arr) => Ok(arr.clone()),
            None => Err(no_result("srvcs-sortascending")),
        },
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded("srvcs-sortascending")),
    }
}

/// Ask a float dependency for `result` as an `f64`, mapping failures to the
/// response this service should return.
async fn ask_float(url: &str, payload: &Value, dependency: &str) -> Result<f64, Response> {
    match client::call(url, payload).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => match body.get("result").and_then(Value::as_f64) {
            Some(v) => Ok(v),
            None => Err(no_result(dependency)),
        },
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute the `p`-th percentile of `values` via linear interpolation.
///
/// This service does no arithmetic of its own. It asks `srvcs-sortascending` to
/// sort the list, then computes the interpolation rank locally and delegates the
/// real-valued steps — `sorted[lo+1] - sorted[lo]`, `frac * diff`, and
/// `sorted[lo] + scaled` — to `srvcs-floatsubtract`, `srvcs-floatmultiply`, and
/// `srvcs-floatadd` respectively. Invalid input is rejected by a leaf dependency
/// and the resulting `422` is forwarded unchanged.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = PercentileResponse),
        (status = 422, description = "the list is empty, or an element is not a valid integer (forwarded)"),
        (status = 500, description = "a dependency returned an unusable response"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    if req.values.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "values must be a non-empty list" })),
        )
            .into_response();
    }

    // sorted = sortascending(values).result
    let sorted = match ask_sort(&deps.sortascending_url, &req.values).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let n = sorted.len();
    if n == 0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "values must be a non-empty list" })),
        )
            .into_response();
    }

    // Interpolation rank, computed locally.
    let idx = (req.p / 100.0) * ((n - 1) as f64);
    let lo = idx.floor() as usize;
    let frac = idx - lo as f64;

    let result: f64 = if lo + 1 >= n {
        // sorted[lo] as f64
        match sorted[lo].as_f64() {
            Some(v) => v,
            None => return no_result("srvcs-sortascending"),
        }
    } else {
        // diff = floatsubtract(sorted[lo+1], sorted[lo]).result
        let diff = match ask_float(
            &deps.floatsubtract_url,
            &json!({ "a": sorted[lo + 1], "b": sorted[lo] }),
            "srvcs-floatsubtract",
        )
        .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // scaled = floatmultiply(frac, diff).result
        let scaled = match ask_float(
            &deps.floatmultiply_url,
            &json!({ "a": frac, "b": diff }),
            "srvcs-floatmultiply",
        )
        .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // result = floatadd(sorted[lo], scaled).result
        match ask_float(
            &deps.floatadd_url,
            &json!({ "a": sorted[lo], "b": scaled }),
            "srvcs-floatadd",
        )
        .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "values": req.values, "p": req.p, "result": result })),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, PercentileResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-percentile");
        assert_eq!(
            info.concern,
            "comparison: p-th percentile of a list (linear interpolation)"
        );
        assert_eq!(
            info.depends_on,
            vec![
                "srvcs-sortascending",
                "srvcs-floatadd",
                "srvcs-floatmultiply",
                "srvcs-floatsubtract"
            ]
        );
    }
}
