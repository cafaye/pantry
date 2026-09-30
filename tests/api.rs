//! The HTTP surface, through the router and no socket.
//!
//! `tower::ServiceExt::oneshot` drives axum in-process, so every assertion here
//! is a function call. There is no port, no bind, no sleep and no retry in this
//! file, which is what makes "tests never touch the network" a property of the
//! test code rather than a promise in a README.

use std::path::Path;

use axum::body::{Body, to_bytes};
use axum::http::{HeaderName, Request, StatusCode, header};
use pantry::http::{self, AppState};
use pantry::registry::registry_dir;
use serde_json::{Value, json};
use tower::ServiceExt as _;

struct Response {
    status: StatusCode,
    content_type: Option<String>,
    trace_id: Option<String>,
    body: Value,
}

/// Renders the whole response, so an assertion's failure message shows the body
/// and not just the field that did not match.
impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status.as_u16(), self.body)
    }
}

/// W3C `traceparent`. `http` has no constant for it because it is a W3C header
/// and not one of the two that crate defines.
const TRACEPARENT: HeaderName = HeaderName::from_static("traceparent");

async fn call(app: axum::Router, uri: &str) -> Response {
    let request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = app.oneshot(request).await.expect("responds");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let trace_id = response
        .headers()
        .get("x-trace-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");

    Response {
        status,
        content_type,
        trace_id,
        body: serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "{uri} returned a body that is not JSON ({error}): {:?}",
                &bytes[..bytes.len().min(200)]
            )
        }),
    }
}

fn app() -> axum::Router {
    http::router(AppState::from_registry(
        pantry::registry::Registry::load(&registry_dir()).expect("the official registry loads"),
    ))
}

/// A pantry whose registry did not load — the state the process is in when the
/// registry directory is missing from the image. Built by pointing it at a
/// directory that does not exist rather than by synthesising an error, so the
/// test exercises the same path a deployment takes.
fn broken_app() -> axum::Router {
    http::router(AppState::from_dir(Path::new(
        "/nonexistent/pantry/registry",
    )))
}

fn names(response: &Response) -> Vec<String> {
    response.body["data"]
        .as_array()
        .expect("data is an array")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name").to_string())
        .collect()
}

// ------------------------------------------------------------ the registry

#[tokio::test]
async fn the_registry_is_the_whole_official_set_sorted_by_name() {
    let response = call(app(), "/v1/services").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        names(&response),
        [
            "billing", "caf", "darkroom", "guard", "identity", "muse", "pantry"
        ],
        "every official service, including pantry itself: a registry that cannot \
         describe the registry is one `caf dev` has to special-case"
    );

    let page = &response.body["page"];
    assert_eq!(page["has_more"], json!(false));
    assert_eq!(page["next_cursor"], Value::Null);
}

/// The shape `caf` is generated from. If a key is renamed or dropped the
/// generated client changes with it, so this list is the contract and the test
/// that holds it.
#[tokio::test]
async fn a_service_object_carries_exactly_the_documented_keys() {
    let response = call(app(), "/v1/services/identity").await;

    assert_eq!(response.status, StatusCode::OK);
    let entry = &response.body;
    let mut keys: Vec<&str> = entry
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "basePath",
            "consumes",
            "core",
            "dependencies",
            "description",
            "exposes",
            "kind",
            "language",
            "name",
            "owner",
            "repository",
        ],
        "every key is a key of the service's own cafaye.yml, plus kind and basePath"
    );

    assert_eq!(entry["name"], json!("identity"));
    assert_eq!(entry["language"], json!("go"));
    assert_eq!(entry["kind"], json!("api"));
    assert_eq!(entry["core"], json!("^0.1.0"));
    assert_eq!(entry["basePath"], json!("/v1"));
    assert_eq!(entry["exposes"]["api"], json!("openapi/v1.yaml"));
    assert_eq!(
        entry["repository"]["url"],
        json!("git@github.com:cafaye/identity.git")
    );
    assert_eq!(entry["repository"]["defaultBranch"], json!("master"));
    assert_eq!(entry["owner"]["team"], json!("identity"));
}

