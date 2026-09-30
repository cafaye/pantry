//! The drift test.
//!
//! Everything else in this suite checks that pantry is internally consistent.
//! This file is the only one that checks pantry is *true*: that each entry in
//! `registry/` still says what the service it describes actually says, and
//! that each fact pantry cannot read off a manifest — `kind`, `basePath` —
//! still agrees with the service's own OpenAPI document.
//!
//! A copy of a manifest with no drift test is a copy that rots, and a registry
//! that rots is worse than no registry: `caf deploy` would resolve the wrong
//! service to the wrong repository and the failure would surface as a deploy
//! that mysteriously cannot find a branch.
//!
//! The real services are sibling checkouts. When they are not there — a
//! single-repository clone, a published crate — every test in this file says so
//! on stderr and returns. A skip is reported, never hidden (kit's AGENTS.md).

use std::path::{Path, PathBuf};

use pantry::manifest::{self, Language};
use pantry::registry::{self, Registry, ServiceKind, registry_dir};

/// The cafaye workspace: the directory holding `core/`, `identity/`, and the
/// rest. `PANTRY_CAFAYE_ROOT` overrides it; otherwise the parent of this
/// repository, which is where `moon/cafaye/<service>` puts them.
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
        // A directory that merely exists is not proof. The workspace has core in
        // it, and that is the one repository every cafaye repo depends on.
        root.join("core/schemas/cafaye.manifest.schema.json")
            .is_file()
    })
}

/// Where a registered service's own files live.
///
/// Every service is a sibling checkout, and pantry is this repository — the one
/// name whose "real service" and registry copy are the same file in the same
/// tree. That case is resolved here rather than special-cased at each use, so
/// all four drift checks agree about it.
fn service_root(root: &Path, name: &str) -> PathBuf {
    if name == "pantry" {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    } else {
        root.join(name)
    }
}

/// Skips loudly, and says exactly what would make it run.
macro_rules! require_workspace {
    ($what:expr) => {
        match cafaye_root() {
            Some(root) => root,
            None => {
                eprintln!(
                    "SKIP {}: no cafaye workspace found next to this repository. Set \
                     PANTRY_CAFAYE_ROOT to the directory holding core/, identity/, \
                     billing/, guard/ and muse/ and re-run. A drift test that cannot \
                     see the real services has proven nothing, so this is a skip and \
                     not a pass.",
                    $what
                );
                return;
            }
        }
    };
}

/// The drift test proper: every field pantry publishes about a service is the
/// field the service itself publishes. Compared value by value, not byte by
/// byte, because a comment-only edit upstream is not drift and failing on it
/// would train people to ignore this test.
#[test]
fn every_registered_entry_matches_the_real_service_on_disk() {
    let root = require_workspace!("registry drift");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    assert!(
        !registry.entries().is_empty(),
        "the registry is empty, so nothing was checked"
    );

    for entry in registry.entries() {
        let name = entry.name();
        let real = service_root(&root, name).join("cafaye.yml");

        assert!(
            real.is_file(),
            "{} is registered but {} does not exist. Either the repository moved \
             or the entry is fiction; both are drift.",
            name,
            real.display()
        );

        let upstream = manifest::read(&real)
            .unwrap_or_else(|error| panic!("{}/cafaye.yml: {error}", root.join(name).display()));

        // identity is the worked example: if someone adds a field to
        // registry/services/identity.cafaye.yml that upstream does not have,
        // this is the line that says so.
        assert_eq!(
            entry.manifest,
            upstream,
            "{} does not match {}",
            name,
            real.display()
        );

        // The identity fields, named out loud, because a struct comparison
        // fails with a diff and a reader should not have to know which field
        // the diff refers to.
        assert_eq!(
            entry.manifest.repository.url, upstream.repository.url,
            "{} repository.url",
            name
        );
        assert_eq!(
            entry.manifest.core, upstream.core,
            "{} core constraint",
            name
        );
        assert_eq!(
            entry.manifest.language, upstream.language,
            "{} language",
            name
        );
    }
}

/// The repository URL has to name the repository that is actually open: the
/// remote's `origin` is the one thing in the workspace that cannot drift from
/// GitHub without a commit.
#[test]
fn every_registered_repository_url_is_the_real_services_remote() {
    let root = require_workspace!("repository remote drift");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");

    for entry in registry.entries() {
        let name = entry.name();
        let repository = service_root(&root, name);

        // A checkout with no git metadata is a tarball, not a worktree. Skip
        // that one entry rather than the whole test, and say so.
        if !repository.join(".git").exists() {
            eprintln!(
                "SKIP remote check for {name}: {} is not a git checkout",
                repository.display()
            );
            continue;
        }

        let remote = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["config", "--get", "remote.origin.url"])
            .output()
            .unwrap_or_else(|error| panic!("git on {}: {error}", repository.display()));

        if !remote.status.success() {
            eprintln!("SKIP remote check for {name}: no remote.origin.url configured");
            continue;
        }

        let actual = String::from_utf8_lossy(&remote.stdout).trim().to_string();
        let declared = entry.manifest.repository.url.trim_end_matches(".git");

        let normalise = |url: &str| url.trim_end_matches(".git").to_string();
        assert_eq!(
            declared,
            normalise(&actual),
            "{} declares remote {declared} but the checkout's origin is {actual}",
            name
        );
    }
}

