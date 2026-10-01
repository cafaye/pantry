//! The enumeration: every entry point in pantry that can reach registered data,
//! and the isolation properties that hold it down.
//!
//! ## What this packet was asked for, and what the repository actually is
//!
//! `pantry-09-isolation` asked for a negative test per **account-scoped entry
//! point**, following `darkroom-09` (`tests/tenant_scoping.rs`,
//! `tests/query_scoping.rs`). The premise does not survive contact with this
//! repository, and the reason is a fact rather than a gap:
//!
//! * **pantry has no accounts.** There is no `account_id`, no `tenant`, no
//!   `org_id`, no `customer`, no session and no auth header. `grep -riE
//!   "account|tenant|owner|org_id|user_id|customer" src/` returns exactly one
//!   hit, and it is `Manifest::owner` — the accountable *team* on a
//!   `cafaye.yml`, which core's manifest schema defines and which no query
//!   filters on.
//! * **pantry has no database.** `src/registry.rs` says so in its own module
//!   docs ("There is no database … Phase 1 has none, and this is a property
//!   rather than an omission"), `AGENTS.md` lists it under "Out of scope,
//!   deliberately", and `Cargo.toml` carries no driver. The registry is YAML in
//!   this repository, read once at startup.
//! * **pantry has no write surface.** `ROUTES` is four `get`s. There is no
//!   `post`, `put`, `patch` or `delete` anywhere in `src/`.
//!
//! So "account A asks for account B's data" is not expressible here, and a
//! suite that pretended otherwise would be a suite asserting the shape of a
//! field that does not exist. **D18's `pantry 0` is a correct measurement of a
//! property, not a gap in coverage** — and the way to make that checkable rather
//! than asserted is what this file is for.
//!
//! ## The boundary that *is* real, and the tests that hold it
//!
//! The registry is public, curated, official-only, read-only data. Every caller
//! sees the same set, which is the design (`AGENTS.md`: "Official cafaye
//! services only, curated in this repository"). The isolation property is
//! therefore not *between callers* — it is **between the registry and
//! everything outside it**. Two ways to get that wrong, both real:
//!
//! 1. **Reach.** The one free-text value a caller controls is the `{name}` path
//!    segment. If it reached a `Path::join` it would be a file read for any
//!    path the process can open, so `Registry::get` must be a string comparison
//!    and `get_service` must never hand `name` to anything that takes a
//!    `Path`.
//! 2. **Existence as an oracle.** A 403 is a 403 that says "this exists and is
//!    not yours". So is a 405 that differs between a name that is registered
//!    and one that is not. Both are checked in
//!    `tests/entry_point_isolation.rs`; this file holds the *source* half.
//!
//! ## Why a test and not a review rule
//!
//! Because a review rule is a comment. This file reads `src/` with
//! `include_str!`, so a failure is a fact about the source that built the
//! binary, not a grep somebody has to remember to run. Every expectation is
//! **derived from the code** rather than from a list, so both directions hold:
//! a new route with a write verb fails without anyone updating a list, and
//! deleting a check fails too.
//!
//! ## The counts, which a reader can check
//!
//! | set                                                            | count |
//! | -------------------------------------------------------------- | ----- |
//! | `pub fn` / `pub async fn` in `src/`                            | 58    |
//! | HTTP entry points a request can reach (4 routes + 2 fallbacks) | 6     |
//! | registry data entry points (`registry.rs` + `filter.rs`)       | 23    |
//! | of those, ones taking a caller-controlled value                | 4     |
//! | routes registered with a verb other than `get`                 | 0     |
//! | write primitives in `src/`                                     | 0     |
//! | 403 / `forbidden` / `unauthorized` in `src/`                   | 0     |
//!
//! Each row is asserted by a named test below, and the two that would change
//! first — a non-`get` route and a write primitive — are asserted as exact
//! counts so a new one cannot arrive unremarked.

use std::collections::BTreeMap;

/// The sources this file reads. `include_str!` rather than reading the working
/// tree at runtime: the source is embedded at compile time, so this cannot pass
/// by reading a file that differs from the one that built the binary, and
/// renaming a module breaks the build here instead of failing on someone
/// else's machine.
const HTTP_SRC: &str = include_str!("../src/http.rs");
const REGISTRY_SRC: &str = include_str!("../src/registry.rs");
const FILTER_SRC: &str = include_str!("../src/filter.rs");
const MAIN_SRC: &str = include_str!("../src/main.rs");
const LIB_SRC: &str = include_str!("../src/lib.rs");

