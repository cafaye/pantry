//! The HTTP surface.
//!
//! Four routes and two probes, all `/v1` except the probes, because core's
//! conventions put the path prefix on contract surface and leave
//! `/healthz`/`/readyz` as infrastructure an orchestrator reaches by convention.
//!
//! The probes are the part worth reading. `/healthz` is liveness and answers
//! `200` whenever the process is running, *including when the registry failed
//! to load* — a process that cannot serve is still alive, and restarting it in a
//! loop turns a data problem into an outage. `/readyz` is the one that knows
//! whether the registry loaded and validated, so an orchestrator holds traffic
//! back instead of sending it somewhere that will answer with nothing. See
//! [`AppState`].
//!
//! Every response carries `X-Trace-Id`, and a non-2xx is RFC 9457
//! `application/problem+json` — including the framework's own 404 and 405,
//! which [`router`] replaces with fallbacks that speak it. "No service invents
//! its own error body" includes the framework's.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::filter::{Filter, Page};
use crate::problem::Problem;
use crate::registry::{Registry, RegistryError};
use crate::view::{HealthView, PageView, ServiceList, ServiceView};

/// Every route the router serves, as `(method, path)`, exactly as
/// `openapi/v1.yaml` spells it.
///
/// It exists so a test can hold the router and the committed OpenAPI document to
/// each other. A contract-first service whose document is not checked against
/// its router has a document that describes an intention.
pub const ROUTES: &[(&str, &str)] = &[
    ("get", "/v1/services"),
    ("get", "/v1/services/{name}"),
    ("get", "/healthz"),
    ("get", "/readyz"),
];

/// What the handlers need, and whether the registry is usable.
///
/// The load error is kept rather than discarded, for two reasons: `/readyz`
/// says which directory failed, and a failed registry leaves the process
/// running so the failure is observable instead of a crash loop.
#[derive(Debug, Clone)]
pub struct AppState {
    registry: Result<Arc<Registry>, Arc<RegistryError>>,
}

impl AppState {
    /// Loads from a registry directory, recording a failure rather than
    /// returning one. This is what `main` uses: a registry that will not load is
    /// a readiness problem, not a reason to refuse to start and hide behind a
    /// liveness probe that never gets a chance to fail.
    pub fn from_dir(dir: &std::path::Path) -> AppState {
        match Registry::load(dir) {
            Ok(registry) => AppState {
                registry: Ok(Arc::new(registry)),
            },
            Err(error) => {
                tracing::error!(%error, "the registry did not load; /readyz will report 503");
                AppState {
                    registry: Err(Arc::new(error)),
                }
            }
        }
    }

    pub fn from_registry(registry: Registry) -> AppState {
        AppState {
            registry: Ok(Arc::new(registry)),
        }
    }

    /// Readiness: did the registry actually load?
    pub fn is_ready(&self) -> bool {
        self.registry.is_ok()
    }

    pub fn registry(&self) -> Result<&Registry, &RegistryError> {
        match &self.registry {
            Ok(registry) => Ok(registry),
            Err(error) => Err(error),
        }
    }
}

/// The routes, with the trace layer and the two fallbacks.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/services", get(list_services))
        .route("/v1/services/{name}", get(get_service))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(axum::middleware::from_fn(trace_id))
        .with_state(state)
}

// ------------------------------------------------------------------ routes

