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
/// field the service itself publishes.
///
/// Compared as parsed values, because a struct comparison names the field that
/// moved and a reader should not have to work out which one from a diff. That
/// is the whole of what this test claims — and it used to claim it was the
/// whole of what the drift test claimed, which was not true:
/// `registry/services/guard/cafaye.yml` was missing guard's entire `guard-04`
/// `DECISION NEEDED (REDIS_URL)` block, and billing's copy was stale the same
/// way, and this test passed on both. Byte-equality is
/// `every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes` in this
/// same file, and it is the one that catches a copy nobody has refreshed.
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
        // registry/services/identity/cafaye.yml that upstream does not have,
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

/// A registry copy is **verbatim, comments included**, and this is the test that
/// says so.
///
/// This is not a style preference and it is not belt-and-braces. It settles a
/// contradiction that was live in this repository until this packet:
///
/// - `AGENTS.md` said copies are kept "verbatim, including its comments: the
///   copy is what a reviewer reads when asking 'what does pantry think this
///   service is'".
/// - the doc comment on `every_registered_entry_matches_the_real_service_on_disk`
///   said "a comment-only edit upstream is not drift".
///
/// Both were in this file's own test suite and they cannot both be true. The
/// proof that the second was wrong is that the first was being violated in
/// silence: guard's copy had lost its whole `guard-04` `DECISION NEEDED
/// (REDIS_URL)` block, and billing's had lost the entire rewritten `billing-04`
/// section — a service with two open decisions in the registry and three in the
/// repository. A reviewer asking "what does pantry think billing is" would have
/// been told billing has no open questions, and the field comparison had
/// nothing to say about it.
///
/// These copies carry their services' `DECISION NEEDED` blocks on purpose. A
/// stale comment in a registry copy is not cosmetic; it is the registry
/// answering a question wrongly. The failure was silent, which is the worst
/// kind of failure, so this check exists to make it loud.
///
/// The cost is that a comment-only edit upstream turns the suite red, and the
/// fix is mechanical rather than a judgement call:
///
/// ```sh
/// cp ../<service>/cafaye.yml registry/services/<service>/cafaye.yml
/// ```
///
/// That is the trade, and the failure message below says it out loud so nobody
/// has to infer it.
#[test]
fn every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes() {
    let root = require_workspace!("registry copy is verbatim");

    let registry = Registry::load(&registry_dir()).expect("the official registry loads");
    assert!(
        !registry.entries().is_empty(),
        "the registry is empty, so nothing was checked"
    );

    let mut stale: Vec<String> = Vec::new();

    for entry in registry.entries() {
        let name = entry.name();
        let copy = registry_dir()
            .join("services")
            .join(name)
            .join("cafaye.yml");
        let service = service_root(&root, name);
        let real = service.join("cafaye.yml");

        let copied = std::fs::read(&copy)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", copy.display()));
        let upstream = std::fs::read(&real)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", real.display()));

        if copied == upstream {
            continue;
        }

        // Say which kind of drift this is, because the two need different
        // attention and "they differ" sends a reader to a diff to find out
        // which one they have. The field comparison is the other test in this
        // file; reporting it here too means one failure answers the question
        // instead of sending the reader to the next one.
        let fields_match = manifest::read(&copy)
            .ok()
            .zip(manifest::read(&real).ok())
            .is_some_and(|(left, right)| left == right);

        // The first differing line, so the reader does not have to run diff to
        // find out where to look. Both files are UTF-8 YAML by construction —
        // `manifest::read` above would have said otherwise — so this cannot
        // panic on a byte boundary it did not expect.
        let copied_text = String::from_utf8_lossy(&copied);
        let upstream_text = String::from_utf8_lossy(&upstream);
        let at = copied_text
            .lines()
            .zip(upstream_text.lines())
            .position(|(left, right)| left != right)
            .map(|index| index + 1)
            .unwrap_or_else(|| {
                copied_text
                    .lines()
                    .count()
                    .min(upstream_text.lines().count())
                    + 1
            });

        // Every stale copy is reported in one failure, not the first one. A
        // packet that refreshes copies should see the whole list from a single
        // run: reporting one at a time turns "run the test, fix, run it again"
        // into a loop whose length nobody can see in advance, and this is
        // exactly the failure that reached master in the first place — one
        // service noticed, the next one never looked.
        stale.push(format!(
            "  {name}\n    \
             copy   registry/services/{name}/cafaye.yml ({} bytes)\n    \
             real    {name}/cafaye.yml ({} bytes) — first differs at line {at}\n    \
             {fields}\n    \
             fix     cp {}/cafaye.yml registry/services/{name}/cafaye.yml",
            copied.len(),
            upstream.len(),
            service.display(),
            fields = if fields_match {
                "every YAML field already matches — a COMMENT-ONLY drift. The copy is out \
                 of date, not wrong: it is missing the decisions the service has recorded \
                 since it was taken."
            } else {
                "a YAML field moved too — this entry is not merely stale, it is WRONG. \
                 every_registered_entry_matches_the_real_service_on_disk fails as well and \
                 names the field."
            },
        ));
    }

    assert!(
        stale.is_empty(),
        "{} registry cop{} not byte-identical to the service they were copied from.\n\
         \n\
         {}\n\
         \n\
         A registry copy is VERBATIM, comments included (AGENTS.md, \"Registering or changing \
         a service\"): the copy is what a reviewer reads when asking what pantry thinks a \
         service is, and a service's DECISION NEEDED blocks live in its comments. A stale \
         comment is not cosmetic — it is the registry answering a question wrongly, and it \
         is a service with open decisions that looks settled.\n\
         \n\
         Fix each one with the `cp` above, in the same commit as whatever changed upstream. \
         Do NOT \"fix\" this by loosening the check: a copy nobody checks is a copy that \
         rots, and an exemption here is a weakened check, which PLAN.md §1 forbids.",
        stale.len(),
        if stale.len() == 1 { "y is" } else { "ies are" },
        stale.join("\n"),
    );
}

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

    // Worktrees are not repositories: `moon/cafaye` holds several, and a
    // worktree's directory name is `<service>-worker-<packet>`. A manifest inside
    // one is that service's manifest, already checked through its own checkout,
    // so counting it again would double-report every service mid-packet.
    let is_worktree = |name: &str| name.contains("-worker-");

    let mut missing: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&root).expect("the cafaye root is readable") {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if is_worktree(&name) || !entry.path().join("cafaye.yml").is_file() {
            continue;
        }

        let registered = registry.get(&name).is_some();
        let excluded = index.excluded.iter().any(|e| e.name == name);

        if !registered && !excluded {
            missing.push(format!(
                "  {name} — carries a valid cafaye.yml at {} and appears in no curation list",
                entry.path().join("cafaye.yml").display()
            ));
        }
    }

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
