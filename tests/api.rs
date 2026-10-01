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

/// What `registry/services/<name>/cafaye.yml` itself says `core:` is.
///
/// This exists because a *previous* version of this file hardcoded the pins —
/// `^0.1.0` for identity, courier and guard — and the fleet-wide raise to
/// `^0.2.0` turned four tests here red. The pins were never the thing under
/// test: `every_filter_narrows_the_list` is about whether the filter reads the
/// same field it serves, and a hardcoded pin makes that test fail every time core
/// cuts a release while saying nothing whatever about the filter.
///
/// Reading the file is not a tautology. The registry is *loaded* from disk at
/// startup and then projected through `serde_json` into an HTTP body, so this
/// compares what a manifest says against what a client fetches, with the whole
/// parse-and-project path in between. The claim that survives is the one worth
/// making: **pantry serves each manifest's `core` unchanged.** The claim that
/// was being made — "courier is on ^0.1.0" — was core's to change, and core
/// changed it.
fn manifest_core_pin(service: &str) -> String {
    let path = registry_dir()
        .join("services")
        .join(service)
        .join("cafaye.yml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    text.lines()
        .find_map(|line| line.strip_prefix("core:"))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| {
            panic!(
                "{} declares no top-level `core:`. A registry entry with no pin cannot be \
                 filtered by contract, and this helper exists so that is a panic here rather \
                 than a `null` in somebody's generated client.",
                path.display()
            )
        })
}

// ------------------------------------------------------------ the registry

#[tokio::test]
async fn the_registry_is_the_whole_official_set_sorted_by_name() {
    let response = call(app(), "/v1/services").await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        names(&response),
        [
            "billing",
            "caf",
            "cafaye-ts",
            "courier",
            "darkroom",
            "guard",
            "identity",
            "muse",
            "pantry"
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
    assert_eq!(
        entry["core"],
        json!(manifest_core_pin("identity")),
        "served unchanged from registry/services/identity/cafaye.yml. The value is core's to \
         move — it moved once already, from ^0.1.0 to ^0.2.0, and this file did not follow, \
         which is what this assertion is for"
    );
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
        json!("cli"),
        "curated, and true: the manifest declares no surface at all, so nothing can derive \
         this value, and `api` for a binary was the registry recording a falsehood"
    );
}

/// The entry a reader is most likely to misfile, pinned where a client can see
/// it.
///
/// cafaye-ts vendors the fleet's OpenAPI documents — one per service that has
/// one, six in all — into `specs/`, and it serves none of them. Its own manifest
/// spends fourteen lines on exactly this, and the sentence that matters is that
/// the documents "are vendored INPUTS … the provenance record for an input is
/// not a contract surface". So on the wire the six documents are visible only as
/// their **absence**, which is the only honest way to show them: `exposes` is
/// `null`, and there is no base path to prefix anything with.
#[tokio::test]
async fn a_package_that_vendors_the_fleets_documents_serves_none_of_them() {
    let response = call(app(), "/v1/services/cafaye-ts").await;

    assert_eq!(response.status, StatusCode::OK, "{response}");
    assert_eq!(response.body["name"], json!("cafaye-ts"));
    assert_eq!(response.body["language"], json!("typescript"));
    assert_eq!(response.body["core"], json!("^0.2.0"));
    assert_eq!(response.body["kind"], json!("cli"));
    assert_eq!(
        response.body["exposes"],
        Value::Null,
        "six OpenAPI documents are vendored in specs/ and not one of them is a surface \
         this package serves. `exposes.api: specs/` would be the easy mistake and it \
         would say this package SERVES those APIs; it serves nothing"
    );
    assert_eq!(
        response.body["basePath"],
        Value::Null,
        "and with no document there is no prefix: basePath is derived from a document \
         that exists, and a prefix invented for a repository that publishes no surface \
         is the guess core's rule exists to prevent"
    );
    assert_eq!(response.body["consumes"], json!([]));
    assert_eq!(
        response.body["dependencies"],
        json!([]),
        "absent rather than empty is a different fact. The package builds on all six \
         services, and `required: true` is the only value the schema offers, which is \
         untrue — so the relationship is recorded where a sha can carry it, in \
         specs/index.json, and here it is honestly absent"
    );
}

