//! Reading a `cafaye.yml`.
//!
//! The manifest is core's contract, not pantry's, so these tests are mostly
//! about refusing to be flexible: an unknown key, a wrong type, or YAML that
//! does not parse must fail with a message that names the file and the problem.
//! A registry that quietly drops a field it does not understand is a registry
//! that answers questions nobody asked it.

use std::path::Path;

use pantry::manifest::{self, Language};

fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pantry-manifest-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cafaye.yml");
    std::fs::write(&path, contents).expect("write");
    path
}

/// A manifest that satisfies core's schema, used as the base every negative
/// case mutates one line at a time. It is the shape of core's own
/// `examples/valid/go-api.cafaye.yml`.
const VALID: &str = r#"
name: identity
description: A service.
language: go
core: ^0.1.0

exposes:
  api: openapi/v1.yaml
  events:
    - identity.user.created

consumes:
  - billing.customer.created

dependencies:
  - name: billing
    version: ^0.2.0
    required: false

repository:
  url: git@github.com:cafaye/identity.git
  defaultBranch: master
  visibility: public

owner:
  team: identity
  contact: identity@cafaye.com
"#;

#[test]
fn a_valid_manifest_parses_into_its_own_fields() {
    let manifest = manifest::parse(VALID).expect("a schema-valid manifest parses");

    assert_eq!(manifest.name, "identity");
    assert_eq!(manifest.description.as_deref(), Some("A service."));
    assert_eq!(manifest.language, Language::Go);
    assert_eq!(manifest.core, "^0.1.0");
    assert_eq!(
        manifest.repository.url,
        "git@github.com:cafaye/identity.git"
    );
    assert_eq!(
        manifest.repository.default_branch.as_deref(),
        Some("master")
    );
    assert_eq!(manifest.repository.visibility.as_deref(), Some("public"));
    assert_eq!(manifest.owner.team, "identity");
    assert_eq!(
        manifest.owner.contact.as_deref(),
        Some("identity@cafaye.com")
    );

    let exposes = manifest.exposes.as_ref().expect("exposes is declared");
    assert_eq!(exposes.api.as_deref(), Some("openapi/v1.yaml"));
    assert_eq!(
        exposes.events.as_deref(),
        Some(&["identity.user.created".to_string()][..])
    );

    assert_eq!(
        manifest.consumes.as_deref(),
        Some(&["billing.customer.created".to_string()][..])
    );

    // `required: false` is a soft dependency and survives the round trip as
    // written: the field is optional in the schema, and pantry reports what the
    // manifest says rather than what core's default would have said.
    let dependency = &manifest.dependencies.as_ref().expect("declared")[0];
    assert_eq!(dependency.name, "billing");
    assert_eq!(dependency.version.as_deref(), Some("^0.2.0"));
    assert_eq!(dependency.required, Some(false));
}

#[test]
fn an_absent_surface_and_an_empty_one_are_different_facts() {
    // caf's contract package makes this exact point in a comment: absent and
    // empty are different facts, and a repository with no `exposes` is a
    // library while `exposes: {events: []}` is a bug. Collapsing the two here
    // would collapse it for every consumer of this registry.
    let manifest = manifest::parse(
        r#"
name: core
language: spec
core: ^0.2.0
repository:
  url: git@github.com:cafaye/core.git
owner:
  team: core
"#,
    )
    .expect("parses");

    assert!(manifest.exposes.is_none(), "exposes is absent, not empty");
    assert!(manifest.consumes.is_none());
    assert!(manifest.dependencies.is_none());
    assert!(manifest.description.is_none());
}

#[test]
fn a_manifest_without_an_exposes_surface_still_parses() {
    // guard is exactly this shape today: no `exposes`, no `events`. Rejecting
    // it would delete the gateway from the registry.
    let manifest = manifest::parse(
        r#"
name: guard
language: typescript
core: ^0.1.0
consumes: []
dependencies:
  - name: identity
    version: ^0.1.0
    required: true
repository:
  url: git@github.com:cafaye/guard.git
owner:
  team: guard
"#,
    )
    .expect("parses");

    assert!(manifest.exposes.is_none());
    assert_eq!(manifest.consumes.as_deref(), Some(&[][..]));
    assert_eq!(manifest.dependencies.as_ref().map(Vec::len), Some(1));
}

/// The malformed-manifest requirement: rejected, and the message has to be
/// worth reading. An error that says "invalid yaml" sends a person back to a
/// diff; one that says which file, which key, and what was allowed sends them
/// to the fix.
#[test]
fn malformed_yaml_names_the_file_and_the_position() {
    let broken = "name: identity\nlanguage: go\n  core: ^0.1.0\n";
    let path = write_temp("syntax", broken);

    let error = manifest::read(&path).expect_err("bad indentation is not a manifest");

    let message = error.to_string();
    assert!(
        message.contains("cafaye.yml"),
        "message names the file: {message}"
    );
    assert!(
        message.contains("line"),
        "message carries a position, not just a rejection: {message}"
    );
}

