//! The CI workflow is a file in this repository, so it rots like any other copy.
//!
//! `.github/workflows/ci.yml` contains a `workspace-drift` job that clones the
//! cafaye fleet and runs the whole gate against it. That job is the only thing
//! in this repository's CI that makes the registry *true* rather than
//! internally consistent, and it is also the easiest thing here to break by
//! accident — narrow the clone list, drop a `secrets.` line in, or set
//! `if: false` again — with a green badge still sitting there over the top.
//!
//! So the claims that job makes are asserted here, in the `build` job, on a
//! clone of pantry alone. None of these tests needs a cafaye workspace, which
//! is the point: the job they defend is the one that has to fetch a workspace
//! to run, so a test that could only run *after* the clones would not catch a
//! clone list that stopped covering anything.
//!
//! The relationship this file enforces is deliberately one-directional. What
//! pantry *curates* — every name in `registry/index.yml`, registered or
//! excluded — must be *cloned*, because a repository the workspace does not
//! contain is a repository no drift test can compare against anything. The
//! reverse is not enforced and cannot be: a cafaye repository nobody has
//! written down anywhere is invisible to a list, and the only honest cure is
//! the org API, whose anonymous rate limit would make this job flaky. That
//! residual gap is stated on the job itself rather than papered over here.

use std::collections::BTreeSet;
use std::path::Path;

use pantry::registry::{self, registry_dir};
use serde_yaml::Value;

/// Where the workflow lives, relative to the crate root. These tests run in
/// the `build` job too, so they must not need a workspace.
const WORKFLOW: &str = ".github/workflows/ci.yml";

/// The job that clones the fleet. Named once so a rename fails here loudly
/// rather than silently deleting the coverage.
const DRIFT_JOB: &str = "workspace-drift";

fn workflow_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(WORKFLOW)
}

fn workflow() -> Value {
    let path = workflow_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));

    serde_yaml::from_str(&raw)
        .unwrap_or_else(|error| panic!("{} parses as YAML: {error}", path.display()))
}

fn drift_job() -> Value {
    workflow()["jobs"][DRIFT_JOB].clone()
}

/// Every repository this registry curates: the ones it registers and the ones
/// it holds back with a reason. Both are facts about a repository on disk, and
/// the drift tests need that repository present to check either of them.
fn curated_repositories() -> BTreeSet<String> {
    let dir = registry_dir();
    let index = registry::read_index(&dir).expect("registry/index.yml parses");

    let curated: BTreeSet<String> = index
        .services
        .keys()
        .cloned()
        .chain(index.excluded.iter().map(|excluded| excluded.name.clone()))
        // `pantry` is the repository this suite is running in: the job checks
        // it out with `actions/checkout` rather than cloning a second copy of
        // it beside itself, so it is not on the clone list by design.
        .filter(|name| name != "pantry")
        .collect();

    assert!(
        !curated.is_empty(),
        "no curated repositories were found, so this test would pass on an empty list"
    );

    curated
}

/// The repositories the drift job clones, read out of the one place that says
/// so. A single whitespace-separated value rather than a shell array, so that
/// this test and the job are reading the same list rather than two spellings
/// of it that can drift apart.
fn cloned_repositories() -> BTreeSet<String> {
    let job = drift_job();
    let raw = job
        .get("env")
        .and_then(|env| env.get("CAFAYE_REPOS"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "the {DRIFT_JOB} job has no CAFAYE_REPOS, so the repositories it clones are \
                 only discoverable by reading shell out of a `run:` block"
            )
        })
        .to_string();

    let cloned: BTreeSet<String> = raw.split_whitespace().map(str::to_string).collect();

    assert!(
        !cloned.is_empty(),
        "CAFAYE_REPOS is empty, so the job would clone nothing and every drift test would \
         skip — which is the exact failure this file exists to make impossible"
    );

    cloned
}