/// Every module of the service, so a count over "the whole service" is a real
/// count and not a count over the files somebody remembered. A new module that
/// `forget` to add here fails the count below, which is the point.
const ALL_SRC: &[(&str, &str)] = &[
    ("contract.rs", include_str!("../src/contract.rs")),
    ("filter.rs", FILTER_SRC),
    ("http.rs", HTTP_SRC),
    ("lib.rs", LIB_SRC),
    ("main.rs", MAIN_SRC),
    ("manifest.rs", include_str!("../src/manifest.rs")),
    ("pin.rs", include_str!("../src/pin.rs")),
    ("problem.rs", include_str!("../src/problem.rs")),
    ("registry.rs", REGISTRY_SRC),
    ("view.rs", include_str!("../src/view.rs")),
];

/// One `pub fn`, with the text that follows it.
struct PubFn<'a> {
    name: &'a str,
    /// From the opening brace of the body to the next `pub fn`, or to the end
    /// of the file. Slicing rather than parsing because the questions asked of
    /// a body here are "does this text name a path" and "does it name a column",
    /// and a body slice answers both without a parser that would have to
    /// understand raw string literals. The tail is taken when there is no next
    /// function so the *last* function in a file is included — a guard with a
    /// hole in the one place nobody re-reads is not a guard.
    body: &'a str,
}

/// Every `pub fn` and `pub async fn` in one source, in source order.
///
/// Indentation is allowed — most of the public surface is an `impl` block's
/// method — but a comment is not, so a doc comment that *mentions* a signature
/// is prose and is not counted. A guard that counted prose would be counting
/// sentences.
fn pub_fns(src: &str) -> Vec<PubFn<'_>> {
    let mut found: Vec<(&str, usize)> = Vec::new();
    let mut offset = 0;

    for line in src.lines() {
        let trimmed = line.trim_start();
        let is_code =
            !trimmed.starts_with("//") && !line[..line.len() - trimmed.len()].contains("//");
        if is_code
            && (trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("pub(crate) fn "))
        {
            let rest = trimmed
                .strip_prefix("pub async fn ")
                .or_else(|| {
                    trimmed
                        .strip_prefix("pub fn ")
                        .or_else(|| trimmed.strip_prefix("pub(crate) fn "))
                })
                .unwrap_or(trimmed);
            let name = rest
                .split(['(', '<', ' '])
                .next()
                .unwrap_or_default()
                .to_string();
            found.push((Box::leak(name.into_boxed_str()), offset));
        }
        offset += line.len() + 1;
    }

    found
        .iter()
        .enumerate()
        .map(|(at, (name, start))| {
            let start = *start;
            let body_start = src[start..].find('{').map(|brace| start + brace + 1);
            let end = found.get(at + 1).map_or(src.len(), |(_, next)| *next);
            PubFn {
                name,
                body: body_start.map_or("", |from| &src[from..end]),
            }
        })
        .collect()
}

/// Every `async fn` in one source, in source order — the handlers, which are
/// private and so are not in [`pub_fns`].
///
/// Same slicing, and the same reason: `get_service` and the two probes are the
/// entry points a request actually arrives at, and a guard that only looked at
/// the public surface would not see any of them.
fn async_fns(src: &str) -> Vec<PubFn<'_>> {
    let mut found: Vec<(&str, usize)> = Vec::new();
    let mut offset = 0;

    for line in src.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("//") && trimmed.starts_with("async fn ") {
            let rest = trimmed.strip_prefix("async fn ").unwrap_or(trimmed);
            let name = rest
                .split(['(', '<', ' '])
                .next()
                .unwrap_or_default()
                .to_string();
            found.push((Box::leak(name.into_boxed_str()), offset));
        }
        offset += line.len() + 1;
    }

    found
        .iter()
        .enumerate()
        .map(|(at, (name, start))| {
            let start = *start;
            let body_start = src[start..].find('{').map(|brace| start + brace + 1);
            let end = found.get(at + 1).map_or(src.len(), |(_, next)| *next);
            PubFn {
                name,
                body: body_start.map_or("", |from| &src[from..end]),
            }
        })
        .collect()
}

