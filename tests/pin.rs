//! The pin resolver, and the two states it is allowed to be wrong in.
//!
//! [`pantry::pin`] exists because a consumer must resolve core's examples at the
//! ref it has actually vendored. Two properties matter and both are tested here
//! rather than described:
//!
//! 1. **A pin is a commit and nothing else.** A branch is a question, and this
//!    fleet has a real one (`docs` records `CORE_REF: 'master'`), so "we found
//!    something in the file" is not the same claim as "we found a pin".
//! 2. **No pin is a named outcome.** [`pantry::pin::PinResolution::Missing`]
//!    carries the paths that were searched, because a skip nobody can act on is
//!    how this whole packet started.

use std::path::Path;

use pantry::pin::{self, PinResolution, PinSource};

/// A sha that exists in this repository's own history-shaped space. The resolver
/// is a text reader, so these tests do not need a real repository for the
/// reading half — only for the git-reading half, which has its own section.
const REAL_SHA: &str = "71d01fd90eae42d452db4431b999e4835949a548";
const OTHER_SHA: &str = "a463e7c00000000000000000000000000000000a";

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pantry-pin-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a parent directory");
    }
    std::fs::write(path, body).expect("the fixture is written");
}

#[test]
fn a_lockfile_sha_is_a_pin_and_names_the_file_it_came_from() {
    let dir = scratch("lockfile");
    write(
        &dir.join("vendir.lock.yml"),
        &format!(
            "apiVersion: vendir.k14s.io/v1alpha1\ndirectories:\n  - path: schemas\n    contents:\n      - path: .\n        git:\n          url: git@github.com:cafaye/core.git\n          sha: {REAL_SHA}\n"
        ),
    );

    match pin::resolve(&dir) {
        PinResolution::Found(found) => {
            assert_eq!(found.sha, REAL_SHA);
            assert_eq!(found.source, PinSource::VendirLockfile);
        }
        other => panic!("expected a resolved pin, got {other:?}"),
    }
}

#[test]
fn a_core_ref_is_a_pin_and_names_the_shape_it_came_from() {
    let dir = scratch("core-ref");
    write(
        &dir.join(".github/workflows/ci.yml"),
        &format!("env:\n  CORE_REF: {REAL_SHA}\n"),
    );

    match pin::resolve(&dir) {
        PinResolution::Found(found) => {
            assert_eq!(found.sha, REAL_SHA);
            assert_eq!(found.source, PinSource::CoreRef);
        }
        other => panic!("expected a resolved pin, got {other:?}"),
    }
}

/// The preference is a real decision, not an accident of the order the two
/// readers happen to run in: a lockfile is what `vendir` and Renovate maintain,
/// and a repository carrying both is mid-migration. `kit/tests/staleness.py`
/// prefers the lockfile for the same reason and in the same order, and the two
/// disagreeing is reported rather than resolved silently (see the next test).
#[test]
fn a_lockfile_wins_over_a_core_ref_when_they_both_exist() {
    let dir = scratch("both-agreeing");
    write(
        &dir.join("vendir.lock.yml"),
        &format!("directories:\n  - contents:\n      - git:\n          sha: {REAL_SHA}\n"),
    );
    write(
        &dir.join(".github/workflows/ci.yml"),
        &format!("env:\n  CORE_REF: {REAL_SHA}\n"),
    );

    let resolution = pin::resolve(&dir);
    let found = resolution.pin().expect("one pin");
    assert_eq!(found.source, PinSource::VendirLockfile);
}

/// A repository mid-migration whose two pins name different commits is a
/// reportable state, not a rounding error: the bytes under test came from one
/// commit and the workflow checks another. Collapsing it to "whichever was read
/// first" would make the disagreement invisible, which is the failure mode
/// `staleness.py` calls out by name in `pins_disagree`.
#[test]
fn two_pins_that_disagree_are_reported_as_a_disagreement_not_silently_resolved() {
    let dir = scratch("both-disagreeing");
    write(
        &dir.join("vendir.lock.yml"),
        &format!("directories:\n  - contents:\n      - git:\n          sha: {REAL_SHA}\n"),
    );
    write(
        &dir.join(".github/workflows/ci.yml"),
        &format!("env:\n  CORE_REF: {OTHER_SHA}\n"),
    );

    match pin::resolve(&dir) {
        PinResolution::Disagreeing(lock, reference) => {
            assert_eq!(lock.sha, REAL_SHA, "the lockfile pin");
            assert_eq!(reference.sha, OTHER_SHA, "the CORE_REF pin");
        }
        other => panic!("expected a disagreement, got {other:?}"),
    }
}

/// `docs` carries `CORE_REF: 'master'`. That is a dependency declaration and not
/// a pin: it resolves to whatever is newest at the moment it is asked, which is
/// the property this module exists to remove. Reporting it as `current` would be
/// worse than not looking, so it is reported as what it is — something in the
/// file, and not a commit.
#[test]
fn a_branch_is_not_a_pin_and_is_reported_as_such() {
    let dir = scratch("branch");
    write(
        &dir.join(".github/workflows/ci.yml"),
        "env:\n  CORE_REF: 'master'\n",
    );

    match pin::resolve(&dir) {
        PinResolution::Missing(missing) => {
            let (source, value) = missing
                .non_commit_ref
                .expect("the non-commit value is named, not dropped");
            assert_eq!(source, PinSource::CoreRef);
            assert_eq!(value, "master");
        }
        other => panic!("a branch must not resolve as a pin, got {other:?}"),
    }
}

