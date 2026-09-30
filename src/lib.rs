//! pantry — the cafaye service registry.
//!
//! What a cafaye service *is*: its identity, its contract, and how to reach it,
//! read from one place so `caf dev`, `caf deploy` and a developer reading the
//! platform do not each answer from a hardcoded list.
//!
//! ## The three facts a consumer needs
//!
//! 1. **Identity** — name, language, repository and branch. All of it from the
//!    service's own `cafaye.yml`.
//! 2. **Contract** — the core spec version the service was written against, the
//!    OpenAPI document it publishes, the events it emits and consumes, and the
//!    services it depends on. Again from its own manifest.
//! 3. **Reachability** — whether it serves HTTP or runs as a worker, and the
//!    path prefix its API lives under. The first is `kind`, the second is
//!    `basePath`; both are in `registry/index.yml` because core's manifest schema
//!    closes with `additionalProperties: false` and has no room for them.
//!
//! ## Scope
//!
//! Official cafaye services only, curated in this repository. There is no
//! marketplace, no third-party submission flow, no registration webhook, no
//! plugin loader and no UI. Those are Phase 5, and building them early is how a
//! registry becomes an unaudited code-execution surface. pantry describes how to
//! reach a service; it never calls one, never polls its health, and never reads
//! another service's data.

pub mod contract;
pub mod filter;
pub mod http;
pub mod manifest;
pub mod pin;
pub mod problem;
pub mod registry;
pub mod view;

pub use contract::{Constraint, Operator, Version};
pub use filter::{Filter, Page};
pub use http::{AppState, ROUTES, router};
pub use manifest::{Language, MANIFEST_SCHEMA, Manifest};
pub use problem::Problem;
pub use registry::{Registry, ServiceEntry, ServiceKind, registry_dir};
pub use view::{ServiceList, ServiceView};
