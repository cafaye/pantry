//! The registry: the official cafaye service set, as data in this repository.
//!
//! ## Where the data lives
//!
//! `registry/services/<name>/cafaye.yml` is a verbatim copy of the service's own
//! manifest. `registry/index.yml` holds the two facts a manifest cannot carry
//! and one record of what is deliberately not registered. That split is the
//! whole design: everything a consumer needs is either in the service's own file
//! or in a file that says which service it is about.
//!
//! ## There is no database
//!
//! Phase 1 has none, and this is a property rather than an omission. A registry
//! whose entries can be written at runtime is a registry that can be given a new
//! service by anything that can reach the process, which is how a registry
//! becomes an unaudited code-execution surface. Adding a service is a commit and
//! a reviewed pull request — that is the entire trust model. Phase 5's
//! marketplace changes it deliberately, not by accident.
//!
//! ## Why a copy at all
//!
//! A container has no sibling checkouts, so the registry has to be
//! self-contained; but a copy nobody checks is a copy that rots. So the copy is
//! verified against the real service by `tests/drift.rs` on every run, field by
//! field. That test is the mechanism. Discipline is not.
//!
//! ## What is not here
//!
//! No submission flow, no third-party registration, no webhook that adds an
//! entry, no plugin loader, no UI, and no health polling of other services —
//! pantry describes how to reach a service, it never calls one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::contract::Constraint;
use crate::manifest::{self, Language, Manifest};

/// The registry directory in this repository.
pub fn registry_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("registry")
}

/// Every registered manifest, in a stable order.
///
/// One directory per service, each holding a file named exactly `cafaye.yml`.
/// That name is not cosmetic: `caf contract lint` only lints files called
/// `cafaye.yml`, so this layout is what lets the platform's own CLI validate
/// every registry entry — `caf contract lint registry/services` — instead of
/// pantry's registry being checkable only by pantry. A flat
/// `<name>.cafaye.yml` looks tidier and is a directory the canonical validator
/// walks straight past.
///
/// Sorted because a directory walk's order is a filesystem detail and a client
/// diffing two responses should see a stable list.
pub fn manifest_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Ok(services) = std::fs::read_dir(dir.join("services")) else {
        return paths;
    };

    for entry in services.filter_map(Result::ok) {
        let path = entry.path().join(MANIFEST_FILE_NAME);
        if path.is_file() {
            paths.push(path);
        }
    }

    paths.sort();
    paths
}

/// The one name a manifest may have. A repository carries exactly one, at its
/// root, and this is the file every tool in the platform looks for.
const MANIFEST_FILE_NAME: &str = "cafaye.yml";

/// One registered service: the manifest it publishes, plus the two facts a
/// manifest cannot state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    pub manifest: Manifest,
    pub kind: ServiceKind,
    /// The common path prefix of the service's OpenAPI document, or `None` when
    /// it publishes none. `/v1` today for every service that has a document.
    pub base_path: Option<String>,
}

impl ServiceEntry {
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn language(&self) -> Language {
        self.manifest.language
    }

    /// The service's `core` constraint, parsed. A manifest that is in the
    /// registry has already had this parsed — the load fails otherwise — so this
    /// cannot fail, and a filter does not have to re-parse on every request.
    pub fn core_constraint(&self) -> Constraint {
        Constraint::parse(&self.manifest.core).expect("a loaded entry has a parseable constraint")
    }

    pub fn serves_http(&self) -> bool {
        self.manifest
            .exposes
            .as_ref()
            .and_then(|exposes| exposes.api.as_deref())
            .is_some()
    }

    pub fn publishes_events(&self) -> bool {
        self.manifest
            .exposes
            .as_ref()
            .and_then(|exposes| exposes.events.as_deref())
            .is_some_and(|events| !events.is_empty())
    }

    pub fn subscribes(&self) -> bool {
        self.manifest
            .consumes
            .as_deref()
            .is_some_and(|consumes| !consumes.is_empty())
    }
}