/// A service with no `exposes` and no `basePath` is a real shape today — guard
/// is it — and the response has to say so with nulls rather than by omitting
/// keys, so a generated client has a field to read.
#[tokio::test]
async fn an_absent_surface_is_null_rather_than_missing() {
    let response = call(app(), "/v1/services/guard").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["exposes"], Value::Null);
    assert_eq!(response.body["basePath"], Value::Null);
    assert_eq!(response.body["consumes"], json!([]));
    assert_eq!(
        response.body["dependencies"][0]["name"],
        json!("identity"),
        "guard depends on identity"
    );
}

/// A second service with no `exposes` and no `basePath`, and a different reason
/// for it: caf is the platform CLI, and its own manifest says it omits `exposes`
/// because it is a binary rather than because nobody has written the document
/// yet. The shape a client sees is the same one guard's is — which is the point
/// of the split. Its `kind` is curated for the same reason, and this row is
/// where that curation is visible from outside the repository.
#[tokio::test]
async fn a_registered_binary_serves_the_same_absent_surface_as_a_registered_gap() {
    let response = call(app(), "/v1/services/caf").await;

    assert_eq!(response.status, StatusCode::OK, "{response}");
    assert_eq!(response.body["name"], json!("caf"));
    assert_eq!(
        response.body["language"],
        json!("go"),
        "read off the manifest, like every other field"
    );
    assert_eq!(response.body["core"], json!("^0.2.0"));
    assert_eq!(
        response.body["exposes"],
        Value::Null,
        "caf declares no `exposes`, and absent is not empty"
    );
    assert_eq!(response.body["basePath"], Value::Null);
    assert_eq!(response.body["consumes"], json!([]));
    assert_eq!(response.body["dependencies"], json!([]));
    assert_eq!(
        response.body["kind"],
        json!("api"),
        "curated: the manifest declares no surface at all, so nothing can derive \
         it, and `api` is the only value the vocabulary and `check_kind` admit"
    );
}

#[tokio::test]
async fn one_service_is_a_bare_object_not_a_wrapped_one() {
    // `GET /v1/services/{name}` answers with the same object the list wraps, so
    // a client reads one shape and not two. core's convention wraps
    // *collections* in `data`/`page`; a single resource is not a collection.
    let single = call(app(), "/v1/services/billing").await;
    let list = call(app(), "/v1/services?language=ruby").await;

    assert_eq!(single.status, StatusCode::OK);
    assert_eq!(single.body["name"], json!("billing"));
    assert!(
        single.body.get("data").is_none(),
        "not a collection envelope"
    );
    assert_eq!(
        single.body, list.body["data"][0],
        "the same object either way"
    );
}

// -------------------------------------------------------------- filtering

#[tokio::test]
async fn every_filter_narrows_the_list() {
    let cases: &[(&str, &[&str])] = &[
        (
            "?kind=api",
            &[
                "billing", "caf", "darkroom", "guard", "identity", "muse", "pantry",
            ],
        ),
        // Two `go` repositories, and they are not the same thing: identity
        // serves HTTP, caf declares no surface at all. `language` is read off
        // the manifest, so it does not care which.
        ("?language=go", &["caf", "identity"]),
        ("?language=ruby", &["billing"]),
        ("?language=rust", &["darkroom", "pantry"]),
        (
            "?contract=%5E0.2.0",
            &["billing", "caf", "darkroom", "muse", "pantry"],
        ),
        // identity and guard are on ^0.1.0, which pre-1.0 does not contain ^0.2.0.
        ("?contract=%5E0.1.0", &["guard", "identity"]),
        ("?kind=api&language=python&contract=%5E0.2.0", &["muse"]),
    ];

    for (query, want) in cases {
        let response = call(app(), &format!("/v1/services{query}")).await;
        assert_eq!(response.status, StatusCode::OK, "{query}");
        assert_eq!(names(&response), *want, "{query}");
    }
}