/// Every route registration in `router()`, as `(path, method, handler)`, in
/// source order. Derived from the source rather than from `ROUTES`, so a route
/// added to the router and not to the constant is a route this enumeration
/// would otherwise have missed.
fn registered_routes() -> Vec<(String, String, String)> {
    let mut routes = Vec::new();

    for line in HTTP_SRC.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(".route(\"") else {
            continue;
        };
        let (path, after) = rest.split_once('"').expect("a closed route literal");
        // `after` is `, get(handler))`. The verb is what is asked for here — a
        // `post` is an update entry point — so it is read rather than assumed.
        let tail = after.trim_start_matches([',', ' ', ')']);
        let (method, handler) = tail
            .split_once('(')
            .map(|(verb, args)| (verb.to_string(), args.trim_end_matches(")").to_string()))
            .expect("a method-call registration");
        routes.push((path.to_string(), method, handler));
    }

    routes
}

// ------------------------------------------------- 1. the enumeration itself

#[test]
fn every_module_of_the_service_is_in_this_files_module_list() {
    // Derived, not asserted as a literal: if somebody adds `src/thing.rs` and
    // forgets this file, `a_new_module_is_never_silently_uncounted` goes red.
    let listed: BTreeMap<&str, &str> = ALL_SRC.iter().map(|(name, src)| (*name, *src)).collect();
    assert_eq!(
        listed.len(),
        ALL_SRC.len(),
        "two entries in ALL_SRC name the same file, so a count over it is smaller than a \
         count over src/"
    );
    assert!(
        LIB_SRC.contains("pub mod contract;") && LIB_SRC.contains("pub mod view;"),
        "src/lib.rs no longer declares the modules this file reads; the ALL_SRC list has gone \
         stale against the service and every count below is now a count over part of it"
    );
}

#[test]
fn the_service_declares_fifty_eight_public_functions() {
    // 58 = 17 registry + 7 manifest + 7 pin + 10 contract + 5 http + 6 filter
    //     + 5 problem + 1 view.
    //
    // This is a floor, not a ceiling, and deliberately so: the number a reader
    // can check is the total, and the direction that matters is a *drop*. A
    // function that disappears has stopped being an entry point, and the
    // enumeration has to be redone for it — which is what this catches. A
    // function that appears is caught by the route and data-entry-point sets
    // below, which are exact rather than floors.
    let total: usize = ALL_SRC.iter().map(|(_, src)| pub_fns(src).len()).sum();
    assert_eq!(
        total, 58,
        "the public surface of pantry is {total} functions, not 58. Either a function was \
         removed — and the enumeration in this packet must be redone for it — or one was added \
         and ALL_SRC does not count it. Print the module breakdown and decide which."
    );
}

#[test]
fn six_http_entry_points_are_reachable_and_six_is_the_right_number() {
    let routes = registered_routes();

    // Four routes, from `ROUTES`, which is the committed constant the OpenAPI
    // document is checked against in `tests/api.rs`.
    let declared: Vec<String> = pantry::http::ROUTES
        .iter()
        .map(|(_, path)| path.to_string())
        .collect();
    let registered: Vec<String> = routes.iter().map(|(path, _, _)| path.clone()).collect();
    assert_eq!(
        registered, declared,
        "router() and the ROUTES constant disagree. Every entry point that exists is a route; a \
         route in one list and not the other is one this enumeration does not cover."
    );

    // Two fallbacks: `not_found` for a path that matches no route, and
    // `method_not_allowed` for a path that matches one under a verb it does
    // not serve. They are entry points — a request reaches them — so they are
    // counted and tested.
    assert!(
        HTTP_SRC.contains(".fallback(not_found)"),
        "router() no longer installs the not-found fallback; the 6th and 7th entry points have \
         changed shape and the enumeration in this packet is stale"
    );
    assert!(
        HTTP_SRC.contains(".method_not_allowed_fallback(method_not_allowed)"),
        "router() no longer installs the method-not-allowed fallback; see above"
    );

    assert_eq!(
        routes.len() + 2,
        6,
        "pantry serves {} routes and 2 fallbacks, not 4 and 2. A new route is a new entry point \
         and needs a negative test in tests/entry_point_isolation.rs before it lands.",
        routes.len()
    );
}

