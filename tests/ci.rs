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
    listed_repositories("CAFAYE_REPOS")
}

/// The repositories this job is **not** expected to read, and why: the ones the
/// runner has no credential for. One name today, and the workflow's comment says
/// which and says why.
///
/// This exists because `the_drift_job_clones_every_repository_pantry_curates` was
/// asserting something false about the world. Its claim was that every curated
/// repository is cloned, and the reason was "a repository the workspace does not
/// contain is a repository no drift test can compare against anything" — which is
/// true of a *public* repository and false of a private one. `cafaye-rb` entered
/// the exclusion record in this packet and turned that into a red: it is a
/// repository pantry curates and no anonymous runner may read it. The
/// alternatives were a credential (what pantry-04 deleted, on the grounds that
/// the fleet is public) or leaving the row out of the registry, which is the
/// silent omission the check exists to prevent. So the fact is stated in the file
/// a reviewer of the workflow reads first, and checked here.
fn unreadable_repositories() -> BTreeSet<String> {
    let listed = listed_repositories("CAFAYE_UNREADABLE");

    assert!(
        listed.is_disjoint(&cloned_repositories()),
        "a repository is in both CAFAYE_REPOS and CAFAYE_UNREADABLE: it is being cloned and \
         also declared unreadable, so one of the two lists is wrong. A clone either succeeds or \
         it does not — there is no state in which a repository is both."
    );

    listed
}

/// One whitespace-separated list out of the drift job's `env`.
fn listed_repositories(key: &str) -> BTreeSet<String> {
    let job = drift_job();
    let raw = job
        .get("env")
        .and_then(|env| env.get(key))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "the {DRIFT_JOB} job has no {key}, so the repositories it clones are only \
                 discoverable by reading shell out of a `run:` block"
            )
        })
        .to_string();

    let listed: BTreeSet<String> = raw.split_whitespace().map(str::to_string).collect();

    if key == "CAFAYE_REPOS" {
        assert!(
            !listed.is_empty(),
            "CAFAYE_REPOS is empty, so the job would clone nothing and every drift test would \
             skip — which is the exact failure this file exists to make impossible"
        );
    }

    listed
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

