//! The response shape `caf pantry` is generated from.
//!
//! One rule decides every key here: **a service object is the service's own
//! manifest, spelled exactly as the manifest spells it, plus `kind` and
//! `basePath`.** No renaming, no translation, no second vocabulary. A consumer
//! that already reads a `cafaye.yml` reads a pantry response with the same field
//! names, and a field that moves in core's schema moves here in the same
//! commit — because there is nothing to translate.
//!
//! The two added keys are registry facts rather than manifest fields, and they
//! are camelCase because the manifest key they sit beside is:
//!
//! | key                     | source |
//! | ----------------------- | ------ |
//! | `name`, `description`, `language`, `core`, `exposes`, `consumes`, `dependencies`, `repository`, `owner` | the service's own `cafaye.yml` |
//! | `kind`                  | `registry/index.yml` — a manifest cannot state it |
//! | `basePath`              | `registry/index.yml` — derived from the service's OpenAPI document, `null` when it publishes none |
//!
//! Two normalisations, both deliberate and both documented here because a
//! consumer can see them:
//!
//! * `exposes` is `null` when the manifest declares no surface — absent and
//!   empty are different facts and pantry keeps the difference.
//! * `consumes` and `dependencies` are `[]` when absent, so a client can
//!   iterate them without a null check. No consumer needs to tell "declared
//!   empty" from "absent", and both mean "none".
//!
//! The envelope around them is core's, spelled core's way and not renamed:
//! `data` for a collection, `page.next_cursor` / `page.has_more` for paging, and
//! RFC 9457 problem details for every failure.

use serde::Serialize;

use crate::registry::ServiceEntry;

/// One page of `GET /v1/services`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceList {
    pub data: Vec<ServiceView>,
    pub page: PageView,
}

/// core's paging envelope. `data` is always an array — empty rather than
/// absent — and `next_cursor` is null on the last page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageView {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// One registered service. See the module docs for the field-by-field
/// provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub language: String,
    pub kind: String,
    /// The service's constraint on the core spec version, in core's grammar.
    pub core: String,
    /// The common path prefix of the service's OpenAPI document, or `null` when
    /// it publishes none.
    pub base_path: Option<String>,
    /// `null` when the manifest declares no contract surface.
    pub exposes: Option<ExposesView>,
    pub consumes: Vec<String>,
    pub dependencies: Vec<DependencyView>,
    pub repository: RepositoryView,
    pub owner: OwnerView,
}

/// The service's published surface, with the manifest's own optionality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExposesView {
    /// Repository-relative path to the OpenAPI 3.1 document, or `null` when the
    /// service publishes events only.
    pub api: Option<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyView {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `false` is a soft dependency. `None` means the manifest did not say, and
    /// core's schema defaults it to `true` — pantry reports what the file says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryView {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerView {
    pub team: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

/// The body of `/healthz` and of a ready `/readyz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthView {
    pub status: &'static str,
    /// How many services are registered. On `/readyz` only: liveness is
    /// deliberately contentless, because an orchestrator that restarts on the
    /// content of `/healthz` restarts a process for the wrong reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<usize>,
}

impl ServiceEntry {
    /// The entry as a client sees it.
    pub fn to_view(&self) -> ServiceView {
        let manifest = &self.manifest;

        ServiceView {
            name: manifest.name.clone(),
            description: manifest.description.clone(),
            language: manifest.language.to_string(),
            kind: self.kind.to_string(),
            core: manifest.core.clone(),
            base_path: self.base_path.clone(),
            exposes: manifest.exposes.as_ref().map(|exposes| ExposesView {
                api: exposes.api.clone(),
                events: exposes.events.clone().unwrap_or_default(),
            }),
            consumes: manifest.consumes.clone().unwrap_or_default(),
            dependencies: manifest
                .dependencies
                .iter()
                .flatten()
                .map(|dependency| DependencyView {
                    name: dependency.name.clone(),
                    version: dependency.version.clone(),
                    required: dependency.required,
                })
                .collect(),
            repository: RepositoryView {
                url: manifest.repository.url.clone(),
                default_branch: manifest.repository.default_branch.clone(),
                visibility: manifest.repository.visibility.clone(),
            },
            owner: OwnerView {
                team: manifest.owner.team.clone(),
                contact: manifest.owner.contact.clone(),
            },
        }
    }
}
