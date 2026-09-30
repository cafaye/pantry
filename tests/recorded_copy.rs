//! Every registered copy matches the service it was taken from — **at the ref
//! this registry records**.
//!
//! ## Why this file is not the test it replaced
//!
//! `tests/drift.rs` compared `registry/services/<name>/cafaye.yml` against
//! `<service>/cafaye.yml` **in the working tree**. That is the same defect MD15
//! ruled on for core's examples, wearing different clothes: the claim was "this
//! copy is what the service says", and what it actually checked was "this copy
//! is what the service said at the moment this test ran". The two come apart the
//! first time a service merges, and the packet heard about it three times:
//!
//! ```text
//! identity  copy 11591 bytes, real 13302 bytes — first differs at line 216
//!           COMMENT-ONLY drift
//! muse      copy  2930 bytes, real  6280 bytes — first differs at line 49
//!           A YAML FIELD MOVED TOO — the copy is not merely stale, it is WRONG
//! ```
//!
//! Every one of those reds was reported as *pantry* being broken. Pantry was not
//! broken: `identity-09` and `muse-06` had landed, and the copies had not been
//! refreshed. **A merge in another repository could only ever be reported as a
//! failure here**, which is what made the first two reports unreadable without a
//! timestamp — the reader cannot tell a defect in this repository from a merge in
//! somebody else's.
//!
//! ## What is different now
//!
//! Each entry carries a `recordedAt` commit in `registry/index.yml`, and this
//! file checks the copy against **that commit**, not against whatever the
//! sibling checkout happens to be sitting on. So the check's claim becomes:
//!
//! > this copy is byte-for-byte what service S said at ref R
//!
//! — which is true or false for a reason *inside this repository*. A merge in
//! `identity` no longer turns this gate red; it turns
//! `the_registry_says_which_ref_each_copy_was_taken_at` green and hands over to
//! the staleness report below, which names the distance and the fix.
//!
//! ## The copy is still a copy, and that is a decision
//!
//! An offline registry cannot resolve refs at request time: a container has no
//! sibling checkouts and no network, and `Registry::load` runs at startup. So
//! `registry/services/*/cafaye.yml` is a copy, and this file is why that is safe.
//! What changed is *what it is a copy of* — a named commit rather than "recently".
//! See `AGENTS.md`, "The registry is a copy", and `DECISIONS.md` D4.

use std::path::{Path, PathBuf};

use pantry::manifest;
use pantry::pin;
use pantry::registry::{self, registry_dir};

/// The cafaye workspace. Same contract as `tests/drift.rs` and `tests/schema.rs`:
/// `PANTRY_CAFAYE_ROOT`, else the parent of this repository.
fn cafaye_root() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("PANTRY_CAFAYE_ROOT").map(PathBuf::from),
        Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .to_path_buf(),
        ),
    ];

    candidates.into_iter().flatten().find(|root| {
        // A directory that merely exists is not proof: the workspace has core in
        // it, and core is what every cafaye repository depends on.
        root.join("core/schemas/cafaye.manifest.schema.json")
            .is_file()
    })
}

/// Skips loudly and says exactly what would make it run. A drift check that
/// cannot see the real services has proven nothing, so this is a skip and not a
/// pass.
fn skip_without_workspace(what: &str) {
    eprintln!(
        "SKIP {what}: no cafaye workspace found. Set PANTRY_CAFAYE_ROOT to the directory \
         holding core/, identity/, billing/, guard/ and muse/, or run the suite from a checkout \
         inside moon/cafaye/. This is a skip, not a pass — nothing was verified."
    );
}

/// Where a registered service's own files live.
///
/// `pantry` is the one name whose registry copy and real service are the same
/// file in the same tree, resolved here so every check in the suite agrees about
/// it rather than each special-casing it.
fn service_root(root: &Path, name: &str) -> PathBuf {
    if name == "pantry" {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    } else {
        root.join(name)
    }
}

/// The registry copy, which is the file every check in this file is about.
fn copy_of(name: &str) -> PathBuf {
    registry_dir()
        .join("services")
        .join(name)
        .join("cafaye.yml")
}

