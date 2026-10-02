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
use pantry::registry::{Registry, ServiceKind, registry_dir};
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
    assert_eq!(
        names,
        [
            "billing",
            "caf",
            "cafaye-ts",
            "courier",
            "darkroom",
            "guard",
            "identity",
            "muse",
            "pantry",
            "parlor"
        ]
    );
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
            ServiceKind::Api => assert_eq!(
                matched,
                [
                    "billing", "courier", "darkroom", "guard", "identity", "muse", "pantry",
                    "parlor"
                ]
            ),
            // The two artifacts a person installs, and the reason this arm
            // exists. Both are curated — nothing in either manifest can derive
            // the value — and neither is going to change: `cli` is what caf is,
            // permanently, and a client that asked for `api` and got a binary
            // would have been told to route HTTP to a command. cafaye-ts is the
            // same shape for a different reason, and the gap that opens is named
            // in `registry/index.yml`: a package other things import is not a
            // process anyone runs, and the vocabulary has no word for that yet.
            ServiceKind::Cli => assert_eq!(matched, ["caf", "cafaye-ts"]),
            // No official service is a pure worker or a hybrid today, and that is
            // a fact about the registry rather than a broken filter. courier is
            // the obvious worker candidate — it publishes five events and relays
            // them — and it is `api` because its own manifest declares an
            // `exposes.api` and an empty `consumes`, so the vocabulary says api
            // and the manifest is decisive. darkroom-worker does not exist yet.
            // These empty lists are the honest answer until those rows land, and
            // they are the check that catches a real `worker` the moment one
            // does — a registry that quietly had no workers would be a registry
            // nobody had checked.
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
        (Language::Go, &["caf", "identity"]),
        (Language::Ruby, &["billing"]),
        // Three typescript services, and the fact that they are three is the
        // point: `language` is read off the manifest and knows nothing about
        // kind. guard serves HTTP, cafaye-ts is a package serving nothing, and
        // parlor serves nothing either but is curated `api` because it declares
        // no `exposes` at all. Asking by language returns all three because
        // that is what the manifest says, which is what the filter promises.
        (Language::Typescript, &["cafaye-ts", "guard", "parlor"]),
        (Language::Python, &["muse"]),
        // The one Elixir service. This list used to read `&[]` and was the
        // honest answer while courier's events were two-segment and its manifest
        // did not validate; it is a member now, which is the exclusion tripwire
        // doing its job a third time.
        (Language::Elixir, &["courier"]),
        (Language::Rust, &["darkroom", "pantry"]),
    ];

    for (language, want) in expected {
        let filter = Filter {
            language: Some(*language),
            ..Filter::default()
        };
        assert_eq!(names(filter), *want, "{language}");
    }
}

