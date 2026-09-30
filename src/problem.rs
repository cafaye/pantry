//! RFC 9457 problem details, with core's cafaye extensions.
//!
//! core's `docs/openapi-conventions.md` is explicit: every non-2xx response is
//! `application/problem+json` and "no service invents its own error body". So
//! the framework's empty-bodied 405 and its text 404 are both wrong here, and
//! [`crate::http`] installs its own fallbacks so nothing escapes without this
//! envelope.
//!
//! The field names are core's and are not renamed: `type` is the stable
//! `https://errors.cafaye.com/<code>` URI, `code` is the same slug, and
//! `trace_id` is always present and always matches the `X-Trace-Id` header —
//! support starts from that id.

use std::fmt;

use axum::http::Method;
use serde::Serialize;

/// The stable machine-readable contract for a failure.
pub const ERROR_BASE: &str = "https://errors.cafaye.com/";

/// A failure, in the shape every cafaye client already parses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: &'static str,
    pub status: u16,
    /// Specific to this occurrence. Clients do not parse it.
    pub detail: String,
    /// The request path this is about.
    pub instance: String,
    pub code: &'static str,
    /// Always equal to the `X-Trace-Id` response header.
    pub trace_id: String,
}

impl Problem {
    /// 404 — the resource is not there. Used for an unknown service name and an
    /// unknown route; both mean the same thing to a caller.
    pub fn not_found(instance: &str, detail: String, trace_id: &str) -> Problem {
        Problem::new("not_found", "Not found", 404, instance, detail, trace_id)
    }

    /// 422-family, used for a request a client got wrong: core's reserved
    /// `validation_failed`. Status 400 because core says "400 only for
    /// malformed syntax the client could not have known" — and a filter value
    /// outside the vocabulary is exactly that.
    pub fn validation_failed(instance: &str, detail: String, trace_id: &str) -> Problem {
        Problem::new(
            "validation_failed",
            "Validation failed",
            400,
            instance,
            detail,
            trace_id,
        )
    }

    /// 405 — the route exists and the method does not. `code` is
    /// `method_not_allowed`, which is not in core's reserved list; it is
    /// reserved here because a client reading `code` needs to branch on it, and
    /// core's own conventions say the list is where such codes go. Flagged in
    /// the README for the manager rather than guessed silently.
    pub fn method_not_allowed(instance: &str, method: &Method, trace_id: &str) -> Problem {
        Problem::new(
            "method_not_allowed",
            "Method not allowed",
            405,
            instance,
            format!("{instance} exists, but not for {method}"),
            trace_id,
        )
    }

    /// 503 — pantry is alive and cannot answer. What `/readyz` says when the
    /// registry did not load.
    pub fn unavailable(instance: &str, detail: String, trace_id: &str) -> Problem {
        Problem::new(
            "unavailable",
            "Service unavailable",
            503,
            instance,
            detail,
            trace_id,
        )
    }

    /// 500 — a bug in pantry. Reached only by a path the tests do not cover,
    /// which is why it says so in the detail rather than pretending to know.
    pub fn internal(instance: &str, detail: String, trace_id: &str) -> Problem {
        Problem::new("internal", "Internal error", 500, instance, detail, trace_id)
    }

    fn new(
        code: &'static str,
        title: &'static str,
        status: u16,
        instance: &str,
        detail: String,
        trace_id: &str,
    ) -> Problem {
        Problem {
            kind: format!("{ERROR_BASE}{code}"),
            title,
            status,
            detail,
            instance: instance.to_string(),
            code,
            trace_id: trace_id.to_string(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.status, self.code, self.detail)
    }
}

/// Renders a failure as the envelope core requires. `Problem` is the error
/// type every handler returns: there is no second error type that could be
/// rendered some other way, which is the mechanical reason "no service invents
/// its own error body" holds here.
impl std::error::Error for Problem {}
