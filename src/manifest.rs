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

    #[error(
        "{path}: is empty; a manifest declares at least name, language, core, repository and owner"
    )]
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
    if text.trim().is_empty() {
        return Err(ManifestError::Empty {
            path: PathBuf::from("<inline>"),
        });
    }

    // The two steps are separate on purpose. serde knows what a *field* is, so
    // it can say `unknown field "version"` and name every key a manifest may
    // declare; core's schema knows what a *document* is, so it catches the
    // pattern and cross-field rules serde has no idea exist. Running the field
    // check first means the most specific message wins, and running the schema
    // check first would report five violations of which none is the cause.
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|error| syntax_error(Path::new("<inline>"), &error))?;

    serde_yaml::from_value(value).map_err(|error| shape_error(Path::new("<inline>"), &error))
}

/// Validates YAML text against core's schema and returns the typed manifest.
///
/// The returned `Manifest` is only trustworthy once this has passed: the typed
/// fields are read from the same bytes the schema just cleared, so a caller can
/// never hold a `Manifest` that core's schema would reject. That is what lets
/// `/readyz` mean something — a registry that loaded has been validated, not
/// merely read.
pub fn validate_schema(bytes: &[u8], path: &Path) -> Result<Manifest, ManifestError> {
    let text = std::str::from_utf8(bytes).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        message: format!("is not UTF-8: {error}"),
    })?;

    if text.trim().is_empty() {
        return Err(ManifestError::Empty {
            path: path.to_path_buf(),
        });
    }

    // YAML to a JSON value, because the contract is a JSON Schema and a YAML
    // parser that had its own opinion about types would be a second contract to
    // keep in sync. caf does exactly this in `internal/contract/manifest.go`.
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|error| syntax_error(path, &error))?;
    let document = serde_json::to_value(&value).map_err(|error| ManifestError::Syntax {
        path: path.to_path_buf(),
        location: "unknown".to_string(),
        message: format!("is not representable as a JSON document: {error}"),
    })?;

    let schema: serde_json::Value =
        serde_json::from_str(MANIFEST_SCHEMA).expect("the vendored schema is valid JSON");
    let validator = jsonschema::validator_for(&schema)
        .expect("the vendored schema is a schema this validator understands");

    let errors: Vec<String> = validator
        .iter_errors(&document)
        .map(|error| explain_schema_error(&error))
        .collect();
    if !errors.is_empty() {
        return Err(ManifestError::Schema {
            path: path.to_path_buf(),
            message: errors.join("\n"),
        });
    }

    serde_yaml::from_value(value).map_err(|error| shape_error(path, &error))
}

/// A YAML syntax error, with the position serde_yaml knows and a message that
/// says what a manifest is. "Invalid YAML" on its own sends a person back to a
/// diff; a line number sends them to the line.
fn syntax_error(path: &Path, error: &serde_yaml::Error) -> ManifestError {
    ManifestError::Syntax {
        path: path.to_path_buf(),
        location: error
            .location()
            .map(|location| format!("line {}, column {}", location.line(), location.column()))
            .unwrap_or_else(|| "an unknown position".to_string()),
        message: error.to_string(),
    }
}

/// A field-level rejection, rewritten into something worth reading.
///
/// serde's message names the field and lists what was expected; what it does not
/// do is say what a cafaye manifest *is*. Both are in the message here, because
/// the person reading it may be looking at a file for the first time.
fn shape_error(path: &Path, error: &serde_yaml::Error) -> ManifestError {
    let raw = error.to_string();
    let mut message = raw.clone();

    if let Some(field) = quoted_after(&raw, "unknown field") {
        message = format!(
            "unknown field {field}; a cafaye.yml may declare only {}",
            DECLARABLE_FIELDS.join(", ")
        );
    } else if let Some(field) = quoted_after(&raw, "missing field") {
        message = format!(
            "missing required field {field}; a cafaye.yml must declare {}",
            DECLARABLE_FIELDS.join(", ")
        );
    } else if let Some(field) = quoted_after(&raw, "invalid type") {
        message = format!(
            "{field} has the wrong type; a manifest is a document, not a configuration of arbitrary values"
        );
    }

    ManifestError::Shape {
        path: path.to_path_buf(),
        message,
    }
}

/// Every top-level key core's schema allows, in the schema's own order. Used to
/// tell a person what a manifest may declare rather than only what it may not.
const DECLARABLE_FIELDS: &[&str] = &[
    "name",
    "description",
    "language",
    "core",
    "exposes",
    "consumes",
    "dependencies",
    "repository",
    "owner",
];

