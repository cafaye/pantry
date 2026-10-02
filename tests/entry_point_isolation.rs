//! A negative case per entry point, driven through the router.
//!
//! ## The rule this file holds: **absence, never 403**
//!
//! A 403 is a 403 that says *"it exists and is not yours"*. That is worse than a
//! 404 for a caller who is probing, and it is the oracle darkroom-09 was written
//! to remove from `identity` and `courier`. So every negative here asserts two
//! things together:
//!
//! 1. the status is not 401 or 403 — and not 405 either, because a 405 that
//!    differs between a registered name and an unregistered one is the same
//!    oracle wearing a different status; and
//! 2. the response **shape** is byte-identical to the baseline for a name that
//!    has never existed.
//!
//! By "shape" it means the fields a machine branches on — `status`, `type`,
//! `title`, `code`, and the content type — with `detail` and `instance` masked.
//! Those two are masked on purpose and for a stated reason: both quote the
//! caller's *own* input back. `instance` is the path that was asked for and
//! `detail` is the name that was asked for, so neither can tell a caller
//! anything it did not already know. A test that demanded byte-identical bodies
//! including `detail` would be demanding that pantry stop telling a caller what
//! it asked for, which would be a worse service.
//!
//! ## What "the other account" is here
//!
//! pantry has no accounts, so `tests/scoping.rs` (the enumeration half) explains
//! why at length. The boundary that *is* real is **the registry, and nothing
//! outside it**, and the names below are chosen to attack exactly that:
//!
//! * **The six excluded repositories** — `core`, `docs`, `cafaye-rb`,
//!   `cafaye-py`, `site`, `kit`. These are real cafaye repositories,
//!   three of which carry a valid `cafaye.yml` (`core`, `docs`, `cafaye-rb`) and
//!   one more of which carries one that does not parse (`site`), and they
//!   are held back with a stated reason. A registry that answered for one of
//!   them would be publishing a service the platform cannot start. This is the
//!   closest thing to "account B's data" that exists in this repository, and it
//!   is the set most worth pinning. `site` was `parlor` renamed; `parlor` itself
//!   used to be on this list and left it when its manifest came to validate and
//!   it was registered, so this is a set of six now and the count of negative
//!   names below moved with it. `site` is read from the index rather than written
//!   here, so the set itself is not duplicated in this file.
//! * **Path shapes** — `..`, `../courier`, `%2e%2e%2fetc%2fpasswd`. The `{name}`
//!   segment is the only free text a caller controls, and these are the values
//!   that would read a file if it reached a `Path::join`.
//! * **Near misses** — `Courier`, `courier `, `courierX`. The exact byte
//!   comparison in `Registry::get` is what makes each of them a miss, and a
//!   normalisation anywhere in that path would turn one into a hit.
//!
//! ## The counts
//!
//! | entry point                              | op      | negative cases |
//! | ---------------------------------------- | ------- | -------------- |
//! | `GET /v1/services`                       | list    | 19            |
//! | `GET /v1/services/{name}`                | read    | 45            |
//! | `GET /healthz`                           | probe   | 6             |
//! | `GET /readyz`                            | probe   | 6             |
//! | fallback (`not_found`)                   | —       | 8             |
//! | `method_not_allowed` fallback            | —       | 16 + 2        |
//! | **update**                               | update  | **0 entry points** |
//! | **delete**                               | delete  | **0 entry points** |
//!
//! update and delete have **zero** entry points and that is asserted rather than
//! assumed: `every_write_verb_on_every_registered_path_is_405` sweeps all four
//! write verbs against all four registered paths, 16 requests, and requires every
//! one to be 405 in the core envelope.
//!
//! **The counts here that are deliberately not pinned** are the sizes of the two
//! sets this file *derives* — the on-disk probe set in
//! `the_only_names_that_answer_two_hundred_are_the_registered_ones` and the
//! exclusion record in `excluded_names`. Both are floored or checked for
//! contradictions instead, and the reason is worth recording: an exact count on
//! the first went red when this packet added its own report file, which turned a
//! **reach** guard into a tripwire on the repository's file count, and the second
//! went red in pantry-23 when `site` became a seventh excluded repository.
//! Neither was a defect — a new file is not a defect and a new curation row is
//! not a defect — and a number that goes red for that reason teaches people to
//! bump it rather than read it. Both sets are read from the index or the
//! filesystem, so growing correctly needs no edit here at all.
//!
//! The one count that IS pinned is the number of negative **names** each entry
//! point is probed with, and it is pinned for the opposite reason: it is a
//! number a reader checks against the table above, so a class added without
//! updating the table should go red.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderName, Request, StatusCode, header};
use pantry::http::{AppState, router};
use pantry::registry::{Registry, registry_dir};
use serde_json::Value;
use tower::ServiceExt as _;