/// `GET /v1/services` — the registry, filtered and paged.
///
/// A filter that matches nothing is a 200 with an empty array. A 404 would say
/// "there is no such collection", which is the one thing the caller already
/// knows is false.
async fn list_services(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<ServiceList>, Problem> {
    let trace_id = TraceId::current();
    let instance = "/v1/services";

    let filter = Filter::from_query(&query)
        .map_err(|error| Problem::validation_failed(instance, error.to_string(), &trace_id))?;
    let page = Page::from_query(&query)
        .map_err(|error| Problem::validation_failed(instance, error.to_string(), &trace_id))?;

    // An unloaded registry answers 503 rather than an empty list. `200` with
    // `data: []` would be indistinguishable from "the platform has no
    // services", which is the one answer a registry must never give.
    let registry = state
        .registry()
        .map_err(|error| Problem::unavailable(instance, error.to_string(), &trace_id))?;

    let entries = registry
        .page(&filter, &page)
        .map_err(|error| Problem::validation_failed(instance, error.to_string(), &trace_id))?;

    Ok(Json(ServiceList {
        data: entries.items.iter().map(|entry| entry.to_view()).collect(),
        page: PageView {
            next_cursor: entries.next_cursor,
            has_more: entries.has_more,
        },
    }))
}

/// `GET /v1/services/{name}` — one entry, or a 404 in the core envelope.
///
/// The body is the same object the list wraps, so a client reads one shape.
async fn get_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ServiceView>, Problem> {
    let trace_id = TraceId::current();
    let instance = format!("/v1/services/{name}");

    let registry = state
        .registry()
        .map_err(|error| Problem::unavailable(&instance, error.to_string(), &trace_id))?;

    let entry = registry.get(&name).ok_or_else(|| {
        Problem::not_found(
            &instance,
            format!("no official cafaye service is named {name:?}"),
            &trace_id,
        )
    })?;

    Ok(Json(entry.to_view()))
}

/// `GET /healthz` — liveness. Says nothing about the registry, on purpose.
async fn healthz() -> Json<HealthView> {
    Json(HealthView {
        status: "ok",
        services: None,
    })
}

/// `GET /readyz` — readiness. Checks the thing an orchestrator is about to send
/// traffic to.
///
/// No query parameters are read here. `/readyz` answers about pantry, not about
/// the registry, so a `?kind=` on it is ignored rather than refused — an
/// orchestrator that probes with a stray parameter still gets the answer it came
/// for, and the filters' vocabulary stays where it belongs, on
/// `GET /v1/services`.
async fn readyz(State(state): State<AppState>) -> Response {
    let trace_id = TraceId::current();
    let instance = "/readyz";

    match state.registry() {
        Ok(registry) => Json(HealthView {
            status: "ok",
            services: Some(registry.len()),
        })
        .into_response(),
        Err(error) => Problem::unavailable(
            instance,
            format!("the registry did not load, so pantry cannot answer: {error}"),
            &trace_id,
        )
        .into_response(),
    }
}

async fn not_found(method: axum::http::Method, uri: axum::http::Uri) -> Response {
    let trace_id = TraceId::current();
    let instance = uri.path();
    Problem::not_found(
        instance,
        format!("{method} {instance} is not a route on pantry"),
        &trace_id,
    )
    .into_response()
}

async fn method_not_allowed(method: axum::http::Method, uri: axum::http::Uri) -> Response {
    let trace_id = TraceId::current();
    Problem::method_not_allowed(uri.path(), &method, &trace_id).into_response()
}

// ------------------------------------------------------------------- trace

/// Reads the request's trace id, which the middleware below established.
struct TraceId;

impl TraceId {
    /// Reads the id this request's middleware established. A handler can only
    /// be reached through the router, so the fallback is unreachable in
    /// practice and exists so a future route cannot panic on it.
    fn current() -> String {
        TRACE_ID
            .try_with(|id| id.clone())
            .unwrap_or_else(|_| Uuid::new_v4().simple().to_string())
    }
}

tokio::task_local! {
    static TRACE_ID: String;
}

/// Gives every request an id, and every response the same one in
/// `X-Trace-Id`.
///
/// PLAN.md §7: "Adopt from first deploy: every service propagates
/// traceparent." An inbound `traceparent`'s trace-id becomes this service's
/// `X-Trace-Id`, so one call chain has one id from end to end and support starts
/// from it. A malformed header is ignored rather than echoed: a caller with a
/// broken header still gets an answer, and the garbage does not land in every
/// downstream log line.
async fn trace_id(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let trace_id = request
        .headers()
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .and_then(trace_id_from_traceparent)
        .unwrap_or_else(new_trace_id);

    let mut response = TRACE_ID
        .scope(trace_id.clone(), async move { next.run(request).await })
        .await;

    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-trace-id"), value);
    }

    response
}

/// The 32 hex characters of a W3C `traceparent`: `version-traceid-spanid-flags`.
/// Anything that is not exactly that shape is not a traceparent.
fn trace_id_from_traceparent(value: &str) -> Option<String> {
    let trace = value.split('-').nth(1)?;
    (trace.len() == 32 && trace.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| trace.to_ascii_lowercase())
}

/// A fresh 32-hex id, the same shape `trace_id_from_traceparent` accepts, so a
/// generated id and a propagated one are indistinguishable to a reader.
fn new_trace_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Renders the problem envelope with the content type core requires. There is
/// no other error type a handler can return, which is the mechanical reason
/// "no service invents its own error body" holds here.
impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        tracing::warn!(problem = %self, "request refused");

        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
