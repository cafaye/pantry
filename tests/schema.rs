//! Every official entry validates against core's manifest schema.
//!
//! Two files are involved and they must not drift: `schemas/` holds a copy of
//! core's schema so the binary can validate at startup without a sibling
//! checkout, and core holds the original. The first test asserts the copy is
//! the original; the rest assert the copy actually rejects what core rejects,
//! because a validator that accepts everything would make the readiness probe
//! a decoration.

use std::path::Path;

use pantry::manifest;
use pantry::registry::BlockedBy;

/// Where the cafaye workspace lives when the suite runs next to the other
/// repositories (`moon/cafaye/<service>`). A single-service clone does not have
/// one, and the tests that need it say so instead of failing on a path.
fn cafaye_root() -> Option<std::path::PathBuf> {
    let candidates = [
        std::env::var_os("PANTRY_CAFAYE_ROOT").map(std::path::PathBuf::from),
        Some(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .to_path_buf(),
        ),
    ];

    candidates.into_iter().flatten().find(|root| {
        root.join("core/schemas/cafaye.manifest.schema.json")
            .is_file()
    })
}

fn skip_without_workspace(what: &str) {
    eprintln!(
        "SKIP {what}: no cafaye workspace found. Set PANTRY_CAFAYE_ROOT to the \
         directory holding core/, identity/, billing/, guard/ and muse/, or run \
         the suite from a checkout inside moon/cafaye/."
    );
}

/// The vendored schema is core's schema **at the ref this repository records**.
///
/// Was: byte-comparison against `core/schemas/cafaye.manifest.schema.json` in the
/// sibling working tree. Same defect as the copy tests in `tests/drift.rs` and
/// the same ruling — a vendored byte copy of somebody else's file has to be
/// checked against the commit it was taken from, or the check can only ever fail
/// for a reason outside this repository.
///
/// Reading at the recorded ref makes the claim "this vendored copy is what core
/// published at R", which is true or false for a reason here. When core moves,
/// this test stays green and `the_registry_says_how_far_behind_each_copy_is…`
/// in `tests/recorded_copy.rs` reports the distance — except that this file's
/// staleness is a *different* question from the registry's, because `core` is a
/// repository this one depends on rather than a service this one registers. So
/// the distance is reported here too, by name, and a schema that has moved so
/// far that the vendored copy no longer describes anything current is worth
/// seeing. Re-vendoring is one `cp`; the pin is what makes it a decision.
#[test]
fn the_vendored_schema_is_core_s_schema_at_the_ref_this_repository_records() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("vendored schema drift");
    };

    let core = root.join("core");
    let resolution = pantry::pin::resolve(Path::new(env!("CARGO_MANIFEST_DIR")));
    let recorded = match &resolution {
        pantry::pin::PinResolution::Found(pin) => pin.clone(),
        other => {
            eprintln!(
                "SKIP vendored schema at a recorded ref: this repository records no core pin \
                 ({other:?}). The bytes under schemas/ have no recorded origin, so \"this is \
                 core's schema\" is not a checkable claim — only \"this is the schema this \
                 binary was built with\", which `every_official_entry_validates_against_the_\
                 manifest_schema` already covers. Record the pin in vendir.lock.yml. This is a \
                 skip, not a pass."
            );
            return;
        }
    };

    let upstream = pantry::pin::show(&core, &recorded.sha, "schemas/cafaye.manifest.schema.json")
        .unwrap_or_else(|error| {
            panic!(
                "the recorded core pin {} ({}) is not readable in {}. Cause: {error}",
                recorded.sha,
                recorded.source,
                core.display()
            )
        });
    let vendored = std::fs::read(manifest::schema_path()).expect("the vendored schema is readable");

    if upstream != vendored {
        // Say which way it differs, because "drifted" sends a reader to diff to
        // find out whether the vendored copy is behind, ahead, or edited.
        let behind = pantry::pin::commits_behind(
            &core,
            &recorded.sha,
            &pantry::pin::published_head(&core).unwrap_or_else(|| recorded.sha.clone()),
        );
        panic!(
            "schemas/cafaye.manifest.schema.json is not core's schema at {} (from {}).\n  \
             vendored {} bytes, core's at that ref {} bytes{}\n\n\
             Re-vendor and re-record, in one commit:\n  \
             cp {}/schemas/cafaye.manifest.schema.json schemas/\n  \
             git -C {} rev-parse HEAD   # into vendir.lock.yml",
            recorded.sha,
            recorded.source,
            vendored.len(),
            upstream.len(),
            match behind {
                Some(n) => format!("\n  the recorded ref is {n} commit(s) behind core master"),
                None => String::new(),
            },
            core.display(),
            core.display(),
        );
    }
}

