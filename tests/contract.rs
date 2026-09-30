//! The cafaye constraint grammar.
//!
//! Ported from `caf/internal/contract/version.go`, because that is the one
//! resolver in the platform and a registry that answers `?contract=` with a
//! different answer than `caf contract resolve` would be a second source of
//! truth. The parity table at the bottom is copied from caf's own
//! `TestResolveMatrix`; if core's grammar changes, that table is where pantry
//! hears about it.
//!
//! One thing is pantry's own: `intersects`. A manifest holds a *constraint*,
//! not a version, so `?contract=^0.2.0` cannot ask "is this version inside
//! the entry's range" — there is no version to ask about. It asks whether the
//! two ranges have a version in common, which is what `caf deploy` means by
//! "does this filter match this service".

use pantry::contract::{Constraint, Operator, Version};

#[test]
fn constraint_bounds_match_the_grammar() {
    // The four forms, from core's docs/manifest-conventions.md: `^` allows
    // anything that does not change the left-most non-zero component, `~`
    // pins the minor, `>=` is an open floor, and a bare version is exact.
    let cases = [
        // exact
        ("1.2.3", Operator::Exact, "1.2.3", Some("1.2.3"), false),
        // caret
        ("^1.2.3", Operator::Caret, "1.2.3", Some("2.0.0"), false),
        ("^0.1.0", Operator::Caret, "0.1.0", Some("0.2.0"), false),
        ("^0.0.3", Operator::Caret, "0.0.3", Some("0.0.4"), false),
        ("^2.0.0", Operator::Caret, "2.0.0", Some("3.0.0"), false),
        // tilde
        ("~1.2.3", Operator::Tilde, "1.2.3", Some("1.3.0"), false),
        ("~0.1.0", Operator::Tilde, "0.1.0", Some("0.2.0"), false),
        // floor
        (">=1.2.3", Operator::Floor, "1.2.3", None, true),
    ];

    for (text, want_operator, want_floor, want_ceiling, want_open) in cases {
        let constraint = Constraint::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let interval = constraint.interval();

        assert_eq!(constraint.operator(), want_operator, "{text}");
        assert_eq!(interval.floor.to_string(), want_floor, "{text}");
        assert_eq!(interval.is_open(), want_open, "{text}");
        match (want_ceiling, interval.upper) {
            (Some(want), Some(got)) => assert_eq!(got.to_string(), want, "{text}"),
            (None, None) => {}
            (want, got) => panic!("{text}: ceiling want {want:?} got {got:?}"),
        }
    }
}

#[test]
fn satisfies_is_ported_from_caf() {
    // Copied verbatim from caf's TestResolveMatrix: same constraint, same
    // version, same answer. Every branch of the grammar, both sides of every
    // edge.
    let cases = [
        // exact
        ("1.2.3", "1.2.3", true),
        ("1.2.3", "1.2.4", false),
        ("1.2.3", "1.2.2", false),
        ("0.0.0", "0.0.0", true),
        // caret
        ("^1.2.3", "1.2.3", true),
        ("^1.2.3", "1.2.99", true),
        ("^1.2.3", "1.9.0", true),
        ("^1.2.3", "2.0.0", false),
        ("^1.2.3", "0.9.9", false),
        ("^0.1.0", "0.1.7", true),
        ("^0.1.0", "0.2.0", false),
        ("^0.0.3", "0.0.4", false),
        ("^0.0.3", "0.0.3", true),
        // tilde
        ("~1.2.3", "1.2.9", true),
        ("~1.2.3", "1.3.0", false),
        ("~1.2.3", "2.0.0", false),
        ("~0.1.0", "0.2.0", false),
        ("~0.1.0", "0.1.9", true),
        // floor
        (">=1.2.3", "1.2.3", true),
        (">=1.2.3", "9.0.0", true),
        (">=1.2.3", "1.2.2", false),
        (">=0.1.0", "0.2.0", true),
        // the versions cafaye services are actually pinned to
        ("^0.1.0", "0.1.0", true),
        ("^0.1.0", "0.2.0", false),
    ];

    for (constraint, version, want) in cases {
        let constraint = Constraint::parse(constraint).expect("grammar");
        let version = Version::parse(version).expect("version");
        let got = constraint.satisfies(&version);
        assert_eq!(got, want, "{constraint} against {version}: {}", constraint.rationale(&version));
    }
}

#[test]
fn versions_compare_numerically() {
    // 10.0.0 is newer than 9.0.0, which a string comparison gets backwards.
    let cases = [
        ("1.2.3", "1.2.3"),
        ("1.2.4", "1.2.3"),
        ("1.3.0", "1.2.99"),
        ("10.0.0", "9.0.0"),
        ("0.0.1", "0.0.0"),
        ("1.2.3", "1.2.4"),
        ("2.0.0", "1.99.99"),
    ];

    for (left, right) in cases {
        let left = Version::parse(left).expect("version");
        let right = Version::parse(right).expect("version");
        assert_eq!(left.cmp(&right), right.cmp(&left).reverse(), "{left} vs {right}");
    }
}

#[test]
fn a_leading_zero_is_not_a_number() {
    // `01.2.3` looks like a version to a person and silently widens a range to
    // anything that sorts the same way. caf rejects it; so does pantry.
    for bad in ["01.2.3", "1.02.3", "1.2.03", "^01.2.3"] {
        assert!(
            Constraint::parse(bad).is_err(),
            "{bad} must not parse as a constraint"
        );
    }
}

