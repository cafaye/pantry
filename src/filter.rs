//! Query parameters: what a caller may ask, and what it gets for asking wrong.
//!
//! Three filters and two paging parameters, and both halves are closed. A
//! filter value outside the vocabulary is a 400 that lists the vocabulary,
//! because an empty list in reply to `?kind=database` is indistinguishable from
//! a registry with no services — and a caller cannot tell a typo from a fact.
//! An unknown parameter is also a 400: if a future pantry adds `?runtime=` and
//! an older one ignores it, the caller gets the whole list and believes it
//! asked a question.

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::contract::{Constraint, ConstraintError};
use crate::manifest::Language;
use crate::registry::ServiceKind;

/// core: `limit` defaults to 25 and is capped at 100.
pub const DEFAULT_LIMIT: usize = 25;
pub const MAX_LIMIT: usize = 100;

/// The filters, all of them optional. `Default` is "no filter", which is also
/// what an empty query string means.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub kind: Option<ServiceKind>,
    pub language: Option<Language>,
    /// A constraint on the entry's own `core` constraint. The two are matched by
    /// range intersection, not equality — see [`crate::contract::Constraint::intersects`].
    pub contract: Option<Constraint>,
}

impl Filter {
    /// Reads the query string. Unknown parameters are rejected; `limit` and
    /// `cursor` are paging, not filtering, and are read by [`Page::from_query`].
    pub fn from_query(query: &HashMap<String, String>) -> Result<Filter, FilterError> {
        let mut filter = Filter::default();

        for (key, value) in query {
            match key.as_str() {
                "kind" => filter.kind = Some(ServiceKind::from_filter_value(value)?),
                "language" => filter.language = Some(Language::from_filter_value(value)?),
                "contract" => {
                    filter.contract =
                        Some(Constraint::parse(non_empty(key, value)?).map_err(|source| {
                            FilterError::BadConstraint {
                                value: value.clone(),
                                source,
                            }
                        })?)
                }
                // Paging, not filtering. `Page::from_query` reads these; a
                // caller asking for both in one request is normal.
                "limit" | "cursor" => {}
                other => {
                    return Err(FilterError::UnknownParameter {
                        parameter: other.to_string(),
                        value: value.clone(),
                    })
                }
            }
        }

        Ok(filter)
    }

    /// Whether an entry passes every filter that is set. Filters are ANDed:
    /// two filters combined narrow, and a caller asking two questions is
    /// asking for both answers.
    pub fn matches(&self, entry: &crate::registry::ServiceEntry) -> bool {
        if let Some(kind) = self.kind
            && kind != entry.kind
        {
            return false;
        }
        if let Some(language) = self.language
            && language != entry.language()
        {
            return false;
        }
        if let Some(constraint) = self.contract
            && !constraint.intersects(&entry.core_constraint())
        {
            return false;
        }
        true
    }
}

impl ServiceKind {
    fn from_filter_value(value: &str) -> Result<ServiceKind, FilterError> {
        ServiceKind::all()
            .iter()
            .find(|kind| kind.to_string() == value)
            .copied()
            .ok_or_else(|| FilterError::NotInVocabulary {
                parameter: "kind".to_string(),
                value: value.to_string(),
                allowed: ServiceKind::all()
                    .iter()
                    .map(|kind| kind.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            })
    }
}

impl Language {
    fn from_filter_value(value: &str) -> Result<Language, FilterError> {
        Language::service_languages()
            .iter()
            .find(|language| language.to_string() == value)
            .copied()
            .ok_or_else(|| FilterError::NotInVocabulary {
                parameter: "language".to_string(),
                value: value.to_string(),
                // `spec` is deliberately absent from this list: it is a
                // specification, not a service, so no registered entry can ever
                // have it. Saying so beats returning an empty list for a
                // language that cannot occur.
                allowed: Language::service_languages()
                    .iter()
                    .map(|language| language.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            })
    }
}

/// Paging, as core's conventions define it: `?limit=&cursor=&order=`, opaque
/// base64url cursor, `data` always an array, `page.next_cursor` null on the last
/// page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub limit: usize,
    pub cursor: Option<String>,
}

impl Default for Page {
    fn default() -> Self {
        Page {
            limit: DEFAULT_LIMIT,
            cursor: None,
        }
    }
}

impl Page {
    pub fn new(limit: usize, cursor: Option<String>) -> Result<Page, PageError> {
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(PageError::BadLimit(limit));
        }
        if let Some(cursor) = cursor.as_deref()
            && cursor.is_empty()
        {
            return Err(PageError::BadCursor(String::new()));
        }

        Ok(Page { limit, cursor })
    }

    pub fn from_query(query: &HashMap<String, String>) -> Result<Page, PageError> {
        let limit = match query.get("limit") {
            None => DEFAULT_LIMIT,
            Some(raw) => {
                // A non-numeric limit is a client error, not a default. Reading
                // it as "25" would answer a question nobody asked.
                if raw.trim().is_empty() {
                    return Err(PageError::BadLimitValue(raw.clone()));
                }
                raw.parse::<usize>().map_err(|_| PageError::BadLimitValue(raw.clone()))?
            }
        };

        Page::new(limit, query.get("cursor").cloned())
    }

    /// The index the cursor points at.
    ///
    /// The cursor is base64url of an index, so it is opaque on the wire and
    /// stable in a log. An undecodable cursor is rejected rather than treated as
    /// page one: silently restarting is how a caller loses rows without noticing.
    pub fn offset(&self, length: usize) -> Result<usize, PageError> {
        let Some(cursor) = self.cursor.as_deref() else {
            return Ok(0);
        };

        let decoded = URL_SAFE_NO_PAD
            .decode(cursor)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| PageError::BadCursor(cursor.to_string()))?;

        let offset: usize = decoded
            .parse()
            .map_err(|_| PageError::BadCursor(cursor.to_string()))?;

        if offset > length {
            return Err(PageError::PastEnd { length });
        }

        Ok(offset)
    }

    /// The cursor that resumes at `offset`.
    pub fn cursor_for(&self, offset: usize) -> String {
        URL_SAFE_NO_PAD.encode(offset.to_string())
    }
}

fn non_empty<'a>(parameter: &str, value: &'a str) -> Result<&'a str, FilterError> {
    if value.trim().is_empty() {
        return Err(FilterError::EmptyValue {
            parameter: parameter.to_string(),
        });
    }
    Ok(value)
}

/// Why a query was refused. Each message names the parameter, the value that
/// was sent, and what was allowed, because a client author reading it once
/// should not need the source.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FilterError {
    #[error(
        "{parameter}={value:?} is not a filter; the filters are kind, language and \
         contract, and limit/cursor are the paging parameters"
    )]
    UnknownParameter { parameter: String, value: String },

    #[error("{parameter} is empty; that is not the same as not asking for the filter")]
    EmptyValue { parameter: String },

    #[error("{parameter}={value:?} is not one of: {allowed}")]
    NotInVocabulary {
        parameter: String,
        value: String,
        allowed: String,
    },

    #[error("contract={value:?} is not a cafaye constraint: {source}")]
    BadConstraint { value: String, source: ConstraintError },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PageError {
    #[error("limit must be between 1 and {MAX_LIMIT}, got {0}")]
    BadLimit(usize),

    #[error("limit={0:?} is not a whole number of services")]
    BadLimitValue(String),

    #[error("cursor {0:?} is not one this service issued")]
    BadCursor(String),

    #[error("cursor points past the end of a list of {length}")]
    PastEnd { length: usize },
}