/// The semver filter is range-**intersection**, not string equality: a service
/// pinned to `^0.2.0` matches a caller asking about `>=0.2.0`, and one pinned
/// to `^0.1.0` does not match a caller asking about `^0.2.0` because pre-1.0 a
/// caret pins the minor.
///
/// **This test used to assert the wrong thing.** Its cases table named which
/// service sat on which pin — `^0.1.0` was courier, guard and identity — so it
/// was a test about *fleet placement*, wearing a test about semver's clothes. It
/// went red the moment core's version standard moved the whole fleet from
/// `^0.1.0` to `^0.2.0`, and what it reported was a failure of range
/// intersection when range intersection had not changed at all. Placement is
/// core's decision; this file has no vote in it.
///
/// What is left is the part that is this build's: **each pin selects exactly the
/// services carrying it**, the pins **partition** the fleet, and the empty side
/// is empty for a reason that follows from the pins rather than from a guess.
#[test]
fn contract_filter_matches_by_range_intersection() {
    let registry = registry();

    let mut by_pin: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for entry in registry.entries() {
        by_pin
            .entry(entry.core_constraint().to_string())
            .or_default()
            .push(entry.name().to_string());
    }
    assert!(!by_pin.is_empty(), "the registry is not empty");

    // 1. Each pin selects exactly the services carrying it — the filter reads
    //    the same field the entry serves.
    let mut covered: Vec<String> = Vec::new();
    for (pin, members) in &by_pin {
        let filter = Filter {
            contract: Some(Constraint::parse(pin).expect("a pin parses")),
            ..Filter::default()
        };
        let mut got = names(filter);
        let mut want = members.clone();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(got, want, "contract={pin}");
        covered.extend(members.iter().cloned());
    }

    // 2. The pins partition the fleet: every service is under exactly one. A
    //    filter that ignored `contract` would satisfy (1) on a one-pin fleet,
    //    which is what this fleet was for a fortnight.
    covered.sort_unstable();
    let mut every: Vec<String> = registry
        .entries()
        .iter()
        .map(|entry| entry.name().to_string())
        .collect();
    every.sort_unstable();
    assert_eq!(
        covered, every,
        "the pins do not partition the fleet: a service is under no pin, or under two"
    );

    // 3. An open floor at or below the lowest pin every service satisfies
    //    matches all of them, and an open floor *above* the highest matches
    //    none. Both follow from the pins rather than from a remembered version,
    //    and both are the property a client actually depends on: it asks what
    //    it can use, not which service happens to be where.
    let (lowest, _highest) = floors(&by_pin).expect("at least one pin with a floor and a ceiling");
    let all = Filter {
        contract: Some(Constraint::parse(&format!(">={lowest}")).expect("grammar")),
        ..Filter::default()
    };
    assert_eq!(
        names(all).len(),
        registry.len(),
        ">={lowest} is a floor nothing is under"
    );

    for pin in by_pin.keys() {
        let ceiling = ceiling_of(pin);
        let none = Filter {
            contract: Some(Constraint::parse(&format!("^0.{}.0", ceiling + 1)).expect("grammar")),
            ..Filter::default()
        };
        assert_eq!(
            names(none),
            Vec::<String>::new(),
            "nothing is on ^0.{}.0 yet: it is a minor above {pin}, and a caret on a 0.x pins the \
             minor",
            ceiling + 1
        );
    }

    // 4. The boundary the whole thing exists for, stated without naming who is
    //    where: two distinct pins are ranges that cannot share a version, so
    //    no service may appear under both.
    let pins: Vec<&String> = by_pin.keys().collect();
    for (i, a) in pins.iter().enumerate() {
        for b in &pins[i + 1..] {
            let in_a = Filter {
                contract: Some(Constraint::parse(a).expect("grammar")),
                ..Filter::default()
            };
            let in_b = Filter {
                contract: Some(Constraint::parse(b).expect("grammar")),
                ..Filter::default()
            };
            let both: std::collections::BTreeSet<&String> =
                names(in_a).iter().map(|s| leak(s)).collect();
            for service in names(in_b) {
                assert!(
                    !both.contains(&service),
                    "{service} matched both {a} and {b}. Those are two different pins, so the \
                     filter is comparing something other than the range"
                );
            }
        }
    }
}

/// The lowest floor and the highest `0.x` minor any pin in the registry names.
fn floors(by_pin: &std::collections::BTreeMap<String, Vec<String>>) -> Option<(String, u32)> {
    let mut lowest: Option<String> = None;
    let mut highest: Option<u32> = None;
    for pin in by_pin.keys() {
        let bare = pin.trim_start_matches(['^', '~', '=', '>', '<', ' ']);
        let version = bare.split(&['-', '+'][..]).next().unwrap_or(bare);
        let mut parts = version.split('.');
        let major: u32 = parts.next()?.parse().ok()?;
        let minor: u32 = parts.next()?.parse().ok()?;
        if major != 0 {
            // The ceiling rule below is stated for 0.x because that is where
            // caret pins the minor. A 1.x pin in this fleet would need the
            // rule restated rather than approximated, so it is refused here.
            return None;
        }
        if lowest.as_deref().map(|l| version < l).unwrap_or(true) {
            lowest = Some(version.to_string());
        }
        highest = Some(highest.map_or(minor, |h: u32| h.max(minor)));
    }
    lowest.map(|l| (l, highest.unwrap_or(0)))
}

