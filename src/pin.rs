//! Which commit of `core` has this repository actually vendored?
//!
//! pantry reads core twice: once at build time (`schemas/cafaye.manifest.schema.json`
//! is a byte copy compiled into the binary) and once at test time (the manifest
//! schema and the example manifests are read out of a sibling `core/` checkout).
//! The second read is the dangerous one, because a checkout on `master` is not a
//! claim about what this repository shipped — it is whatever `core` looks like
//! at the moment the test happens to run.
//!
//! That is MD15, and the fleet has paid for learning it three times: `core-09`
//! added two valid gate declarations to `core/examples/valid/`, pantry's gate
//! went red in a repository nobody had touched, and the failure was reported
//! against pantry because pantry is where the gate is. **A consumer resolves
//! core's examples at the ref it has actually vendored, never from the working
//! tree.**
//!
//! ## Why this is not a Python call into another repository
//!
//! `kit/tests/staleness.py` reads the same two file shapes and does it better
//! for its own job — it is a fleet-wide report with discovery and a GitHub-API
//! path. Three reasons this is Rust instead of a shell-out to that script:
//!
//! 1. pantry's CI clones the fleet, not `kit`, and adding a runtime dependency
//!    on a script in a repository this one does not clone would make the check
//!    unrunnable in CI rather than merely absent.
//! 2. What is needed here is three functions — read a pin, read a blob at a
//!    ref, count commits between two refs — not the report. Reusing the *file
//!    shapes* is the reuse; importing the *tool* would be a heavier dependency
//!    for a narrower need.
//! 3. `staleness.py` deliberately exits non-zero only for "unreadable", and it
//!    discovers repositories from `--repos-dir` or a GitHub organisation. A test
//!    needs the opposite: a pin resolved from *this* directory, and a missing
//!    pin that names what was searched.
//!
//! The two file shapes are the same two on purpose. A repository that records
//! its core pin in a form this module does not recognise is reported as
//! `missing`, never as `current` — that distinction is the whole difference
//! between a resolver and a rubber stamp.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The lockfile form: the resolved `sha:` of the first git content entry.
///
/// What `vendir` writes and what Renovate updates, and the form the fleet is
/// migrating toward, so it is preferred when a repository carries both.
pub const LOCKFILE: &str = "vendir.lock.yml";

/// The hand-bumped form: a `CORE_REF: <sha>` in a workflow env block.
///
/// Still what `muse` uses today. A migration that only learned to read
/// lockfiles would report every hand-bumped repository as undeclared on the
/// day it landed, which is the worst possible moment to stop being able to see
/// them.
const WORKFLOW_DIR: &str = ".github/workflows";

/// Which file shape a pin was read from. Carried rather than collapsed into the
/// sha, because "pantry validates against the schema at `71d01fd`" is a claim a
/// reviewer should be able to follow back to a line in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSource {
    /// `vendir.lock.yml`, a resolved sha under a git content entry.
    VendirLockfile,
    /// `CORE_REF: <sha>` in a file under `.github/workflows/`.
    CoreRef,
}

impl PinSource {
    /// The path, relative to the repository root, that carries this pin.
    pub fn file(self) -> &'static str {
        match self {
            PinSource::VendirLockfile => LOCKFILE,
            PinSource::CoreRef => ".github/workflows/*.yml",
        }
    }
}

impl std::fmt::Display for PinSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinSource::VendirLockfile => f.write_str(LOCKFILE),
            PinSource::CoreRef => f.write_str("CORE_REF in .github/workflows/"),
        }
    }
}

/// A recorded pin, and where it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePin {
    /// A full 40-hex commit sha. Never a branch, never an abbreviated sha: this
    /// module exists to make "which bytes" answerable, and both of those are
    /// questions rather than answers.
    pub sha: String,
    pub source: PinSource,
}