/// What a service *is*, as a thing `caf dev` has to bring up and `guard` has to
/// route to.
///
/// Three values are derived from the manifest, and derived wherever the manifest
/// can speak:
///
/// | manifest says                            | kind     |
/// | ---------------------------------------- | -------- |
/// | `exposes.api`, empty `consumes`          | `api`    |
/// | `exposes.api` and a non-empty `consumes` | `both`   |
/// | no `exposes.api`, some event work        | `worker` |
///
/// The fourth row is a manifest that declares nothing, and there the vocabulary
/// is **two curated values** — `api` and `cli` — because that is the honest
/// count and one of them is not derivable at all:
///
/// | manifest says | kind | who says so |
/// | ------------- | ---- | ----------- |
/// | nothing, and the repository serves HTTP | `api` | a person, in `registry/index.yml`. guard: a gateway whose OpenAPI document has not been written yet. |
/// | nothing, and the repository is a binary | `cli` | a person, in `registry/index.yml`. caf: "caf is a binary, not a service: it exposes no HTTP surface and publishes no events." |
///
/// **Why curation and not derivation.** A manifest that declares no contract
/// surface has no way to say what it is. core's conventions (rule 3, "The rules
/// the schema cannot state") say such a repository "is a library or a spec
/// repo" — but neither of the two entries that match the shape is either, and
/// the manifests do not differ: both declare no `exposes`, both declare an empty
/// `consumes`, and `language` names a toolchain, not a shape. Propose a
/// derivation anyway — "a compiled language and no surface is a CLI" — and it
/// calls guard a binary and, since `BlockedBy::Library` arrived, cafaye-rb one
/// too. Three repositories, one rule, two wrong answers. The distinction is
/// real and it is not in the manifest, so the registry records the judgement and
/// [`check_kind`] holds it honest in both directions: for a manifest with no
/// surface **only** these two are admitted, and a `cli` that grows a document
/// fails the load.
///
/// The cost of this row is the cost of every curated fact in the registry: a
/// human said so, and a test is the only thing that can notice them being wrong.
/// There is one of those tests, `tests/kind.rs`, and it is this comment's
/// reason for being specific about which value is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Api,
    Worker,
    Both,
    /// A binary: installed and run rather than brought up and routed to. `caf`
    /// is the one in the fleet, and it is `api` on the wire nowhere — it was,
    /// before this variant existed, which is a value the vocabulary offered for
    /// a command and which a client reading it would have taken literally.
    Cli,
}

impl ServiceKind {
    pub fn all() -> &'static [ServiceKind] {
        &[
            ServiceKind::Api,
            ServiceKind::Worker,
            ServiceKind::Both,
            ServiceKind::Cli,
        ]
    }
}

impl std::fmt::Display for ServiceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            ServiceKind::Api => "api",
            ServiceKind::Worker => "worker",
            ServiceKind::Both => "both",
            ServiceKind::Cli => "cli",
        };
        f.write_str(text)
    }
}

/// The index: which services exist, and what the manifest cannot say about them.
///
/// `deny_unknown_fields` here too, for the same reason as on a manifest. An
/// index key nobody reads is a fact somebody believes they stated.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub services: BTreeMap<String, IndexEntry>,
    #[serde(default)]
    pub excluded: Vec<Excluded>,
}

/// The two registry-side facts, per service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexEntry {
    pub kind: ServiceKind,
    #[serde(default)]
    pub base_path: Option<String>,
}

/// A known cafaye repository that is deliberately not registered, and the check
/// that holds it back. See `registry/index.yml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Excluded {
    pub name: String,
    pub blocked_by: BlockedBy,
    pub reason: String,
    pub verify: Option<String>,
    pub lint: Option<String>,
}

/// Which check keeps a repository out of the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedBy {
    /// The repository's `cafaye.yml` does not validate against core's schema.
    Schema,
    /// The repository carries no `cafaye.yml` on master yet.
    NoManifest,
    /// The manifest validates, but the repository is not a service:
    /// `language: spec`.
    NotAService,
    /// The manifest validates and declares no contract surface at all, and the
    /// repository is something a client depends on rather than something it
    /// brings up: a library, a gem, a documentation site.
    ///
    /// The fourth value exists because `NotAService` cannot be stretched to
    /// cover this case. It means `language: spec`, which is a fact core's schema
    /// states and `every_exclusion_reason_is_still_true` can check; a gem is
    /// `language: ruby` and a Starlight site is `language: typescript`, and both
    /// record the judgement in their own files. What the two share is the shape
    /// — no `exposes`, no `consumes` — and the shape is not the fact. A service
    /// with the same shape is guard, which is registered.
    ///
    /// So this value says what `spec` cannot: **not something `caf dev` brings
    /// up.** Registration is a claim about starting a process; a documentation
    /// site and a shared gem are not processes anyone starts, and listing them
    /// beside the services would make `kind` mean two different things in one
    /// column.
    Library,
}

/// The loaded registry. Immutable once loaded, which is what lets a request
/// handler read it without a lock and what makes "the registry loaded" a
/// question with one answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    entries: Vec<ServiceEntry>,
}