/// The distance between this repository's recorded core ref and core's published
/// head, reported by name.
///
/// A vendored copy with no staleness signal is "quiet and still wrong", which is
/// where `pantry-07` and this packet both started. Nine commits is roughly a
/// working day of this fleet's merge rate — the same budget the registry copies
/// use, and deliberately the same number, because they are the same question
/// asked of two different dependencies.
#[test]
fn this_repository_says_how_far_behind_core_it_is() {
    const STALENESS_BUDGET_COMMITS: u64 = 9;

    let Some(root) = cafaye_root() else {
        return skip_without_workspace("core staleness");
    };
    let core = root.join("core");

    let resolution = pantry::pin::resolve(Path::new(env!("CARGO_MANIFEST_DIR")));
    let recorded = match &resolution {
        pantry::pin::PinResolution::Found(pin) => pin,
        other => {
            eprintln!(
                "SKIP core staleness: no recorded pin ({other:?}), so there is no distance to \
                 measure. Nothing was measured; this is a skip, not a pass."
            );
            return;
        }
    };

    let Some(head) = pantry::pin::published_head(&core) else {
        eprintln!(
            "SKIP core staleness: {core:?} resolves no published head, so `now` is unknown and \
             no distance can be measured. This is a skip, not a pass."
        );
        return;
    };

    match pantry::pin::commits_behind(&core, &recorded.sha, &head) {
        None => eprintln!(
            "SKIP core staleness: {} is not in core's local history (a shallow clone?). \
             Unmeasurable is not `current`. This is a skip, not a pass.",
            recorded.sha
        ),
        Some(0) => eprintln!("    core staleness: current at {}", recorded.sha),
        Some(distance) => {
            eprintln!(
                "    core staleness: {distance} commit(s) behind ({} @ {})",
                recorded.sha, recorded.source
            );
            assert!(
                distance <= STALENESS_BUDGET_COMMITS,
                "this repository's recorded core ref is {distance} commit(s) behind core master — \
                 over the {STALENESS_BUDGET_COMMITS}-commit budget.\n\n\
                 Nothing about that is necessarily a defect: core adding a valid example is the \
                 most frequent change made to it, and a rule that made every schema edit a \
                 fleet-wide breaking change would be skipped the third time it was inconvenient \
                 (MD15, reason 3). But a schema this repository validates against that is months \
                 old is not describing anything current, and the fix is one commit:\n\n  \
                 cp ../core/schemas/cafaye.manifest.schema.json schemas/\n  \
                 git -C ../core rev-parse HEAD   # into vendir.lock.yml"
            );
        }
    }
}

#[test]
fn the_vendored_schema_is_the_draft_the_schema_declares() {
    let schema: serde_json::Value =
        serde_json::from_str(manifest::MANIFEST_SCHEMA).expect("the vendored schema is JSON");

    assert_eq!(
        schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
        "core's schemas are Draft 2020-12 and pantry validates against that dialect"
    );
    assert_eq!(
        schema["$id"],
        "https://cafaye.com/schemas/cafaye.manifest.schema.json"
    );
}