/// The clone lines only, with comments stripped.
///
/// `every_script` includes the explanatory comments above each command, and the
/// comments necessarily NAME the flags they argue against (`--depth 1`, by
/// sentence). A substring check over text that quotes the thing it forbids is a
/// check that can never pass, so both this and any future one has to read the
/// commands rather than the prose. `#` is the only comment syntax in a bash `run`
/// block, and stripping the rest of the line cannot change a command — a `#`
/// inside a quoted string would, and there is none in this job.
fn every_command() -> String {
    every_script()
        .lines()
        .map(|line| match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        })
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
/// job red until somebody widens `CAFAYE_REPOS` on purpose, or says in
/// `CAFAYE_UNREADABLE` why the runner cannot read it.
///
/// The unreadable list is not a softener on that. It is one repository, it is
/// named in the workflow next to the clone list, and two tests below keep it to
/// what it can honestly be: no registered service, nothing already cloned, and
/// nothing the registry has stopped curating.
#[test]
fn the_drift_job_clones_every_repository_pantry_curates() {
    let cloned = cloned_repositories();
    let unreadable = unreadable_repositories();
    let curated = curated_repositories();

    let missing: Vec<&String> = curated
        .difference(&cloned)
        .filter(|name| !unreadable.contains(*name))
        .collect();

    assert!(
        missing.is_empty(),
        "{} cafaye repositor{} curated in registry/index.yml that {WORKFLOW} neither clones nor \
         declares unreadable:\n\n{}\n\nA repository the job cannot read is a repository no \
         drift test can compare against anything, and the tests will not say so: the ones that \
         need it print a SKIP and return green. Add the names to CAFAYE_REPOS, or — if the \
         runner genuinely has no credential for it, as it does for the private cafaye-rb — name \
         it in CAFAYE_UNREADABLE with the reason in the comment beside that list. Do NOT leave \
         it off both lists: that is the failure this test was written for.",
        missing.len(),
        if missing.len() == 1 {
            "y is"
        } else {
            "ies are"
        },
        missing
            .iter()
            .map(|name| format!("  {name} — curated, and read by neither list"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// What `CAFAYE_UNREADABLE` may and may not contain, which is the whole reason
/// it is a list rather than a comment.
///
/// The failure it has to prevent is the quiet one: a repository goes on this
/// list for a real reason today, and the reason goes away — the repository
/// becomes public — and the entry stays. The job keeps not cloning it, nobody
/// is told, and a coverage hole that was justified becomes unjustified without
/// anything going red. Two of the four assertions below exist only for that.
///
/// 1. Nothing already cloned. Handled in `unreadable_repositories`.
/// 2. Nothing this repository *registers*. A registered entry is a fact pantry
///    publishes to every client that asks, so it is the one thing that must
///    always be checkable against the real service. An unreadable service is a
///    registry publishing an unverified claim, which is worse than an unlisted
///    one.
/// 3. Nothing the registry has stopped curating. A name here that no longer
///    appears in `registry/index.yml` is a leftover: it is not describing
///    anything, and it is the shape a stale exclusion takes.
/// 4. The workflow says why. A repository in this list that the comment beside
///    it never mentions is an exemption nobody approved, and this is the check
///    that notices.
#[test]
fn an_unreadable_repository_is_neither_cloned_nor_registered() {
    let unreadable = unreadable_repositories();
    let index = registry::read_index(&registry_dir()).expect("registry/index.yml parses");

    for name in &unreadable {
        assert!(
            !index.services.contains_key(name),
            "{name} is in CAFAYE_UNREADABLE, so this job cannot read the service it describes \
             — and it is REGISTERED, which means pantry publishes a claim about it that no CI \
             run can verify. Either the repository becomes readable (add it to CAFAYE_REPOS) or \
             the entry comes out of the registry. A registry entry nothing can check is a \
             claim, not a fact."
        );

        assert!(
            index.excluded.iter().any(|excluded| &excluded.name == name),
            "{name} is in CAFAYE_UNREADABLE but registry/index.yml no longer curates it at \
             all, so the list is describing a repository this registry does not care about. \
             Remove it from CAFAYE_UNREADABLE: a leftover entry here is how an unjustified \
             coverage hole survives."
        );
    }

    let raw = std::fs::read_to_string(workflow_path()).expect("the workflow is readable");
    for name in &unreadable {
        assert!(
            raw.contains(name.as_str()),
            "{name} is declared unreadable in {WORKFLOW} and the file never says why. Every \
             other name in these two lists is a fact a reader can check; this one is an \
             exemption, and an exemption with no stated reason is the thing this file exists \
             to stop."
        );
    }
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
    let commands = every_command();

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
             public, so it needs no credential: `git clone \
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
        !commands.contains("--depth"),
        "the clone step is shallow again (`--depth`). That was right until `core-11`, and it is \
         wrong now: `tests/recorded_copy.rs` verifies each registry copy against the `recordedAt` \
         commit in `registry/index.yml`, and `tests/schema.rs` verifies `schemas/` against the \
         sha in `vendir.lock.yml`. `git show <sha>:<path>` cannot reach a commit a shallow clone \
         does not have, so a shallow clone turns every one of those checks into a SKIP — an \
         honest one, naming the ref it could not read, but a CI run that verified nothing about \
         any registry copy while looking green. Full histories of public repositories are a few \
         MB each and anonymous."
    );
}

/// The clone must be able to REACH a recorded ref, not merely contain a `.git`.
///
/// This exists because the fix to the above is invisible from the outside: a
/// workflow that clones shallowly still has a `.git` directory in every checkout,
/// so `prove the workspace is real` passes and `cafaye_root()` resolves and every
/// test runs — and the recorded-ref checks skip inside those runs. Nothing about
/// the badge would say so. So the assertion is about the recorded refs
/// themselves: this repository's own `vendir.lock.yml`, and one `recordedAt` from
/// `registry/index.yml`, are both non-tip commits relative to a `--depth 1` clone
/// of the fleet, and the workflow must therefore fetch history.
#[test]
fn the_drift_job_clones_deep_enough_to_reach_a_recorded_ref() {
    let commands = every_command();

    assert!(
        !commands.contains("--depth"),
        "the drift job clones shallowly, so a recorded ref that is not the tip is unreachable \
         and every recorded-ref check skips. See \
         `the_drift_job_clones_anonymously_and_needs_no_credential` for the same line."
    );

    // And the thing that would make a shallow clone sufficient is not present:
    // if some future change adds an explicit `git fetch origin <recorded-sha>`
    // per repository, `--depth 1` becomes correct again and this assertion would
    // be refusing a good change. Say so here rather than leaving the next reader
    // to work it out.
    assert!(
        !commands.contains("git fetch"),
        "the drift job now fetches specific refs, which would make `--depth 1` viable again. \
         Remove this test and the `--depth` assertion above in the same commit, and say why the \
         fetch is sufficient."
    );

    // The refs that must be reachable are real, so this test fails loudly if a
    // future packet edits `vendir.lock.yml` into a shape this reasoning does not
    // cover.
    let pin = pantry::pin::resolve(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let recorded = pin
        .pin()
        .unwrap_or_else(|| panic!("this repository records no core pin, so `the_drift_job_clones_deep_enough_to_reach_a_recorded_ref` has nothing to check: {pin:?}"));
    assert_eq!(
        recorded.sha.len(),
        40,
        "the recorded core pin is not a full sha, so `git show` reachability is not the question \
         it was"
    );

    let index =
        pantry::registry::read_index(&pantry::registry::registry_dir()).expect("the index parses");
    let recorded_services: Vec<&str> = index
        .services
        .values()
        .filter_map(|entry| entry.recorded_at.as_deref())
        .collect();
    assert!(
        recorded_services.len() == index.services.len(),
        "{} of {} registered services record a `recordedAt`, so this test's claim that every \
         registered copy has a ref to reach is not true",
        recorded_services.len(),
        index.services.len()
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