/// Read a service's `cafaye.yml` at `ref`, or say precisely why it could not be.
///
/// Three failure modes with three different fixes, kept distinct on purpose: a
/// ref nobody has fetched (a shallow clone in CI), a ref that does not exist (a
/// typo in the index), and a service with no `cafaye.yml` at that ref (the file
/// was added or removed upstream). Collapsing them into "could not read" is what
/// makes a skip unfixable.
fn manifest_at(root: &Path, name: &str, reference: &str) -> Result<Vec<u8>, String> {
    let service = service_root(root, name);
    pin::show(&service, reference, "cafaye.yml").map_err(|error| match error {
        pin::GitError::UnknownRef { .. } => format!(
            "{name} has no commit {reference} locally, so this registry copy cannot be checked. \
             Either the recorded ref is wrong or the clone is shallow — \
             `git -C {} fetch --unshallow` and re-run. Cause: {error}",
            service.display()
        ),
        pin::GitError::MissingBlob { .. } => format!(
            "{name}@{reference} has no cafaye.yml, so the copy recorded for it cannot be \
             verified against it. If the service renamed or removed its manifest, delete the \
             entry and record why. Cause: {error}"
        ),
        other => format!("{name}@{reference}: {other}"),
    })
}

/// Parse manifest bytes into the typed form.
///
/// `manifest::read` takes a path, which is the right default but wrong here: the
/// bytes in question were read out of git at a ref and never touch the working
/// tree, so there is no path to name. Validated against the vendored schema on
/// the way in, because a struct comparison between a schema-validated copy and
/// an unvalidated blob would be a comparison with something weaker on one side.
fn parse(bytes: &[u8]) -> Option<manifest::Manifest> {
    manifest::validate_against(
        manifest::MANIFEST_SCHEMA.as_bytes(),
        bytes,
        Path::new("<recorded>"),
    )
    .ok()
}

/// The first line where two blobs differ, 1-indexed, for a message that does not
/// send the reader to `diff` to find out where to look.
fn first_differing_line(left: &str, right: &str) -> usize {
    left.lines()
        .zip(right.lines())
        .position(|(a, b)| a != b)
        .map(|index| index + 1)
        .unwrap_or_else(|| left.lines().count().min(right.lines().count()) + 1)
}

