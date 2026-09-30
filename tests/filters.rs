//! The filters `GET /v1/services` accepts.
//!
//! Two things are worth more than the filtering itself: a filter that matches
//! nothing returns `200` with an empty list and never a 404 (a 404 would tell a
//! caller "there is no such service" when the truth is "no service matches"),
//! and a filter value outside the vocabulary is a 400 with the allowed values
//! in the message rather than a silently empty result.

use pantry::contract::Constraint;
use pantry::filter::{Filter, Page};
use pantry::manifest::Language;
use pantry::registry::{registry_dir, Registry, ServiceKind};
use std::collections::HashMap;


fn registry() -> Registry {
    Registry::load(&registry_dir()).expect("the official registry loads")
}

fn names(filter: Filter) -> Vec<String> {
    let registry = registry();
    registry
        .query(&filter)
        .into_iter()
        .map(|entry| entry.name().to_string())
        .collect()
}

fn query(pairs: &[(&str, &str)]) -> Filter {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    Filter::from_query(&map).expect("a valid query")
}

#[test]
fn no_filter_returns_every_official_service_sorted_by_name() {
    let registry = registry();
    let entries = registry.query(&Filter::default());

    let names: Vec<&str> = entries.iter().map(|e| e.name()).collect();
    assert_eq!(names, ["billing", "guard", "identity", "muse"]);
    // Sorted, not filesystem order: a directory walk is not a contract, and a
    // client diffing two responses should see a stable list.
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "the list is sorted by name");
}

#[test]
fn kind_filter_accepts_every_kind_in_the_vocabulary() {
    for kind in ServiceKind::all().iter().copied() {
        let filter = Filter {
            kind: Some(kind),
            ..Filter::default()
        };
        let matched = names(filter);

        match kind {
            ServiceKind::Api => assert_eq!(matched, ["billing", "guard", "identity", "muse"]),
            // No official service is a pure worker or a hybrid today: courier
            // and darkroom-worker are the candidates and both are held out of
            // the registry with a recorded reason (registry/index.yml). An
            // empty list here is a fact about the registry, not a broken
            // filter, and it is the honest answer until those rows land.
            ServiceKind::Worker | ServiceKind::Both => assert_eq!(
                matched,
                Vec::<String>::new(),
                "{kind} is a real kind with no members yet"
            ),
        }
    }
}

#[test]
fn language_filter_covers_every_language_the_manifest_schema_allows_for_a_service() {
    // The schema's enum minus `spec`, which is for specification-only
    // repositories and which pantry refuses to register (core itself is the
    // example). The filter vocabulary is exactly the registerable set: asking
    // for a language no service can have is a 400, not an empty list.
    let expected: &[(Language, &[&str])] = &[
        (Language::Go, &["identity"]),
        (Language::Ruby, &["billing"]),
        (Language::Typescript, &["guard"]),
        (Language::Python, &["muse"]),
        (Language::Elixir, &[]),
        (Language::Rust, &[]),
    ];

    for (language, want) in expected {
        let filter = Filter {
            language: Some(*language),
            ..Filter::default()
        };
        assert_eq!(names(filter), *want, "{language}");
    }
}

/// The semver filter is range-intersection, not string equality: a service
/// pinned to `^0.2.0` matches a caller asking about `>=0.2.0`, and one pinned
/// to `^0.1.0` does not match a caller asking about `^0.2.0` because pre-1.0
/// a caret pins the minor.
#[test]
fn contract_filter_matches_by_range_intersection() {
    let cases: &[(&str, &[&str])] = &[
        ("^0.2.0", &["billing", "muse"]),
        ("^0.1.0", &["guard", "identity"]),
        ("~0.2.0", &["billing", "muse"]),
        (">=0.2.0", &["billing", "muse"]),
        (">=0.1.0", &["guard", "identity"]),
        ("0.1.0", &["guard", "identity"]),
        ("0.2.0", &["billing", "muse"]),
        // A caret on a future minor intersects nothing on this platform yet.
        ("^0.3.0", &[]),
        ("^0.0.1", &[]),
        (">=9.0.0", &[]),
        // The boundary: ^0.2.0 and ^0.1.0 touch at 0.2.0 without sharing a
        // version, so a caller asking "is anything on ^0.2.0" must not get the
        // services still on ^0.1.0.
        ("^0.1.0", &["guard", "identity"]),
    ];

    for (range, want) in cases {
        let filter = Filter {
            contract: Some(Constraint::parse(range).expect("grammar")),
            ..Filter::default()
        };
        assert_eq!(names(filter), *want, "contract={range}");
    }
}

#[test]
fn a_filter_that_matches_nothing_is_an_empty_list_not_an_error() {
    let registry = registry();
    let filter = query(&[("kind", "worker"), ("language", "elixir")]);

    assert!(registry.query(&filter).is_empty());
}

#[test]
fn two_filters_are_both_applied() {
    let filter = query(&[("kind", "api"), ("language", "ruby")]);
    assert_eq!(names(filter), ["billing"]);

    // Both halves match something on their own; together they match nothing.
    let each_alone = query(&[("language", "go")]);
    assert_eq!(names(each_alone), ["identity"]);

    let registry = registry();
    let impossible = query(&[("language", "go"), ("contract", "^0.2.0")]);
    assert!(
        registry.query(&impossible).is_empty(),
        "identity is on ^0.1.0, so language=go plus contract=^0.2.0 matches nothing"
    );

    let three = query(&[("kind", "api"), ("language", "python"), ("contract", "^0.2.0")]);
    assert_eq!(names(three), ["muse"]);
}