/// The shell of every step in the drift job, as one string. Used for the checks
/// that are about the job as a whole rather than about one step.
fn every_script() -> String {
    drift_job()["steps"]
        .as_sequence()
        .expect("the drift job has steps")
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// **The trap this file is mostly about.** Cloning only the repositories the
/// registry happens to name today is the obvious optimisation and the wrong
/// one: it makes the job's coverage a function of the registry, so the moment a
/// ninth service is registered the job is silently wrong in the direction that
/// matters — the repository nobody remembered is the one nobody checks.
///
/// So the clone list is hand-maintained, and this test is what makes
/// hand-maintained mean something: a new curation list entry turns the `build`
/// job red until somebody widens `CAFAYE_REPOS` on purpose.
#[test]
fn the_drift_job_clones_every_repository_pantry_curates() {
    let cloned = cloned_repositories();
    let curated = curated_repositories();

    let missing: Vec<&String> = curated.difference(&cloned).collect();

    assert!(
        missing.is_empty(),
        "{} cafaye repositor{} curated in registry/index.yml and absent from CAFAYE_REPOS in \
         {WORKFLOW}:\n\n{}\n\nA repository the job does not clone is a repository no drift test \
         can compare against anything, and the tests will not say so: the ones that need it print \
         a SKIP and return green. Add the names to CAFAYE_REPOS.",
        missing.len(),
        if missing.len() == 1 {
            "y is"
        } else {
            "ies are"
        },
        missing
            .iter()
            .map(|name| format!("  {name} — curated but never cloned"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The clone list must not be allowed to shrink into the registry it is meant
/// to check. Cloning the services the registry names is a subset; the job also
/// needs `core` (the workspace marker every drift test looks for) and the
/// repositories the registry deliberately does not describe, because the
/// coverage tests ask about those too.
#[test]
fn the_drift_job_clones_more_than_the_registered_services() {
    let cloned = cloned_repositories();

    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");
    let registered: BTreeSet<&str> = index
        .services
        .keys()
        .map(String::as_str)
        .filter(|name| *name != "pantry")
        .collect();

    let beyond: Vec<&String> = cloned
        .iter()
        .filter(|name| !registered.contains(name.as_str()))
        .collect();

    assert!(
        !beyond.is_empty(),
        "CAFAYE_REPOS is exactly the set of registered services, so this job is coupled to the \
         registry: registering a ninth service would be the only thing that could widen it. It \
         also clones nothing the registry does not describe, and `core/schemas/\
         cafaye.manifest.schema.json` — the file `cafaye_root()` uses to decide a workspace \
         exists at all — would be missing, so every drift test would skip."
    );
}

/// Every cafaye repository is public, so a hosted runner clones all of them
/// over anonymous HTTPS with no credential of any kind. `CAFAYE_CI_SSH_KEY`
/// existed to work around a problem that does not exist, and a long-lived
/// private key in a repository's settings is a liability: it is a secret whose
/// only purpose is to be unnecessary, and the next person to read this workflow
/// will assume there is a reason for it.
///
/// So this is asserted rather than promised. The next person who reaches for a
/// credential here fails the `build` job, on a clone of pantry alone, in a
/// minute, instead of discovering the question in a merge review.
#[test]
fn the_drift_job_clones_anonymously_and_needs_no_credential() {
    let raw = std::fs::read_to_string(workflow_path()).expect("the workflow is readable");
    let script = every_script();

    for (what, needle) in [
        ("a secret reference", "secrets."),
        ("an SSH remote", "git@github.com:"),
        ("an SSH scheme", "ssh://"),
        ("a deploy key", "id_ed25519"),
        ("an install of a key", "install -m 700 -d ~/.ssh"),
    ] {
        assert!(
            !raw.contains(needle),
            "{WORKFLOW} mentions {what} (`{needle}`). Every repository the drift job clones is \
             public, so it needs no credential: `git clone --depth 1 \
             https://github.com/cafaye/<repo>.git <repo>` is the whole mechanism. If a future \
             cafaye repository is private, that is a fact to raise as a DECISION NEEDED, not a \
             long-lived key to add back here."
        );
    }

    assert!(
        script.contains("https://github.com/cafaye/"),
        "the clone step does not clone over https://github.com/cafaye/. The clone URL is the \
         mechanism this file is asserting, and it is the line a reader checks to see whether a \
         credential is involved."
    );
    assert!(
        script.contains("--depth 1"),
        "the clone step lost `--depth 1`. These are checked out to be read, not built, and a \
         full clone of eight repositories on every run is minutes of transfer for no history \
         anything here reads."
    );
}

/// `if: false` is how this coverage was absent while looking present. A
/// disabled job leaves a green badge over the top of it, and the failure mode
/// is invisible precisely because nothing goes red.
///
/// Disabling this job again is a legitimate thing to want — a fleet-wide
/// incident, an org outage — and the way to do it is to change this test in
/// the same commit, so the commit says why the coverage is off. A silent
/// `if: false` is not that.
#[test]
fn the_drift_job_is_enabled() {
    let job = drift_job();

    assert!(
        !job.is_null(),
        "{WORKFLOW} has no `{DRIFT_JOB}` job. The drift tests skip without a cafaye workspace, \
         so deleting this job turns the badge green by removing the only check that reads the \
         fleet — and nothing in the suite would notice."
    );

    let condition = job
        .get("if")
        .map(|value| match value {
            Value::Bool(flag) => format!("`if: {flag}`"),
            other => format!("`if: {other:?}`"),
        })
        .unwrap_or_default();

    assert!(
        condition.is_empty() || condition == "`if: true`",
        "the {DRIFT_JOB} job is gated on {condition} in {WORKFLOW}, so it does not run and the \
         registry is verified against itself. If it has to be off, say why in the comment on the \
         job and change `the_drift_job_is_enabled` in this file in the same commit — a gate that \
         is switched off quietly is how this coverage went missing the first time."
    );
}

/// The job can be enabled, correctly configured, and still verify nothing: if
/// `PANTRY_CAFAYE_ROOT` is not the directory the clones landed in, `cafaye_root()`
/// finds no workspace, and every drift test prints `SKIP …` and returns green.
/// That is not a hypothetical shape — it is what a pantry-only clone does, and
/// it is what this job used to be.
#[test]
fn the_drift_job_points_the_suite_at_the_checkouts() {
    let job = drift_job();
    let steps = job["steps"]
        .as_sequence()
        .expect("the drift job has steps")
        .clone();

    let primes = steps.iter().any(|step| {
        step["run"]
            .as_str()
            .is_some_and(|run| run.contains("./bin/prime"))
    });

    assert!(
        primes,
        "the {DRIFT_JOB} job never runs ./bin/prime, so nothing in it reads the fleet"
    );

    let root = steps
        .iter()
        .find_map(|step| {
            step["env"]
                .get("PANTRY_CAFAYE_ROOT")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .expect("the step that runs ./bin/prime sets no PANTRY_CAFAYE_ROOT");

    assert_eq!(
        root.trim(),
        "${{ github.workspace }}",
        "PANTRY_CAFAYE_ROOT is `{root}` where it must be `${{ github.workspace }}` — the \
         directory the fleet was cloned into. Any other value makes cafaye_root() miss, and \
         a miss is not a failure: every drift test prints SKIP and returns green, which is the \
         outcome this whole packet exists to end."
    );
}