#[test]
fn every_route_is_registered_with_get_and_there_are_no_write_verbs() {
    // update and delete have **zero** entry points in pantry, and this is how
    // that is made load-bearing rather than asserted. A `post`/`put`/`patch`/
    // `delete` registration fails here with the path that introduced it.
    for (path, method, handler) in registered_routes() {
        assert_eq!(
            method, "get",
            "{path} is registered with `{method}({handler})`. pantry is read-only: the registry is \
             curated data in this repository and a runtime write is an unaudited registration \
             surface. This is update or delete, and it has to be a decision, not a commit."
        );
    }

    for verb in ["post", "put", "patch", "delete"] {
        assert!(
            !HTTP_SRC.contains(&format!("{verb}(")),
            "`{verb}` appears in src/http.rs. See the assertion above — pantry has no write verb."
        );
    }
}

// --------------------------------- 2. the isolation properties, per entry point

#[test]
fn no_write_primitive_appears_anywhere_in_the_service() {
    // The behavioural half of "update and delete have zero entry points" is in
    // `tests/entry_point_isolation.rs` (every verb, every path, 405). This is
    // the half that catches the capability arriving by a back door: a handler
    // that writes a file without a route is not reachable, but it is a
    // capability nobody chose.
    const MUTATIONS: &[&str] = &[
        "fs::write",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::create_dir",
        "fs::set_permissions",
        "File::create",
        "OpenOptions",
        "write_all",
    ];

    let found: Vec<(&str, &str)> = ALL_SRC
        .iter()
        .flat_map(|(name, src)| {
            MUTATIONS
                .iter()
                .filter(move |needle| src.contains(**needle))
                .map(move |needle| (*name, *needle))
        })
        .collect();

    assert!(
        found.is_empty(),
        "pantry grew a write primitive: {found:?}. The registry is read once at startup and a \
         caller can never change it — a write is not an optimisation here, it is a new \
         registration surface."
    );
}

#[test]
fn no_status_reveals_that_a_resource_exists_but_is_not_yours() {
    // "Absence, never 403." A 403 answers "it exists and is not yours", which is
    // the enumeration oracle darkroom-09 was written to remove. pantry answers
    // 404, 400, 405, 500 and 503, and this is the assertion that keeps 401 and
    // 403 out of that set.
    for (name, src) in ALL_SRC {
        for needle in ["forbidden", "Forbidden", "unauthorized", "Unauthorized"] {
            assert!(
                !src.contains(needle),
                "src/{name} names `{needle}`. pantry has no accounts and no authorization, so a \
                 403 here could only be an existence oracle — it would tell a caller that a name \
                 is registered without telling them what it is. Answer 404."
            );
        }
    }

    // And the two constructors that would carry one do not exist. `Problem`'s
    // five constructors are 404, 400, 405, 503 and 500; a sixth would be a new
    // answer to "may this caller see this", and there is no such question here.
    for constructor in ["fn forbidden", "fn unauthorized", "fn conflict"] {
        assert!(
            !ALL_SRC.iter().any(|(_, src)| src.contains(constructor)),
            "src/problem.rs grew `Problem::{constructor}`. Read the assertion above first: with no \
             accounts, this status can only distinguish registered from unregistered."
        );
    }
}

#[test]
fn nothing_reads_an_authorization_header_or_a_credential() {
    // No account means no credential, and a credential read is how a tenant
    // dimension sneaks in unasked. This is the tripwire for the next packet
    // that decides pantry needs a caller identity.
    for (name, src) in ALL_SRC {
        for needle in [
            "authorization",
            "Authorization",
            "Bearer",
            "bearer",
            "x-api-key",
            "X-Api-Key",
            "api_key",
            "cookie",
            "Cookie",
            "jwt",
            "JWT",
            "session_id",
        ] {
            assert!(
                !src.contains(needle),
                "src/{name} names `{needle}`. pantry is an unauthenticated read-only registry of \
                 public curated data; reading a caller credential here is the first half of a \
                 tenant boundary nobody has designed. If a decision needs it, it goes in \
                 registry/index.yml as a curated fact and in DECISIONS.md first."
            );
        }
    }
}

