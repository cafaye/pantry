//! The `kind` rules, as a check rather than as documentation.
//!
//! `Registry::load` calls `check_kind` on every entry, so the rules in the
//! [`pantry::registry::ServiceKind`] table are enforced at startup and not only
//! written down in a comment. Everything else in this suite loads the official
//! registry; this file builds throwaway registries on disk so the *refusals*
//! can be observed, which is the half that matters: a vocabulary that can be
//! extended is also a vocabulary that can be extended wrongly, and the arm a
//! new value lands in is exactly where the holes appear.
//!
//! The fixture is a real registry directory rather than a mocked load because
//! the rule being tested is a rule about *files agreeing with each other*: a
//! `kind` row, a manifest, and a `basePath`. A test that builds only half of
//! that is testing a different thing.
//!
//! No cafaye workspace is needed here, so these run in the `build` job too —
//! which matters, because they are the checks that keep the `build` job honest
//! about the vocabulary it serves.

use std::path::PathBuf;

use pantry::registry::{Registry, RegistryError};

/// A manifest that satisfies core's schema and serves HTTP. The shape of core's
/// own `examples/valid/go-api.cafaye.yml`.
const SERVES: &str = r#"name: thing
language: go
core: ^0.1.0

exposes:
  api: openapi/v1.yaml

repository:
  url: git@github.com:cafaye/thing.git
  defaultBranch: master
  visibility: public

owner:
  team: thing
"#;

/// caf's shape: no `exposes`, no `consumes`, and it says so in its own words —
/// a binary publishes no contract surface. This is the manifest a `kind` cannot
/// be derived from, which is the whole reason `cli` is a curated value.
const NO_SURFACE: &str = r#"name: thing
description: A binary, not a service.
language: go
core: ^0.2.0

consumes: []
dependencies: []

repository:
  url: git@github.com:cafaye/thing.git
  defaultBranch: master
  visibility: public

owner:
  team: thing
"#;

/// The same, plus one event it subscribes to: an api surface would still be the
/// derivation, so this is `both`, never `worker` and never `cli`.
const EVENT_WORK: &str = r#"name: thing
language: go
core: ^0.1.0

consumes:
  - identity.user.created

dependencies: []

repository:
  url: git@github.com:cafaye/thing.git
  defaultBranch: master
  visibility: public

owner:
  team: thing
"#;

/// Writes a one-entry registry and loads it, which is the only way to reach
/// `check_kind` — it is private, deliberately, because a caller has no business
/// checking one entry and ignoring the others.
fn load_one(
    what: &str,
    manifest: &str,
    kind: &str,
    base_path: Option<&str>,
) -> Result<Registry, RegistryError> {
    let dir = temp_registry(what);
    std::fs::write(dir.join("services/thing/cafaye.yml"), manifest).expect("write the manifest");

    let base_path = match base_path {
        Some(path) => format!("basePath: {path}\n"),
        None => String::new(),
    };
    let index =
        format!("schemaVersion: 1\n\nservices:\n  thing:\n    kind: {kind}\n    {base_path}");
    std::fs::write(dir.join("index.yml"), index).expect("write the index");

    Registry::load(&dir)
}

fn temp_registry(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pantry-kind-{}-{what}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("services").join("thing")).expect("temp dir");
    dir
}

/// The message a refusal has to carry. AGENTS.md: "Errors are for the person
/// reading them: name the file, the field, the value and what was allowed."
fn refusal(error: RegistryError, expected: &str) -> String {
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "the refusal names the rule it broke (`{expected}`), so a reader does not have to \
         guess which of them applied: {message}"
    );
    assert!(
        message.contains("thing"),
        "the refusal names the service it is about: {message}"
    );
    message
}

// ------------------------------------------------------------ what is admitted

/// `cli` is a value the vocabulary admits, and a binary's manifest is the shape
/// it exists for: no `exposes`, no `consumes`, a real `language`.
///
/// This is the load that failed before `cli` existed — `registry/index.yml` said
/// `kind: cli` and serde refused the row with "unknown variant", so the registry
/// could not record what `caf` is and had to record `api` for a binary
/// instead.
#[test]
fn a_binary_with_no_surface_is_a_cli() {
    let registry = load_one("cli-loads", NO_SURFACE, "cli", None)
        .unwrap_or_else(|error| panic!("a binary with no surface is a cli: {error}"));

    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry.get("thing").expect("the entry").kind.to_string(),
        "cli"
    );
}