/// `defaultBranch` is `const "master"` in core's schema, so it cannot drift in
/// the manifest — but it can drift in what pantry *serves*, and it can be
/// absent from a manifest, in which case pantry must not invent it.
#[test]
fn a_missing_default_branch_is_reported_absent_not_invented() {
    let root = require_workspace!("default branch");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");

    for entry in registry.entries() {
        let upstream =
            manifest::read(&service_root(&root, entry.name()).join("cafaye.yml")).expect("parses");
        assert_eq!(
            entry.manifest.repository.default_branch,
            upstream.repository.default_branch,
            "{} must not state a branch the service does not state",
            entry.name()
        );
    }
}

/// `kind` and `basePath` are the two facts a manifest cannot state, so nothing
/// upstream will ever tell pantry they are wrong. These are the checks that
/// stand in for that.
#[test]
fn kind_agrees_with_what_the_manifest_can_prove() {
    require_workspace!("kind consistency");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");

    for entry in registry.entries() {
        let manifest = &entry.manifest;
        let name = entry.name();

        let serves_http = manifest
            .exposes
            .as_ref()
            .and_then(|e| e.api.as_deref())
            .is_some();
        let subscribes = manifest.consumes.as_ref().is_some_and(|c| !c.is_empty());
        let publishes = manifest
            .exposes
            .as_ref()
            .and_then(|e| e.events.as_deref())
            .is_some_and(|e| !e.is_empty());

        match entry.kind {
            ServiceKind::Api => assert!(
                !subscribes,
                "{name} is registered as `api` but consumes events; that is `both`"
            ),
            ServiceKind::Worker => assert!(
                !serves_http,
                "{name} declares exposes.api and cannot be a pure worker"
            ),
            ServiceKind::Both => {
                assert!(
                    serves_http && subscribes,
                    "{name} is registered as `both` but the manifest does not show an \
                     api surface and a subscription"
                )
            }
        }

        if serves_http {
            assert!(
                matches!(entry.kind, ServiceKind::Api | ServiceKind::Both),
                "{name} declares exposes.api, so its kind must be `api` or `both` — the \
                 manifest is not ambiguous here and a curated `worker` would be wrong"
            );
        } else if publishes || subscribes {
            assert_eq!(
                entry.kind,
                ServiceKind::Worker,
                "{name} declares no api surface but does declare event work, so it is a \
                 worker"
            );
        }

        // The one curated fact in the registry, stated here so the next person
        // to touch it knows it is load-bearing: a service whose manifest
        // declares no surface has kind nothing can derive, and pantry records it.
        if !serves_http && !publishes && !subscribes {
            eprintln!(
                "note: {name} declares no contract surface at all, so its kind ({}) is \
                 curated. It stays correct until {name}/cafaye.yml declares `exposes`.",
                entry.kind
            );
        }
    }
}

/// `basePath` is the common path prefix of the service's own OpenAPI document,
/// read from the checkout — not from pantry's memory of it.
#[test]
fn base_path_agrees_with_the_services_own_openapi_document() {
    let root = require_workspace!("base path");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");

    for entry in registry.entries() {
        let name = entry.name();
        let Some(reference) = entry
            .manifest
            .exposes
            .as_ref()
            .and_then(|e| e.api.as_deref())
        else {
            assert_eq!(
                entry.base_path, None,
                "{name} declares no OpenAPI document, so it has no base path — a \
                 non-null value here would be a path nobody published"
            );
            continue;
        };

        let document = service_root(&root, name).join(reference);
        assert!(
            document.is_file(),
            "{name} declares exposes.api: {reference}, which does not exist at {}",
            document.display()
        );

        let actual = manifest::openapi_base_path(&std::fs::read(&document).expect("readable"))
            .unwrap_or_else(|error| panic!("{name}/{}: {error}", document.display()));

        assert_eq!(
            entry.base_path.as_deref(),
            Some(actual.as_str()),
            "{name}/{} serves paths under {actual}, not {:?}",
            document.display(),
            entry.base_path
        );
    }
}

/// A service cannot be registered as a specification. core's schema defines
/// `language: spec` as "for specification-only repositories", and core itself
/// is the example: it is the substrate, not a service anything routes to.
#[test]
fn no_specification_repository_is_registered_as_a_service() {
    let registry = Registry::load(&registry_dir()).expect("the official registry loads");

    for entry in registry.entries() {
        assert_ne!(
            entry.manifest.language,
            Language::Spec,
            "{} is a specification repository and must stay in the exclusion record",
            entry.name()
        );
    }
}

/// The registry must know about every cafaye repository in the workspace, and
/// every repository it does not register must be excluded with a reason. This
/// is the test that stops the registry from quietly describing only the
/// services somebody remembered.
#[test]
fn every_service_repository_in_the_workspace_is_registered_or_excluded() {
    let root = require_workspace!("registry coverage");

    let dir = registry_dir();
    let index = registry::read_index(&dir).expect("registry/index.yml parses");
    let registry = Registry::load(&dir).expect("the official registry loads");

    // A cafaye repository is a directory holding a cafaye manifest or that
    // core's own docs name as one. The list is explicit because guessing
    // "which of these 30 directories are services" is exactly the kind of
    // inference that produces a registry nobody can audit.
    let known = [
        "identity", "billing", "courier", "darkroom", "guard", "muse", "parlor", "core", "kit",
        "caf", "pantry",
    ];

    for name in known {
        if !root.join(name).exists() {
            continue;
        }
        let registered = registry.get(name).is_some();
        let excluded = index.excluded.iter().any(|e| e.name == name);

        assert!(
            registered || excluded,
            "{name} exists in the workspace but is neither registered in \
             registry/services/ nor recorded in registry/index.yml's exclusion list. \
             Add it to one or the other — a registry that silently omits a service \
             is a registry that cannot be audited."
        );
    }
}