/// The `0.x` minor a pin's caret stops at.
fn ceiling_of(pin: &str) -> u32 {
    let bare = pin.trim_start_matches(['^', '~', '=', '>', '<', ' ']);
    bare.split('.')
        .nth(1)
        .and_then(|minor| minor.parse().ok())
        .unwrap_or_else(|| panic!("{pin} has no 0.x minor to take a ceiling from"))
}

/// Leaks a `String` so a `BTreeSet<&String>` can be built from owned values.
///
/// Test-only, and named for what it is: the alternative is cloning the whole
/// name set per pair, and this function is called from a test whose cost is
/// already nine services.
fn leak(value: &str) -> &'static String {
    Box::leak(Box::new(value.to_owned()))
}

#[test]
fn a_filter_that_matches_nothing_is_an_empty_list_not_an_error() {
    let registry = registry();
    // courier is the only elixir service and it is an `api`, so this is empty
    // for a reason that is worth naming: the combination is impossible, not
    // unimplemented. It used to be empty because courier was excluded.
    let filter = query(&[("kind", "worker"), ("language", "elixir")]);

    assert!(registry.query(&filter).is_empty());
}

#[test]
fn two_filters_are_both_applied() {
    let filter = query(&[("kind", "api"), ("language", "ruby")]);
    assert_eq!(names(filter), ["billing"]);

    // Both halves match something on their own; together they match nothing.
    let each_alone = query(&[("language", "go")]);
    assert_eq!(names(each_alone), ["caf", "identity"]);

    let registry = registry();

    // **The disjoint pairs are computed, not remembered.** This test used to
    // name them — `language=python&contract=^0.1.0` and
    // `language=elixir&contract=^0.2.0` — and both were empty only by
    // coincidence of where the fleet happened to sit. The fleet-wide raise to
    // `^0.2.0` emptied the coincidence and left the assertion, and the test
    // reported a semver failure that had not happened. Its own comment already
    // said two filters "are worth nothing if they cannot survive the next entry
    // in the registry"; this is that, applied to the test itself.
    //
    // The rule: a language exactly one service carries, paired with a pin that
    // service provably cannot satisfy. Caret on a `0.x` pins the minor, so one
    // minor below is disjoint by arithmetic rather than by memory.
    let mut checked = 0usize;
    let mut unreached: Vec<String> = Vec::new();
    for entry in registry.entries() {
        let language = entry.language();
        let alone = Filter {
            language: Some(language),
            ..Filter::default()
        };
        if names(alone).len() != 1 {
            continue;
        }
        let pin = entry.core_constraint().to_string();
        let Some(disjoint) = disjoint_pin(&pin) else {
            unreached.push(format!("{} is on {pin}", entry.name()));
            continue;
        };

        let both = Filter {
            language: Some(language),
            contract: Some(Constraint::parse(&disjoint).expect("grammar")),
            ..Filter::default()
        };
        assert!(
            registry.query(&both).is_empty(),
            "language={language} plus contract={disjoint} cannot match: {} is the only \
             {language} service and it is on {pin}. A hit here means the two filters are not \
             both being applied",
            entry.name()
        );
        checked += 1;
    }

    assert!(
        unreached.is_empty(),
        "a pin form this cannot reason about is a reason to look: {}",
        unreached.join("; ")
    );
    assert!(
        checked > 0,
        "no language in this fleet is carried by exactly one service, so this test proved \
         nothing about the conjunction — it only ever proved it about the first pair it was \
         given, and the pairs have all since changed"
    );
}

