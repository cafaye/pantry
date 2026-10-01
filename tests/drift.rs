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

use std::ffi::OsStr;
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

/// **Moved to `tests/recorded_copy.rs`.** Both copy-comparison tests that used
/// to live here compared `registry/services/<name>/cafaye.yml` against
/// `<service>/cafaye.yml` **in the working tree**, and both are now there —
/// reading the service's `cafaye.yml` at the `recordedAt` commit this registry
/// records instead.
///
/// The reason is MD15 applied one repository over. A working-tree comparison
/// asserts "this copy is what the service says *right now*", which is a claim
/// about somebody else's repository: `identity-09` and `muse-06` landed, the
/// copies were not refreshed, and the gate said **pantry is broken**. It was
/// not. Pantry's copy of muse's manifest was publishing `required: false` for a
/// dependency muse had made required, and a reader had no way to tell that apart
/// from a defect in pantry's own code.
///
/// At a recorded ref the same comparison is "this copy is what the service said
/// at ref R" — true or false for a reason inside this repository. How far behind
/// R is becomes a *report* with a name and a fix
/// (`the_registry_says_how_far_behind_each_copy_is_and_names_the_fix`), not a
/// failure here. See `tests/recorded_copy.rs` and `DECISIONS.md` D4.
///
/// Kept as a tombstone rather than deleted silently, because a reader who greps
/// for `every_registered_entry_is_a_verbatim_copy` should find out where it went
/// and why rather than find nothing.
///
/// What stays in this file is the drift that is genuinely *about this
/// repository*: that every curated `kind` and `basePath` is what the service's
/// own manifest and OpenAPI document imply. Those have no recorded ref — a
/// `basePath` is derived from a document the service publishes today — and they
/// are checked below against the workspace, loudly, because that is the shape of
/// the claim they make.
#[allow(dead_code)]
fn moved_to_recorded_copy_rs() {}

/// A remote is one repository written several ways, and which way it is
/// written is not a fact about the repository.
///
/// `git@github.com:cafaye/guard.git`, `ssh://git@github.com/cafaye/guard.git`
/// and `https://github.com/cafaye/guard` are three transports for the same
/// `cafaye/guard`, and which one a checkout has is decided by whoever cloned
/// it: a developer with an SSH remote (PLAN.md §1 — the only form allowed for
/// anything cafaye owns) and this repository's own CI, which clones
/// anonymously over HTTPS because that is the only credential-free way to reach
/// a public repository from a hosted runner.
///
/// Comparing the two strings instead of the repository they name is how
/// `every_registered_repository_url_is_the_real_services_remote` below came to
/// be unrunnable in the one environment that matters most: with the drift job
/// enabled and a real workspace of anonymous HTTPS clones, it failed on
/// `billing` for a checkout whose origin was correct in every respect a
/// registry can observe. The registry was right and the check was measuring the
/// clone mechanism.
#[test]
fn a_git_remote_and_an_https_url_can_name_the_same_repository() {
    for (url, expected) in [
        ("git@github.com:cafaye/guard.git", "cafaye/guard"),
        ("git@github.com:cafaye/guard", "cafaye/guard"),
        ("ssh://git@github.com/cafaye/guard.git", "cafaye/guard"),
        ("https://github.com/cafaye/guard.git", "cafaye/guard"),
        ("https://github.com/cafaye/guard", "cafaye/guard"),
        ("https://github.com/cafaye/guard/", "cafaye/guard"),
        // A credential in the URL is a credential, not part of the name.
        (
            "https://x-access-token:ghs_abc123@github.com/cafaye/guard.git",
            "cafaye/guard",
        ),
    ] {
        assert_eq!(
            repository_identity(url).as_deref(),
            Some(expected),
            "{url} names {expected}"
        );
    }
}

/// The other half, and the one that keeps the comparison above honest: a
/// repository is `owner/name` on one host. Anything that is not that has no
/// identity to compare, and a check that quietly treated every unparseable
/// origin as a match would make the whole remote test decorative.
#[test]
fn only_a_github_remote_has_a_repository_identity() {
    for url in [
        "../guard",                               // a path, not a remote
        "/Users/kaka/Code/any/moon/cafaye/guard", // an absolute local path
        "https://gitlab.com/cafaye/guard.git",    // a different host
        "git@gitlab.com:cafaye/guard.git",
        "https://github.com/cafaye", // no repository name
        "https://github.com/",
        "https://github.com/cafaye/guard/tree/main", // a path, not a remote
        "banana",
    ] {
        assert_eq!(
            repository_identity(url),
            None,
            "{url} is not a GitHub remote, so it has no repository identity to compare"
        );
    }
}

/// The `owner/name` a GitHub remote names, or `None` if it is not one.
///
/// Deliberately not a URL parser: the three spellings a cafaye remote takes are
/// enumerated in the tests above and a fourth is a fact to add here rather than
/// a case to discover in a panic message. `None` means "this is not a GitHub
/// remote", which is never a match — a checkout with a local-path origin is a
/// finding to report, not an origin to wave through.
fn repository_identity(url: &str) -> Option<String> {
    let url = url.trim();

    let (host, path) = match url.split_once("://") {
        // A real URL: scheme, authority, path. The userinfo is a credential
        // and not part of the repository's name, and actions/checkout may put
        // one there.
        Some((scheme, rest)) => {
            if !matches!(scheme, "https" | "http" | "ssh" | "git") {
                return None;
            }
            let (authority, path) = rest.split_once('/')?;
            (authority.rsplit('@').next()?, path)
        }
        // The scp form — `git@github.com:owner/repo` — which git accepts and
        // which is not a URL at all: no scheme, and a colon where a URL has a
        // slash. Nothing else may take this path, so `user` is checked rather
        // than assumed.
        None => {
            let (user_at_host, path) = url.split_once(':')?;
            let (user, host) = user_at_host.split_once('@')?;
            if user != "git" {
                return None;
            }
            (host, path)
        }
    };

    if host != "github.com" {
        return None;
    }

    // `https://github.com/cafaye/guard/` is a remote git accepts, so a
    // trailing slash is not a different repository.
    let path = path.trim_end_matches('/');

    let mut segments = path.split('/');
    let owner = segments.next()?;
    let repository = segments.next()?.trim_end_matches(".git");

    // Deeper than `owner/name` is a link to something inside a repository, not
    // a remote for one.
    if segments.next().is_some() || owner.is_empty() || repository.is_empty() {
        return None;
    }

    Some(format!("{owner}/{repository}"))
}