/// The first backticked token after `marker` in serde's message.
fn quoted_after(message: &str, marker: &str) -> Option<String> {
    quoted_after_in(message, marker, '`')
}

fn quoted_after_in(message: &str, marker: &str, quote: char) -> Option<String> {
    let rest = message.split(marker).nth(1)?;
    let start = rest.find(quote)? + 1;
    let end = rest[start..].find(quote)? + start;
    Some(rest[start..end].to_string())
}

/// Rewrites one schema violation into a message that says what to do.
///
/// Only the top level is rewritten. A nested violation — an unexpected key in
/// `repository`, say — keeps the schema's own wording, which already names the
/// path precisely and would only be muddied by a list of top-level fields. The
/// top-level case is the one a cafaye service author actually hits: the four
/// fields a pre-core draft carried (`version`, `languages`, `contracts`, `dev`)
/// are the single most common way to fail this schema.
fn explain_schema_error(error: &jsonschema::ValidationError<'_>) -> String {
    let raw = error.to_string();

    if error.instance_path().is_empty()
        && let Some(property) =
            quoted_after_in(&raw, "Additional properties are not allowed (", '\'')
    {
        return format!(
            "unknown field {property:?}; a cafaye.yml may declare only {}",
            DECLARABLE_FIELDS.join(", ")
        );
    }

    raw
}

/// The API version prefix an OpenAPI document's paths share — `/v1`.
///
/// This is where `basePath` comes from, and it is core's rule rather than
/// pantry's: `docs/openapi-conventions.md` says "every path is prefixed" with a
/// single `/vN`, that the prefix "is the API version", and that `caf contract
/// lint` will enforce it. A base path is what a client prepends to build a URL
/// and what a gateway routes by, so the version prefix is the only prefix with a
/// meaning.
///
/// Not the longest common path prefix, which looks equivalent and is not:
/// muse's document publishes exactly one path, `/v1/route`, and its longest
/// common prefix is `/v1/route` — a resource, not a base. A document that does
/// not put every path under one `/vN` breaks a core convention, and the answer
/// here is an error naming the rule rather than a prefix invented to paper over
/// it.
pub fn openapi_base_path(bytes: &[u8]) -> Result<String, ManifestError> {
    let document: serde_yaml::Value = serde_yaml::from_slice(bytes)
        .map_err(|error| syntax_error(Path::new("<openapi>"), &error))?;

    let paths = document
        .get("paths")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| ManifestError::Shape {
            path: PathBuf::from("<openapi>"),
            message: "has no `paths` object, so it describes no HTTP surface".to_string(),
        })?;

    // The two probes are infrastructure and are excluded by name. core's
    // conventions say so — billing's document leaves them out entirely, and
    // identity's lists them explicitly as infrastructure — and they must not
    // enter this derivation: they share no `/vN` prefix with the contract paths,
    // so including them would make every service's base path ambiguous. These
    // two names are the platform's, not pantry's; a third probe is a core
    // amendment rather than a pantry guess.
    let prefixes: Vec<&str> = paths
        .keys()
        .filter_map(|path| path.as_str())
        .filter(|path| !matches!(*path, "/healthz" | "/readyz"))
        .map(|path| path.trim_start_matches('/'))
        .filter_map(|path| path.split('/').next())
        .filter(|segment| is_version_prefix(segment))
        .collect();

    let unique: Vec<&str> = {
        let mut unique = prefixes.clone();
        unique.sort_unstable();
        unique.dedup();
        unique
    };

    match unique.as_slice() {
        [only] => Ok(format!("/{only}")),
        [] => Err(ManifestError::Shape {
            path: PathBuf::from("<openapi>"),
            message: "publishes no path under a /vN prefix. core's openapi-conventions say \
                      every path carries one, and a base path cannot be derived from a \
                      document that does not"
                .to_string(),
        }),
        several => Err(ManifestError::Shape {
            path: PathBuf::from("<openapi>"),
            message: format!(
                "publishes paths under {} prefixes at once. core's conventions allow a new \
                 prefix alongside the old one during a transition, but a registry has one \
                 basePath per service, so this document needs a decision rather than a \
                 derivation",
                several
                    .iter()
                    .map(|prefix| format!("/{prefix}"))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        }),
    }
}

/// `v` and at least one digit. Deliberately not a full semver check: the prefix
/// is an API version label and core's rule is about its shape in a path.
fn is_version_prefix(segment: &str) -> bool {
    segment.strip_prefix('v').is_some_and(|digits| {
        !digits.is_empty() && digits.chars().all(|digit| digit.is_ascii_digit())
    })
}