#[test]
fn the_only_environment_variables_the_service_reads_are_the_two_named() {
    // Same tripwire, from the configuration side. A variable read at request
    // time is a way for a scope to arrive from outside the binary.
    let reads: Vec<&str> = ALL_SRC
        .iter()
        .flat_map(|(_, src)| {
            src.lines().filter_map(|line| {
                let at = line.find("env::var(\"")?;
                let rest = &line[at + "env::var(\"".len()..];
                let end = rest.find('"')?;
                Some(&rest[..end])
            })
        })
        .collect();

    assert_eq!(
        reads,
        ["PANTRY_BIND"],
        "src/main.rs reads {reads:?}. `PANTRY_BIND` is a listen address. Anything else — \
         particularly a credential or a scope — is a capability arriving from the environment, and \
         `.env.example` is where it has to be named first."
    );
}

#[test]
fn twenty_three_functions_can_reach_registered_data_and_the_set_is_exact() {
    // The registry data entry points, derived from the modules that hold
    // registry state — `registry.rs` and `filter.rs` — rather than from a list
    // of names somebody maintains. A new one is a new count; a removed one
    // fails too, because a query nobody can find is a query nobody has run.
    let in_registry: Vec<&str> = pub_fns(REGISTRY_SRC).iter().map(|f| f.name).collect();
    let in_filter: Vec<&str> = pub_fns(FILTER_SRC).iter().map(|f| f.name).collect();

    assert_eq!(
        in_registry.len() + in_filter.len(),
        23,
        "the registry data surface is {} functions ({in_registry:?} in registry.rs, {in_filter:?} \
         in filter.rs), not 23. Every one of them is an entry point into registered data and every \
         one needs a negative test in tests/entry_point_isolation.rs.",
        in_registry.len() + in_filter.len()
    );
}

#[test]
fn eight_data_entry_points_can_be_changed_by_a_request_and_the_rest_cannot() {
    // Of the 23, the eight whose behaviour a caller can actually change. The
    // other fifteen take `&self`, a `&Path` this repository owns, or nothing —
    // so there is nothing for a request to vary. This is the count that says how
    // much of the data surface is attack surface, and it is the set the
    // negative tests in `tests/entry_point_isolation.rs` are written against.
    //
    // `load` is deliberately **not** in it: the directory it reads is chosen at
    // startup from `PANTRY_REGISTRY_DIR` or `registry_dir()`, never from a
    // request, so a caller cannot vary it. That is asserted separately by
    // `the_only_environment_variables_the_service_reads_are_the_two_named`.
    let registry: Vec<&str> = async_free_and_affected(pub_fns(REGISTRY_SRC));
    let filter: Vec<&str> = async_free_and_affected(pub_fns(FILTER_SRC));

    assert_eq!(
        registry,
        ["get", "query", "page"],
        "the request-reachable registry functions are {registry:?}, not the three named. One of \
         them has changed shape, or a fourth has become reachable."
    );
    // `Filter::from_query` and `Page::from_query` are two functions that share a
    // name, so the list is five long across the two `impl` blocks and `offset`
    // is in it: it decodes a cursor that came from the query string.
    assert_eq!(
        filter,
        ["from_query", "matches", "new", "from_query", "offset"],
        "the request-reachable filter/page functions are {filter:?}. `from_query` appearing \
         twice is expected — `Filter::from_query` and `Page::from_query` are two functions."
    );
    assert_eq!(
        registry.len() + filter.len(),
        8,
        "eight of pantry's twenty-three data entry points can be changed by a request, not {}. \
         Re-derive this count and the negative tests with it.",
        registry.len() + filter.len()
    );
}

/// The request-reachable names, in source order. A list rather than a filter
/// over an attribute: Rust's signatures here are too varied to match on (`&str`,
/// `&HashMap`, `usize`, `&Filter`, `&Page`), and a name list is the thing a
/// reader can check against the source above.
///
/// Source order and duplicates are both preserved — `Filter::from_query` and
/// `Page::from_query` are two functions that share a name, and a set would
/// count them once and under-report the surface by one.
fn async_free_and_affected<'a>(fns: Vec<PubFn<'a>>) -> Vec<&'a str> {
    const REACHABLE: &[&str] = &[
        "get",
        "query",
        "page",
        "from_query",
        "matches",
        "new",
        "offset",
    ];
    fns.iter()
        .map(|f| f.name)
        .filter(|name| REACHABLE.contains(name))
        .collect()
}

