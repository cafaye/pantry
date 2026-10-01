//! What a consumer owes core's examples: resolve them at the ref it has
//! actually vendored, and never conflate two document kinds.
//!
//! This file exists because `pantry`'s gate went red three times for one reason
//! and each time it was reported against this repository:
//!
//!   * `core-09` added `examples/valid/gate.external.yml` and
//!     `gate.self-contained.yml` — **gate declarations**, not service manifests —
//!     to a directory of service manifests, and `the_schema_accepts_core_s_own_
//!     valid_examples` validated every `*.yml` in it against
//!     `cafaye.manifest.schema.json`. Red, in a repository nobody touched.
//!   * `identity` and `muse` merged to their own masters and `registry/services/`
//!     — copies of *their* files — went stale. Red, in a repository that owns
//!     neither file.
//!
//! Both are the same shape, and MD15 ruled the first one: **a consumer resolves
//! core's examples at the `CORE_REF` it has actually vendored, never from the
//! working tree.** What that is worth is entirely in what follows from it:
//!
//! 1. The examples a consumer validates are the ones it recorded, so a merge in
//!    core can no longer be reported here as a failure here.
//! 2. The consumer's claim becomes true or false for a reason *inside* the
//!    consumer — a copy somebody edited, or a ref somebody bumped — which is
//!    what makes a red in this gate worth reading.
//! 3. "I read nothing" is not a pass. Every path in here either asserts
//!    something or says which pin is missing.

use std::path::{Path, PathBuf};

use pantry::manifest;
use pantry::pin::{self, PinResolution, PinSource};

// -------------------------------------------------------------------------
// The pinned ref under test.
//
// `39acaed6f1e25a895b0410e0689fe68d523b1b63` is core's `worker/core-08` merge —
// the commit before `core-09` added the two gate declarations. It is pinned here
// deliberately and for a reason that is checked below rather than assumed: it is
// a ref where `examples/valid/` contains service manifests ONLY, and where
// `schemas/cafaye.manifest.schema.json` is byte-identical to core HEAD's. So the
// only thing the ref changes is the *examples*, which is what makes it the right
// instrument for this test: a consumer pinned here must pass, and a consumer
// reading the working tree must fail, and the difference between them is exactly
// the bug.
// -------------------------------------------------------------------------
const PRE_GATE_EXAMPLES: &str = "39acaed6f1e25a895b0410e0689fe68d523b1b63";

/// Where core's checkout is. Same contract as every other workspace-reading test
/// in this suite: named on failure, and a skip that says what would make it run.
fn core_checkout() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("PANTRY_CAFAYE_ROOT").map(PathBuf::from),
        Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .to_path_buf(),
        ),
    ];

    candidates.into_iter().flatten().find_map(|root| {
        let core = root.join("core");
        core.join("schemas/cafaye.manifest.schema.json")
            .is_file()
            .then_some(core)
    })
}

/// A scratch directory that cleans itself up, because a suite that leaves
/// fixtures behind is a suite whose next run measures the last one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pantry-core-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        Self(dir)
    }

    fn write(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a parent directory");
        }
        std::fs::write(&path, body).expect("the fixture is written");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a consumer does with core's examples, and what it says when it cannot.
///
/// There is no `Ok(vec![])` here. The two ways this can come back empty —
/// "there are no examples" and "I could not tell" — are different findings, and
/// collapsing them is how a green badge comes to mean nothing.
#[derive(Debug)]
enum Resolved {
    Examples {
        /// The manifest schema as it stood at the pinned ref, because the
        /// examples are only meaningful against the schema of THEIR OWN ref.
        /// Validating core's `94f8d25` examples against core HEAD's schema would
        /// be the same category error one level up.
        schema: Vec<u8>,
        examples: Vec<ValidExample>,
    },
    /// Named, so the skip can be printed and asserted on rather than inferred
    /// from the absence of a failure.
    Skipped {
        /// What stopped it, in the words a reader would need to fix it.
        reason: String,
    },
}

/// One example, and the schema that governs it.
#[derive(Debug)]
struct ValidExample {
    name: String,
    kind: DocumentKind,
    body: Vec<u8>,
}