/// The repository URL has to name the repository that is actually open: the
/// remote's `origin` is the one thing in the workspace that cannot drift from
/// GitHub without a commit.
///
/// The comparison is by repository, not by string, because the transport is not
/// a fact about the repository — see
/// `a_git_remote_and_an_https_url_can_name_the_same_repository` above, which
/// exists because this test used to compare the two forms and failed in CI for
/// a correct checkout.
///
/// The other half of the policy is not here and is not lost: that a *declared*
/// `repository.url` must be SSH is core's schema, whose pattern accepts only
/// the two SSH spellings and says "HTTPS remotes for cafaye/anywaye repos are
/// a policy violation (PLAN.md §1)". `tests/schema.rs` validates every entry
/// against that vendored copy, and `the_vendored_schema_is_byte_identical_to_\
/// cores` anchors the copy to core. So the rule is enforced where it is written
/// down, and this test enforces the part only the filesystem can answer.
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
        let declared = entry.manifest.repository.url.as_str();

        let declared_repository = repository_identity(declared).unwrap_or_else(|| {
            panic!(
                "{name} declares remote {declared}, which is not a GitHub remote naming an \
                 owner and a repository. core's schema accepts only the two SSH spellings, so \
                 either this entry has not been validated or the schema's pattern is wrong."
            )
        });

        let actual_repository = repository_identity(&actual).unwrap_or_else(|| {
            panic!(
                "{name} declares remote {declared} but {} has origin {actual}, which names no \
                 GitHub repository. A checkout of a cafaye service is cloned from its GitHub \
                 remote; a local path here means the workspace is not what it appears to be.",
                repository.display()
            )
        });

        assert_eq!(
            declared_repository, actual_repository,
            "{} declares remote {declared} ({declared_repository}) but the checkout's origin is \
             {actual} ({actual_repository}). Same host, different repository: that is drift.",
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
            // `cli` says the repository is a binary, so a manifest that declares
            // any surface at all contradicts it — and the value that fits then
            // is not a judgement, because the manifest has become decisive. caf
            // is the entry today and it is a curated value, so this is the check
            // on the one curated value that is not a temporary gap.
            ServiceKind::Cli => assert!(
                !serves_http && !publishes && !subscribes,
                "{name} is registered as `cli`, which is a binary: a manifest that declares an \
                 api surface, publishes events or consumes them makes it a service, and the \
                 row must then say `api`, `both` or `worker`"
            ),
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

        // The curated facts in the registry, stated here so the next person to
        // touch one knows it is load-bearing: a service whose manifest declares
        // no surface has a kind nothing can derive — `api` for guard, which is
        // waiting for its document, and `cli` for caf, which will never have
        // one — and pantry records the distinction rather than deriving a wrong
        // one. Only the first of the two stops being correct when the manifest
        // declares `exposes`.
        if !serves_http && !publishes && !subscribes {
            eprintln!(
                "note: {name} declares no contract surface at all, so its kind ({}) is \
                 curated, not derived. `api` stops being right when {name}/cafaye.yml \
                 declares `exposes`; `cli` stops being right at the same moment, and \
                 neither value can be derived before it.",
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
    //
    // `docs` and `cafaye-rb` are here because both are curated now: neither is
    // registered and both are excluded with `blockedBy: library`, and a name
    // that appears in the exclusion record without appearing here would be
    // checked by `every_exclusion_reason_is_still_true` and by nothing else.
    // `cafaye-ts` is here for the opposite reason: it IS registered, as a `cli`,
    // and it is the entry most likely to be argued with rather than forgotten.
    let known = [
        "identity",
        "billing",
        "courier",
        "darkroom",
        "guard",
        "muse",
        "parlor",
        "core",
        "kit",
        "caf",
        "pantry",
        "docs",
        "cafaye-rb",
        "cafaye-ts",
        // A directory rather than a repository, which is why the workspace walk
        // below needed widening: this is the one name in the list that carries no
        // cafaye.yml and is not a checkout, and it is curated anyway because the
        // registry's opinion about a planned repository is worth more than a
        // silent directory. See DECISIONS.md D2.
        "cafaye-py",
    ];

    // The list above is hand-maintained, and a hand-maintained list has a
    // failure mode the rest of this file is built to avoid: a repository that
    // lands in the workspace and is never added to it is invisible to every
    // test here. That is not hypothetical — see
    // `no_workspace_repository_is_missing_from_the_curation_lists` below, which
    // found two of them, and which is the reason this list now says thirteen
    // names rather than the eleven it said when this test was written.
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

/// A `cli` is curated, so no manifest can hold the registry to it, and a `cli`
/// that has quietly written an OpenAPI document is the state where that matters:
/// the document is the manifest's own evidence arriving after the fact. A service
/// that writes its document and has not yet declared `exposes` is a real
/// half-finished packet, and for a `cli` it is the one state that means the
/// curated row is wrong — because the file a `cli` must not have is precisely the
/// file whose existence would make `api` a derivation instead of a judgement.
///
/// cafaye-ts is why this check exists, and it is the awkward case: it vendors
/// **six** OpenAPI documents into `specs/`. They are inputs, and its own manifest
/// says so at length. So the rule is not "a `cli` has no file whose name contains
/// `openapi`" — it is where core's conventions put a document a repository
/// PUBLISHES. Every service in the fleet publishes at `openapi/v1.yaml`, or at
/// `openapi.yaml` in the repository root, and a repository that publishes a
/// document publishes it there. A `cli` with one of those has a surface.
#[test]
fn a_registered_cli_publishes_no_openapi_document_of_its_own() {
    let root = require_workspace!("cli surface");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    let clis: Vec<&str> = registry
        .entries()
        .iter()
        .filter(|entry| entry.kind == ServiceKind::Cli)
        .map(|entry| entry.name())
        .collect();

    assert!(
        !clis.is_empty(),
        "no `cli` is registered, so nothing was checked. The curated value exists and \
         the fleet has members of it, and this is the only check that would notice one \
         of them turning into a service"
    );

    // Every finding in one failure, not the first one: a repository that has
    // written a document usually has one, and a reader sent back twice for two
    // lines in the same tree learns less from the second run.
    let published: Vec<String> = clis
        .iter()
        .flat_map(|name| {
            published_documents(&service_root(&root, name))
                .into_iter()
                .map(move |document| format!("  {name} — {}\n", document.display()))
        })
        .collect();

    assert!(
        published.is_empty(),
        "{} registered `cli`{} publishing an OpenAPI document:\n\n{}\n\
         A `cli` is a thing installed and run — brought up by nobody, routed to by \
         nobody — and a document in one of those two positions is a contract surface. \
         The moment it exists the row's kind is no longer a curation: it is `api`, and \
         nothing in the manifest will say so for you.\n\n\
         Either the document is an INPUT rather than a surface, in which case it does \
         not belong where core's conventions put published ones — cafaye-ts keeps its \
         six vendored documents in specs/ and records their provenance in \
         specs/index.json — or the repository is a service, in which case declare \
         `exposes.api` in its cafaye.yml and change the row to `api`. `Registry::load` \
         then derives the value and refuses the curated one.",
        published.len(),
        if published.len() == 1 { " is" } else { "s are" },
        published.join("\n"),
    );
}

/// Every OpenAPI document a checkout **publishes**, which is to say every one in
/// the position core's conventions give a published document: a directory named
/// `openapi`, or a file called `openapi.{yaml,yml,json}`.
///
/// The narrowness is the whole test. cafaye-ts carries six documents under
/// `specs/` and they are correctly not matched, because a vendored copy of
/// somebody else's specification is an input and the provenance record for an
/// input is not a surface. `node_modules`, `target` and `.git` are skipped for
/// the ordinary reason that another project's files are not this repository's.
fn published_documents(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();

            if matches!(name.as_str(), ".git" | "node_modules" | "target") {
                continue;
            }

            // A file we cannot stat is a file we cannot classify, and a check
            // that guesses is a check that has stopped checking.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                if name == "openapi" {
                    found.push(entry.path());
                }
                stack.push(entry.path());
            } else if matches!(
                name.as_str(),
                "openapi.yaml" | "openapi.yml" | "openapi.json"
            ) {
                found.push(entry.path());
            }
        }
    }

    found.sort();
    found
}