#[test]
fn the_grammar_is_exactly_four_forms() {
    // Everything npm allows and core deliberately does not: core calls the
    // grammar "deliberately tiny" and a full semver implementation "a
    // dependency and a footgun" (docs/manifest-conventions.md).
    for bad in [
        "1.2",
        "1.2.3.4",
        "v1.2.3",
        "1.2.3-rc.1",
        "1.2.3+build",
        "*",
        "1.x",
        "1.2.x",
        ">1.2.3",
        "<=1.2.3",
        "^1.2.3 || ^2.0.0",
        "1.2.3 - 2.0.0",
        ">= 1.2.3",
        "^",
        "",
    ] {
        assert!(
            Constraint::parse(bad).is_err(),
            "{bad:?} must not parse as a cafaye constraint"
        );
    }
}

/// caf's TestParseConstraintErrorNamesTheGrammar, ported: the message is what
/// a person reads when their constraint is wrong, so it has to say what was
/// expected rather than "invalid syntax".
#[test]
fn a_parse_error_names_the_grammar() {
    let error = Constraint::parse("^1.2").expect_err("a missing patch is not a constraint");

    let message = error.to_string();
    for fragment in ["^1.2", "MAJOR.MINOR.PATCH", "^", "~", ">="] {
        assert!(message.contains(fragment), "error {message:?} omits {fragment:?}");
    }
}

#[test]
fn a_constraint_round_trips_to_the_string_it_was_parsed_from() {
    for text in ["1.2.3", "^0.1.0", "~1.2.3", ">=0.0.1"] {
        assert_eq!(Constraint::parse(text).expect("grammar").to_string(), text);
    }
}

// ------------------------------------------------------------- intersects

/// The filter semantic, on its own: two constraints match when a version
/// exists inside both. The `^0.2.0`-does-not-match-`^0.1.0` case is the whole
/// reason this is a range intersection and not a string comparison — pre-1.0 a
/// caret pins the minor, so the two ranges touch at 0.2.0 without overlapping.
#[test]
fn ranges_intersect_when_they_share_a_version() {
    let cases = [
        // identical ranges
        ("^0.2.0", "^0.2.0", true),
        ("^0.1.0", "^0.1.0", true),
        // pre-1.0 carets pin the minor: adjacent, never overlapping
        ("^0.2.0", "^0.1.0", false),
        ("^0.1.0", "^0.2.0", false),
        ("^0.1.0", "^0.3.0", false),
        // a floor is open ended, so it intersects anything at or above it
        (">=0.2.0", "^0.1.0", false),
        (">=0.1.0", "^0.1.0", true),
        (">=0.1.0", "^0.9.0", true),
        (">=9.0.0", "^0.1.0", false),
        // an exact version is a single point
        ("0.2.0", "^0.2.0", true),
        ("0.1.0", "^0.2.0", false),
        ("0.2.0", "^0.1.0", false),
        ("0.1.0", "^0.1.0", true),
        ("0.2.0", ">=0.2.0", true),
        ("0.1.9", ">=0.2.0", false),
        ("0.2.0", "0.2.0", true),
        ("0.2.1", "0.2.0", false),
        // tilde pins the minor, so it overlaps the caret of the same minor
        ("~0.2.1", "^0.2.0", true),
        ("~0.2.1", "^0.3.0", false),
        ("~0.2.1", "0.2.9", true),
        ("~0.2.1", "0.3.0", false),
        // majors
        ("^1.0.0", "^2.0.0", false),
        ("^1.0.0", ">=1.5.0", true),
        ("^1.0.0", ">=2.0.0", false),
    ];

    for (filter, entry, want) in cases {
        let filter = Constraint::parse(filter).expect("grammar");
        let entry = Constraint::parse(entry).expect("grammar");
        assert_eq!(
            filter.intersects(&entry),
            want,
            "filter {filter} against entry {entry}"
        );
    }
}

/// The boundary, stated as the property that makes the matrix above correct:
/// for every pair, intersects == "some version satisfies both". Rather than
/// trust the table, check it against the definition on every version in a
/// small neighbourhood of the interesting boundaries.
#[test]
fn intersection_agrees_with_satisfies_on_a_neighbourhood() {
    let interesting = [
        "0.0.0", "0.0.3", "0.0.4", "0.1.0", "0.1.9", "0.2.0", "0.2.1", "0.2.9", "0.3.0", "1.0.0",
        "1.9.9", "2.0.0", "9.0.0",
    ];
    let ranges = [
        "^0.1.0", "^0.2.0", "~0.2.1", ">=0.2.0", "0.2.0", "0.2.1", "^1.0.0", ">=9.0.0",
    ];

    for filter_text in ranges {
        let filter = Constraint::parse(filter_text).expect("grammar");
        for entry_text in ranges {
            let entry = Constraint::parse(entry_text).expect("grammar");

            let exists = interesting
                .iter()
                .any(|v| filter.satisfies(&Version::parse(v).expect("version"))
                    && entry.satisfies(&Version::parse(v).expect("version")));

            assert_eq!(
                filter.intersects(&entry),
                exists,
                "filter {filter_text} against entry {entry_text}: a version in both \
                 ranges exists in the neighbourhood = {exists}"
            );
        }
    }
}