#[tokio::test]
async fn a_filter_that_matches_nothing_is_an_empty_list() {
    for query in [
        "?kind=worker",
        "?language=elixir",
        "?contract=%5E9.0.0",
        // guard is the only `typescript` repository and it is on ^0.1.0, which
        // pre-1.0 does not contain ^0.2.0. This used to be `language=go` and
        // stopped being empty when caf registered: two filters are worth
        // nothing if they cannot survive the next entry in the registry.
        "?language=typescript&contract=%5E0.2.0",
    ] {
        let response = call(app(), &format!("/v1/services{query}")).await;

        assert_eq!(
            response.status,
            StatusCode::OK,
            "{query} is a question, not a mistake"
        );
        assert_eq!(response.body["data"], json!([]), "{query}");
        assert_eq!(response.body["page"]["has_more"], json!(false), "{query}");
    }
}

#[tokio::test]
async fn a_bad_filter_value_is_a_400_naming_the_vocabulary() {
    let cases = [
        ("?kind=database", "kind"),
        ("?language=cobol", "language"),
        ("?language=spec", "language"),
        ("?contract=1.x", "contract"),
        ("?unknown=1", "unknown"),
    ];

    for (query, parameter) in cases {
        let response = call(app(), &format!("/v1/services{query}")).await;

        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(
            response.content_type.as_deref(),
            Some("application/problem+json"),
            "{query}"
        );
        let problem = &response.body;
        assert_eq!(problem["code"], json!("validation_failed"), "{query}");
        assert_eq!(problem["status"], json!(400), "{query}");
        assert_eq!(
            problem["type"],
            json!("https://errors.cafaye.com/validation_failed")
        );
        assert_eq!(problem["instance"], json!("/v1/services"));
        assert!(
            problem["detail"].as_str().unwrap().contains(parameter),
            "{query} detail names the parameter: {problem}"
        );
    }
}

// ---------------------------------------------------------------- paging

#[tokio::test]
async fn a_page_limit_slices_the_list_and_the_cursor_finishes_it() {
    let first = call(app(), "/v1/services?limit=2").await;
    assert_eq!(names(&first), ["billing", "caf"]);
    assert_eq!(first.body["page"]["has_more"], json!(true));

    let cursor = first.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let second = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    assert_eq!(names(&second), ["darkroom", "guard"]);
    assert_eq!(
        second.body["page"]["has_more"],
        json!(true),
        "four of seven returned means three are left"
    );

    let cursor = second.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let third = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    assert_eq!(
        names(&third),
        ["identity", "muse"],
        "six of seven returned means one is left"
    );
    assert_eq!(third.body["page"]["has_more"], json!(true));

    let cursor = third.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let fourth = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    assert_eq!(names(&fourth), ["pantry"]);
    assert_eq!(fourth.body["page"]["has_more"], json!(false));
    assert_eq!(fourth.body["page"]["next_cursor"], Value::Null);
}

#[tokio::test]
async fn paging_parameters_are_bounded_and_their_errors_speak_problem_json() {
    let cases = [
        ("?limit=0", "validation_failed"),
        ("?limit=101", "validation_failed"),
        ("?limit=lots", "validation_failed"),
        ("?cursor=not-a-cursor", "validation_failed"),
    ];

    for (query, code) in cases {
        let response = call(app(), &format!("/v1/services{query}")).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(response.body["code"], json!(code), "{query}");
    }
}

// -------------------------------------------------------------- not found