/// **The gate.** Every registry copy is byte-for-byte what its service said at
/// the ref this registry records.
///
/// This is the check that used to read the working tree and fail for merges
/// elsewhere. It now reads `recordedAt`, so:
///   * a service merging new content does NOT turn this red;
///   * a copy edited in this repository, or a `recordedAt` bumped without
///     re-copying, DOES — and both are defects here.
///
/// Every stale copy is reported in one failure rather than the first one, so a
/// refresh sees the whole list from a single run.
#[test]
fn every_registered_copy_is_verbatim_at_the_ref_this_registry_records() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("registry copies at their recorded ref");
    };

    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");
    assert!(
        !index.services.is_empty(),
        "the registry has no services, so nothing was checked"
    );

    let mut stale: Vec<String> = Vec::new();

    for (name, entry) in &index.services {
        let copy = copy_of(name);
        let recorded_at = entry.recorded_at.as_deref().unwrap_or_else(|| {
            panic!(
                "{} has no `recordedAt` in registry/index.yml, so this copy has no recorded \
                 origin. A copy with no ref is a copy nobody can check: pantry cannot say what \
                 it is a copy OF, and a reader cannot tell a fresh copy from a five-month-old \
                 one. Add the service's current commit — `git -C ../{name} rev-parse HEAD` — in \
                 the same commit that copies the file.",
                copy.display()
            )
        });

        let upstream = match manifest_at(&root, name, recorded_at) {
            Ok(bytes) => bytes,
            Err(reason) => {
                stale.push(format!("  {name}\n    {reason}"));
                continue;
            }
        };

        let copied = std::fs::read(&copy)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", copy.display()));

        if copied == upstream {
            continue;
        }

        // Say which kind of drift this is. Comment-only means the copy is out of
        // date rather than wrong; a moved field means the registry is publishing
        // something the service does not say, which is the failure that matters.
        let fields_match = manifest::read(&copy)
            .ok()
            .zip(parse(&upstream))
            .is_some_and(|(a, b)| a == b);

        let at = first_differing_line(
            &String::from_utf8_lossy(&copied),
            &String::from_utf8_lossy(&upstream),
        );

        stale.push(format!(
            "  {name}\n    \
             copy     registry/services/{name}/cafaye.yml ({} bytes)\n    \
             recorded {name}@{recorded_at}\n    \
             at ref   {upstream_len} bytes — first differs at line {at}\n    \
             {fields}\n    \
             fix       cp ../{name}/cafaye.yml registry/services/{name}/cafaye.yml && \
             git -C ../{name} rev-parse HEAD   # then put that sha in `recordedAt`",
            copied.len(),
            upstream_len = upstream.len(),
            fields = if fields_match {
                "every YAML field already matches — COMMENT-ONLY drift. The copy is missing \
                 decisions the service has recorded since this ref."
            } else {
                "a YAML FIELD MOVED — this entry is not merely stale, it is WRONG. The \
                 registry is publishing something the service does not say."
            },
        ));
    }

    assert!(
        stale.is_empty(),
        "{} registry cop{} not byte-identical to the service at the ref this registry records.\n\
         \n\
         {}\n\
         \n\
         A registry copy is VERBATIM, comments included (AGENTS.md, \"Registering or changing a \
         service\"): the copy is what a reviewer reads when asking what pantry thinks a service \
         is, and a service's DECISION NEEDED blocks live in its comments. A stale comment is \
         not cosmetic — it is the registry answering a question wrongly.\n\
         \n\
         Fix each one with the `cp` above AND update `recordedAt` in the same commit. A copy \
         refreshed without the ref moves the failure from \"loud and wrong\" to \"quiet and \
         still wrong\".",
        stale.len(),
        if stale.len() == 1 { "y is" } else { "ies are" },
        stale.join("\n"),
    );
}

/// The same claim, field by field, because a struct comparison names the field
/// that moved and a reader should not have to work out which one from a diff.
///
/// Kept as a separate test from the byte-equality one for the reason the two were
/// separate before: the byte check says "out of date", this one says "wrong", and
/// the second is the one that says which field.
#[test]
fn every_registered_copy_agrees_with_the_service_at_the_ref_this_registry_records() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("registry fields at their recorded ref");
    };

    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");

    for (name, entry) in &index.services {
        let recorded_at = entry
            .recorded_at
            .as_deref()
            .unwrap_or_else(|| panic!("{} records no `recordedAt`", name));

        let upstream_bytes =
            manifest_at(&root, name, recorded_at).unwrap_or_else(|reason| panic!("{reason}"));

        let copied = manifest::read(&copy_of(name)).expect("the registry copy parses");
        let upstream = parse(&upstream_bytes)
            .unwrap_or_else(|| panic!("{name}@{recorded_at} does not parse as a manifest"));

        if copied != upstream {
            // Name the fields that moved. A `Debug` diff on a struct is a wall of
            // text; the field that actually matters — `dependencies[0].required`
            // for muse, which is the difference between "runs degraded" and "503
            // on every request" — is one line here.
            let mut report = vec![format!("  {name} (recorded {recorded_at})")];

            if copied.core != upstream.core {
                report.push(format!(
                    "    core: copy {:?}, recorded {:?}",
                    copied.core, upstream.core
                ));
            }
            if copied.repository.url != upstream.repository.url {
                report.push(format!(
                    "    repository.url: copy {:?}, recorded {:?}",
                    copied.repository.url, upstream.repository.url
                ));
            }
            if copied.language != upstream.language {
                report.push(format!(
                    "    language: copy {:?}, recorded {:?}",
                    copied.language, upstream.language
                ));
            }
            report.extend(dependencies_named(&copied, &upstream));

            panic!(
                "{} is not what {} said at {recorded_at}:\n{}\n\n\
                 The registry is publishing something the service does not say. Refresh the copy \
                 with `cp ../{name}/cafaye.yml registry/services/{name}/cafaye.yml` and record \
                 the new commit in `recordedAt`.",
                copy_of(name).display(),
                name,
                report.join("\n")
            );
        }
    }
}

