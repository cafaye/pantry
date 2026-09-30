//! Every official entry validates against core's manifest schema.
//!
//! Two files are involved and they must not drift: `schemas/` holds a copy of
//! core's schema so the binary can validate at startup without a sibling
//! checkout, and core holds the original. The first test asserts the copy is
//! the original; the rest assert the copy actually rejects what core rejects,
//! because a validator that accepts everything would make the readiness probe
//! a decoration.

use std::path::{Path, PathBuf};

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

#[test]
fn the_vendored_schema_is_byte_identical_to_cores() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("vendored schema drift");
    };

    let upstream = std::fs::read(root.join("core/schemas/cafaye.manifest.schema.json"))
        .expect("core's schema is readable");
    let vendored = std::fs::read(manifest::schema_path()).expect("the vendored schema is readable");

    assert_eq!(
        String::from_utf8_lossy(&upstream),
        String::from_utf8_lossy(&vendored),
        "schemas/cafaye.manifest.schema.json has drifted from core's copy. Copy it \
         again: cp ../core/schemas/cafaye.manifest.schema.json schemas/"
    );
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

/// The same, from the accepting side. A validator that rejects everything is a
/// validator that makes `/readyz` red and teaches everyone to ignore it.
#[test]
fn the_schema_accepts_core_s_own_valid_examples() {
    let Some(root) = cafaye_root() else {
        return skip_without_workspace("core examples");
    };

    let examples: Vec<PathBuf> = std::fs::read_dir(root.join("core/examples/valid"))
        .expect("core's examples directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yml"))
        .collect();
    assert!(
        !examples.is_empty(),
        "core has no valid examples to check against"
    );

    for path in examples {
        manifest::validate_schema(&std::fs::read(&path).expect("readable"), &path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

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