#[tokio::test]
async fn an_unknown_service_is_a_404_in_the_core_error_envelope() {
    let response = call(app(), "/v1/services/nope").await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(
        response.content_type.as_deref(),
        Some("application/problem+json"),
        "core: every non-2xx is application/problem+json"
    );

    let problem = &response.body;
    assert_eq!(
        problem["type"],
        json!("https://errors.cafaye.com/not_found")
    );
    assert_eq!(problem["title"], json!("Not found"));
    assert_eq!(problem["status"], json!(404));
    assert_eq!(problem["instance"], json!("/v1/services/nope"));
    assert_eq!(problem["code"], json!("not_found"));
    assert!(
        problem["detail"].as_str().unwrap().contains("nope"),
        "the detail says which name was not found: {problem}"
    );
    assert!(
        problem["trace_id"].is_string(),
        "core: trace_id is always present: {problem}"
    );
    assert_eq!(
        problem["trace_id"].as_str(),
        response.trace_id.as_deref(),
        "the body's trace_id is the X-Trace-Id header"
    );
}

/// The probes are the one thing a service must never get wrong, because an
/// orchestrator restarts a process that answers them wrongly.
#[tokio::test]
async fn an_unknown_route_is_also_a_problem_json_404() {
    let response = call(app(), "/v1/nope").await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(
        response.content_type.as_deref(),
        Some("application/problem+json")
    );
    assert_eq!(response.body["code"], json!("not_found"));
}

#[tokio::test]
async fn a_wrong_method_is_a_405_that_still_speaks_problem_json() {
    // core: "No service invents its own error body", and an empty-bodied 405
    // from the framework is exactly that.
    let request = Request::builder()
        .method("POST")
        .uri("/v1/services")
        .body(Body::empty())
        .expect("request");
    let response = app().oneshot(request).await.expect("responds");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
    let problem: Value = serde_json::from_slice(&bytes).expect("a problem body");
    assert_eq!(problem["status"], json!(405));
}

// --------------------------------------------------------------- probes

#[tokio::test]
async fn healthz_is_liveness_and_says_nothing_about_the_registry() {
    // The split matters: a process whose registry failed is ALIVE and must not
    // be restarted in a loop. It is not READY, and that is /readyz's job.
    let response = call(app(), "/healthz").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], json!("ok"));

    let broken = call(broken_app(), "/healthz").await;
    assert_eq!(
        broken.status,
        StatusCode::OK,
        "a process with no registry is still alive"
    );
    assert_eq!(broken.body["status"], json!("ok"));
}

#[tokio::test]
async fn readyz_reports_a_loaded_registry_and_refuses_an_unloaded_one() {
    let ready = call(app(), "/readyz").await;
    assert_eq!(ready.status, StatusCode::OK);
    assert_eq!(ready.body["status"], json!("ok"));
    assert_eq!(
        ready.body["services"],
        json!(7),
        "readiness counts what it loaded"
    );

    let broken = call(broken_app(), "/readyz").await;
    assert_eq!(
        broken.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a 200 here would be a lie: there is nothing to serve"
    );
    assert_eq!(
        broken.content_type.as_deref(),
        Some("application/problem+json")
    );
    assert_eq!(broken.body["code"], json!("unavailable"));
    assert_eq!(broken.body["status"], json!(503));
    assert!(
        broken.body["detail"]
            .as_str()
            .unwrap()
            .contains("/nonexistent"),
        "the detail names the directory that could not be read: {broken}"
    );
}

// --------------------------------------------------------------- tracing

#[tokio::test]
async fn every_response_carries_a_trace_id() {
    for uri in [
        "/v1/services",
        "/v1/services/identity",
        "/v1/services/nope",
        "/healthz",
        "/readyz",
        "/v1/nope",
    ] {
        let response = call(app(), uri).await;
        let trace_id = response
            .trace_id
            .clone()
            .expect("X-Trace-Id on every response");

        assert_eq!(trace_id.len(), 32, "{uri}: {trace_id}");
        assert!(
            trace_id.chars().all(|c| c.is_ascii_hexdigit()),
            "{uri}: {trace_id}"
        );

        // Only a problem body repeats the id. A 200 has no `trace_id` field —
        // core requires the id on the problem body because that is the body a
        // support conversation starts from, and putting it on every success
        // response would be a second place to keep it correct.
        if response.status.is_client_error() {
            assert_eq!(
                response.body["trace_id"].as_str(),
                Some(trace_id.as_str()),
                "{uri}: a problem body repeats the header"
            );
        } else {
            assert!(
                response.body.get("trace_id").is_none(),
                "{uri}: a success body has no trace_id field: {response}"
            );
        }
    }
}