/// A name that has never existed, is not a cafaye repository, and cannot become
/// one by a request. Every negative is measured against the shape of this.
const NEVER_EXISTED: &str = "zz-no-such-service-9f3a";

/// The repositories `registry/index.yml` holds back, with a stated reason.
///
/// Read from the index rather than written here, so a row added to the
/// exclusion record is a new name this file immediately probes. A curation list
/// that grows is a curation list whose held-back entries need a test.
///
/// # The count that was here, and why it is gone
///
/// This function used to end `assert_eq!(names.len(), 6, …)`, and pantry-23
/// removed it. The trigger was ordinary: `site` became a seventh excluded
/// repository and the number went red. What is worth recording is that the
/// number could only ever go red for that reason.
///
/// `names` is read out of the index three lines above the assertion, so "a new
/// row is a new probe" — the thing the failure message said it was protecting —
/// is true by construction and needs no number to defend it. What the number
/// actually watched was the size of a data file, and this file's module header
/// had already ruled that shape of assertion out on the sibling count, for
/// exactly the reason it applies here: *"a number that goes red because a file
/// grew teaches people to bump it rather than read it."* A curation row is not
/// a defect.
///
/// The two invariants a reader might expect to find in its place are NOT
/// reimplemented here, because both are already checked and reimplementing them
/// would be a duplicate that reads like coverage:
///   * no excluded name is also a registered one —
///     `tests/schema.rs::a_registered_service_is_never_also_excluded`;
///   * the record is not empty — `tests/schema.rs::every_exclusion_reason_is_
///     still_true`, and the `!index.excluded.is_empty()` assert at its head.
///
/// What is left is the one vacuity risk that is LOCAL to this file and that
/// neither of those covers: the `excluded` arm of the probe loop below
/// contributes zero cases if this returns nothing, and the `probed` count it
/// feeds is a written-down number somebody can bump to match. A hardcoded total
/// plus an empty source is a suite that verifies nothing and reports 46.
fn excluded_names() -> Vec<String> {
    let dir = registry_dir();
    let text = std::fs::read_to_string(dir.join("index.yml")).expect("registry/index.yml reads");
    let index: pantry::registry::RegistryIndex =
        serde_yaml::from_str(&text).expect("registry/index.yml parses");
    let names: Vec<String> = index.excluded.iter().map(|row| row.name.clone()).collect();

    assert!(
        !names.is_empty(),
        "registry/index.yml held back no repositories, so the `excluded` arm of the probe loop \
         below contributed zero cases and the {PROBED} negative names this file claims to send \
         were all near-misses and path shapes. Every probe in this file still passed, which is the \
         point: this suite reports on shape, and a missing CLASS is invisible to a shape check."
    );

    names
}

/// The number of negative names `read_a_name_that_is_not_a_registry_entry…`
/// sends. Named so the vacuity assertion above can say what the total would have
/// been, rather than leaving a reader to count.
///
/// **45, not 46, and the 46 is the number to be careful about.** This count moved
/// DOWN by one when `parlor` was registered, and it is worth being explicit about
/// why that is not a lost probe.
///
/// The three sources are `excluded_names()` (read out of the index), the
/// near-miss list and the path-shape list. Two of the three are written here and
/// have not moved: 29 near misses, 10 path shapes. The third is DERIVED, and
/// `parlor` leaving the exclusion record took it from seven rows to six. So 6 +
/// 29 + 10 = 45, and the arithmetic is the argument — the count did not shrink
/// because a class was dropped, it shrank because a curation row stopped being
/// needed.
///
/// That is also why `parlor` must NOT be added to the near-miss list to "put the
/// number back". It is a registered service now: it answers 200, and adding it
/// to a negative sweep would assert that a registry entry is absent. The right
/// place for the fact that `parlor` is reachable is
/// `the_only_names_that_answer_two_hundred_are_the_registered_ones` below, which
/// derives that set from the registry and so already contains it.
const PROBED: usize = 45;