/// The state this fleet is actually in for `caf` and `pantry`: vendored bytes,
/// no recorded origin. The outcome must name every path that was searched, or a
/// reader who has to guess where the pin is supposed to live cannot fix it.
#[test]
fn no_recorded_pin_names_every_where_a_pin_would_have_been() {
    let dir = scratch("undeclared");
    write(
        &dir.join("README.md"),
        "a repository with vendored bytes and no origin\n",
    );

    match pin::resolve(&dir) {
        PinResolution::Missing(missing) => {
            let searched: Vec<String> = missing
                .searched
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            assert!(
                searched
                    .iter()
                    .any(|path| path.ends_with("vendir.lock.yml")),
                "the lockfile location is named, got {searched:?}"
            );
            assert!(
                searched
                    .iter()
                    .any(|path| path.contains(".github/workflows")),
                "the CORE_REF location is named, got {searched:?}"
            );
        }
        other => panic!("expected a missing pin, got {other:?}"),
    }
}

/// An empty `CORE_REF:` and a truncated `sha:` are both "there is a key but no
/// value", which is a different failure from "there is no key" and would
/// otherwise be silently indistinguishable from it.
#[test]
fn a_key_with_no_commit_in_it_is_not_a_pin() {
    for (name, files) in [
        (
            "empty-core-ref",
            vec![(".github/workflows/ci.yml", "env:\n  CORE_REF:\n")],
        ),
        (
            "short-sha",
            vec![("vendir.lock.yml", "directories:\n  - sha: 71d01fd\n")],
        ),
        (
            "non-hex",
            vec![("vendir.lock.yml", "directories:\n  - sha: zzzz\n")],
        ),
    ] {
        let dir = scratch(name);
        for (path, body) in &files {
            write(&dir.join(path), body);
        }
        assert!(
            matches!(pin::resolve(&dir), PinResolution::Missing(_)),
            "{name} must not resolve as a pin"
        );
    }
}

// -------------------------------------------------------------------------
// The git-reading half. These need a real repository, and the one they use is
// this repository — which is a git checkout by construction, and whose history
// contains at least two commits.
// -------------------------------------------------------------------------

fn this_repository() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A blob read at a ref is the mechanism the whole ruling rests on, so it is
/// exercised against a real commit rather than mocked: `git show <sha>:<path>`
/// must return bytes, and they must be the same bytes `git show HEAD:<path>`
/// returns when the two refs are the same commit.
#[test]
fn a_blob_read_at_a_commit_is_the_blob_at_that_commit() {
    let repo = this_repository();
    let head = pin::published_head(&repo).expect("this repository resolves a published head");

    let at_head = pin::show(&repo, &head, "cafaye.yml").expect("cafaye.yml is readable at HEAD");
    let on_disk = std::fs::read(repo.join("cafaye.yml")).expect("cafaye.yml is on disk");
    assert_eq!(
        String::from_utf8_lossy(&at_head),
        String::from_utf8_lossy(&on_disk),
        "a clean checkout's HEAD blob is the working copy, so this also proves the \
         working tree is not being read behind the ref's back"
    );
}

/// A pin that does not exist locally is a *different error* from a pin that does
/// and whose path is wrong, and they have different fixes — one is a bad pin, the
/// other is a typo in a path. `GitError` keeps them apart for exactly that
/// reason, and this asserts it rather than leaving it to the enum's shape.
#[test]
fn an_unknown_commit_and_a_missing_path_are_two_different_errors() {
    let repo = this_repository();

    let unknown = pin::show(&repo, &"0".repeat(40), "cafaye.yml")
        .expect_err("a sha nobody has is not readable");
    assert!(
        matches!(unknown, pin::GitError::UnknownRef { .. }),
        "an unknown commit is UnknownRef, got {unknown}"
    );

    let head = pin::published_head(&repo).expect("a head");
    let missing =
        pin::show(&repo, &head, "no/such/file.yml").expect_err("a path nobody has is not readable");
    assert!(
        matches!(missing, pin::GitError::MissingBlob { .. }),
        "a missing path is MissingBlob, got {missing}"
    );
}

/// `commits_behind` must be able to say "I could not tell". `0` would claim the
/// two commits are the same, which is a different claim, and it is the claim a
/// staleness report most needs to get right: reporting an unreadable pin as
/// `current` is the expensive direction.
#[test]
fn an_unmeasurable_distance_is_not_reported_as_zero() {
    let repo = this_repository();
    assert_eq!(
        pin::commits_behind(&repo, &"0".repeat(40), &"0".repeat(40)),
        None,
        "a sha nobody has has no measurable distance"
    );
}

#[test]
fn a_ref_is_zero_commits_behind_itself() {
    let repo = this_repository();
    let head = pin::published_head(&repo).expect("a head");
    assert_eq!(pin::commits_behind(&repo, &head, &head), Some(0));
}