/// `dependencies[i].required` differences, named.
///
/// This is the one that bit. `muse-06` made `identity` a **required** dependency
/// because every token is verified against identity's JWKS, so a muse without
/// identity answers 503 for every request — and the registry copy said
/// `required: false`, which is a registry telling a topology tool that muse can
/// start with no issuer. A copy that is merely stale loses a decision; a copy
/// whose fields moved is the registry publishing a falsehood, and the difference
/// between those two is worth a helper that says which one happened.
fn dependencies_named(copied: &manifest::Manifest, upstream: &manifest::Manifest) -> Vec<String> {
    let empty = Vec::new();
    let left = copied.dependencies.as_ref().unwrap_or(&empty);
    let right = upstream.dependencies.as_ref().unwrap_or(&empty);

    let mut report = Vec::new();
    if left.len() != right.len() {
        report.push(format!(
            "    dependencies: copy has {} entr{} ({}), recorded has {} ({}).",
            left.len(),
            if left.len() == 1 { "y" } else { "ies" },
            names(left),
            right.len(),
            names(right)
        ));
        return report;
    }

    for (a, b) in left.iter().zip(right) {
        if a != b {
            report.push(format!(
                "    dependencies[{name}]: copy says {a:?}, recorded says {b:?}",
                name = a.name
            ));
        }
    }
    report
}