struct Response {
    status: StatusCode,
    content_type: Option<String>,
    body: Value,
}

/// What a caller can branch on, with the caller's own input masked.
///
/// This is the whole of "the same answer as nonexistent". A status a machine
/// switches on, a `code` a machine switches on, and a content type — identical
/// across every negative in this file.
fn shape(response: &Response) -> String {
    let field = |name: &str| {
        response
            .body
            .get(name)
            .map_or("missing".to_string(), |value| value.to_string())
    };
    format!(
        "{} status={} type={} title={} code={} content_type={}",
        response.status.as_u16(),
        field("status"),
        field("type"),
        field("title"),
        field("code"),
        response.content_type.as_deref().unwrap_or("none"),
    )
}

/// The status a caller sees, plus the machine fields. A negative that answers
/// 401 or 403 fails here with the status in the message — that is the finding
/// darkroom-09's rule exists to produce, and it must be loud.
fn assert_not_an_oracle(response: &Response, what: &str) {
    assert!(
        !matches!(
            response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ),
        "{what} answered {}. 401/403 says \"this exists and is not yours\" — an enumeration \
         oracle. pantry has no accounts and no authorization, so the only honest answer here is \
         absence: 404 for a name, 200 for a list, 400 for a parameter it does not read.",
        response.status.as_u16()
    );
}

fn app() -> axum::Router {
    router(AppState::from_registry(
        Registry::load(&registry_dir()).expect("the official registry loads"),
    ))
}

async fn call(method: &str, uri: &str) -> Response {
    call_with_headers(method, uri, &[]).await
}

async fn call_with_headers(method: &str, uri: &str, headers: &[(&str, &str)]) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("a request");

    for (name, value) in headers {
        request.headers_mut().insert(
            HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
            value.parse().expect("a header value"),
        );
    }

    let response = app().oneshot(request).await.expect("pantry responds");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    Response {
        status,
        content_type,
        body,
    }
}