/// And `guard` — a service that serves HTTP but has not written its OpenAPI
/// document — is still `api` on the same manifest shape. **Neither value can be
/// refused and neither can be derived**, because a manifest that declares
/// nothing is the same bytes whether the repository is a gateway waiting for a
/// document or a binary that will never have one. That is why both are curated
/// and why `check_kind` admits exactly this pair and nothing else: the load
/// cannot tell them apart, so a rule that pretended to would be guessing.
#[test]
fn a_service_with_no_document_is_still_an_api() {
    let registry = load_one("api-loads", NO_SURFACE, "api", None)
        .unwrap_or_else(|error| panic!("a curated api is admitted: {error}"));

    assert_eq!(
        registry.get("thing").expect("the entry").kind.to_string(),
        "api"
    );
}

// ------------------------------------------------------------- what is refused

/// The refusal that matters most, because `cli` is a new arm and a new arm is
/// where a check gets its holes: **`cli` is refused the moment the manifest
/// declares a surface.** A binary that starts publishing an API document is not
/// a binary any more, and a registry that still called it one would route a
/// client to a command that has no HTTP surface — the exact failure `cli` was
/// added to end, reached from the other direction.
#[test]
fn a_cli_is_refused_for_a_manifest_that_declares_a_surface() {
    let error = load_one("cli-with-surface", SERVES, "cli", Some("/v1"))
        .expect_err("a service that declares exposes.api is not a binary");

    refusal(error, "declares exposes.api");
}

/// A `cli` publishes no OpenAPI document, so a `basePath` on its row is a path
/// nobody serves. `basePath` is derived from a document for every other entry,
/// so a non-null one here would be a prefix invented for a command.
#[test]
fn a_cli_may_not_name_a_base_path() {
    let error = load_one("cli-with-base-path", NO_SURFACE, "cli", Some("/v1"))
        .expect_err("a binary serves no path, so there is no prefix to name");

    refusal(error, "basePath");
}

/// The curated set is **closed**: a manifest with no contract surface is an
/// `api` or a `cli` and nothing else.
///
/// `both` is the case worth watching. It was admitted by `check_kind` — the
/// curated arm refused `worker` and said so, and said nothing about `both` —
/// and was caught only by a test in another file. A check that relies on a
/// sibling test to reject a value is one edit away from accepting it, so the
/// refusal belongs here, in the arm that can see the whole surface fact.
#[test]
fn a_manifest_with_no_surface_is_neither_a_worker_nor_a_both() {
    for kind in ["worker", "both"] {
        let error = load_one(&format!("surface-less-{kind}"), NO_SURFACE, kind, None)
            .expect_err("a manifest with no contract surface is neither");

        let message = refusal(error, "declares no contract surface");
        assert!(
            message.contains("cli") && message.contains("api"),
            "the refusal names what WAS allowed, not only what was not: {message}"
        );
        assert!(
            message.contains("curated"),
            "and it says this row is a curation rather than a derivation, which is the one \
             thing a reader of a wrong kind needs to know: {message}"
        );
    }
}

/// The pre-existing derivations do not move for a new value. A manifest that
/// declares a surface and consumes nothing is an `api` whatever the vocabulary
/// grows to contain, one that does event work without a surface is a `worker`,
/// and neither may be `cli`.
#[test]
fn a_manifest_with_a_surface_still_refuses_every_kind_but_api_and_both() {
    let error = load_one("surface-not-worker", SERVES, "worker", Some("/v1"))
        .expect_err("a service that serves HTTP cannot be a pure worker");
    refusal(error, "declares exposes.api");

    // No api surface but a real subscription: the derivation says `worker`, and
    // the message names it. This case is also why `cli` cannot be admitted on
    // "declares no `exposes`" alone — a subscriber declares no `exposes`
    // either, and it is emphatically not a binary.
    let error = load_one("event-work-not-cli", EVENT_WORK, "cli", None)
        .expect_err("a subscriber is not a binary");
    let message = refusal(error, "does declare event work");
    assert!(
        message.contains("worker"),
        "and it says which value the manifest derives: {message}"
    );
}

/// The row and the manifest are read from disk together, so a `kind` this build
/// does not know is refused by the load rather than served as a string. A
/// vocabulary that grew in `registry/index.yml` but not in `src` would
/// otherwise be a registry that answers `/v1/services` with a value no client
/// can parse.
#[test]
fn a_kind_outside_the_vocabulary_is_refused_by_the_load() {
    let error = load_one("unknown-kind", SERVES, "database", Some("/v1"))
        .expect_err("database is not a kind");

    assert!(
        error.to_string().contains("database"),
        "the refusal names the value it could not read: {error}"
    );
}