fn names(entries: &[manifest::ServiceRef]) -> String {
    if entries.is_empty() {
        return "none".to_string();
    }
    entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// **The staleness report**, and the reason this packet's fix is not just a `cp`.
///
/// A refreshed copy with no staleness signal moves the failure from "loud and
/// wrong" to "quiet and still wrong": the gate goes green, the copy is accurate
/// as of its recorded ref, and nobody is told that `identity` has merged four
/// times since. This test is what tells them — by name, with the distance, and
/// with the command that closes it.
///
/// It is a **report, not a gate**, and the distinction is deliberate:
/// `kit/tests/staleness.py` says the same thing in its own docstring — "a stale
/// copy is LEGAL — it is a copy that has not been bumped yet — and a scheduled
/// report that is red every week is a report that gets muted". A merge in
/// `identity` must not be able to turn *this* repository's gate red; that is
/// precisely the failure MD15 ruled on. So the distances are printed on every
/// run, the count is asserted to be non-vacuous, and a copy that has fallen
/// **too far** behind is a failure with a stated threshold — see
/// [`STALENESS_BUDGET_COMMITS`].
///
/// Pass and skip counts are reported separately at the end, because a skipped
/// check is not a passing check and the whole defect in this packet's history is
/// a test that reported nothing while not testing the thing it named.
#[test]
fn the_registry_says_how_far_behind_each_copy_is_and_names_the_fix() {
    /// How many commits a copy may fall behind before it is a failure here.
    ///
    /// Not zero — zero would put this back where it started, failing the moment
    /// any service merges, which is the defect. Not unbounded — unbounded is the
    /// "quiet and still wrong" this replaces. Nine commits is roughly a working
    /// day of this fleet's merge rate, so a copy inside the budget is refreshed
    /// by a person noticing the report, and a copy outside it says the refresh
    /// has been missed for long enough to be worth stopping for.
    const STALENESS_BUDGET_COMMITS: u64 = 9;

    let Some(root) = cafaye_root() else {
        eprintln!(
            "SKIP registry staleness: no cafaye workspace found. Set PANTRY_CAFAYE_ROOT. \
             Nothing below was measured; this is a skip, not a pass."
        );
        return;
    };

    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");
    assert!(
        !index.services.is_empty(),
        "the registry has no services, so staleness was measured over nothing"
    );

    let mut current = 0usize;
    let mut behind = 0usize;
    let mut unmeasured: Vec<String> = Vec::new();
    let mut over_budget: Vec<String> = Vec::new();

    for (name, entry) in &index.services {
        let Some(recorded_at) = entry.recorded_at.as_deref() else {
            unmeasured.push(format!("{name} (no `recordedAt`)"));
            continue;
        };
        let service = service_root(&root, name);

        let Some(head) = pin::published_head(&service) else {
            unmeasured.push(format!("{name} (cannot resolve a published head)"));
            continue;
        };

        match pin::commits_behind(&service, recorded_at, &head) {
            None => unmeasured.push(format!(
                "{name} ({recorded_at} is not in {name}'s local history — a shallow clone, or a \
                 ref nobody fetched)"
            )),
            Some(0) => {
                current += 1;
                eprintln!("    current  {name} @ {recorded_at}");
            }
            Some(distance) => {
                behind += 1;
                eprintln!("    behind   {name} @ {recorded_at} — {distance} commit(s) behind");
                if distance > STALENESS_BUDGET_COMMITS {
                    over_budget.push(format!(
                        "  {name} is {distance} commit(s) behind ({recorded_at} vs {head})"
                    ));
                }
            }
        }
    }

    // Counts, separately, always. A reader who sees "all green" above must be
    // able to tell whether 9 copies were checked or 0.
    eprintln!(
        "\n    registry staleness: {current} current, {behind} behind, {} unmeasured, \
         {} service(s) total (budget: {STALENESS_BUDGET_COMMITS} commits)",
        unmeasured.len(),
        index.services.len()
    );
    for note in &unmeasured {
        eprintln!("    UNMEASURED {note}");
    }

    assert!(
        current + behind > 0,
        "no copy was measured at all — every service was unmeasurable, so this report says \
         nothing about anything. {unmeasured:?}"
    );

    assert!(
        over_budget.is_empty(),
        "{} registry cop{} more than {STALENESS_BUDGET_COMMITS} commit(s) behind the service they \
         were taken from:\n{}\n\n\
         This is the budget, not a judgement about the merge. A copy inside the budget is legal \
         and this report is how you find it; a copy outside it means the refresh has been missed \
         for long enough to be worth stopping for.\n\n\
         Close each one with:\n  \
         cp ../<service>/cafaye.yml registry/services/<service>/cafaye.yml\n  \
         git -C ../<service> rev-parse HEAD   # into `recordedAt`",
        over_budget.len(),
        if over_budget.len() == 1 {
            "y is"
        } else {
            "ies are"
        },
        over_budget.join("\n"),
    );
}

/// Every registered service records a ref, and the ref is a real commit in that
/// service's own repository.
///
/// Without this, the check above is `stale.is_empty()` over a loop that silently
/// measures nothing, and a `recordedAt` that is a branch name or a typo would
/// make every copy unverifiable while the gate stayed green.
#[test]
fn every_registered_service_records_a_ref_that_resolves() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("recorded refs resolve");
    };

    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");

    for (name, entry) in &index.services {
        let recorded_at = entry.recorded_at.as_deref().unwrap_or_else(|| {
            panic!(
                "{} records no `recordedAt`. Every copy needs a recorded origin, or \
                 `every_registered_copy_is_verbatim_at_the_ref_this_registry_records` has \
                 nothing to check and says so by passing.",
                name
            )
        });

        assert_eq!(
            recorded_at.len(),
            40,
            "{name} records `recordedAt: {recorded_at}`, which is not a full commit sha. A \
             branch or a short sha is a question that changes answer over time; a full sha is \
             the answer."
        );
        assert!(
            recorded_at
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "{name} records `recordedAt: {recorded_at}`, which is not lower-case hex"
        );

        let service = service_root(&root, name);
        assert!(
            service.join(".git").exists(),
            "{name} has no checkout at {}, so its recorded ref cannot be resolved. A recorded \
             ref nobody can read is a ref nobody is maintaining.",
            service.display()
        );
        assert!(
            pin::published_head(&service).is_some(),
            "{name} at {} does not resolve a published head, so `recordedAt` cannot be compared \
             against anything",
            service.display()
        );
    }
}