/// The baseline every negative is compared against: the answer for a name that
/// has never existed and cannot.
async fn baseline_for_a_missing_name() -> (String, String) {
    let response = call("GET", &format!("/v1/services/{NEVER_EXISTED}")).await;
    assert_not_an_oracle(&response, "a name that has never existed");
    assert_eq!(
        response.status,
        StatusCode::NOT_FOUND,
        "a name that cannot exist must be a 404, and it is {}. If this is not 404 the baseline every \
         negative in this file is measured against is wrong.",
        response.status.as_u16()
    );
    (
        shape(&response),
        response.body["detail"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

// ================================================================ read: 1 entry point

/// `GET /v1/services/{name}` — 45 negative names, one shape.
#[tokio::test]
async fn read_a_name_that_is_not_a_registry_entry_answers_exactly_as_one_that_never_existed() {
    let (baseline, _) = baseline_for_a_missing_name().await;

    // 6 excluded repositories + 39 more (29 near misses, 10 path shapes), grouped by what
    // each is trying to do
    // so a reader can see the classes rather than count a list.
    //
    // Percent-encoding throughout, and that is not decoration. A space, a
    // newline and a null byte are not legal URI characters, so `Request::builder`
    // refuses to build the request and the case never reaches a handler at all —
    // a fact about `http`'s parser rather than about pantry, and a test counting
    // it would be counting the wrong thing. The encoded spellings are what a
    // client actually sends, and they are the ones that arrive decoded at
    // `Registry::get`.
    let excluded: Vec<String> = excluded_names();
    let near_misses: Vec<String> = vec![
        "Courier",            // case
        "COURIER",            // case
        "courier%20",         // trailing space
        "%20courier",         // leading space
        "cour%20ier",         // interior space
        "courierx",           // one character longer
        "courie",             // one character shorter
        "courier%0A",         // a trailing newline
        "courier%09",         // a tab
        "%00courier",         // a leading null byte
        "cafaye",             // the workspace, which is not a service
        "cafaye-ts2",         // a near miss on a real name with a digit
        "index",              // index.yml, without the extension
        "cafaye.yml",         // this repository's own manifest file
        "courier%0Acourier",  // two names in one segment
        "courier%00.yaml",    // an extension with a null in front of it
        "identity%23courier", // a fragment character, encoded
        "courier%3Fkind=api", // a query character inside a path segment
        "courier%2F",         // a trailing encoded slash
        "%2Fcourier",         // a leading encoded slash
        // Scope-shaped names. These are the strings a 403 oracle would be
        // written around — `if name.contains("account") { forbidden() }` is a
        // three-line edit and a plausible one, because "this exists and is not
        // yours" is what a tenant boundary looks like to somebody adding one.
        // They are here so that edit fails here rather than shipping, and they
        // belong beside the near misses because they attack the same property:
        // the answer must depend only on whether the name is a registry entry.
        "account-1",
        "account_1",
        "accounts",
        "tenant",
        "tenant-1",
        "tenant_1",
        "org-1",
        "user-1",
        "courier%2Faccount-1",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let path_shapes: Vec<String> = vec![
        "..",
        ".",
        "../courier",
        "../../etc/passwd",
        "services/courier",
        "registry/services/courier/cafaye.yml",
        "openapi/v1.yaml",
        // Percent-encoded: axum decodes a path parameter, so these reach
        // `Registry::get` as `../etc/passwd` and `..`. If the decoded value ever
        // reaches a filesystem call this is the test that catches it.
        "%2e%2e%2fetc%2fpasswd",
        "%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "%2e%2e%2fservices%2fcourier%2fcafaye.yml",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    let mut probed = 0;
    for (label, names) in [
        ("an excluded repository", excluded),
        ("a near miss on a real name", near_misses),
        ("a path shape", path_shapes),
    ] {
        for name in names {
            let uri = format!("/v1/services/{name}");
            let response = call("GET", &uri).await;
            probed += 1;

            assert_not_an_oracle(&response, &format!("{label} asked for as `{name}`"));
            assert_eq!(
                shape(&response),
                baseline,
                "{label} `{name}` does not answer exactly as a name that never existed. The \
                 shape differs, which means a caller probing names can tell a miss from a miss in \
                 some other way."
            );
        }
    }

    assert_eq!(
        probed, PROBED,
        "this test probes {probed} negative names, not {PROBED}. Update the count in the module \
         header when a class is added or removed — it is the number a reader checks."
    );
}

/// The complement of the sweep above, and the stronger of the two: the set of
/// names that answer 200 is **exactly the registry**, so nothing in this
/// repository that is not an entry is reachable by name.
#[tokio::test]
async fn the_only_names_that_answer_two_hundred_are_the_registered_ones() {
    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    let registered: Vec<String> = registry
        .entries()
        .iter()
        .map(|entry| entry.name().to_string())
        .collect();
    assert_eq!(
        registered.len(),
        10,
        "the registry holds {} entries, not 10. An eleventh is a new reachable name and this \
         test's baseline has moved.",
        registered.len()
    );

    // Every directory under the repository's own root, plus every file in it,
    // plus the six excluded repositories. These are the names that *exist* on
    // this machine and are not registry entries — the shapes an accidental
    // filesystem read would answer.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut present: Vec<String> = std::fs::read_dir(root)
        .expect("the repository root reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    present.extend(excluded_names());
    present.sort();
    present.dedup();

    let mut reachable: Vec<String> = Vec::new();
    for name in &present {
        let response = call("GET", &format!("/v1/services/{name}")).await;
        if response.status == StatusCode::OK {
            reachable.push(name.clone());
        } else {
            assert_not_an_oracle(&response, &format!("`{name}`, which exists on disk"));
            assert_eq!(
                response.status,
                StatusCode::NOT_FOUND,
                "`{name}` exists in this repository and is not a registry entry, so it must be a \
                 404. It is {}. Either a directory or file here is reachable by name — which is \
                 the reach failure `tests/scoping.rs` guards at the source — or the answer is \
                 wrong.",
                response.status.as_u16()
            );
        }
    }

    assert!(
        reachable.is_empty(),
        "{reachable:?} are files and directories in this repository and answer 200 from \
         GET /v1/services/{{name}}. The registry is curated data; a repository path is not a \
         service."
    );

    // The size of the probe set is floored, not pinned. An earlier version
    // asserted it exactly, and that was the test measuring the wrong thing:
    // adding this packet's own report file moved the number by one and turned a
    // **reach** guard into a tripwire on the repository's file count. A new
    // top-level file is not a defect, so an exact count would have gone red for
    // no reason and taught everyone to bump a number rather than read the
    // failure.
    //
    // What is still asserted is that the set is real work: a walk that returned
    // nothing, or stopped finding the held-back repositories, would make every
    // assertion above vacuous. So the floor is on the root's own entries and the
    // held-back names must be *in* the set, not merely counted.
    let excluded = excluded_names();
    let from_root = present.len()
        - excluded
            .iter()
            .filter(|name| present.contains(name))
            .count();

    assert!(
        from_root >= 15,
        "the walk over this repository's root found only {from_root} entries, so most of the \
         assertions above ran against an empty set and this test was vacuous. The walk is \
         `read_dir(CARGO_MANIFEST_DIR)`; if this fires, that read is not seeing this repository."
    );
    for name in &excluded {
        assert!(
            present.contains(name),
            "the held-back repository `{name}` is not in the probe set, so the strongest negative \
             case in this file did not run. `excluded_names()` read the index and the walk read \
             the root, and the two must overlap."
        );
    }
}

// =============================================================== list: 1 entry point

/// `GET /v1/services` — 19 negative parameters, one shape.
#[tokio::test]
async fn list_refuses_a_parameter_it_does_not_read_exactly_as_a_typo() {
    // The baseline: a parameter that is plainly a typo. If a scope-shaped
    // parameter were answered differently — 403, or 400 with a longer message, or
    // 200 with a narrower list — that difference would be the oracle.
    let typo = call("GET", "/v1/services?colour=blue").await;
    assert_not_an_oracle(&typo, "a mistyped parameter");
    assert_eq!(
        typo.status,
        StatusCode::BAD_REQUEST,
        "an unknown parameter must be a 400, and it is {}. core's rule is that a 400 is for \
         \"malformed syntax the client could not have known\"; an unread parameter is exactly \
         that, and a 404 would be indistinguishable from an empty registry.",
        typo.status.as_u16()
    );
    let baseline = shape(&typo);

    // The shape of the question this packet was asked, asked of pantry. None of
    // these is a filter, and all of them must be refused like the typo — not
    // honoured, and not answered with a 403 that confirms the registry has
    // accounts to be scoped by.
    let scope_shaped = [
        "account_id=other",
        "accountId=other",
        "tenant=other",
        "tenant_id=other",
        "org_id=other",
        "organization=other",
        "owner=other",
        "user_id=other",
        "customer_id=other",
        "scope=other",
        "visible_to=other",
    ];

    // And the parameters that are read but were given a value outside the
    // vocabulary. These must also be indistinguishable from the typo: a caller
    // probing `?kind=` learns nothing about what is registered, because a bad
    // value and a bad name are the same answer.
    let out_of_vocabulary = [
        "kind=database",
        "language=spec",
        "contract=not%20a%20constraint",
        "contract=",
        "limit=0",
        "limit=101",
        "limit=abc",
        "cursor=!!!!",
    ];

    let mut probed = 0;
    for query in scope_shaped.iter().chain(out_of_vocabulary.iter()) {
        let uri = format!("/v1/services?{query}");
        let response = call("GET", &uri).await;
        probed += 1;

        assert_not_an_oracle(&response, &format!("`?{query}`"));
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "`?{query}` must be a 400 and it is {}. A parameter pantry does not read is a 400 with \
             the vocabulary in the message, whatever the parameter is called.",
            response.status.as_u16()
        );
        assert_eq!(
            shape(&response),
            baseline,
            "`?{query}` does not answer exactly as a mistyped parameter. If the message names it \
             differently, a caller can tell a parameter pantry recognises-but-refuses from one it \
             has never heard of — which is the same oracle as a 403, in a 400."
        );
    }

    assert_eq!(
        probed, 19,
        "this test probes {probed} negative parameters, not 19, and the module header says 19 for \
         the list entry point. Fix the header and this together."
    );
}

/// A filter that matches nothing is an empty list, never a 404 — and never a
/// narrower one. This is the list half of "absence, never 403": a caller that
/// filters to a combination nobody has must not be able to tell that combination
/// from one that is forbidden.
#[tokio::test]
async fn a_filter_that_matches_nothing_answers_an_empty_list_and_not_a_refusal() {
    // `go` and `^9.0.0` are outside the vocabulary of a registry this
    // repository holds, so nothing can match. A *combinations* that are in
    // vocabulary but incompatible is the better test, so it is here too.
    for query in [
        // `go` and `rust` are both registered, so these are combinations rather
        // than impossible values — the better test, because a caller can construct
        // one of them from the vocabulary alone.
        "kind=worker&language=go",
        "kind=cli&language=ruby",
        "kind=worker&language=python",
        // A constraint no service can satisfy: every registered entry is `^0.x`.
        "contract=^99.0.0",
        "kind=api&contract=^99.0.0",
        "kind=api&contract=^99.0.0&language=python",
    ] {
        let response = call("GET", &format!("/v1/services?{query}")).await;
        assert_not_an_oracle(&response, &format!("`?{query}`"));
        assert_eq!(
            response.status,
            StatusCode::OK,
            "a filter that matches nothing must be a 200 with an empty array, and `?{query}` is \
             {}. A 404 would say \"there is no such collection\", which is the one thing the \
             caller already knows is false.",
            response.status.as_u16()
        );
        assert_eq!(
            response.body["data"].as_array().map(Vec::len),
            Some(0),
            "`?{query}` must return `data: []` and returned {}. An empty list and a narrower one \
             are different answers.",
            response.body["data"]
        );
        assert_eq!(
            response.body["page"]["has_more"], false,
            "`?{query}` returned an empty page that claims to have more."
        );
        assert_eq!(
            response.body["page"]["next_cursor"],
            Value::Null,
            "`?{query}` returned an empty page with a cursor to follow."
        );
    }
}

/// The list is the same for every caller, because there is no caller to differ.
///
/// The headers below are the shapes a tenant scope arrives in. None of them
/// changes the answer, and the assertion is on the *body*, so a header that
/// started being read would fail here rather than silently narrowing the list.
#[tokio::test]
async fn the_list_is_byte_identical_whatever_scope_shaped_header_a_caller_sends() {
    let plain = call("GET", "/v1/services").await;
    assert_eq!(plain.status, StatusCode::OK, "the registry lists");
    let expected = plain.body.clone();

    // Names only. No value is written into the failure message on purpose: this
    // file does not print header values, so a failure here reports which header
    // changed the answer without echoing what it carried.
    let scope_headers = [
        "x-account-id",
        "x-tenant-id",
        "x-org-id",
        "x-owner",
        "x-user-id",
        "authorization",
        "cookie",
        "x-forwarded-user",
    ];

    for name in scope_headers {
        let response = call_with_headers("GET", "/v1/services", &[(name, "someone-else")]).await;
        assert_eq!(
            response.status, plain.status,
            "the `{name}` header changed the status of GET /v1/services"
        );
        assert_eq!(
            response.body, expected,
            "the `{name}` header changed the body of GET /v1/services. pantry reads no request \
             header, so every caller is served the whole registry — and a header that narrowed it \
             would be a scope nobody designed."
        );
    }

    // And the same for the read path, which is the one a scope would narrow.
    let courier = call("GET", "/v1/services/courier").await;
    assert_eq!(courier.status, StatusCode::OK, "courier is registered");
    for name in scope_headers {
        let response =
            call_with_headers("GET", "/v1/services/courier", &[(name, "someone-else")]).await;
        assert_eq!(
            response.body, courier.body,
            "the `{name}` header changed GET /v1/services/courier. If a header can withhold one \
             registered service it can also be used to probe for one, and the answer would be a \
             403 in all but name."
        );
    }
}

// ============================================== probes: 2 entry points, 6 cases each

/// `/healthz` and `/readyz` read no query, so a stray parameter — including a
/// scope-shaped one — is ignored rather than refused, and cannot change the
/// answer. A probe that answered per caller would be a scope computed from
/// nothing.
#[tokio::test]
async fn neither_probe_can_be_scoped_and_a_stray_parameter_does_not_change_the_answer() {
    let mut probed = 0;
    for (probe, expected_status) in [("/healthz", StatusCode::OK), ("/readyz", StatusCode::OK)] {
        let plain = call("GET", probe).await;
        assert_eq!(
            plain.status,
            expected_status,
            "{probe} answers {} with no query",
            plain.status.as_u16()
        );
        let expected = plain.body.clone();

        for query in [
            "account_id=other",
            "tenant=other",
            "owner=other",
            "kind=api",
            "limit=1",
            "name=courier",
        ] {
            let response = call("GET", &format!("{probe}?{query}")).await;
            assert_eq!(
                response.status,
                expected_status,
                "{probe}?{query} answered {}. The probes read no query parameters on purpose — an \
                 orchestrator that probes with a stray one still gets the answer it came for.",
                response.status.as_u16()
            );
            assert_eq!(
                response.body, expected,
                "{probe}?{query} answered differently from {probe}. A probe answers about pantry, \
                 not about a caller, so there is nothing for a parameter to scope on."
            );
            probed += 1;
        }
    }

    assert_eq!(
        probed, 12,
        "this test probes {probed} probe queries, not 12 (2 probes x 6)."
    );
}

/// `/readyz` reports the registry's size, and that number is the whole registry
/// rather than a caller's slice. If it were per caller, the readiness probe would
/// be a registry-size oracle.
#[tokio::test]
async fn readiness_reports_the_whole_registry_and_never_a_callers_share_of_it() {
    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    let response = call("GET", "/readyz").await;

    assert_eq!(response.status, StatusCode::OK, "/readyz answers 200");
    assert_eq!(
        response.body["services"].as_u64(),
        Some(registry.len() as u64),
        "/readyz reports {:?} and the registry holds {}. If these ever differ, one of them is \
         scoped and the other is not.",
        response.body["services"],
        registry.len()
    );
    assert_eq!(
        response.body["status"], "ok",
        "/readyz is not reporting ok for a registry that loaded"
    );
}

// =============================================== fallback: 1 entry point, 8 cases

/// The `not_found` fallback — a path that matches no route. A caller probing
/// pantry's shape learns only that the path is not a route, and the answer is
/// indistinguishable from a service that does not exist.
#[tokio::test]
async fn an_unmatched_path_answers_exactly_as_a_missing_route() {
    let baseline_response = call("GET", "/v1/accounts").await;
    assert_not_an_oracle(&baseline_response, "a path that matches no route");
    assert_eq!(
        baseline_response.status,
        StatusCode::NOT_FOUND,
        "an unmatched path must be a 404, and it is {}. pantry installs its own fallback so the \
         framework's empty-bodied 404 never escapes — \"no service invents its own error body\" \
         includes the framework's.",
        baseline_response.status.as_u16()
    );
    let baseline = shape(&baseline_response);

    let paths = [
        "/v1/accounts",
        "/v1/accounts/1",
        "/v1/tenants",
        "/v1/services/a/b/c",
        "/admin",
        "/",
        "/v1",
        "/healthz/extra",
    ];

    let mut probed = 0;
    for path in paths {
        let response = call("GET", path).await;
        probed += 1;
        assert_not_an_oracle(&response, path);
        assert_eq!(
            shape(&response),
            baseline,
            "{path} does not answer exactly as another path that matches no route. The framework \
             falling through to its own 404 for one shape and not another is how a caller maps a \
             service's surface without being told it."
        );
    }

    assert_eq!(
        probed, 8,
        "this test probes {probed} unmatched paths, not 8."
    );
}

// ================================ method_not_allowed: 1 entry point, 16 + 2 cases

/// update and delete have **zero** entry points, and this is how that is
/// asserted rather than assumed.
///
/// Four write verbs against all four registered paths: 16 requests, every one
/// 405 in the core envelope. A pantry that could be written to would answer one
/// of these with a 2xx, and the count of non-2xx answers is the assertion.
#[tokio::test]
async fn every_write_verb_on_every_registered_path_is_405_and_never_a_success() {
    let verbs = ["POST", "PUT", "PATCH", "DELETE"];
    let paths = [
        "/v1/services",
        "/v1/services/courier",
        "/healthz",
        "/readyz",
    ];

    let baseline_response = call("POST", "/v1/services").await;
    assert_not_an_oracle(&baseline_response, "a write verb on a read route");
    let baseline = shape(&baseline_response);

    let mut probed = 0;
    for verb in verbs {
        for path in paths {
            let response = call(verb, path).await;
            probed += 1;

            assert!(
                !response.status.is_success(),
                "{verb} {path} answered {}. pantry is read-only: the registry is curated data in \
                 this repository, and a runtime write is an unaudited registration surface. This \
                 is update or delete and it cannot arrive as a commit.",
                response.status.as_u16()
            );
            assert_not_an_oracle(&response, &format!("{verb} {path}"));
            assert_eq!(
                shape(&response),
                baseline,
                "{verb} {path} does not answer exactly as {verb} on another read route. A 405 \
                 that differed by path would tell a caller which paths exist."
            );
        }
    }

    assert_eq!(
        probed, 16,
        "this test sweeps {probed} verb/path pairs, not 16 (4 verbs x 4 paths)."
    );
}

/// The 405 must not be an existence oracle either.
///
/// `DELETE /v1/services/courier` and `DELETE /v1/services/no-such-service` both
/// match the route `/v1/services/{name}` and are both refused for the same
/// reason — the verb, not the name. If the second answered 404 and the first 405,
/// a caller could enumerate the registry with `DELETE`, which is exactly the
/// oracle a 403 would be.
#[tokio::test]
async fn a_write_verb_answers_the_same_whether_or_not_the_name_is_registered() {
    let registered = call("DELETE", "/v1/services/courier").await;
    let unregistered = call("DELETE", "/v1/services/zz-no-such-service-9f3a").await;
    let never = call("DELETE", &format!("/v1/services/{NEVER_EXISTED}")).await;

    for (label, response) in [
        ("a registered name", &registered),
        ("an unregistered name", &unregistered),
        ("a name that cannot exist", &never),
    ] {
        assert_not_an_oracle(response, &format!("DELETE on {label}"));
    }

    assert_eq!(
        shape(&registered),
        shape(&unregistered),
        "DELETE answers differently for a registered name than for an unregistered one. Both match \
         the route and both are refused for the verb, so any difference is a statement about \
         whether the name exists."
    );
    assert_eq!(
        shape(&registered),
        shape(&never),
        "DELETE on a registered name answers differently from DELETE on a name that has never \
         existed. Same route, same verb, same reason for the refusal."
    );
    assert_eq!(
        registered.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "a write verb on a read route is 405 and it is {}. A 404 would be the route not matching, \
         which is not what happened.",
        registered.status.as_u16()
    );
}

/// A GET on a path that does not match is 404 and a GET on one that does is 200
/// — which *is* an existence oracle, and is the one pantry has on purpose.
///
/// The registry is public curated data: official cafaye services, published so
/// `caf dev`, `caf deploy` and a developer reading the platform all get one
/// answer. So this is recorded as a fact rather than treated as a defect, and
/// the test exists so that the decision is written down: if the registry ever
/// stops being public, this is the line that has to change, and it should change
/// deliberately.
#[tokio::test]
async fn the_registry_is_public_and_serving_two_hundred_is_deliberate() {
    let hit = call("GET", "/v1/services/courier").await;
    assert_eq!(hit.status, StatusCode::OK, "courier is registered");
    let miss = call("GET", &format!("/v1/services/{NEVER_EXISTED}")).await;
    assert_eq!(miss.status, StatusCode::NOT_FOUND, "the baseline misses");

    // What makes it deliberate rather than an oversight: the *content* served is
    // the same content the repository holds, verbatim, and every caller is
    // served all of it. Nothing here is per caller, so there is no caller-shaped
    // thing to withhold.
    let list = call("GET", "/v1/services").await;
    let served = list.body["data"].as_array().expect("an array").len();
    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    assert_eq!(
        served,
        registry.len(),
        "GET /v1/services serves {served} of the registry's {}. A partial list would be a scope.",
        registry.len()
    );
}