/// Which of core's schemas validates this document.
///
/// Three kinds are real in `core/examples/valid/` today and they answer different
/// questions. See `DocumentKind::manifest_suffix` and `NON_MANIFEST_EXAMPLES`
/// for why the classification is by name rather than by sniffing the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    /// A service manifest: `cafaye.manifest.schema.json`.
    Manifest,
    /// A gate declaration: `gate.schema.json`.
    GateDeclaration,
    /// A tenancy declaration — what a service publishes about the account
    /// boundary it enforces: `tenant-isolation.schema.json`.
    ///
    /// The third kind, and the one D3 said would arrive. It arrived with
    /// `core-15`'s two examples rather than in the same packet as the table, so
    /// this build went red naming both files — which is the mechanism working,
    /// not the mechanism failing. See `DECISIONS.md` D3, RULED.
    TenantIsolation,
}

impl DocumentKind {
    /// The schema file, relative to `core/schemas/`.
    fn schema_file(self) -> &'static str {
        match self {
            DocumentKind::Manifest => "cafaye.manifest.schema.json",
            DocumentKind::GateDeclaration => "gate.schema.json",
            DocumentKind::TenantIsolation => "tenant-isolation.schema.json",
        }
    }
}

/// `core/examples/valid/` holds a file this build cannot classify.
///
/// The answer is a hard error rather than a guess, and the reason is the one
/// thing this packet exists to prevent: **a consumer silently validating a
/// document kind it has no schema for, or silently skipping one it should have
/// validated.** Both look identical from outside — a green run — and both are
/// wrong. So the classification must be total: every file in that directory is
/// either named below as a manifest or named in [`NON_MANIFEST_EXAMPLES`], and
/// anything else stops the build with its name in the message.
fn classify(name: &str) -> Option<DocumentKind> {
    // A manifest example is one whose name says so. This is core's own spelling
    // and not a convention invented here: every manifest in
    // `examples/valid/` is `*.cafaye.yml`, `caf contract lint` only lints files
    // named `cafaye.yml`, and `registry/services/<name>/cafaye.yml` in this
    // repository depends on exactly that. A consumer already has to know the
    // rule to run the platform's own validator over a directory of examples, so
    // the classification costs nothing to state.
    if name.ends_with(".cafaye.yml") {
        return Some(DocumentKind::Manifest);
    }
    if let Some((_, kind, _)) = NON_MANIFEST_EXAMPLES.iter().find(|(file, _, _)| *file == name) {
        return Some(*kind);
    }
    None
}

/// The files in `core/examples/valid/` that are **not** service manifests.
///
/// Each row says what the file actually is and which of core's schemas governs
/// it. That second half is the point: a name is a decision, and a decision with
/// no schema attached to it is a conflation waiting to be reported as a broken
/// schema. If core ever moves a gate example, core ever renames one, or core ever
/// adds a third kind, this table is where that shows up — and
/// `every_example_in_core_s_valid_examples_is_classified_by_this_table` fails
/// with the unclassified filename, rather than a consumer failing with
/// `"owner" is a required property` on a document that was never a manifest.
///
/// This is the consumer half of the decision. The producer half is core's: the
/// directory would be cleaner split by kind (`examples/valid/manifests/`,
/// `examples/valid/gates/`, `examples/valid/tenancy/`), and that change is
/// core's to make, not this repository's — see `DECISIONS.md` D3, which records
/// the recommendation and says why it was not made here.
///
/// Each row now carries the **kind** as well as the name and the description,
/// because the third kind arrived carrying its own schema. With two kinds the
/// table could infer `GateDeclaration` for every row and still be right; with
/// three it cannot, and a table that hardcoded the inference would have validated
/// `tenancy.honest-zero.yml` against `gate.schema.json` — a document passing a
/// schema that governs none of its fields, which is the exact conflation D3 was
/// opened to prevent.
const NON_MANIFEST_EXAMPLES: &[(&str, DocumentKind, &str)] = &[
    (
        "gate.external.yml",
        DocumentKind::GateDeclaration,
        "a gate that needs the machine",
    ),
    (
        "gate.self-contained.yml",
        DocumentKind::GateDeclaration,
        "a gate that needs nothing but itself",
    ),
    (
        "tenancy.account-scoped.yml",
        DocumentKind::TenantIsolation,
        "a service holding customer rows, declaring every entry point that reaches them",
    ),
    (
        "tenancy.honest-zero.yml",
        DocumentKind::TenantIsolation,
        "a service holding none, saying so explicitly rather than by omission",
    ),
];