impl Registry {
    /// Reads `registry/index.yml` and every `registry/services/*/cafaye.yml`, checks
    /// every entry against core's schema, and refuses to hand back a registry
    /// that is internally inconsistent.
    ///
    /// Every failure here is a bug in this repository or in a service's
    /// manifest, and every one of them is fatal to the load rather than to the
    /// entry: a registry that serves half its entries is a registry nobody can
    /// reason about. `/readyz` reports the failure and the process keeps running
    /// so the failure is observable rather than a crash loop.
    pub fn load(dir: &Path) -> Result<Registry, RegistryError> {
        let index = read_index(dir)?;

        if index.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(RegistryError::Invariant(format!(
                "registry index declares schemaVersion {} and this build reads {SUPPORTED_SCHEMA_VERSION}; \
                 a pantry that guessed at a newer index would serve entries it did not understand",
                index.schema_version
            )));
        }

        let mut entries = Vec::new();

        for path in manifest_paths(dir) {
            let manifest = manifest::read(&path).map_err(|source| RegistryError::Manifest {
                path: path.clone(),
                source,
            })?;

            let entry = entry_for(&manifest, &index, &path)?;
            entries.push(entry);
        }

        if entries.is_empty() {
            return Err(RegistryError::Invariant(format!(
                "{} holds no manifests; an empty registry answers every question with \"no\" and \
                 that is indistinguishable from a platform with no services",
                dir.join("services").display()
            )));
        }

        // Every manifest has an index row and every index row has a manifest. An
        // orphan in either direction is a curation mistake, and both would
        // otherwise be invisible: the orphan manifest is served with no kind and
        // the orphan row is a service the registry claims and cannot describe.
        for name in index.services.keys() {
            if !entries.iter().any(|entry| entry.name() == name) {
                return Err(RegistryError::Invariant(format!(
                    "registry/index.yml lists {name} but there is no \
                     services/{name}/{MANIFEST_FILE_NAME} under {}",
                    dir.display()
                )));
            }
        }

        entries.sort_by(|left, right| left.name().cmp(right.name()));

        Ok(Registry { entries })
    }

    pub fn entries(&self) -> &[ServiceEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&ServiceEntry> {
        self.entries.iter().find(|entry| entry.name() == name)
    }

    /// The entries a filter matches, sorted by name. See [`crate::filter`].
    pub fn query(&self, filter: &crate::filter::Filter) -> Vec<&ServiceEntry> {
        let mut matched: Vec<&ServiceEntry> = self
            .entries
            .iter()
            .filter(|entry| filter.matches(entry))
            .collect();
        matched.sort_by(|left, right| left.name().cmp(right.name()));
        matched
    }

    /// One page of a filtered query. See [`crate::filter::Page`].
    pub fn page<'a>(
        &'a self,
        filter: &crate::filter::Filter,
        page: &crate::filter::Page,
    ) -> Result<PageOfEntries<'a>, crate::filter::PageError> {
        let matched = self.query(filter);
        let start = page.offset(matched.len())?;

        let limit = page.limit.min(matched.len() - start);
        let next_cursor = (start + limit < matched.len()).then(|| page.cursor_for(start + limit));

        Ok(PageOfEntries {
            items: matched[start..start + limit].to_vec(),
            has_more: next_cursor.is_some(),
            next_cursor,
        })
    }
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOfEntries<'a> {
    pub items: Vec<&'a ServiceEntry>,
    pub has_more: bool,
    /// Opaque to the client, as core's conventions require: the encoding may
    /// change without notice.
    pub next_cursor: Option<String>,
}

/// Reads `registry/index.yml`.
pub fn read_index(dir: &Path) -> Result<RegistryIndex, RegistryError> {
    let path = dir.join("index.yml");
    let text = std::fs::read_to_string(&path).map_err(|source| RegistryError::Index {
        path: path.clone(),
        message: format!("cannot be read: {source}"),
    })?;

    serde_yaml::from_str(&text).map_err(|error| RegistryError::Index {
        path,
        message: format!("is not a registry index: {error}"),
    })
}