#[test]
fn an_unknown_key_is_rejected_and_named() {
    // `version:` is exactly the field identity's manifest dropped when core
    // froze the schema. A parser that ignored it would serve a registry whose
    // version nobody controls.
    let text = VALID.replace("name: identity\n", "name: identity\nversion: 1\n");
    let path = write_temp("unknown-key", &text);

    let error = manifest::read(&path).expect_err("additionalProperties is false");

    let message = error.to_string();
    assert!(message.contains("unknown field"), "{message}");
    assert!(
        message.contains("version"),
        "names the offending key: {message}"
    );
    for allowed in ["name", "language", "core", "exposes", "repository", "owner"] {
        assert!(
            message.contains(allowed),
            "message lists what a manifest may declare ({allowed}): {message}"
        );
    }
}

#[test]
fn a_wrong_type_is_rejected_and_named() {
    let text = VALID.replace("core: ^0.1.0", "core:\n  minimum: 0.1.0");
    let path = write_temp("wrong-type", &text);

    let error = manifest::read(&path).expect_err("core is a string");

    let message = error.to_string();
    assert!(message.contains("core"), "names the field: {message}");
    assert!(
        message.contains("expected") || message.contains("string"),
        "says what was expected: {message}"
    );
}

#[test]
fn a_missing_required_field_is_rejected_and_named() {
    let text = VALID.replace("language: go\n", "");
    let path = write_temp("missing", &text);

    let error = manifest::read(&path).expect_err("language is required");

    let message = error.to_string();
    assert!(
        message.contains("language"),
        "names the missing field: {message}"
    );
}

#[test]
fn a_missing_file_is_a_read_error_not_a_parse_error() {
    let missing = Path::new("/nonexistent/pantry/cafaye.yml");

    let error = manifest::read(missing).expect_err("a missing file is an error");

    assert!(
        matches!(error, manifest::ManifestError::Read { .. }),
        "expected a Read error, got {error:?}"
    );
}

/// An empty file is a real trap: YAML reads it as null, and a parser that
/// treats "no document" as "no fields" reports four missing-field errors
/// instead of saying the file is empty.
#[test]
fn an_empty_manifest_is_rejected_as_empty() {
    let path = write_temp("empty", "");

    let error = manifest::read(&path).expect_err("an empty file is not a manifest");

    assert!(
        error.to_string().contains("empty"),
        "the message should say the file is empty: {error}"
    );
}

/// `basePath` is core's `/vN` rule, applied. The muse case is the reason this is
/// not "the longest common path prefix": muse publishes one path, `/v1/route`,
/// and the longest common prefix of one path is the whole path.
#[test]
fn base_path_is_the_api_version_prefix_a_service_serves_under() {
    let document = |paths: &str| {
        format!("openapi: 3.1.0\npaths:\n{paths}")
            .as_bytes()
            .to_vec()
    };

    let prefix =
        |paths: &str| manifest::openapi_base_path(&document(paths)).expect("a single /vN prefix");

    assert_eq!(
        prefix("  /v1/users:\n    get: {}\n  /v1/me:\n    get: {}\n"),
        "/v1"
    );
    assert_eq!(
        prefix("  /v1/route:\n    get: {}\n"),
        "/v1",
        "one resource under /v1 is still /v1, and its path is not the base"
    );
    assert_eq!(
        prefix("  /v2/plans:\n    get: {}\n  /healthz:\n    get: {}\n  /readyz:\n    get: {}\n"),
        "/v2",
        "the probes are infrastructure and never take part in the derivation"
    );

    for broken in [
        // Two prefixes at once: a transition core permits, a basePath cannot
        // express.
        "  /v1/users:\n    get: {}\n  /v2/users:\n    get: {}\n",
        // No version prefix at all.
        "  /users:\n    get: {}\n",
        // An empty surface is not a prefix.
        "",
    ] {
        let error = manifest::openapi_base_path(&document(broken))
            .expect_err("a document with no single /vN prefix has no base path");
        assert!(
            !error.to_string().is_empty(),
            "the rejection has to say which rule was broken"
        );
    }
}

#[test]
fn a_document_that_is_not_openapi_is_rejected_rather_than_guessed_at() {
    let error = manifest::openapi_base_path(b"openapi: 3.1.0\ninfo:\n  title: x\n")
        .expect_err("no paths, no base path");

    assert!(error.to_string().contains("paths"), "{error}");
}

#[test]
fn every_official_registry_manifest_parses() {
    let dir = pantry::registry::registry_dir();
    let paths = pantry::registry::manifest_paths(&dir);
    assert!(!paths.is_empty(), "the registry has no entries to parse");

    for path in paths {
        let parsed =
            manifest::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let stem = path
            .file_stem()
            .expect("a file name")
            .to_string_lossy()
            .trim_end_matches(".cafaye")
            .to_string();
        assert_eq!(
            parsed.name,
            stem,
            "{} declares name {:?}, which does not match its file name",
            path.display(),
            parsed.name
        );
    }
}