// ---------------------------------------------------------------------------
// THE WORKSPACE WALK, and the two questions it is asked
// ---------------------------------------------------------------------------

/// One directory in the workspace root, as far as curation is concerned.
struct WorkspaceDirectory {
    name: String,
    path: PathBuf,
    /// A `cafaye.yml` at its root. This is the shape of a cafaye repository,
    /// and it is a *condition* rather than a definition — a repository that
    /// lost its manifest is a repository no longer — which is why
    /// `every_directory_in_the_workspace_is_a_repository_the_registry_curates`
    /// asks about every directory and this file's other walk does not.
    carries_manifest: bool,
}

/// Which of the two questions about a directory the walk is being asked.
///
/// They differ in what they accept as a repository and nowhere else, which is
/// why they share one walk and one rule below rather than two copies of each.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Curation {
    /// `no_workspace_repository_is_missing_from_the_curation_lists`: only a
    /// directory carrying a `cafaye.yml` counts.
    ManifestCarrying,
    /// `every_directory_in_the_workspace_is_a_repository_the_registry_curates`:
    /// every directory counts, because a repository that lost its manifest is
    /// invisible to the other question.
    Every,
}

/// Every directory in the workspace root, sorted by name.
///
/// Sorted because `read_dir` order is a filesystem detail and a failure
/// message that reshuffles itself between two runs of the same tree reads as
/// two findings.
fn workspace_directories(root: &Path) -> Vec<WorkspaceDirectory> {
    let mut directories = Vec::new();

    let Ok(entries) = std::fs::read_dir(root) else {
        return directories;
    };

    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let path = entry.path();

        directories.push(WorkspaceDirectory {
            carries_manifest: path.join("cafaye.yml").is_file(),
            name,
            path,
        });
    }

    directories.sort_by(|left, right| left.name.cmp(&right.name));
    directories
}

/// Whether `registry/index.yml` has an opinion about `name` — registered, or
/// excluded with a `blockedBy` and a reason.
fn curated(name: &str, registry: &Registry, index: &registry::RegistryIndex) -> bool {
    registry.get(name).is_some() || index.excluded.iter().any(|excluded| excluded.name == name)
}

/// The name every cafaye working copy carries: `moon/cafaye/wt-<service>-<packet>`.
///
/// It used to be `<service>-worker-<packet>`, and the walk's rule was
/// `name.contains("-worker-")` for four packets after the convention moved.
/// That is the whole defect in one line: **a stale rule about worktrees is not
/// a stale test, it is an ungateable fleet.** The moment a worker started, two
/// tests went red and stayed red until it stopped, and the gate is worth
/// exactly nothing in the window where there is something to decide.
///
/// So the rule is three clauses and the name is only the first of them, and the
/// other two are there because a name is a claim anybody can make.
const WORKTREE_PREFIX: &str = "wt-";