/// A pin that cannot contain any version `pin` can: caret on a `0.x` pins the
/// minor, so `^0.2.0` and `^0.1.0` are disjoint ranges.
fn disjoint_pin(pin: &str) -> Option<String> {
    let minor = pin
        .strip_prefix("^0.")?
        .split('.')
        .next()?
        .parse::<u32>()
        .ok()?;
    (minor > 0).then(|| format!("^0.{}.0", minor - 1))
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
        // The list is what the 400 says was allowed, so it is the vocabulary in
        // full. It used to be three values and `cli` was refused by name: the
        // registry could not record what caf is, so caf was `api`.
        ("kind", "database", &["api", "worker", "both", "cli"][..]),
        (
            "language",
            "cobol",
            &["go", "ruby", "elixir", "python", "typescript", "rust"][..],
        ),
        (
            "language",
            "spec",
            &["go", "ruby", "elixir", "python", "typescript", "rust"][..],
        ),
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
    let map: HashMap<String, String> = [("runtime".to_string(), "node".to_string())].into();

    let error = Filter::from_query(&map).expect_err("runtime is not a filter");

    assert!(error.to_string().contains("runtime"), "{error}");
}

// ----------------------------------------------------------------- paging

/// Three per page, and not two on purpose — the same reason as the sibling test
/// `a_page_limit_slices_the_list_and_the_cursor_finishes_it` in `tests/api.rs`,
/// which says so at length.
///
/// What this test asserts is that the final page is SHORT and that `has_more` is
/// false on it. That is the claim "the limit is a maximum, not a size", and it is
/// only observable when the limit does not divide the registry length. A limit of
/// 2 over the nine services this registry held when the test was written produced
/// a short last page by luck of the count; over the ten it holds now it produces
/// five full pages and never reaches either assertion. Choosing 3 keeps the
/// property under test rather than letting it lapse because a service was
/// registered.
const PAGE_LIMIT: usize = 3;

#[test]
fn a_page_limit_slices_the_filtered_list_and_says_whether_more_is_left() {
    let registry = registry();
    let filter = Filter::default();

    let first = Page::new(PAGE_LIMIT, None).expect("the limit");
    let page = registry.page(&filter, &first).expect("a page");

    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0].name(), "billing");
    assert_eq!(page.items[1].name(), "caf");
    assert_eq!(page.items[2].name(), "cafaye-ts");
    assert!(page.has_more, "three of ten returned means seven are left");
    assert!(
        page.next_cursor.is_some(),
        "a caller needs somewhere to go next"
    );

    let second = Page::new(PAGE_LIMIT, page.next_cursor).expect("the cursor pantry handed out");
    let page = registry.page(&filter, &second).expect("a page");

    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0].name(), "courier");
    assert_eq!(page.items[1].name(), "darkroom");
    assert_eq!(page.items[2].name(), "guard");

    let third = Page::new(PAGE_LIMIT, page.next_cursor).expect("still a cursor pantry issued");
    let page = registry.page(&filter, &third).expect("a page");

    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0].name(), "identity");
    assert_eq!(page.items[1].name(), "muse");
    assert_eq!(page.items[2].name(), "pantry");
    assert!(page.has_more, "six of ten returned means four are left");

    // A short final page, which is what a length the limit does not divide
    // produces: the limit is a maximum, not a size. A caller that assumed three
    // per page would ask for a fifth page and be told there is nothing there.
    let fourth = Page::new(PAGE_LIMIT, page.next_cursor).expect("the last cursor pantry issued");
    let page = registry.page(&filter, &fourth).expect("a page");

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name(), "parlor");
    assert!(
        !page.has_more,
        "the list is ten long and all four pages are taken"
    );
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
        cursor
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
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
    assert!(
        Page::new(0, None).is_err(),
        "a page of nothing is a bug, not a filter"
    );
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