/// The two entries whose `kind` cannot be derived, and the reason they are not
/// the same value. Both manifests declare no contract surface, so both rows are
/// curated — `guard` is a service waiting for its document and `caf` is a
/// binary that will never have one, and nothing in either manifest tells those
/// apart. This is the visible consequence of that: a client can now ask for
/// `?kind=cli` and get caf, which it could not do before `cli` was a value.
#[tokio::test]
async fn the_two_curated_kinds_are_distinguishable_from_outside() {
    let cli = call(app(), "/v1/services?kind=cli").await;
    assert_eq!(cli.status, StatusCode::OK, "{cli}");
    assert_eq!(
        names(&cli),
        ["caf", "cafaye-ts"],
        "the artifacts a person installs, and the two the manifest cannot describe: a \
         command, and a package that is imported rather than run. Both are `cli` because \
         `api` and `both` would be false and `worker` is refused for a manifest with no \
         surface — see the DECISION NEEDED on cafaye-ts's row for what that leaves \
         unsaid, and for why the Ruby gem of the same shape is held back instead"
    );

    let api = call(app(), "/v1/services?kind=api").await;
    assert_eq!(api.status, StatusCode::OK, "{api}");
    assert_eq!(
        names(&api),
        [
            "billing", "courier", "darkroom", "guard", "identity", "muse", "pantry"
        ],
        "guard stays here: it serves HTTP and has not written the document yet, which is a \
         gap rather than a false answer"
    );
}

/// The one entry whose OpenAPI document is not shaped like the others, and the
/// part of that which is observable from outside: `exposes.api` is served
/// **verbatim**. `openapi.yaml`, at the repository root, is what the manifest
/// says and that is what a client reads and fetches. pantry does not normalise it
/// to `openapi/v1.yaml` for symmetry, because a client that reads this field has
/// to be right.
///
/// This test was named for courier's document being PARTIAL, and it was:
/// courier-03 shipped it covering `/v1/webhook_endpoints` only, with the
/// notification preferences routes in the router and not in the file. courier-05
/// completed it to every route the router serves except the two probes. The rule
/// that made it registrable then — a partial document still yields a base path
/// from the paths it publishes — is unchanged and still pinned, by a synthetic
/// document in `a_partial_openapi_document_still_yields_a_base_path_from_the_\
/// paths_it_publishes` in `tests/manifest.rs`, because the next service to
/// publish an incomplete document has to meet it too. What is pinned *here* is
/// the path, which has not changed and is the part a client can get wrong.
#[tokio::test]
async fn a_document_at_the_repository_root_is_served_verbatim() {
    let response = call(app(), "/v1/services/courier").await;

    assert_eq!(response.status, StatusCode::OK, "{response}");
    assert_eq!(response.body["name"], json!("courier"));
    assert_eq!(response.body["language"], json!("elixir"));
    assert_eq!(
        response.body["core"],
        json!(manifest_core_pin("courier")),
        "what the manifest says, read from registry/services/courier/cafaye.yml. Its own \
         comment once called the pin unresolved; the registry records the file, it does not \
         predict core's first release — so the assertion is against the file, not against a \
         remembered version. This test was written when the answer was ^0.1.0 and stayed green \
         through the fleet-wide raise only because the registry copy was stale"
    );
    assert_eq!(
        response.body["exposes"]["api"],
        json!("openapi.yaml"),
        "the repository-relative path the manifest states, not a normalised one"
    );
    assert_eq!(response.body["basePath"], json!("/v1"));
    assert_eq!(
        response.body["kind"],
        json!("api"),
        "derived, not curated: exposes.api is declared and consumes is empty"
    );
    assert_eq!(
        response.body["exposes"]["events"],
        json!([
            "courier.email.queued",
            "courier.email.delivered",
            "courier.email.bounced",
            "courier.email.complained",
            "courier.notification.suppressed"
        ]),
        "three segments each, which is the whole reason courier was excluded and \
         is now registered: `courier-03` renamed them to satisfy core's grammar"
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
                "billing", "courier", "darkroom", "guard", "identity", "muse", "pantry",
            ],
        ),
        // Neither `cli` is in the `api` list, and neither was ever honestly in
        // it. caf was there because the vocabulary had no value for a binary and
        // the registry recorded the least-wrong answer; `?kind=cli` is the true
        // one, and a client that routed on `api` would have looked for HTTP on a
        // command. cafaye-ts is the same refusal about a package: it serves
        // nothing either, whatever the six documents it vendors might suggest.
        ("?kind=cli", &["caf", "cafaye-ts"]),
        // Two `go` repositories, and they are not the same thing: identity
        // serves HTTP, caf declares no surface at all. `language` is read off
        // the manifest, so it does not care which.
        ("?language=go", &["caf", "identity"]),
        ("?language=ruby", &["billing"]),
        ("?language=elixir", &["courier"]),
        ("?language=rust", &["darkroom", "pantry"]),
        // Two `typescript` repositories and they are not the same thing, which
        // is the point of asking by language: guard serves HTTP on ^0.1.0 and
        // cafaye-ts is the client, on ^0.2.0, serving nothing.
        ("?language=typescript", &["cafaye-ts", "guard"]),
        (
            "?kind=api&language=python&contract=%5E0.2.0",
            &["muse"],
        ),
    ];

    for (query, want) in cases {
        let response = call(app(), &format!("/v1/services{query}")).await;
        assert_eq!(response.status, StatusCode::OK, "{query}");
        assert_eq!(names(&response), *want, "{query}");
    }
}