/// The repository `directory` is a linked git worktree of — or `None`, which
/// means it is not one, whatever it is called.
///
/// Three clauses, and each is load-bearing:
///
/// 1. **`wt-`** — the convention above. It is what makes tomorrow's worktrees
///    skip without this file carrying a list of today's names, and a list of
///    today's names is a hardcoded test that is green until tomorrow.
/// 2. **A `.git` FILE naming `<repository>/.git/worktrees/<id>`** — the proof,
///    and git wrote it. A linked worktree is the one thing git gives a `.git`
///    *file* to, and the administrative directory it points at is always under
///    `worktrees/`, so a plain directory called `wt-whatever` has no such file,
///    a genuine new repository called `wt-whatever` has a `.git` *directory*,
///    and a submodule's `.git` file points at `.git/modules/<name>` instead.
///    All three are caught here, and the fixture holds all three.
/// 3. **The repository is one this registry curates** — in `curation_covers`,
///    because it is a claim about the registry rather than about git.
///
/// **The third clause is what makes the first two safe.** A worktree is skipped
/// because it is a *second working copy of a directory the registry already
/// describes* — its `cafaye.yml` is that repository's manifest at that branch,
/// already checked through the repository's own checkout, and counting it again
/// would report every service once per worker in flight. Without clause 3 the
/// rule would be a name in a trusted list, and a new service repository's first
/// worktree would be exactly how a repository nobody registered slips past the
/// walk. With it, a worktree of an uncurated repository is a repository, and is
/// reported like one.
///
/// **Clause 2 does not require the administrative directory to exist**, and that
/// is deliberate rather than an oversight: `git worktree prune` deletes that
/// directory and leaves the working copy on disk, which is the shape of a stale
/// worktree — the gigabytes-nothing-uses case this exclusion exists to keep
/// from blocking a gate. Failing on it would mean the cleanup the exclusion
/// enables is the thing that makes the gate unrunnable again.
fn worktree_repository(directory: &Path) -> Option<PathBuf> {
    if !directory
        .file_name()?
        .to_str()?
        .starts_with(WORKTREE_PREFIX)
    {
        return None;
    }

    let pointer = directory.join(".git");
    if !pointer.is_file() {
        return None;
    }

    let administrative = std::fs::read_to_string(&pointer).ok()?;
    let administrative = PathBuf::from(administrative.trim().strip_prefix("gitdir:")?.trim());

    // git writes an absolute path here, and a relative one is resolved against
    // the working copy — which is how a submodule's pointer reads. Resolving
    // both the same way means the submodule is rejected by the shape test
    // below rather than by a special case, and the special case is what would
    // have been the exemption.
    let administrative = if administrative.is_absolute() {
        administrative
    } else {
        directory.join(administrative)
    };

    // `<repository>/.git/worktrees/<id>`, so the repository is three levels up
    // and each level is checked rather than assumed.
    let worktrees = administrative.parent()?;
    if worktrees.file_name() != Some(OsStr::new("worktrees")) {
        return None;
    }
    let git = worktrees.parent()?;
    if git.file_name() != Some(OsStr::new(".git")) || !git.is_dir() {
        return None;
    }

    git.parent().map(Path::to_path_buf)
}

/// Whether the workspace directory `repository` is one the registry curates.
///
/// A worktree of it may be skipped, so this is the clause that decides whether
/// the exclusion is justified. It asks the registry — registered or excluded —
/// rather than a list in this file, which is the difference between a rule and
/// a hardcoded answer.
fn curation_covers(
    root: &Path,
    repository: &Path,
    registry: &Registry,
    index: &registry::RegistryIndex,
) -> bool {
    // A repository outside this workspace is not curated by this registry's
    // workspace, whatever it is called: a worktree of somebody's laptop clone
    // in the fleet directory is a directory nobody registered, and is reported.
    let Some(parent) = repository.parent() else {
        return false;
    };
    if !same_directory(parent, root) {
        return false;
    }

    let Some(name) = repository.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    curated(name, registry, index)
}

/// Two paths are the same directory even when one of them reached it through a
/// symlink.
///
/// `git worktree add` writes the resolved path into a worktree's `.git` file,
/// while `cafaye_root()` reports whatever `PANTRY_CAFAYE_ROOT` or
/// `CARGO_MANIFEST_DIR` spelled — so the two names for one workspace differ
/// whenever either is a symlink, and a worktree the walk cannot recognise is a
/// red on the whole fleet. This is the difference between the rule working and
/// the rule depending on how someone typed the path. Canonicalisation failing
/// is not a reason to say no: `canonicalize` is only used to compare, and a path
/// that cannot be resolved is compared as written.
fn same_directory(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolve(left) == resolve(right)
}

/// Whether `directory` is a worktree the walk may skip, and why.
///
/// The whole exclusion, in one function, with the three clauses above as its
/// parts. `Ok(())` is the skip; `Err(reason)` is why not, and the reason is
/// kept as text because the only consumer of it is a human reading a failure.
///
/// > DECISION NEEDED (pantry): D30b — **should the gate REPORT a worktree, and
/// > how much?** The exclusion above answers "is this directory a repository the
/// > registry is missing", and a worktree is not one. It says nothing about a
/// > worktree left behind by a worker that died, which is the thing that eats
/// > gigabytes and the reason anyone cleans them up. Reporting is not failing,
/// > so nothing here is blocked on it, and the shape of the question is narrow:
/// >   * *the branch* is a fact about the working copy itself
/// >     (`git -C <worktree> rev-parse --abbrev-ref HEAD` reads a local file) and
/// >     is safe to state;
/// >   * *whether the branch has been merged* is NOT. It is a claim about a
/// >     mutable ref in a **sibling clone**, which is exactly what rule 3 of this
/// >     repository's AGENTS.md forbids asserting — D26 is that defect already
/// >     open elsewhere in this file's neighbourhood — so a version of this that
/// >     FAILS on merge state would break the rule it lives under, and a version
/// >     that reports it must label it "as of this clone".
/// >   * the one fact that is unambiguous and is not a merge question at all:
/// >     a `wt-` directory whose administrative directory `git worktree prune`
/// >     has already removed. Git does not know it any more; the bytes are
/// >     still there. That is stale by definition.
/// > Recommended: a report, never a failure, and the branch first — a worker
/// > worktree's branch is the handle the manager needs to land or discard it.
/// > Cost of flipping: one function and one `eprintln!` in this file, no
/// > assertion, nothing in `src/`, and no change to any HTTP contract. What must
/// > NOT be built is a staleness *threshold* — "a worktree older than N days is a
/// > failure" is a clock this repository would then depend on, and a red that
/// > fires on a developer's schedule is a red that gets disabled.
/// > Not built in this packet: the manager asked for the narrow fix and for a
/// > view on this, and a test that shells out to git in a sibling clone to answer
/// > a merge question is a different packet's decision, not this one's.
fn worktree_verdict(
    root: &Path,
    directory: &WorkspaceDirectory,
    registry: &Registry,
    index: &registry::RegistryIndex,
) -> Result<(), String> {
    let Some(repository) = worktree_repository(&directory.path) else {
        return Err(format!(
            "{} is not a linked git worktree",
            directory.path.display()
        ));
    };

    if curation_covers(root, &repository, registry, index) {
        Ok(())
    } else {
        Err(format!(
            "{} is a linked worktree of {}, which registry/index.yml curates in \
             neither direction",
            directory.path.display(),
            repository.display()
        ))
    }
}