#[test]
fn an_empty_value_for_a_known_filter_is_rejected() {
    // `?kind=` is not "no filter". Accepting it would make a client that fails
    // to interpolate a variable silently receive the whole registry.
    for (key, value) in [("kind", ""), ("language", ""), ("contract", "")] {
        let map: HashMap<String, String> = [("x", "y")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .chain(std::iter::once((key.to_string(), value.to_string())))
            .collect();
        let error = Filter::from_query(&map).expect_err("an empty value is not a value");
        assert!(
            error.to_string().contains(key),
            "the error names the parameter: {error}"
        );
    }
}

#[test]
fn a_filter_value_outside_the_vocabulary_is_rejected_with_the_vocabulary() {
    let cases = [
        ("kind", "database", &["api", "worker", "both"][..]),
        ("language", "cobol", &["go", "ruby", "elixir", "python", "typescript", "rust"][..]),
        ("language", "spec", &["go", "ruby", "elixir", "python", "typescript", "rust"][..]),
        ("contract", "1.x", &["MAJOR.MINOR.PATCH"][..]),
    ];

    for (key, value, allowed) in cases {
        let map: HashMap<String, String> = [(key.to_string(), value.to_string())].into();
        let error = Filter::from_query(&map).expect_err("outside the vocabulary");

        let message = error.to_string();
        assert!(message.contains(key), "names the parameter: {message}");
        for fragment in allowed {
            assert!(
                message.contains(fragment),
                "message lists what is allowed ({fragment}): {message}"
            );
        }
    }
}

/// An unknown filter is an error, not an ignored parameter. If a future pantry
/// adds `?runtime=` and an older one silently ignores it, the caller gets the
/// unfiltered list and believes it asked a question.
#[test]
fn an_unknown_query_parameter_is_rejected() {
    let map: HashMap<String, String> =
        [("runtime".to_string(), "node".to_string())].into();

    let error = Filter::from_query(&map).expect_err("runtime is not a filter");

    assert!(error.to_string().contains("runtime"), "{error}");
}

// ----------------------------------------------------------------- paging

#[test]
fn a_page_limit_slices_the_filtered_list_and_says_whether_more_is_left() {
    let registry = registry();
    let filter = Filter::default();

    let first = Page::new(2, None).expect("limit 2");
    let page = registry.page(&filter, &first).expect("a page");

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].name(), "billing");
    assert_eq!(page.items[1].name(), "guard");
    assert!(page.has_more, "two of four returned means two left");
    assert!(page.next_cursor.is_some(), "a caller needs somewhere to go next");

    let second = Page::new(2, page.next_cursor).expect("the cursor pantry handed out");
    let page = registry.page(&filter, &second).expect("a page");

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].name(), "identity");
    assert_eq!(page.items[1].name(), "muse");
    assert!(!page.has_more, "the list is four long and both halves are taken");
    assert_eq!(page.next_cursor, None);
}

#[test]
fn a_cursor_is_opaque_and_a_rewritten_one_is_rejected() {
    let registry = registry();
    let page = registry
        .page(&Filter::default(), &Page::new(2, None).expect("limit"))
        .expect("a page");
    let cursor = page.next_cursor.expect("a next cursor");

    // Not an offset a client may edit: core says cursors are opaque and their
    // encoding may change without notice. Base64url of the index, which also
    // means it never appears in a log as a bare integer.
    assert_ne!(cursor, "2");
    assert!(
        cursor.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "base64url, no padding: {cursor}"
    );

    assert!(
        Page::new(2, Some("2".to_string())).is_err(),
        "a bare integer is not a cursor pantry issued"
    );
    assert!(Page::new(2, Some("!!!!".to_string())).is_err());
    assert!(Page::new(2, Some(String::new())).is_err());
}

#[test]
fn page_limits_follow_core_s_bounds() {
    // core: `limit` defaults to 25 and is capped at 100.
    assert!(Page::new(0, None).is_err(), "a page of nothing is a bug, not a filter");
    assert!(Page::new(101, None).is_err(), "core caps limit at 100");
    assert!(Page::new(100, None).is_ok());
    assert!(Page::new(usize::MAX, None).is_err());
    assert_eq!(Page::default().limit, 25, "core's default");

    // `from_query` is the only place a string becomes a number, so it is the
    // only place a non-numeric limit can arrive.
    for bad in ["lots", "-1", "2.5", "1e2", "", "100 "] {
        let map: HashMap<String, String> = [("limit".to_string(), bad.to_string())].into();
        let error = Page::from_query(&map)
            .err()
            .unwrap_or_else(|| panic!("limit={bad:?} must be rejected"));
        assert!(
            error.to_string().contains("limit"),
            "the error names the parameter: {error}"
        );
    }
}

#[test]
fn paging_parameters_are_not_filters() {
    // `limit` and `cursor` are not filters, so a filter query carrying them is
    // not carrying an unknown parameter.
    let map: HashMap<String, String> = [
        ("limit".to_string(), "2".to_string()),
        ("cursor".to_string(), "MTA".to_string()),
        ("kind".to_string(), "api".to_string()),
    ]
    .into();

    let filter = Filter::from_query(&map).expect("paging alongside a filter is normal");
    assert_eq!(filter.kind, Some(ServiceKind::Api));

    let page = Page::from_query(&map).expect("the paging half parses on its own");
    assert_eq!(page.limit, 2);
    assert_eq!(page.cursor.as_deref(), Some("MTA"));
}