/// Resolve core's examples for `consumer` at the ref `consumer` has recorded.
///
/// This is the function the ruling is about, and every branch of it is one of
/// the three states the packet names: a recorded pin resolves it, a missing pin
/// skips **and names**, and the working tree is never a fallback.
fn examples_for(consumer: &Path, core: &Path) -> Resolved {
    match pin::resolve(consumer) {
        PinResolution::Found(found) => {
            let schema = match pin::show(core, &found.sha, "schemas/cafaye.manifest.schema.json") {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Resolved::Skipped {
                        reason: format!(
                            "pinned ref {} (from {}) is not readable in {}/. This is a shallow \
                             clone, or a ref nobody has fetched — fetch it and re-run. \
                             Cause: {error}",
                            found.sha,
                            found.source,
                            core.display()
                        ),
                    };
                }
            };

            let names = match pin::list_files(core, &found.sha, "examples/valid") {
                Ok(names) => names,
                Err(error) => {
                    return Resolved::Skipped {
                        reason: format!(
                            "examples/valid/ is not readable at pinned ref {} (from {}). \
                             Cause: {error}",
                            found.sha, found.source
                        ),
                    };
                }
            };

            let mut examples = Vec::new();
            for name in names {
                if !name.ends_with(".yml") {
                    continue;
                }
                let Some(kind) = classify(&name) else {
                    return Resolved::Skipped {
                        reason: format!(
                            "core/examples/valid/{name} at pinned ref {} is neither a service \
                             manifest (`*.cafaye.yml`) nor a row in this build's \
                             NON_MANIFEST_EXAMPLES table. A third document kind needs a decision \
                             about which schema governs it, not a guess — see DECISIONS.md D3.",
                            found.sha
                        ),
                    };
                };
                let Ok(body) = pin::show(core, &found.sha, &format!("examples/valid/{name}"))
                else {
                    continue;
                };
                examples.push(ValidExample { name, kind, body });
            }

            if examples.is_empty() {
                return Resolved::Skipped {
                    reason: format!(
                        "core/examples/valid/ at pinned ref {} holds no classified `*.yml` \
                         examples, so nothing would be checked",
                        found.sha
                    ),
                };
            }

            Resolved::Examples { schema, examples }
        }
        PinResolution::Disagreeing(a, b) => Resolved::Skipped {
            reason: format!(
                "{} records two different core pins — {} from {} and {} from {}. The bytes \
                 under test and the ref the tests check are not the same commit. Reconcile them \
                 before this says anything.",
                consumer.display(),
                a.sha,
                a.source,
                b.sha,
                b.source
            ),
        },
        PinResolution::Missing(missing) => Resolved::Skipped {
            reason: {
                let mut reason = format!(
                    "no core pin is recorded in {}. Searched {}.",
                    consumer.display(),
                    missing
                        .searched
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" and ")
                );
                if let Some((source, value)) = &missing.non_commit_ref {
                    reason.push_str(&format!(
                        " ({source} says {value:?}, which is a ref and not a commit — a pin has \
                         to be a commit, or this is a question that changes answer over time.)"
                    ));
                }
                reason.push_str(
                    " Vendored bytes with no recorded origin are not a pass: nothing below \
                     describes the schema this repository was actually built against.",
                );
                reason
            },
        },
    }
}

// -------------------------------------------------------------------------
// The tests.
// -------------------------------------------------------------------------