/// Whether `directory` is something the registry has to have an opinion about.
fn needs_curation(
    root: &Path,
    directory: &WorkspaceDirectory,
    registry: &Registry,
    index: &registry::RegistryIndex,
    curation: Curation,
) -> bool {
    // A hidden directory is configuration *for* repositories (`.git`,
    // `.github`), not one of them. Only the `Every` walk sees it, because only
    // that walk looks at directories rather than at manifests.
    if curation == Curation::Every && directory.name.starts_with('.') {
        return false;
    }
    if curation == Curation::ManifestCarrying && !directory.carries_manifest {
        return false;
    }
    if curated(&directory.name, registry, index) {
        return false;
    }
    worktree_verdict(root, directory, registry, index).is_err()
}

/// Every directory in the workspace that `registry/index.yml` curates in
/// neither direction, in one stable list.
///
/// The two tests below differ in their `Curation` and in nothing else, so
/// there is one answer to this question and both tests read it here. That is
/// the point of the function existing: a rule with two copies is a rule one
/// copy of which is already stale, which is how
/// `name.contains("-worker-")` outlived the convention it named.
fn uncurated(
    root: &Path,
    registry: &Registry,
    index: &registry::RegistryIndex,
    curation: Curation,
) -> Vec<WorkspaceDirectory> {
    workspace_directories(root)
        .into_iter()
        .filter(|directory| needs_curation(root, directory, registry, index, curation))
        .collect()
}

/// Every directory in the workspace that carries a `cafaye.yml` must appear in
/// one of the two curation lists — registered, or excluded with a reason.
///
/// The test above checks the lists are complete *among the names they name*. It
/// cannot check that they name every repository, because the list of names is
/// itself hand-maintained. So this test walks the workspace instead and asks the
/// question from the other direction, which is the only direction that catches a
/// repository nobody remembered.
///
/// **It found two.** `docs` and `cafaye-rb` both carry a valid `cafaye.yml` on
/// master, neither was registered, neither was excluded, and neither appeared in
/// the list above — so until this test existed the registry claimed to describe
/// the fleet while omitting two members of it silently.
///
/// Both were parked in an `UNDECIDED` constant here first, with the reason they
/// were undecided, and **that constant is now gone.** Both have a row in
/// `registry/index.yml` under `blockedBy: library`, which is the fourth value
/// for exactly their case: a valid manifest, and a repository nothing brings up
/// and nothing routes to. The constant went with the decision because it was an
/// exemption — a third list that made a repository's absence from the registry
/// acceptable without a reason — and this file's other tests have caught three
/// real problems precisely because they have no such list. A test whose failure
/// mode is "add it here and the red goes away" is a test that can be made green
/// by not checking.
///
/// So a fourth unlisted repository now fails this test, which is the part that
/// was never a decision: the list has to be complete, whether the answer for any
/// one member is `api`, `cli` or a row in the exclusion record.
#[test]
fn no_workspace_repository_is_missing_from_the_curation_lists() {
    let root = require_workspace!("curation coverage");

    let dir = registry_dir();
    let index = registry::read_index(&dir).expect("registry/index.yml parses");
    let registry = Registry::load(&dir).expect("the official registry loads");

    // Worktrees are not repositories: `moon/cafaye` holds one per in-flight
    // packet. A manifest inside one is its repository's manifest, already
    // checked through that repository's own checkout, so counting it again would
    // report every service once per worker in flight. What counts as a worktree
    // is `worktree_verdict` above — three clauses, and the narrowness of the
    // third is why this list is not simply every `wt-` directory.
    let missing: Vec<String> = uncurated(&root, &registry, &index, Curation::ManifestCarrying)
        .into_iter()
        .map(|directory| {
            format!(
                "  {} — carries a valid cafaye.yml at {} and appears in no curation list",
                directory.name,
                directory.path.join("cafaye.yml").display()
            )
        })
        .collect();

    assert!(
        missing.is_empty(),
        "{} workspace repositor{} a cafaye.yml and appear in NEITHER registry/services/ NOR \
         the exclusion list.\n\
         \n\
         {}\n\
         \n\
         This is the failure mode a hand-maintained list has: `every_service_repository_in_the_\
         workspace_is_registered_or_excluded` checks the names it knows, and a repository \
         nobody added to that list is invisible to it. Add the name to `known` in that test \
         and give it a row — registered, or excluded with a `blockedBy` and a reason. There is \
         no third list to park it in: `docs` and `cafaye-rb` had one until they were given \
         `blockedBy: library`, and a list where an undecided repository needs no reason is a \
         check that can be made green by not checking. Do NOT delete this test, and do NOT \
         weaken it to make the suite green: a registry that silently omits a repository cannot \
         be audited.",
        missing.len(),
        if missing.len() == 1 {
            "y carries"
        } else {
            "ies carry"
        },
        missing.join("\n"),
    );
}