/// `?contract=` narrows by the **same field it serves**, and that is the claim.
///
/// It used to be pinned by listing which services were on `^0.1.0` and which on
/// `^0.2.0` — a hardcoded partition of a fleet that core is entitled to move,
/// which is why this file went red on the raise rather than saying anything
/// about the filter. So the partition is now derived from the registry on every
/// run, and held in **both** directions:
///
///   * every distinct pin in the registry selects exactly the services carrying
///     it — a filter that reads a different field than it serves fails here;
///   * the pins **partition** the fleet — every service appears under exactly one
///     pin. Without this half, a filter that ignored `contract` and returned
///     everything would satisfy the first half on a one-pin fleet, which is what
///     the fleet was for two weeks.
#[tokio::test]
async fn the_contract_filter_partitions_the_fleet_by_the_pin_each_service_serves() {
    let everything = call(app(), "/v1/services").await;
    assert_eq!(everything.status, StatusCode::OK);
    let all = names(&everything);
    assert!(!all.is_empty(), "the fleet is not empty, so this can fail");

    let mut by_pin: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for service in &all {
        let one = call(app(), &format!("/v1/services/{service}")).await;
        assert_eq!(one.status, StatusCode::OK, "{service}");
        let pin = one.body["core"]
            .as_str()
            .unwrap_or_else(|| panic!("{service} serves no string `core`"))
            .to_string();
        by_pin.entry(pin).or_default().push(service.clone());
    }

    let mut covered: Vec<String> = Vec::new();
    for (pin, mut members) in by_pin {
        let query = format!("/v1/services?contract={}", pin.replace('^', "%5E"));
        let response = call(app(), &query).await;
        assert_eq!(response.status, StatusCode::OK, "{query}");
        let mut got = names(&response);
        got.sort_unstable();
        members.sort_unstable();
        assert_eq!(
            got, members,
            "?contract={pin} did not select exactly the services that serve it. The filter and \
             the field it is supposed to read have come apart"
        );
        covered.extend(members);
    }

    covered.sort_unstable();
    let mut expected = all.clone();
    expected.sort_unstable();
    assert_eq!(
        covered, expected,
        "the pins do not partition the fleet: a service is under no pin, or under two"
    );
}