/// Why there is no pin, in enough detail to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingPin {
    /// Every path that was searched, in the order they were tried.
    pub searched: Vec<PathBuf>,
    /// A repository that records a pin in a form that is not a commit — a
    /// branch, a tag, a word. `docs` carries `CORE_REF: 'master'`, which is a
    /// dependency declaration and not a pin, and it is worth naming because the
    /// tempting fix is to accept it and then report the consumer as `current`.
    pub non_commit_ref: Option<(PinSource, String)>,
}

/// What resolving a pin found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinResolution {
    Found(CorePin),
    /// Two pins exist and they name different commits. Not an error — it is a
    /// mid-migration state — but a claim nobody should have to disprove, so it
    /// is its own value rather than one of the two winning quietly.
    Disagreeing(CorePin, CorePin),
    Missing(MissingPin),
}

impl PinResolution {
    /// The pin, if there is exactly one. A disagreeing pair has none.
    pub fn pin(&self) -> Option<&CorePin> {
        match self {
            PinResolution::Found(pin) => Some(pin),
            _ => None,
        }
    }
}

/// A full git sha, and nothing else.
///
/// The gate on every pin this module produces. A branch name (`master`), a tag,
/// a short sha and the empty string are all things a human types meaning "the
/// latest one", and every one of them makes the answer depend on when it was
/// asked — which is the property this module exists to remove.
fn is_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The `sha:` line of a `vendir.lock.yml`, if it has one.
///
/// Read as text rather than as YAML, deliberately: this is a grep for one
/// literal in a known shape, and a YAML round-trip would add a parser
/// dependency to the one part of pantry that must not be able to fail in a new
/// way.
fn read_lockfile_sha(repo_dir: &Path) -> Option<String> {
    let path = repo_dir.join(LOCKFILE);
    let body = std::fs::read_to_string(path).ok()?;
    for line in body.lines() {
        let Some(value) = line.trim().strip_prefix("sha:") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if is_sha(value) {
            return Some(value.to_string());
        }
    }
    None
}