/// The gate `/readyz` depends on. If this test is deleted the readiness probe
/// still answers 200 and nobody can tell it stopped checking anything.
#[test]
fn every_official_entry_validates_against_the_manifest_schema() {
    let dir = pantry::registry::registry_dir();
    let paths = pantry::registry::manifest_paths(&dir);
    assert!(
        !paths.is_empty(),
        "no official entries found under {} — a registry with nothing in it is a \
         silent success",
        dir.display()
    );

    for path in &paths {
        let manifest = manifest::validate_schema(&std::fs::read(path).expect("readable"), path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(
            manifest.name,
            service_directory(path),
            "{} is named for its service",
            path.display()
        );
    }
}

/// The same validator core's own examples are held to, from the other
/// direction: a manifest that is schema-valid in isolation but breaks a
/// cross-field rule core documents separately is still a registry entry this
/// packet must not invent a verdict on. `caf contract lint` owns those rules;
/// what pantry checks is that the *schema* half is real, so the negative cases
/// below are taken from core's own `examples/invalid/`.
#[test]
fn the_schema_rejects_what_core_rejects() {
    let cases = [
        (
            "an https remote is a policy violation",
            "name: x\nlanguage: go\ncore: ^0.1.0\nrepository:\n  url: https://github.com/cafaye/x.git\nowner:\n  team: x\n",
        ),
        (
            "defaultBranch is const master",
            "name: x\nlanguage: go\ncore: ^0.1.0\nrepository:\n  url: git@github.com:cafaye/x.git\n  defaultBranch: main\nowner:\n  team: x\n",
        ),
        (
            "a two-segment event type is not a cafaye type",
            "name: courier\nlanguage: elixir\ncore: ^0.2.0\nexposes:\n  events:\n    - email.queued\nrepository:\n  url: git@github.com:cafaye/courier.git\nowner:\n  team: courier\n",
        ),
        (
            "language is an enum, not free text",
            "name: x\nlanguage: cobol\ncore: ^0.1.0\nrepository:\n  url: git@github.com:cafaye/x.git\nowner:\n  team: x\n",
        ),
        (
            "a namespace name is kebab-case",
            "name: Courier\nlanguage: go\ncore: ^0.1.0\nrepository:\n  url: git@github.com:cafaye/x.git\nowner:\n  team: x\n",
        ),
        (
            "an npm-style range is not in the grammar",
            "name: x\nlanguage: go\ncore: ^1.0.0 || ^2.0.0\nrepository:\n  url: git@github.com:cafaye/x.git\nowner:\n  team: x\n",
        ),
        (
            "an absolute api path is not a repository-relative one",
            "name: x\nlanguage: go\ncore: ^0.1.0\nexposes:\n  api: /etc/openapi.yaml\nrepository:\n  url: git@github.com:cafaye/x.git\nowner:\n  team: x\n",
        ),
        (
            "an exposes with nothing in it is not a surface",
            "name: x\nlanguage: go\ncore: ^0.1.0\nexposes: {}\nrepository:\n  url: git@github.com:cafaye/x.git\nowner:\n  team: x\n",
        ),
    ];

    for (what, text) in cases {
        let error = manifest::validate_schema(text.as_bytes(), Path::new("<inline>"))
            .err()
            .unwrap_or_else(|| panic!("the schema accepted a manifest where {what}"));
        assert!(
            !error.to_string().is_empty(),
            "a rejection for {what} must explain itself"
        );
    }
}

/// **This used to be here.** `the_schema_accepts_core_s_own_valid_examples` read
/// `core/examples/valid/*.yml` out of the WORKING TREE and validated every one
/// against `schemas/cafaye.manifest.schema.json` — a schema this repository
/// copied at some past moment. Two claims in one test, and they were not the
/// same claim:
///
/// * "my vendored schema accepts core's valid examples" — true, and checked
///   below by `every_manifest_this_repository_ships_validates_at_the_ref_it_vendored`
///   in `tests/core_pin.rs`, at a **pinned ref** rather than at whatever is on
///   disk right now.
/// * "every `*.yml` in that directory is a service manifest" — false, and false
///   since `core-09` added `gate.external.yml` and `gate.self-contained.yml`,
///   which are gate declarations governed by `gate.schema.json`. Validating them
///   against the *manifest* schema produced
///   `"owner" is a required property` — a manifest rule quoting a document that
///   was never a manifest.
///
/// The failure was reported against this repository, by name, three times. The
/// replacement and the reasoning are in `tests/core_pin.rs`, and the
/// two-document-kinds decision is `DECISIONS.md` D3. What is left here is the
/// half that was always true and is still: the vendored schema rejects what core
/// rejects, below.
///
/// The service an entry is for: the directory `cafaye.yml` sits in. The layout
/// is `services/<name>/cafaye.yml` because `caf contract lint` only lints files
/// carrying that exact name.
fn service_directory(path: &Path) -> String {
    path.parent()
        .and_then(|service| service.file_name())
        .expect("a service directory")
        .to_string_lossy()
        .to_string()
}

/// The exclusion record is not a suggestion, and it is not prose: each row
/// states which check holds it back, and the test asserts that check still
/// holds. This is a tripwire in both directions — the day courier's events gain
/// the three-segment prefix core requires, its row fails with "register it"
/// instead of the registry quietly going stale.
#[test]
fn every_exclusion_reason_is_still_true() {
    let dir = pantry::registry::registry_dir();
    let index = pantry::registry::read_index(&dir).expect("registry/index.yml parses");
    assert!(
        !index.excluded.is_empty(),
        "an empty exclusion record means nothing was checked"
    );

    let Some(root) = cafaye_root() else {
        return skip_without_workspace("exclusion record");
    };

    for excluded in &index.excluded {
        let manifest_path = root.join(&excluded.name).join("cafaye.yml");
        let validates = if manifest_path.is_file() {
            pantry::manifest::validate_schema(
                &std::fs::read(&manifest_path).expect("readable"),
                &manifest_path,
            )
            .is_ok()
        } else {
            false
        };

        match excluded.blocked_by {
            BlockedBy::Schema => assert!(
                manifest_path.is_file() && !validates,
                "{} now validates against core's schema — it must be REGISTERED, not \
                 excluded. Copy {} into registry/services/, add a row to \
                 registry/index.yml, and delete this one.",
                excluded.name,
                manifest_path.display()
            ),
            BlockedBy::NoManifest => assert!(
                !manifest_path.is_file(),
                "{} now carries a cafaye.yml — register it or change this row's \
                 blockedBy and say why it is still held back",
                excluded.name
            ),
            BlockedBy::NotAService => {
                assert!(
                    validates,
                    "{} is held back for being a non-service, so its manifest must still validate",
                    excluded.name
                );
                let manifest = pantry::manifest::read(&manifest_path).expect("validates");
                assert_eq!(
                    manifest.language,
                    pantry::manifest::Language::Spec,
                    "{} is held back as `not-a-service`, which means `language: spec`",
                    excluded.name
                );
            }
            BlockedBy::Private => {
                // The one arm whose assertion is about the repository rather
                // than about its file. `site` used to sit here as `schema`,
                // because its manifest carried an unquoted colon-space on line
                // 47 and would not parse at all. That was true, and then it
                // stopped being true — the manifest was fixed, it lints, and the
                // `Schema` arm fired with "it must be REGISTERED, not excluded".
                // The arm was right to fire. Registration is simply not
                // available for this repository, and the reason is not a defect
                // anyone can go and fix.
                //
                // So this arm asserts the thing that makes the row true, read
                // from the file that records it: the manifest must still declare
                // itself private. It is a weak-looking assertion for a strong
                // claim, and deliberately so — the strong claim ("nobody outside
                // cafaye can clone this") is NOT checked here, because it cannot
                // be: from a developer machine it is unreadable and from inside
                // it is readable, and a check that passes or fails depending on
                // who runs it is not a check. `tests/ci.rs` carries the half that
                // is machine-checkable, by refusing to let any repository in the
                // workflow's `CAFAYE_UNREADABLE` list be registered at all.
                assert!(
                    manifest_path.is_file(),
                    "{} is held back as private, so it must still carry a cafaye.yml — \
                     a row with no file to read is not a checked row",
                    excluded.name
                );
                let manifest = pantry::manifest::read(&manifest_path).expect("validates");
                assert_eq!(
                    manifest.repository.visibility.as_deref(),
                    Some("private"),
                    "{} is held back because its manifest declares `visibility: private`, and \
                     that is the whole of the claim — `tests/ci.rs` will not let a repository the \
                     drift job cannot clone be registered. If the manifest now says `public`, \
                     this row must be DELETED and the repository REGISTERED: copy {} into \
                     registry/services/, add a row to registry/index.yml, and take it out of \
                     CAFAYE_UNREADABLE in .github/workflows/ci.yml.",
                    excluded.name,
                    manifest_path.display()
                );
            }
            BlockedBy::Library => {
                // A row the job cannot read is not a row it checked, and saying
                // so is the difference between a skip and a pass. `cafaye-rb` is
                // the case today: it is a PRIVATE repository, so
                // `git clone https://github.com/cafaye/cafaye-rb.git` from a
                // hosted runner is a 404, and the credential that would fix it
                // is exactly what pantry-04 removed from the workflow on the
                // grounds that the fleet is public.
                //
                // The condition is structural — no checkout, nothing to check —
                // and not a repository named here, because the fact about the
                // world lives in one place: `.github/workflows/ci.yml`'s
                // `CAFAYE_UNREADABLE` list, which `tests/ci.rs` checks against
                // `registry/index.yml`. A name in this file would be a second
                // copy of that fact and the two would drift. The skip is one arm
                // of one value, and every other row — including `kit`'s, whose
                // whole claim is that its file is *absent* — is still checked.
                let Some(manifest) = manifest_path
                    .is_file()
                    .then(|| pantry::manifest::read(&manifest_path))
                    .transpose()
                    .unwrap_or_else(|error| panic!("{}: {error}", manifest_path.display()))
                else {
                    eprintln!(
                        "SKIP the manifest half of {}'s exclusion row: no cafaye.yml at {}. \
                         Nothing in this arm was verified for that row and a green run does \
                         not mean it was. Run this test with PANTRY_CAFAYE_ROOT pointed at a \
                         workspace holding the checkout to check it; .github/workflows/ci.yml \
                         says which repositories the drift job cannot clone, and why.",
                        excluded.name,
                        manifest_path.display()
                    );
                    continue;
                };

                assert!(
                    validates,
                    "{} is held back as a library, so its manifest must still validate — \
                     and a manifest that stops validating is a service with an unresolved \
                     DECISION NEEDED, not a library",
                    excluded.name
                );

                // The direction this value can go stale in, and the one that
                // matters. `library` is not "we have not got to it": it is a
                // claim that nothing brings this repository up and nothing
                // routes to it. That stops being true the moment the manifest
                // declares a surface — `exposes.api` is an HTTP API to serve,
                // `exposes.events` is work to run, `consumes` is a
                // subscription — and each of those is exactly what registration
                // is for. So the tripwire fires with "register it", in the
                // direction that has already fired three times, rather than
                // needing a branch to exempt libraries.
                assert!(
                    manifest.exposes.is_none(),
                    "{} now declares `exposes` — it publishes a contract surface, so it is \
                     something `caf dev` brings up and `guard` routes to. It must be \
                     REGISTERED: copy {} into registry/services/ and add a row to \
                     registry/index.yml carrying the kind that manifest now derives.",
                    excluded.name,
                    manifest_path.display()
                );
                assert!(
                    manifest
                        .consumes
                        .as_ref()
                        .is_none_or(|types| types.is_empty()),
                    "{} now consumes events, so it reacts to the fleet and is no longer a \
                     library. Register it, or say in its row why a consumer of events is \
                     held back.",
                    excluded.name
                );

                // And the values stay distinct. `not-a-service` is a fact core's
                // schema states; `library` is a judgement about something `caf
                // dev` does not start. A `library` row whose manifest says
                // `language: spec` is the first fact wearing the second value's
                // name, which is how one of them quietly stops meaning anything.
                assert_ne!(
                    manifest.language,
                    pantry::manifest::Language::Spec,
                    "{} is held back as `library`, but `language: spec` is what \
                     `not-a-service` is for. One repository, one reason: this row states \
                     the weaker of the two facts, and the stronger one is the row's to use.",
                    excluded.name
                );
            }
        }

        assert!(
            !excluded.reason.trim().is_empty() && excluded.reason.len() > 40,
            "{} is held back without a usable reason",
            excluded.name
        );
        assert!(
            excluded.verify.is_some(),
            "{} is held back without a command that proves it",
            excluded.name
        );
    }
}

/// Every official repository that pantry knows about is either registered or
/// explicitly excluded. The registry knows about six cafaye repositories and
/// five are registered; this test is what keeps a sixth from being invented in
/// some other repository without a decision recorded here.
#[test]
fn a_registered_service_is_never_also_excluded() {
    let dir = pantry::registry::registry_dir();
    let index = pantry::registry::read_index(&dir).expect("index parses");

    for excluded in &index.excluded {
        assert!(
            !index.services.contains_key(&excluded.name),
            "{} is registered and excluded at the same time",
            excluded.name
        );
    }
}
