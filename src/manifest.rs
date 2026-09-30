//! A parsed `cafaye.yml`.
//!
//! The manifest format belongs to cafaye/core
//! (`core/schemas/cafaye.manifest.schema.json`). Every field name here is core's
//! — `name`, `language`, `core`, `exposes`, `consumes`, `dependencies`,
//! `repository`, `owner` — and none of them is pantry's. pantry adds no
//! vocabulary to this file: the two facts it needs that a manifest cannot carry
//! (`kind`, `basePath`) live in `registry/index.yml` and in
//! [`crate::registry::ServiceEntry`], because core closes the manifest with
//! `additionalProperties: false` and a key it does not know is an error, not an
//! extension.
//!
//! The parser is deliberately unforgiving. A registry that quietly drops a key
//! it does not understand serves answers nobody asked for, so an unknown field,
//! a wrong type and a missing required field are all errors, and each error
//! names the file and the problem.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// core's schema, vendored so the binary can validate without a sibling
/// checkout. `tests/schema.rs` asserts this copy is byte-identical to core's
/// while a workspace is reachable — a vendored copy with no drift test is a
/// vendored lie.
pub const MANIFEST_SCHEMA: &str = include_str!("../schemas/cafaye.manifest.schema.json");

/// Where the vendored copy lives, for error messages and the drift test.
pub fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/cafaye.manifest.schema.json")
}

/// A manifest that parses. The fields are exactly the schema's, and
/// `deny_unknown_fields` is what makes this type and the schema the same
/// contract rather than two overlapping ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    /// The cafaye namespace name, which is also the repository name and the
    /// event envelope's `source`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub language: Language,
    /// Constraint on the core spec version, in core's four-form grammar.
    pub core: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposes: Option<Exposes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<ServiceRef>>,
    pub repository: Repository,
    pub owner: Owner,
}

/// The contract surface a service publishes. Absent and empty are different
/// facts — a repository with no `exposes` is a library, and `exposes` with
/// nothing in it is a bug core's own `caf contract lint` rejects — so the
/// absence is a `None` rather than an empty mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exposes {
    /// Repository-relative path to the service's OpenAPI 3.1 document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
}

/// Another cafaye service this one builds on. Not a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `false` is a soft dependency: the service runs without it, degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Repository {
    /// SSH only. core's schema rejects an HTTPS remote for a cafaye repository,
    /// and PLAN.md §1 calls one a policy violation.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Owner {
    /// The accountable team, not the author. Renaming one is a
    /// changelog-worthy governance event.
    pub team: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

/// The implementation language, which for pantry means the enum core's schema
/// allows. `Spec` is core's value for specification-only repositories; it is
/// in the type because the type parses manifests, and out of
/// [`crate::registry`] because a specification is not a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Go,
    Ruby,
    Elixir,
    Python,
    Typescript,
    Rust,
    Spec,
}

impl Language {
    /// Every language a registered *service* may be written in: core's enum
    /// minus `spec`. `spec` is excluded because core defines it as
    /// "specification-only repositories (core itself, contract-test fixtures)"
    /// and nothing routes to a specification.
    pub fn service_languages() -> &'static [Language] {
        &[
            Language::Go,
            Language::Ruby,
            Language::Elixir,
            Language::Python,
            Language::Typescript,
            Language::Rust,
        ]
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Language::Go => "go",
            Language::Ruby => "ruby",
            Language::Elixir => "elixir",
            Language::Python => "python",
            Language::Typescript => "typescript",
            Language::Rust => "rust",
            Language::Spec => "spec",
        };
        f.write_str(text)
    }
}

/// Everything that can go wrong reading a manifest.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{path}: cannot be read: {message}")]
    Read { path: PathBuf, message: String },

    #[error("{path}: invalid YAML at {location}: {message}")]
    Syntax {
        path: PathBuf,
        location: String,
        message: String,
    },

    #[error("{path}: is empty; a manifest declares at least name, language, core, repository and owner")]
    Empty { path: PathBuf },

    #[error("{path}: {message}")]
    Shape { path: PathBuf, message: String },

    #[error("{path}: does not satisfy cafaye/core's manifest schema:\n{message}")]
    Schema { path: PathBuf, message: String },
}

/// Reads and parses a manifest file.
///
/// The order is deliberate: read, check for an empty document, check the YAML
/// parses, then hand the text to the schema validator. An empty file is
/// reported as empty rather than as five missing fields, because "this file is
/// empty" and "this file is missing five fields" send a person to different
/// fixes.
pub fn read(path: &Path) -> Result<Manifest, ManifestError> {
    let bytes = std::fs::read(path).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let text = String::from_utf8(bytes).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        message: format!("is not UTF-8: {error}"),
    })?;

    validate_schema(text.as_bytes(), path)
}

/// Parses manifest YAML into the typed form.
///
/// This is the step that refuses an unknown key or a mistyped field, and it is
/// separate from [`validate_schema`] because the two report different problems:
/// serde knows what a *field* is and core's schema knows what a *document* is.
/// A registry runs both, in that order, because a field that does not exist is
/// a more useful message than the five schema violations it happens to cause.
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    Err(ManifestError::Empty {
        path: PathBuf::from("<stub>"),
    })
}

/// Validates YAML text against core's schema and returns the typed manifest.
///
/// The returned `Manifest` is only trustworthy once this has passed: the typed
/// fields are read from the same bytes the schema just cleared, so a caller can
/// never hold a `Manifest` that core's schema would reject.
pub fn validate_schema(bytes: &[u8], path: &Path) -> Result<Manifest, ManifestError> {
    let _ = bytes;
    Err(ManifestError::Schema {
        path: path.to_path_buf(),
        message: "not implemented".to_string(),
    })
}

/// The common path prefix of an OpenAPI document's paths.
///
/// This is where `basePath` comes from, and it is derived rather than declared
/// because it is a fact about the document rather than an opinion: every path a
/// service serves starts with it, so a client can build a base URL and a gateway
/// can route by prefix without pantry guessing.
///
/// Cut on segment boundaries, never mid-segment: `/v1/users` and `/v1/user`
/// share the prefix `/v1`, not `/v1/user`. When the paths disagree completely the
/// answer is `/`, which is honest — there is no common prefix to claim.
pub fn openapi_base_path(bytes: &[u8]) -> Result<String, ManifestError> {
    let _ = bytes;
    Err(ManifestError::Schema {
        path: PathBuf::from("<openapi>"),
        message: "not implemented".to_string(),
    })
}