/// Every directory in the workspace is curated: registered, or excluded with a
/// reason. **The test above asks the same question of the directories that carry
/// a `cafaye.yml`, and this one exists because that is a condition, not a
/// definition.**
///
/// A cafaye repository that lost its manifest — a merge that dropped it, a
/// half-finished `git mv` — becomes invisible to the walk above, silently, and
/// the registry keeps describing a fleet that has one fewer member. So does a
/// directory that has been created for a repository nobody has written yet. Both
/// happened: `cafaye-py/` sat in the workspace, empty and unregistered, through
/// four packets, and the tripwire could not see it because the shape it looked
/// for was a file.
///
/// Three exclusions, and each is a fact about what a *repository* is rather than
/// an exemption from the check:
///
/// * a hidden directory is not a repository — `.git` and the workspace's own
///   `.github` are configuration *for* repositories;
/// * a worktree is not a repository — `moon/cafaye` holds one per in-flight
///   packet, and a manifest inside one is its repository's manifest, already
///   checked through that repository's own checkout. Which directories are
///   worktrees is `worktree_verdict` above, and the third of its three clauses
///   is the one that keeps this exclusion honest: only a worktree of a
///   repository **this registry curates** is skipped, so a new service's first
///   worktree cannot become the way it goes unregistered;
/// * anything else in that directory is a cafaye repository the registry has an
///   opinion about, or it is a stray, and a stray is worth finding.
#[test]
fn every_directory_in_the_workspace_is_a_repository_the_registry_curates() {
    let root = require_workspace!("workspace coverage");

    let dir = registry_dir();
    let index = registry::read_index(&dir).expect("registry/index.yml parses");
    let registry = Registry::load(&dir).expect("the official registry loads");

    let uncurated: Vec<String> = uncurated(&root, &registry, &index, Curation::Every)
        .into_iter()
        .map(|directory| {
            let shape = if directory.carries_manifest {
                "carries a cafaye.yml"
            } else {
                "carries NO cafaye.yml — so it is a directory, not yet a repository"
            };
            format!(
                "  {} — {} {shape}",
                directory.name,
                directory.path.display()
            )
        })
        .collect();

    assert!(
        uncurated.is_empty(),
        "{} director{} in the workspace that registry/index.yml curates in neither \
         direction:\n\n{}\n\n\
         Every one of them is either a cafaye repository the registry has no opinion \
         about, or something in the workspace that should not be there. A directory \
         nobody registered is a member of the fleet this file does not describe, and \
         `no_workspace_repository_is_missing_from_the_curation_lists` above cannot see \
         it: that one looks for a cafaye.yml, so an empty directory — or a repository \
         whose manifest was lost — passes it. A `wt-` directory that is NOT a linked \
         worktree of a curated repository is in this list on purpose: see \
         `every_worktree_in_this_workspace_is_a_working_copy_of_a_curated_repository`, \
         which says which of the two things it is.\n\n\
         Add each name to `known` in that test AND give it a row: registered, or \
         excluded with a `blockedBy` and a reason. If it should not be in the \
         workspace at all, delete it — but read the row's reason first, because \
         `blockedBy: no-manifest` on a directory that is not yet a repository is a \
         judgement and not a fact, and `DECISIONS.md` D2 is the open question about \
         it. Do NOT add a list of tolerated directory names here: a check that can be \
         made green by not checking is a check that has stopped checking.",
        uncurated.len(),
        if uncurated.len() == 1 {
            "y is"
        } else {
            "ies are"
        },
        uncurated.join("\n"),
    );
}

// ---------------------------------------------------------------------------
// THE FIXTURES, and the three tests that pin the rule against them
// ---------------------------------------------------------------------------

/// A workspace root built to order, for the tests that pin the walk.
///
/// The names are this registry's own, because the thing under test is the
/// CURATION decision and a name nothing curates cannot produce one: `identity`
/// is registered, `kit` and `parlor` are in the exclusion record, and
/// `not-a-service` is in neither. So a fixture directory can be checked against
/// both answers without a stub registry, and the fixture is the same
/// distinction the real workspace makes.
struct WorkspaceFixture {
    root: PathBuf,
}

impl WorkspaceFixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("pantry-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the fixture root is creatable");
        WorkspaceFixture { root }
    }

    /// A checked-out repository: a `.git` **directory** and, usually, a
    /// manifest. This is what `git clone` leaves behind and what a new cafaye
    /// service repository looks like the moment somebody writes it.
    fn service(&self, name: &str, manifest: bool) -> &Self {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.join(".git")).expect("mkdir");
        if manifest {
            write(&path.join("cafaye.yml"), b"name: fixture\n");
        }
        self
    }

    /// A linked working copy: a `.git` **file** naming an administrative
    /// directory under the service's own `.git`, which is what
    /// `git worktree add` writes and the only shape that says "worktree".
    ///
    /// `admin` is written too, because a real worktree has one — and its
    /// ABSENCE is the case worth having, since `git worktree prune` removes the
    /// administrative directory and leaves the working copy on disk eating
    /// disk. The gate must not go red on the thing the cleanup exists for, so
    /// `stale` is a first-class argument rather than something the rule has to
    /// be lucky about.
    fn worktree(&self, name: &str, of: &str, stale: bool) -> &Self {
        let path = self.root.join(name);
        std::fs::create_dir_all(&path).expect("mkdir");
        let admin = self.root.join(of).join(".git").join("worktrees").join(name);
        if !stale {
            std::fs::create_dir_all(&admin).expect("mkdir");
        }
        write(
            &path.join(".git"),
            format!("gitdir: {}\n", admin.display()).as_bytes(),
        );
        write(&path.join("cafaye.yml"), b"name: fixture\n");
        self
    }

    /// A directory that is not a repository at all: no git metadata, a manifest
    /// or not. `cafaye-py` sat in the real workspace in this shape for four
    /// packets and no test could see it.
    fn directory(&self, name: &str, manifest: bool) -> &Self {
        let path = self.root.join(name);
        std::fs::create_dir_all(&path).expect("mkdir");
        if manifest {
            write(&path.join("cafaye.yml"), b"name: fixture\n");
        }
        self
    }

    /// The uncurated names the walk reports, for one of the two questions.
    fn uncurated(&self, curation: Curation) -> Vec<String> {
        let dir = registry_dir();
        let index = registry::read_index(&dir).expect("registry/index.yml parses");
        let registry = Registry::load(&dir).expect("the official registry loads");

        uncurated(&self.root, &registry, &index, curation)
            .into_iter()
            .map(|directory| directory.name)
            .collect()
    }
}

impl Drop for WorkspaceFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a parent directory");
    }
    std::fs::write(path, bytes).expect("the fixture is writable");
}