/// PLAN.md §7: "Adopt from first deploy: every service propagates traceparent".
/// An inbound traceparent's trace-id is the id this service answers with, so a
/// call chain is one id from end to end.
#[tokio::test]
async fn an_inbound_traceparent_is_propagated() {
    let request = Request::builder()
        .uri("/v1/services")
        .header(
            TRACEPARENT,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .body(Body::empty())
        .expect("request");
    let response = app().oneshot(request).await.expect("responds");

    assert_eq!(
        response.headers().get("x-trace-id").unwrap(),
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
}

#[tokio::test]
async fn a_malformed_traceparent_is_ignored_rather_than_echoed() {
    // A caller with a broken header still gets an answer, and still gets a fresh
    // id. Echoing the garbage would put it in every downstream log line.
    for header_value in [
        "",
        "nonsense",
        "00-short-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736",
    ] {
        let request = Request::builder()
            .uri("/healthz")
            .header(TRACEPARENT, header_value)
            .body(Body::empty())
            .expect("request");
        let response = app().oneshot(request).await.expect("responds");
        let trace_id = response
            .headers()
            .get("x-trace-id")
            .expect("X-Trace-Id")
            .to_str()
            .unwrap();

        assert_eq!(trace_id.len(), 32, "{header_value:?}");
        assert!(
            trace_id.chars().all(|c| c.is_ascii_hexdigit()),
            "{header_value:?}: {trace_id}"
        );
    }
}

// -------------------------------------------------------------- contract

/// The committed OpenAPI document and the router are the same fact stated
/// twice, so a test that fails when they drift is the only thing that makes
/// either of them trustworthy. `openapi/v1.yaml` is what `caf gen` reads.
#[tokio::test]
async fn the_openapi_document_and_the_router_agree() {
    let document: Value =
        serde_yaml::from_str(&std::fs::read_to_string("openapi/v1.yaml").expect("openapi/v1.yaml"))
            .expect("the document is YAML");

    let mut documented: Vec<(&str, &str)> = document["paths"]
        .as_object()
        .expect("paths")
        .iter()
        .flat_map(|(path, item)| {
            item.as_object().unwrap().keys().filter_map(move |method| {
                matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete")
                    .then_some((method.as_str(), path.as_str()))
            })
        })
        .collect();
    documented.sort_unstable();

    let mut served: Vec<(&str, &str)> = http::ROUTES.to_vec();
    served.sort_unstable();

    assert_eq!(
        served, documented,
        "the router and openapi/v1.yaml disagree: every documented operation must be \
         served and every served route documented"
    );
}

/// core's conventions: every path is under one `/v1` prefix, and the two probe
/// endpoints are infrastructure and deliberately not under it.
#[tokio::test]
async fn every_documented_path_follows_core_s_prefix_rule() {
    let document: Value =
        serde_yaml::from_str(&std::fs::read_to_string("openapi/v1.yaml").expect("openapi/v1.yaml"))
            .expect("the document is YAML");

    for path in document["paths"].as_object().expect("paths").keys() {
        let is_probe = path == "/healthz" || path == "/readyz";
        if is_probe {
            continue;
        }
        assert!(
            path.starts_with("/v1/"),
            "{path} is not under the /v1 prefix"
        );
    }

    assert!(
        document["info"]["version"].is_string(),
        "core: info.version is required"
    );
    // The two failure modes the collection and the single resource have, and
    // they are different: a bad filter on the list is a 400 (the request is
    // wrong), an unknown name on the resource is a 404 (the thing is absent).
    for (path, status) in [("/v1/services", "400"), ("/v1/services/{name}", "404")] {
        assert!(
            document["paths"][path]["get"]["responses"][status].is_object(),
            "{path} must document a {status}: a client generated from this document \
             cannot handle an error the document does not mention"
        );
    }
}