/// The index schema this build reads. Bumping it is a breaking change to
/// `registry/index.yml`, which is a pantry-internal file and not core's.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Joins one parsed manifest to its index row, refusing every inconsistency.
///
/// The order of the checks is the order of what a reader needs to be told first:
/// the file is named for the service it declares, the service is one pantry can
/// register, the index has a row for it, and the row's two facts survive the
/// manifest. A load that fails at the first one leaves the later questions
/// unasked, which is right — they are not answerable yet.
fn entry_for(
    manifest: &Manifest,
    index: &RegistryIndex,
    path: &Path,
) -> Result<ServiceEntry, RegistryError> {
    let expected_name = path
        .parent()
        .and_then(|service| service.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if manifest.name != expected_name {
        return Err(RegistryError::Invariant(format!(
            "{} declares name {:?}, so its directory is misnamed: a registry entry is \
             services/<name>/cafaye.yml, and a mismatch means one of the two is wrong",
            path.display(),
            manifest.name
        )));
    }

    // A specification repository is not a service. core's schema defines
    // `language: spec` as "specification-only repositories (core itself,
    // contract-test fixtures)", and nothing routes to a specification or depends
    // on one — registering core here would make the registry claim a namespace
    // for the substrate.
    if manifest.language == Language::Spec {
        return Err(RegistryError::Invariant(format!(
            "{} declares `language: spec`, which core defines for specification-only \
             repositories. It belongs in this file's `excluded` list with a reason, not in \
             the registry: no caller routes to a specification.",
            path.display()
        )));
    }

    let index_entry = index.services.get(&manifest.name).ok_or_else(|| {
        RegistryError::Invariant(format!(
            "{} is not in registry/index.yml. A manifest is only a registry entry once the \
             index says which kind it is and what its base path is — copy it into \
             services/ and add the row in the same commit.",
            path.display()
        ))
    })?;

    // A constraint the resolver cannot read is not a constraint. Failing the
    // load here is what lets `ServiceEntry::core_constraint` be infallible, and
    // therefore what lets a request handler not re-parse on every filter.
    Constraint::parse(&manifest.core).map_err(|error| {
        RegistryError::Invariant(format!(
            "{} declares core: {:?}, which this build cannot resolve: {error}",
            path.display(),
            manifest.core
        ))
    })?;

    if let Some(base_path) = index_entry.base_path.as_deref()
        && !base_path.starts_with('/')
    {
        return Err(RegistryError::Invariant(format!(
            "{}.base_path is {base_path:?}; a base path is a URL path prefix and starts with `/`",
            manifest.name
        )));
    }

    let entry = ServiceEntry {
        manifest: manifest.clone(),
        kind: index_entry.kind,
        base_path: index_entry.base_path.clone(),
    };

    check_kind(&entry)?;

    Ok(entry)
}

/// The kind rules from the [`ServiceKind`] table, as a check rather than as
/// documentation. Called once per entry at load.
///
/// Every arm is exhaustive over the manifest's surface, because a value this
/// function does not name is a value it accepts: `both` was admitted here for a
/// manifest with no surface for three releases, and the only thing that rejected
/// it was a test in another file. The curated arm admits exactly two values and
/// says what they are, so a fifth value in the enum cannot be added without
/// somebody deciding what it means for a manifest that says nothing.
fn check_kind(entry: &ServiceEntry) -> Result<(), RegistryError> {
    let name = entry.name().to_string();
    let serves = entry.serves_http();
    let works = entry.publishes_events() || entry.subscribes();

    if serves && !matches!(entry.kind, ServiceKind::Api | ServiceKind::Both) {
        return Err(RegistryError::Invariant(format!(
            "{name} declares exposes.api, so its kind must be `api` or `both`, not `{}`",
            entry.kind
        )));
    }
    if !serves && works && entry.kind != ServiceKind::Worker {
        return Err(RegistryError::Invariant(format!(
            "{name} declares no api surface but does declare event work, so its kind is \
             `worker`, not `{}`",
            entry.kind
        )));
    }
    if !serves && !works && !matches!(entry.kind, ServiceKind::Api | ServiceKind::Cli) {
        return Err(RegistryError::Invariant(format!(
            "{name} declares no contract surface at all, so its kind is curated and the only \
             two curated values are `api` — a service that serves HTTP and has not published \
             its document yet, which is guard — and `cli` — a binary, which is caf. Not \
             `{}`: there is no manifest fact here for it to be.",
            entry.kind
        )));
    }
    // The reverse of the arm above, and the reason `cli` is checked against more
    // than its manifest: a binary serves no path, so a base path on its row is
    // a prefix nobody published. Every other entry's `basePath` is derived from
    // a document, so this value cannot be derived from one — it can only be
    // invented, and `tests/drift.rs` would then have nothing to compare it to.
    if entry.kind == ServiceKind::Cli && entry.base_path.is_some() {
        return Err(RegistryError::Invariant(format!(
            "{name} is a `cli`, which publishes no OpenAPI document, so its basePath must be \
             null: {:?} is a path a binary does not serve",
            entry.base_path
        )));
    }

    Ok(())
}

/// Everything that can stop the registry from loading.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry index {path} {message}")]
    Index { path: PathBuf, message: String },

    #[error("{path}: {source}")]
    Manifest {
        path: PathBuf,
        source: manifest::ManifestError,
    },

    #[error("{0}")]
    Invariant(String),
}