/// The rule, stated over a workspace built to contain both sides of it.
///
/// This is the test that says what a worktree is, and — the half that matters
/// more — what it is **not**. The exclusion above is the only exemption either
/// workspace walk has, so the test that pins it has to hold both directions at
/// once: a real worktree of a curated repository is skipped, and five things
/// wearing a worktree's name are not.
#[test]
fn a_worktree_is_a_working_copy_of_something_the_registry_already_curates() {
    let fixture = WorkspaceFixture::new("worktree");

    // Curated, in both directions: `identity` is registered, `kit` and `parlor`
    // are in the exclusion record with a `blockedBy` and a reason.
    fixture
        .service("identity", true)
        .service("kit", false)
        .service("parlor", true)
        .worktree("wt-identity-21", "identity", false)
        // A worktree of a repository with no manifest is the awkward case, and
        // it is the one that was in the real workspace twice: `wt-kit-cluster`
        // and `wt-sell-backup` are both working copies of `kit`.
        .worktree("wt-kit-cluster", "kit", false)
        .worktree("wt-parlor-01", "parlor", false)
        // Pruned by `git worktree prune`: the administrative directory is gone
        // and the directory is still there. Stale is not the same as
        // uncurated, and the gate must not confuse them.
        .worktree("wt-stale-identity-21", "identity", true);

    // Not a worktree, in five ways. Each of these is a directory the registry
    // has no opinion about, and the rule has to say so.
    fixture
        // A plain directory wearing the prefix. No `.git` at all.
        .directory("wt-whatever", true)
        // A GENUINE new service repository called `wt-whatever`, which is the
        // case a name-based exclusion would swallow: `git clone` leaves a
        // `.git` DIRECTORY, which is the whole difference.
        .service("wt-new-service", true)
        // A worktree of a repository in this workspace that nothing curates.
        // Its owner is in the fixture rather than conjured, because you cannot
        // have a worktree of a repository without the repository — which is why
        // `newthing` is in the `Every` list below, and why the exclusion being
        // narrow about *which* repository does not change whether the finding
        // is reported: the owner is reported instead.
        .service("newthing", false)
        .worktree("wt-uncurated-owner", "newthing", false);
    // …and the two that are pointers somewhere this rule does not follow.
    let outside = std::env::temp_dir().join(format!("pantry-outside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(outside.join(".git")).expect("mkdir");
    write(
        &fixture.root.join("wt-foreign").join(".git"),
        format!(
            "gitdir: {}\n",
            outside.join(".git/worktrees/wt-foreign").display()
        )
        .as_bytes(),
    );
    write(
        &fixture.root.join("wt-foreign/cafaye.yml"),
        b"name: fixture\n",
    );
    // A submodule's `.git` file has the same shape as a worktree's and points
    // somewhere else entirely — `<super>/.git/modules/<name>`, not
    // `<super>/.git/worktrees/<id>`.
    write(
        &fixture.root.join("wt-submodule").join(".git"),
        format!(
            "gitdir: {}\n",
            fixture.root.join("kit/.git/modules/wt-submodule").display()
        )
        .as_bytes(),
    );
    write(
        &fixture.root.join("wt-submodule/cafaye.yml"),
        b"name: fixture\n",
    );

    assert_eq!(
        fixture.uncurated(Curation::ManifestCarrying),
        [
            // Every one of these carries a cafaye.yml, so both walks see them.
            "wt-foreign",
            "wt-new-service",
            "wt-submodule",
            "wt-uncurated-owner",
            "wt-whatever",
        ],
        "a `wt-` directory is skipped only when it is a linked worktree of a \
         repository this registry curates. Everything here is named as if it \
         were one and is not: wt-whatever is a plain directory, wt-new-service is \
         a genuine repository with a .git DIRECTORY, wt-foreign is a worktree of \
         a repository outside this workspace, wt-uncurated-owner is a worktree of \
         `newthing`, which is in neither curation list, and wt-submodule's \
         pointer is the shape a submodule has."
    );

    // A plain directory with no `.git` and no manifest. The `Every` walk's
    // whole reason for existing, in fixture form: `cafaye-py` was invisible to
    // the other walk for four packets.
    fixture.directory("wt-empty", false);

    assert_eq!(
        fixture.uncurated(Curation::Every),
        [
            // The same five, plus two with no manifest: a repository whose
            // manifest has not landed, and a directory that was never a
            // repository. `newthing` is the first and `wt-empty` the second,
            // which is the whole difference between these two walks.
            "newthing",
            "wt-empty",
            "wt-foreign",
            "wt-new-service",
            "wt-submodule",
            "wt-uncurated-owner",
            "wt-whatever",
        ],
        "the same five, plus the two the manifest walk cannot see. This is the \
         difference between the two walks: `newthing` is a repository whose \
         cafaye.yml has not landed and `wt-empty` is a directory that was never \
         one, and both would be invisible to a walk that looked for a manifest. \
         If the two lists were equal, one of the two walks would not be doing \
         its job."
    );

    let _ = std::fs::remove_dir_all(&outside);
}

/// The two walks' original reason for existing, held down after the exclusion.
///
/// The worktree rule removes an exemption from these tests, and an exemption is
/// the only way a walk stops catching things. So this asserts the thing they
/// were written to catch, in a workspace that has a worktree in it: a new
/// service repository nobody registered, in both the shapes the two walks each
/// exist for — one carrying a manifest, one that has lost it.
#[test]
fn an_unregistered_repository_is_still_reported_by_both_workspace_walks() {
    let fixture = WorkspaceFixture::new("uncurated");

    fixture
        // Curated, in both directions. `docs` is in the exclusion record and
        // carries a manifest, so a row in `excluded` is curation and not a
        // second-class registration.
        .service("identity", true)
        .service("docs", true)
        // A worktree, which is the exemption under test: this one carries a
        // manifest, so without the rule the manifest walk reports it and the
        // fleet cannot be gated while a worker runs.
        .worktree("wt-identity-21", "identity", false)
        // The finding. A new service repository, cloned and written, that
        // nobody added to `registry/index.yml`.
        .service("newthing", true)
        // The same repository one merge too early: present, a repository, and
        // no `cafaye.yml` because the manifest has not landed. Only the
        // `Every` walk can see this one.
        .service("newthing-two", false);

    assert_eq!(
        fixture.uncurated(Curation::ManifestCarrying),
        ["newthing"],
        "a repository carrying a cafaye.yml that registry/index.yml registers \
         in neither direction. This is the failure `docs` and `cafaye-rb` were \
         found by, and it is the whole purpose of \
         `no_workspace_repository_is_missing_from_the_curation_lists`."
    );

    assert_eq!(
        fixture.uncurated(Curation::Every),
        ["newthing", "newthing-two"],
        "the same one, plus a repository that has lost its manifest — which the \
         walk above cannot see by construction, and which is why the second test \
         exists at all. Neither `newthing` nor `newthing-two` is a worktree, and \
         the worktree exclusion must not be the reason this list is short."
    );
}

/// The real workspace, checked against the rule rather than against a fixture.
///
/// Every other test here that involves a worktree builds one, and that is
/// deliberate: a fixture is the same on CI, in a clone and on this machine. It
/// is also not evidence that the rule recognises the worktrees that are
/// *actually* on this disk, and the packet this rule came from was found by
/// exactly that gap — the rule was not wrong about a shape, it was wrong about
/// every shape present. So this one walks the real thing and says what it saw.
///
/// The claim it can make is narrow on purpose, and is the only claim the
/// exclusion is allowed to rest on: **every worktree-shaped directory in the
/// workspace is a working copy of a repository this registry curates.** A
/// `wt-` directory that is not is not a worktree, whatever it is called, and
/// the two tests above will report it — this one exists so the finding says
/// *why* it happened.
#[test]
fn every_worktree_in_this_workspace_is_a_working_copy_of_a_curated_repository() {
    let root = require_workspace!("worktree provenance");

    let dir = registry_dir();
    let index = registry::read_index(&dir).expect("registry/index.yml parses");
    let registry = Registry::load(&dir).expect("the official registry loads");

    let mut explained: Vec<String> = Vec::new();
    let mut unexplained: Vec<String> = Vec::new();

    for directory in workspace_directories(&root) {
        if !directory.name.starts_with(WORKTREE_PREFIX) {
            continue;
        }

        match worktree_verdict(&root, &directory, &registry, &index) {
            Ok(()) => {
                let repository = worktree_repository(&directory.path)
                    .expect("a directory the rule accepted is a worktree");
                explained.push(format!(
                    "  {} — a working copy of {}",
                    directory.name,
                    repository.display()
                ));
            }
            Err(reason) => unexplained.push(format!("  {} — {reason}", directory.name)),
        }
    }

    // A `wt-` directory the rule cannot explain is a finding, not a shrug. It
    // is either a repository wearing a worktree's name — which the exclusion
    // must never swallow — or the convention moved and this rule went stale
    // with it, which is how the fleet became ungate-able once already.
    assert!(
        unexplained.is_empty(),
        "{} worktree-shaped director{} in this workspace that {} not a working copy \
         of a repository this registry curates:\n\n{}\n\n\
         Each of them is one of exactly two things, and the difference is the whole \
         reason the exclusion is narrower than its name.\n\n  \
         * A cafaye repository nobody registered, named `wt-something`. The walk \
         above reports it with a message about registering it; this one exists to \
         say which of the two it is.\n  \
         * A real worktree whose shape the rule no longer matches — a different \
         name, a different git layout, or a repository this registry does not \
         curate. Then the rule in this file is stale and pantry's gate cannot be \
         run at all, which is worse than any single red it prevents.\n\n\
         Do NOT answer this by widening the name test: `a_worktree_is_a_working_\
         copy_of_something_the_registry_already_curates` is what holds the \
         exclusion to three clauses, and it will fail if they are widened.",
        unexplained.len(),
        if unexplained.len() == 1 {
            "y is"
        } else {
            "ies are"
        },
        if unexplained.len() == 1 { "is" } else { "are" },
        unexplained.join("\n"),
    );

    // Reported, not asserted: what the walk skipped and why. The count is here
    // because "this workspace has no worktrees" and "the rule matched nothing"
    // print the same line of output otherwise, and only one of them means the
    // exclusion is doing its job.
    eprintln!(
        "note: {} worktree(s) in this workspace, each a working copy of a directory the \
         registry curates, and each skipped by the walk:\n{}",
        explained.len(),
        if explained.is_empty() {
            "  (none — this workspace holds no wt-* directory, so the exclusion fired \
             nowhere here and the rule is exercised by the fixtures in this file)"
                .to_string()
        } else {
            explained.join("\n")
        }
    );
}

/// The walker's narrowness, pinned by a fixture rather than by cafaye-ts.
///
/// `a_registered_cli_publishes_no_openapi_document_of_its_own` above rests on one
/// distinction: a document in the position core's conventions give a **published**
/// one is a surface, and a document anywhere else is an input. cafaye-ts is the
/// case that makes the distinction real — six vendored documents under `specs/`
/// plus a config file whose name contains the word — and if the walker were
/// widened to match either, that entry would start failing and the tempting fix
/// would be to widen the exemption rather than narrow the rule.
///
/// So the rule is stated here, over a directory built to contain both sides of
/// it. `node_modules`, `target` and `.git` are in there too, because a
/// dependency's own files are not this repository's and a walker that cannot say
/// so is a walker that will be widened.
#[test]
fn the_cli_document_walker_looks_only_where_core_puts_a_published_document() {
    let root = std::env::temp_dir().join(format!("pantry-cli-docs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    // The inputs: a vendored copy of somebody else's specification, and a config
    // file whose name contains the word. Neither is a surface.
    for path in [
        "specs/identity.yaml",
        "openapi-ts.config.ts",
        "README.md",
        "node_modules/hey-api/openapi.yaml",
        ".git/openapi.yml",
        "target/debug/openapi.json",
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, b"paths: {}\n").expect("write");
    }

    // The surfaces: the two positions core's conventions use, and the two
    // spellings of the file at a repository root.
    for path in [
        "openapi/v1.yaml",
        "services/thing/openapi.yaml",
        "openapi.json",
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, b"paths: {}\n").expect("write");
    }

    let found: Vec<String> = published_documents(&root)
        .iter()
        .map(|path| {
            path.strip_prefix(&root)
                .expect("under the root")
                .display()
                .to_string()
        })
        .collect();

    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        found,
        [
            // A directory is reported as itself rather than by the document inside
            // it: "this repository has an openapi/ directory" is the finding, and
            // the file in it is the first thing the reader will go and look at.
            "openapi",
            "openapi.json",
            "services/thing/openapi.yaml",
        ],
        "the two positions core's conventions use, in a stable order. A vendored \
         document under specs/ and a config file called openapi-ts.config.ts are \
         INPUTS and must never appear here: if this list grows, the rule has been \
         widened and a `cli` with vendored documents would start failing."
    );
}