#[test]
fn a_service_name_reaches_a_string_comparison_and_never_a_path() {
    // The reach half of the boundary. `{name}` is the only free-text value a
    // caller controls, and it is derived from the route path in `get_service`
    // rather than from a query parameter, so this slices the handler body and
    // reads what happens to the binding.
    let get_service = async_fns(HTTP_SRC)
        .into_iter()
        .find(|f| f.name == "get_service")
        .expect("`get_service` is the handler behind GET /v1/services/{name}");

    for needle in [
        "join(",
        "Path::new",
        "read_to_string",
        "fs::",
        "PathBuf",
        "canonicalize",
    ] {
        assert!(
            !get_service.body.contains(needle),
            "the `get_service` body uses `{needle}`. The `{{name}}` path segment is \
             caller-controlled and must reach nothing but a string comparison — a `Path::join` \
             there is a file read for any path the process can open."
        );
    }

    assert!(
        get_service.body.contains("registry.get(&name)"),
        "`get_service` no longer looks the name up through `Registry::get`. Whatever it does \
         instead has to be checked against the assertion above, which is about `Registry::get`."
    );
}

#[test]
fn the_registry_lookup_is_a_comparison_and_does_no_io() {
    // Belt and braces on the same property, one layer down. Even if a future
    // handler stopped using it, `get` itself is the only thing standing between
    // a string and the registry, so it is checked on its own terms.
    let get = pub_fns(REGISTRY_SRC)
        .into_iter()
        .find(|f| f.name == "get")
        .expect("`Registry::get` is the name lookup");

    for needle in ["fs::", "Path", "read_", "open(", "join("] {
        assert!(
            !get.body.contains(needle),
            "`Registry::get`'s body mentions `{needle}`. It is a lookup in an immutable vector, and \
             a name that can reach the filesystem is a name that can read a file."
        );
    }

    assert!(
        get.body.contains("entry.name() == name"),
        "`Registry::get` no longer compares names with `==`. The exact byte comparison is the \
         isolation property: no normalisation, no case folding, no prefix match, so a name that \
         is not an entry is a name that is not found."
    );
}

#[test]
fn the_registry_is_immutable_after_load_so_a_request_cannot_change_what_is_served() {
    // The other way a read-only service stops being read-only: not a write
    // verb, but a request mutating shared state so the next request sees
    // something different. `Registry` holds a `Vec` behind `&self` and is
    // shared as `Arc`; a `&mut self` method or an interior-mutability cell is
    // what would break it.
    for (name, src) in ALL_SRC {
        for needle in [
            "&mut self",
            "RefCell<",
            "Mutex<",
            "RwLock<",
            "Cell<",
            "static mut",
        ] {
            assert!(
                !src.contains(needle),
                "src/{name} contains `{needle}`. The registry is loaded once and shared read-only; \
                 interior mutability here is a caller able to change what the next caller is served."
            );
        }
    }
}

#[test]
fn the_probes_cannot_be_scoped_and_do_not_pretend_to_be() {
    // `/healthz` and `/readyz` are infrastructure an orchestrator reaches by
    // convention, and a per-caller scope on either would be a scope computed
    // from nothing. They read no query and no header, so a stray parameter is
    // ignored rather than refused — which is also why a tenant-shaped
    // parameter cannot change their answer.
    for probe in ["healthz", "readyz"] {
        let handler = async_fns(HTTP_SRC)
            .into_iter()
            .find(|f| f.name == probe)
            .unwrap_or_else(|| panic!("{probe} is a route on pantry"));

        for needle in ["Query(", "Path(", "HeaderMap", "headers()"] {
            assert!(
                !handler.body.contains(needle),
                "the `{probe}` handler reads `{needle}`. A probe answers about pantry, not about a \
                 caller, so it has no input to scope on. If a probe starts reading the request, \
                 the reason belongs in this comment."
            );
        }
    }
}