#[tokio::test]
async fn a_filter_that_matches_nothing_is_an_empty_list() {
    for query in [
        "?kind=worker",
        "?kind=both",
        // courier is the only elixir service and it is on ^0.1.0, so `elixir`
// A contract pin no service carries. `^9.0.0` has never been one, and
        // cannot become one without a service being written against it.
        "?contract=%5E9.0.0",
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

/// Two filters that cannot both match must return an empty list.
///
/// This is the case the list above used to carry by naming a `language` and a
/// `contract` that happened not to intersect — `language=elixir&contract=^0.2.0`
/// and `language=python&contract=^0.1.0`, both of which stopped being empty the
/// moment the fleet raised every pin, and both of which were therefore asserting
/// a coincidence rather than a rule. The intersection is now **computed**: pick
/// two filters the fleet makes disjoint on purpose — a language exactly one
/// service has, and a pin that service does not carry — so the emptiness is a
/// consequence of the argument and not of what happens to be registered.
///
/// The negative space is worth the trouble: it is the only thing that tells a
/// client a filter that returned nothing means *nothing matched* rather than
/// *the conjunction is not implemented and returned an error's neighbour*.
#[tokio::test]
async fn two_filters_the_fleet_makes_disjoint_return_an_empty_list() {
    let everything = call(app(), "/v1/services").await;
    let mut by_language: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    let mut every_pin: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for service in names(&everything) {
        let one = call(app(), &format!("/v1/services/{service}")).await;
        let language = one.body["language"].as_str().expect("a language").to_string();
        let pin = one.body["core"].as_str().expect("a core pin").to_string();
        every_pin.insert(pin.clone());
        by_language.entry(language).or_default().push((service, pin));
    }

    let mut checked = 0usize;
    let mut unreached: Vec<String> = Vec::new();
    for (language, members) in &by_language {
        if members.len() != 1 {
            // Two services share this language, so excluding one on a pin
            // cannot be shown to exclude the other — the pair would not be
            // disjoint. Skipping is not the same as asserting, which is what
            // `checked` below is for.
            continue;
        }
        let (service, its_pin) = &members[0];
        let Some(other_pin) = disjoint_from(its_pin) else {
            unreached.push(format!("{service} is on {its_pin}, a pin form this cannot reason about"));
            continue;
        };

        let query = format!(
            "/v1/services?language={language}&contract={}",
            other_pin.replace('^', "%5E")
        );
        let response = call(app(), &query).await;

        assert_eq!(response.status, StatusCode::OK, "{query}");
        assert_eq!(
            response.body["data"],
            json!([]),
            "{query} cannot match: {service} is the only {language} service, and {other_pin} \
             cannot contain {its_pin}. A hit here means the filter is not applying both terms"
        );
        checked += 1;
    }

    assert!(
        unreached.is_empty(),
        "{}. A pin form this cannot reason about is a reason to look, not a reason to stop \
         asking — teach this test the form or say why it cannot be reasoned about",
        unreached.join("; ")
    );
    assert!(
        checked > 0,
        "no language in this fleet is carried by exactly one service, so this test proved \
         nothing. That is a fact about the registry worth failing on rather than a test to \
         delete — a fleet where every language is shared has no disjoint pair to check"
    );
}

/// A pin that provably cannot contain any version `pin` can contain.
///
/// Caret on a `0.x` version pins the **minor**, so `^0.2.0` is `>=0.2.0 <0.3.0`
/// and `^0.1.0` is `>=0.1.0 <0.2.0` — the two ranges cannot meet. That is a
/// property of semver a client already relies on when it filters by contract, so
/// using it here tests the filter rather than restating the standard.
///
/// Returns `None` for anything this cannot reason about, and the caller fails
/// on that rather than skipping: a pin form this does not understand is a
/// reason to look, not a reason to stop asking.
fn disjoint_from(pin: &str) -> Option<String> {
    let rest = pin.strip_prefix("^0.")?;
    let minor = rest.split('.').next()?.parse::<u32>().ok()?;
    (minor > 0).then(|| format!("^0.{}.0", minor - 1))
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

    assert_eq!(names(&second), ["cafaye-ts", "courier"]);
    assert_eq!(
        second.body["page"]["has_more"],
        json!(true),
        "four of nine returned means five are left"
    );

    let cursor = second.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let third = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    assert_eq!(
        names(&third),
        ["darkroom", "guard"],
        "six of nine returned means three are left"
    );
    assert_eq!(third.body["page"]["has_more"], json!(true));

    let cursor = third.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let fourth = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    assert_eq!(names(&fourth), ["identity", "muse"]);
    assert_eq!(fourth.body["page"]["has_more"], json!(true));

    let cursor = fourth.body["page"]["next_cursor"]
        .as_str()
        .expect("a next cursor")
        .to_string();
    let fifth = call(app(), &format!("/v1/services?limit=2&cursor={cursor}")).await;

    // One entry left, so one entry returned: the limit is a maximum and an odd
    // length produces a short page. A client that assumed full pages would ask
    // for a sixth and find nothing.
    assert_eq!(names(&fifth), ["pantry"]);
    assert_eq!(fifth.body["page"]["has_more"], json!(false));
    assert_eq!(fifth.body["page"]["next_cursor"], Value::Null);
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
        json!(9),
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

/// The `kind` vocabulary is written in three places — `src/registry.rs`,
/// `openapi/v1.yaml` and `README.md` — and the path a value takes through all
/// three is short enough for one of them to be forgotten. The document is the
/// one that matters most: it is what `caf gen` reads, so a value the enum does
/// not carry becomes an unknown string in a generated client rather than a
/// compile error at the point where pantry serves it.
#[tokio::test]
async fn the_documented_kind_vocabulary_is_the_vocabulary_that_is_served() {
    let document: Value =
        serde_yaml::from_str(&std::fs::read_to_string("openapi/v1.yaml").expect("openapi/v1.yaml"))
            .expect("the document is YAML");

    let documented: Vec<String> = document["components"]["schemas"]["ServiceKind"]["enum"]
        .as_array()
        .expect("ServiceKind is an enum in the document")
        .iter()
        .map(|value| value.as_str().expect("a string value").to_string())
        .collect();

    let served: Vec<String> = pantry::registry::ServiceKind::all()
        .iter()
        .map(|kind| kind.to_string())
        .collect();

    assert_eq!(
        served, documented,
        "ServiceKind in openapi/v1.yaml and ServiceKind in src/registry.rs are the same \
         vocabulary written twice. A value added to one and not the other is a client that \
         cannot read a response pantry serves — and the enum is what `caf gen` generates from"
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