/// The first `CORE_REF:` in any workflow file, and whatever it says.
///
/// Returns the value whether or not it is a sha, so the caller can distinguish
/// "there is no pin" from "there is something here that is not a pin" — the
/// second is a bug worth naming and the first is a task.
fn read_core_ref(repo_dir: &Path) -> Option<(String, bool)> {
    let root = repo_dir.join(WORKFLOW_DIR);
    let mut workflows: Vec<PathBuf> = walk(&root).into_iter().collect();
    // Sorted because a directory walk's order is a filesystem detail and the
    // answer must not depend on it.
    workflows.sort();

    for path in workflows {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if extension != "yml" && extension != "yaml" {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in body.lines() {
            let Some(value) = line.trim().strip_prefix("CORE_REF:") else {
                continue;
            };
            let value = value.trim().trim_matches('"').trim_matches('\'');
            return Some((value.to_string(), is_sha(value)));
        }
    }
    None
}

/// Every regular file under `root`, or nothing if `root` is not a directory.
fn walk(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// Resolve the core commit `repo_dir` has actually vendored.
///
/// Prefers the lockfile, then `CORE_REF`, in that order — the same preference
/// `kit/tests/staleness.py` uses, and the same reason: a repository carrying
/// both is mid-migration and the lockfile is where it is going.
pub fn resolve(repo_dir: &Path) -> PinResolution {
    let lock_pin = read_lockfile_sha(repo_dir).map(|sha| CorePin {
        sha,
        source: PinSource::VendirLockfile,
    });

    // Read as a pair — the value and whether it is a commit — because "there is
    // no pin here" and "there is something here that is not a pin" are different
    // findings and the second one is the more useful of the pair. `docs` carries
    // `CORE_REF: 'master'`; accepting that would make its vendored bytes a
    // question rather than an answer, and reporting it as `current` would make
    // it worse than not looking.
    let (non_commit_ref, ref_pin) = match read_core_ref(repo_dir) {
        None => (None, None),
        Some((sha, false)) => (Some((PinSource::CoreRef, sha)), None),
        Some((sha, true)) => (
            None,
            Some(CorePin {
                sha,
                source: PinSource::CoreRef,
            }),
        ),
    };

    match (lock_pin, ref_pin) {
        (Some(lock), Some(reference)) if lock.sha != reference.sha => {
            PinResolution::Disagreeing(lock, reference)
        }
        (Some(lock), _) => PinResolution::Found(lock),
        (None, Some(reference)) => PinResolution::Found(reference),
        (None, None) => PinResolution::Missing(MissingPin {
            searched: vec![
                repo_dir.join(LOCKFILE),
                repo_dir.join(WORKFLOW_DIR).join("*.yml"),
            ],
            non_commit_ref,
        }),
    }
}

/// Why a git read failed. Never carries the command's stdout, which is a file
/// path or a diff and not something to paste into an assertion message.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git could not be run in {repo}: {message}")]
    Unrunnable { repo: PathBuf, message: String },
    #[error("{repo} has no commit {sha} locally: {message}")]
    UnknownRef {
        repo: PathBuf,
        sha: String,
        message: String,
    },
    #[error("{path} does not exist at {repo}@{sha}: {message}")]
    MissingBlob {
        repo: PathBuf,
        sha: String,
        path: String,
        message: String,
    },
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("{error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The bytes of `path` as they were at `sha`.
///
/// `git show`, not `git cat-file`, because `show` is the spelling that works on
/// a path inside a tree without asking for the tree first, and because the error
/// it prints already names the file.
pub fn show(repo: &Path, sha: &str, path: &str) -> Result<Vec<u8>, GitError> {
    // Confirm the ref exists before asking for the blob, so an unknown commit
    // and a missing file are two different errors with two different fixes: one
    // is a bad pin, the other is a wrong path.
    git(
        repo,
        &["rev-parse", "--verify", &format!("{sha}^{{commit}}")],
    )
    .map_err(|message| GitError::UnknownRef {
        repo: repo.to_path_buf(),
        sha: sha.to_string(),
        message,
    })?;

    let spec = format!("{sha}:{path}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show", &spec])
        .output()
        .map_err(|error| GitError::Unrunnable {
            repo: repo.to_path_buf(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(GitError::MissingBlob {
            repo: repo.to_path_buf(),
            sha: sha.to_string(),
            path: path.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(output.stdout)
}

/// The names of the files directly in `dir` as they were at `sha`.
///
/// Sorted, for the same reason the workflow walk is: a directory listing's order
/// is a filesystem detail and a test that asserts on a list must not depend on
/// which inode came back first.
pub fn list_files(repo: &Path, sha: &str, dir: &str) -> Result<Vec<String>, GitError> {
    let spec = format!("{sha}:{dir}");
    let output =
        git(repo, &["ls-tree", "--name-only", &spec]).map_err(|message| GitError::MissingBlob {
            repo: repo.to_path_buf(),
            sha: sha.to_string(),
            path: dir.to_string(),
            message,
        })?;
    let mut names: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    names.sort();
    Ok(names)
}

/// How many commits `older` is behind `newer`, or `None` if it cannot be told.
///
/// `None` rather than `0`, because `0` claims the two are identical and "I could
/// not tell" is a different claim from "there is nothing between them".
pub fn commits_behind(repo: &Path, older: &str, newer: &str) -> Option<u64> {
    let range = format!("{older}..{newer}");
    git(repo, &["rev-list", "--count", &range])
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The commit a checkout's default branch is at, preferring the remote-tracking
/// ref.
///
/// `master` before `HEAD` on purpose: a repository that has been sitting on a
/// branch all week is reporting the wrong thing, and the thing being reported
/// should be what the fleet would pull.
pub fn published_head(repo: &Path) -> Option<String> {
    for candidate in [
        "refs/remotes/origin/master",
        "refs/remotes/origin/main",
        "master",
        "HEAD",
    ] {
        if let Ok(value) = git(repo, &["rev-parse", candidate]) {
            let value = value.trim();
            if is_sha(value) {
                return Some(value.to_string());
            }
        }
    }
    None
}
