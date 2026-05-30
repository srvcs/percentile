use axum::body::Body;
use axum::extract::Json as JsonExtract;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_percentile::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Mock `srvcs-sortascending` that ACTUALLY COMPUTES: sorts the integer list and
/// returns `{"values", "result": <sorted array>}`.
async fn spawn_sortascending() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let mut nums: Vec<i64> = req["values"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_i64().unwrap_or(0))
                .collect();
            nums.sort_unstable();
            Json(json!({ "values": req["values"], "result": nums }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatsubtract` that ACTUALLY COMPUTES `a - b`.
async fn spawn_floatsubtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let a = req["a"].as_f64().unwrap_or(0.0);
            let b = req["b"].as_f64().unwrap_or(0.0);
            Json(json!({ "a": a, "b": b, "result": a - b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatmultiply` that ACTUALLY COMPUTES `a * b`.
async fn spawn_floatmultiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let a = req["a"].as_f64().unwrap_or(0.0);
            let b = req["b"].as_f64().unwrap_or(0.0);
            Json(json!({ "a": a, "b": b, "result": a * b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatadd` that ACTUALLY COMPUTES `a + b`.
async fn spawn_floatadd() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let a = req["a"].as_f64().unwrap_or(0.0);
            let b = req["b"].as_f64().unwrap_or(0.0);
            Json(json!({ "a": a, "b": b, "result": a + b }))
        }),
    );
    serve(app).await
}

/// Mock dependency answering `POST /` with a fixed status + body (used to
/// simulate a forwarded `422` rejection).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

fn app_with(deps: Deps) -> axum::Router {
    router(telemetry::metrics_handle_for_tests(), deps)
}

/// All four dependencies are real computing mocks.
async fn computing_deps() -> Deps {
    Deps {
        sortascending_url: spawn_sortascending().await,
        floatadd_url: spawn_floatadd().await,
        floatmultiply_url: spawn_floatmultiply().await,
        floatsubtract_url: spawn_floatsubtract().await,
    }
}

fn dead_deps() -> Deps {
    Deps {
        sortascending_url: DEAD_URL.to_string(),
        floatadd_url: DEAD_URL.to_string(),
        floatmultiply_url: DEAD_URL.to_string(),
        floatsubtract_url: DEAD_URL.to_string(),
    }
}

async fn eval(deps: Deps, values: Value, p: f64) -> (StatusCode, Value) {
    let res = app_with(deps)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "values": values, "p": p }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app_with(dead_deps())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn approx(got: &Value, expected: f64) {
    let g = got.as_f64().expect("result must be a number");
    assert!((g - expected).abs() < 1e-9, "expected ~{expected}, got {g}");
}

// --- Standard endpoints ---

#[tokio::test]
async fn index_ok() {
    assert_eq!(status_of("/").await, StatusCode::OK);
}

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

// --- Correctness, exercised against REAL computing dependencies (1e-9 tol) ---

#[tokio::test]
async fn median_of_even_list_interpolates() {
    // percentile([1,2,3,4], 50) = 2.5
    let (status, body) = eval(computing_deps().await, json!([1, 2, 3, 4]), 50.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 2.5);
    assert_eq!(body["values"], json!([1, 2, 3, 4]));
}

#[tokio::test]
async fn p0_is_minimum() {
    let (status, body) = eval(computing_deps().await, json!([4, 1, 3, 2]), 0.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 1.0);
}

#[tokio::test]
async fn p100_is_maximum() {
    let (status, body) = eval(computing_deps().await, json!([4, 1, 3, 2]), 100.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 4.0);
}

#[tokio::test]
async fn singleton_is_the_element() {
    // n == 1 => lo + 1 >= n, result = sorted[0], no float calls needed.
    let (status, body) = eval(computing_deps().await, json!([7]), 37.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 7.0);
}

#[tokio::test]
async fn quartile_interpolates_between_neighbors() {
    // sorted = [1,2,3,4,5], n=5, p=25 => idx = 0.25*4 = 1.0, exact -> 2.0
    let (status, body) = eval(computing_deps().await, json!([5, 4, 3, 2, 1]), 25.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 2.0);
}

#[tokio::test]
async fn fractional_rank_interpolates() {
    // sorted = [10,20,30], n=3, p=75 => idx = 0.75*2 = 1.5
    // lo=1, frac=0.5, diff = 30-20 = 10, scaled = 5, result = 20 + 5 = 25
    let (status, body) = eval(computing_deps().await, json!([10, 20, 30]), 75.0).await;
    assert_eq!(status, StatusCode::OK);
    approx(&body["result"], 25.0);
}

// --- Error / edge cases ---

#[tokio::test]
async fn empty_list_is_422() {
    let (status, _) = eval(computing_deps().await, json!([]), 50.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn forwards_422_from_sortascending() {
    let deps = Deps {
        sortascending_url: spawn_fixed(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "error": "element is not an integer" }),
        )
        .await,
        floatadd_url: DEAD_URL.to_string(),
        floatmultiply_url: DEAD_URL.to_string(),
        floatsubtract_url: DEAD_URL.to_string(),
    };
    let (status, body) = eval(deps, json!([1, "nope", 3]), 50.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "element is not an integer");
}

#[tokio::test]
async fn degrades_when_sortascending_is_unreachable() {
    let (status, body) = eval(dead_deps(), json!([1, 2, 3, 4]), 50.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-sortascending");
}

#[tokio::test]
async fn degrades_when_floatsubtract_is_unreachable() {
    let deps = Deps {
        sortascending_url: spawn_sortascending().await,
        floatadd_url: spawn_floatadd().await,
        floatmultiply_url: spawn_floatmultiply().await,
        floatsubtract_url: DEAD_URL.to_string(),
    };
    let (status, body) = eval(deps, json!([1, 2, 3, 4]), 50.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatsubtract");
}