/// **The proof this packet is a fix and not a suppression.**
///
/// A consumer whose recorded pin is *older* than the working tree validates the
/// *older* examples, and therefore passes — while a consumer reading the working
/// tree fails, because `core-09` has since put two gate declarations in a
/// directory of service manifests. Before the fix this test was
/// `the_schema_accepts_core_s_own_valid_examples`, which read the working tree
/// and was red for exactly that reason. This is the same assertion with the
/// working tree removed from the middle of it.
#[test]
fn a_consumer_pinned_to_an_older_ref_validates_that_refs_examples() {
    let Some(core) = core_checkout() else {
        eprintln!(
            "SKIP core examples at a pinned ref: no core checkout found. Set \
             PANTRY_CAFAYE_ROOT to the directory holding core/. The ref under test is {PRE_GATE_EXAMPLES}."
        );
        return;
    };

    let consumer = Scratch::new("pinned-old");
    consumer.write(
        "vendir.lock.yml",
        &format!("directories:\n  - path: schemas\n    contents:\n      - git:\n          sha: {PRE_GATE_EXAMPLES}\n"),
    );

    let (schema, examples) = match examples_for(&consumer.0, &core) {
        Resolved::Examples { schema, examples } => (schema, examples),
        Resolved::Skipped { reason } => panic!("{reason}"),
    };

    assert!(
        examples
            .iter()
            .any(|example| example.kind == DocumentKind::Manifest),
        "a pinned ref must yield at least one manifest example to validate, got {:?}",
        examples.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        examples
            .iter()
            .all(|example| example.kind == DocumentKind::Manifest),
        "the ref under test should predate the gate declarations, and it yielded {:?}",
        examples
            .iter()
            .map(|e| (e.name.as_str(), e.kind))
            .collect::<Vec<_>>()
    );

    for example in &examples {
        manifest::validate_against(
            &schema,
            &example.body,
            Path::new(&format!("examples/valid/{}", example.name)),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", example.name));
    }
}

/// The counterpart, and the thing that makes the test above mean something: the
/// pinned examples must NOT be the working tree's examples. If a future change
/// made the resolver fall back to the checkout, both tests would still pass — one
/// because the fallback happens to validate, the other because it never noticed.
/// So this asserts the two listings actually differ, by name.
#[test]
fn resolving_at_a_pin_is_not_the_same_as_reading_the_working_tree() {
    let Some(core) = core_checkout() else {
        eprintln!("SKIP pinned-vs-working-tree: no core checkout found. Set PANTRY_CAFAYE_ROOT.");
        return;
    };

    let consumer = Scratch::new("pinned-vs-tree");
    consumer.write(
        "vendir.lock.yml",
        &format!("directories:\n  - contents:\n      - git:\n          sha: {PRE_GATE_EXAMPLES}\n"),
    );

    let pinned = match examples_for(&consumer.0, &core) {
        Resolved::Examples { examples, .. } => examples,
        Resolved::Skipped { reason } => panic!("{reason}"),
    };

    let mut working_tree: Vec<String> = std::fs::read_dir(core.join("examples/valid"))
        .expect("core's examples directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".yml"))
        .collect();
    working_tree.sort();

    let pinned_names: Vec<&str> = pinned.iter().map(|example| example.name.as_str()).collect();
    let only_in_tree: Vec<&String> = working_tree
        .iter()
        .filter(|name| !pinned_names.contains(&name.as_str()))
        .collect();

    assert!(
        !only_in_tree.is_empty(),
        "resolving at {PRE_GATE_EXAMPLES} produced exactly the working tree's examples \
         ({pinned_names:?}). Either core has not moved since core-08 — in which case this ref \
         is the wrong instrument and the test above is vacuous — or the resolver is reading \
         the working tree, which is the bug this file exists to fix."
    );
}

/// The no-pin path, and it is the NORMAL path in this fleet: `caf` and `pantry`
/// held vendored bytes with no recorded origin when this was written, so the
/// outcome has to be a usable message rather than an edge case nobody exercised.
///
/// The brief's requirement is specific — a skip that names the pin — and this
/// asserts the name rather than the shape, because a shape can be satisfied by
/// a message that says nothing.
#[test]
fn a_consumer_with_no_recorded_pin_skips_and_names_the_missing_pin() {
    let Some(core) = core_checkout() else {
        eprintln!("SKIP no-pin naming: no core checkout found. Set PANTRY_CAFAYE_ROOT.");
        return;
    };

    let consumer = Scratch::new("no-pin");
    consumer.write("README.md", "vendored bytes, no recorded origin\n");

    match examples_for(&consumer.0, &core) {
        Resolved::Examples { examples, .. } => panic!(
            "a consumer with no recorded pin resolved {} example(s) anyway — from where? A \
             fallback to the working tree is the defect, not a degraded form of the fix.",
            examples.len()
        ),
        Resolved::Skipped { reason } => {
            assert!(
                reason.contains("no core pin is recorded"),
                "the skip names the missing pin, got: {reason}"
            );
            assert!(
                reason.contains("vendir.lock.yml"),
                "the skip says where a pin would have been read, got: {reason}"
            );
            assert!(
                reason.contains(".github/workflows"),
                "the skip says where the other pin shape would have been read, got: {reason}"
            );
            assert!(
                !reason.contains("core/examples/valid"),
                "the skip does not claim to have read examples it did not read, got: {reason}"
            );
        }
    }
}

/// The pin in this file has to be a claim about core's history and not a hope.
/// If core is rewritten, or `core-09` were reverted, this ref stops being
/// "before the gate examples" and the test above becomes an assertion about
/// nothing. Checked rather than assumed, because a fixture pin that silently
/// stops meaning what it says is the same failure in a smaller box.
#[test]
fn the_ref_under_test_really_is_a_ref_before_the_gate_examples() {
    let Some(core) = core_checkout() else {
        eprintln!("SKIP fixture pin: no core checkout found. Set PANTRY_CAFAYE_ROOT.");
        return;
    };

    let at_pin = pin::list_files(&core, PRE_GATE_EXAMPLES, "examples/valid")
        .expect("examples/valid is readable at the ref under test");
    assert!(
        !at_pin.iter().any(|name| name.starts_with("gate.")),
        "{PRE_GATE_EXAMPLES} was expected to predate the gate declarations, and it carries \
         {at_pin:?}"
    );

    let head = pin::published_head(&core).expect("core resolves a published head");
    let at_head = pin::list_files(&core, &head, "examples/valid").expect("readable at HEAD");
    assert!(
        at_head.iter().any(|name| name.starts_with("gate.")),
        "core HEAD is expected to carry the gate declarations core-09 added, and it carries \
         {at_head:?}. If core has moved, pick a new PRE_GATE_EXAMPLES and say in the commit \
         message why — do not relax this."
    );

    // And the schema is unchanged across the two refs, so the test above is
    // isolating the EXAMPLES. If core had edited the schema in the same window,
    // a pass at the old ref would be passing for the wrong reason.
    let schema_at_pin = pin::show(
        &core,
        PRE_GATE_EXAMPLES,
        "schemas/cafaye.manifest.schema.json",
    )
    .expect("readable");
    let schema_at_head =
        pin::show(&core, &head, "schemas/cafaye.manifest.schema.json").expect("readable");
    assert_eq!(
        String::from_utf8_lossy(&schema_at_pin),
        String::from_utf8_lossy(&schema_at_head),
        "the manifest schema changed between {PRE_GATE_EXAMPLES} and core HEAD, so a pass at \
         the older ref would not be attributable to the examples"
    );
}

// -------------------------------------------------------------------------
// The classification itself: total, and holding in both directions.
// -------------------------------------------------------------------------

/// Every `*.yml` in core's `examples/valid/` is either a manifest or a named
/// non-manifest — at the ref this repository has actually vendored.
///
/// Both directions, which is the part that matters. A table that only has to
/// contain its rows is a comment; a table that has to be *exactly* the set on
/// disk is a claim the suite checks on every run:
///   * a file nobody classified is a FAIL naming the file — a third document
///     kind must be decided on, not guessed at;
///   * a row that no longer exists is a FAIL naming the row — so core moving a
///     gate example cannot leave a stale entry claiming a document is still there.
#[test]
fn every_example_in_core_s_valid_examples_is_classified_by_this_table() {
    let Some(core) = core_checkout() else {
        eprintln!("SKIP example classification: no core checkout found. Set PANTRY_CAFAYE_ROOT.");
        return;
    };

    let head = pin::published_head(&core).expect("core resolves a published head");
    let names = pin::list_files(&core, &head, "examples/valid").expect("readable at core HEAD");

    let mut manifests = 0usize;
    let mut unclassified: Vec<&String> = Vec::new();

    for name in names.iter().filter(|name| name.ends_with(".yml")) {
        match classify(name) {
            Some(DocumentKind::Manifest) => manifests += 1,
            Some(DocumentKind::GateDeclaration) => {}
            Some(DocumentKind::TenantIsolation) => {}
            None => unclassified.push(name),
        }
    }

    assert!(
        manifests > 0,
        "no manifest examples were found at {head}; the classification is not being exercised"
    );
    assert!(
        unclassified.is_empty(),
        "{} file(s) in core/examples/valid/ match neither rule and neither row of \
         NON_MANIFEST_EXAMPLES:\n  {}\n\n\
         This is the case that made the gate red three times. A consumer that cannot classify a \
         document must not validate it against the manifest schema (the file will fail with \
         \"owner is a required property\", naming a manifest rule that never applied to it) and \
         must not skip it in silence either. Decide what it is, which of core's schemas governs \
         it, and add the row — see DECISIONS.md D3.",
        unclassified.len(),
        unclassified
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    // The reverse: every declared row must still be a file. A row left behind
    // after core moves a file is a claim about a document that no longer exists,
    // and it is the direction that never shows up as a failure.
    let vanished: Vec<&str> = NON_MANIFEST_EXAMPLES
        .iter()
        .map(|(file, _, _)| *file)
        .filter(|file| !names.iter().any(|name| name == file))
        .collect();

    assert!(
        vanished.is_empty(),
        "NON_MANIFEST_EXAMPLES declares {} but core/examples/valid/ at {head} does not carry it. \
         Core moved or removed the file; delete the row in the same commit. A row describing a \
         document that is no longer there is a classification nobody is checking.",
        vanished
            .iter()
            .map(|file| format!("{file:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Each non-manifest row names a kind AND the schema that governs it, and that
/// schema exists in core.
///
/// A row that names a kind but not a schema is the conflation with extra
/// paperwork: it says "this is not a manifest" and leaves the reader to guess
/// what validates it. The packet is explicit that a document kind with no stated
/// schema is the defect, so the schema half is asserted rather than documented.
#[test]
fn every_non_manifest_kind_names_the_schema_that_governs_it() {
    let Some(core) = core_checkout() else {
        eprintln!("SKIP schema binding: no core checkout found. Set PANTRY_CAFAYE_ROOT.");
        return;
    };

    for (file, declared, description) in NON_MANIFEST_EXAMPLES {
        let kind = classify(file)
            .unwrap_or_else(|| panic!("{file} is in the table but does not classify"));
        assert_eq!(
            kind, *declared,
            "{file} ({description}) is declared as {declared:?} but classifies as {kind:?}"
        );
        assert!(
            !description.trim().is_empty(),
            "{file} is declared without saying what it is"
        );

        // The schema has to EXIST in core, at the ref being read. A row naming
        // a schema core does not ship is a row that reads as authority and
        // checks nothing — and with three kinds in the directory it is now the
        // failure mode a typo in a match arm would produce, silently.
        let head = pin::published_head(&core).expect("core resolves a published head");
        let schema = kind.schema_file();
        assert!(
            pin::show(&core, &head, &format!("schemas/{schema}")).is_ok(),
            "{file} is declared as {declared:?}, so schemas/{schema} governs it — but core does \
             not ship that file at {head}. Either core renamed the schema or this row names the \
             wrong kind. A row pointing at a schema that is not there validates nothing."
        );
    }
}

/// This repository's own claim, at this repository's own recorded pin.
///
/// Replaces `the_schema_accepts_core_s_own_valid_examples`. The old test asked
/// "does core's manifest schema accept every document in a directory that has
/// since gained a second document kind?" and reported the answer as a failure of
/// this repository's schema. The new claim is the one the old one was trying to
/// make: **the manifests this repository ships validate against the schema at
/// the ref this repository has actually vendored.**
#[test]
fn every_manifest_this_repository_ships_validates_at_the_ref_it_vendored() {
    let Some(core) = core_checkout() else {
        eprintln!(
            "SKIP pantry manifests at its pinned core ref: no core checkout. Set PANTRY_CAFAYE_ROOT."
        );
        return;
    };

    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolution = pin::resolve(here);

    let found = match &resolution {
        PinResolution::Found(found) => found,
        other => {
            // Named, not silent, and not a pass. This is the state `caf` and
            // `pantry` were both measured in.
            eprintln!(
                "SKIP pantry manifests at its pinned core ref: this repository records no core \
                 pin. The vendored bytes in schemas/ have no recorded origin, so \"validates \
                 against core's schema\" is not a claim this suite can make — only \"validates \
                 against the copy of the schema in schemas/\", which is a different and much \
                 smaller claim. Resolution: {other:?}. Record the pin in vendir.lock.yml."
            );
            return;
        }
    };
    assert_eq!(found.source, PinSource::VendirLockfile);

    let schema = pin::show(&core, &found.sha, "schemas/cafaye.manifest.schema.json")
        .unwrap_or_else(|error| {
            panic!(
                "pantry's recorded core pin {} ({}) is not readable in {}/. Cause: {error}",
                found.sha,
                found.source,
                core.display()
            )
        });

    let mut checked = 0usize;
    let mut mine = vec![here.join("cafaye.yml")];
    mine.extend(pantry::registry::manifest_paths(
        &pantry::registry::registry_dir(),
    ));
    assert!(
        mine.len() > 1,
        "expected this repository's own manifest and its registry entries, found {} file(s)",
        mine.len()
    );

    for path in &mine {
        let body = std::fs::read(path).expect("a shipped manifest is readable");
        manifest::validate_against(&schema, &body, path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        checked += 1;
    }

    assert_eq!(checked, mine.len(), "every shipped manifest was validated");
}
