//! The cafaye constraint grammar, ported from `caf/internal/contract`.
//!
//! There is one resolver in the platform and this is a copy of it, not a second
//! one. `caf contract resolve` already answers "does version X satisfy
//! constraint Y" for every other tool in cafaye; a registry that answered with
//! different bounds would be a place where two answers to the same question
//! exist, and the wrong one would ship a service.
//!
//! What is pantry's own is [`Constraint::intersects`]. A manifest holds a
//! *constraint*, not a version, so `GET /v1/services?contract=^0.2.0` cannot ask
//! whether a version is inside an entry's range — there is no version to ask
//! about. It asks whether the caller's range and the entry's range have a
//! version in common, which is what "does this filter match this service" means
//! for `caf deploy`.
//!
//! The grammar is core's four forms and nothing else
//! (`core/docs/manifest-conventions.md`, "Semver constraints"): `^` allows any
//! change to the left-most non-zero component, `~` pins the minor, `>=` is an
//! open floor, and a bare version is exact. `||`, x-ranges, hyphen ranges and
//! prereleases are rejected on purpose — core calls the grammar "deliberately
//! tiny" and full semver "a dependency and a footgun".

use std::cmp::Ordering;
use std::fmt;

/// A core spec version: three non-negative integers and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

/// The four forms, and no fifth. core's schema's `semverRange` pattern is the
/// authority on which spellings exist; this enum is what the resolver does with
/// each of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// `1.2.3` — that version and nothing else.
    Exact,
    /// `^1.2.3` — anything that does not change the left-most non-zero
    /// component.
    Caret,
    /// `~1.2.3` — the minor is pinned.
    Tilde,
    /// `>=1.2.3` — an open floor.
    Floor,
}

impl Operator {
    fn prefix(self) -> &'static str {
        match self {
            Operator::Caret => "^",
            Operator::Tilde => "~",
            Operator::Floor => ">=",
            Operator::Exact => "",
        }
    }
}

/// A parsed `core` field: an operator and the version it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constraint {
    operator: Operator,
    version: Version,
}

/// The interval a constraint admits.
///
/// `upper: None` is the only way to say "no ceiling", because `0.0.0` is a real
/// version and treating it as the bound would make `>=1.2.3` mean
/// `[1.2.3, 0.0.0)`, which is empty. `upper_inclusive` distinguishes an exact
/// version — the single point `[1.2.3, 1.2.3]` — from a caret's `[1.2.3, 2.0.0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub floor: Version,
    pub upper: Option<Version>,
    pub upper_inclusive: bool,
}

impl Interval {
    pub fn is_open(self) -> bool {
        self.upper.is_none()
    }
}

impl Version {
    /// `MAJOR.MINOR.PATCH`, no operator, no leading zeros.
    ///
    /// A leading zero is the interesting rejection: `01.2.3` reads like a
    /// version to a person and silently widens a range to anything that sorts
    /// the same way. caf rejects it, npm rejects it, so pantry does too.
    pub fn parse(text: &str) -> Result<Version, VersionError> {
        let components: Vec<&str> = text.split('.').collect();
        if components.len() != 3 {
            return Err(VersionError::NotAVersion(text.to_string()));
        }

        let mut numbers = [0u64; 3];
        for (slot, component) in numbers.iter_mut().zip(components) {
            *slot = component_number(component, text)?;
        }

        Ok(Version {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
        })
    }

    /// Numeric, component by component: `10.0.0` is newer than `9.0.0`, which a
    /// string comparison gets backwards.
    ///
    /// Named `compare` rather than `cmp` because clippy's `should_implement_trait`
    /// is right that `cmp` reads as `Ord::cmp`, and implementing `Ord` would
    /// claim a total order over versions that includes prereleases this grammar
    /// deliberately cannot express. This is the only ordering there is.
    pub fn compare(&self, other: &Version) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Constraint {
    /// Reads a constraint in core's grammar, and says what the grammar is when
    /// it says no — the message is what a person reads when they typed it wrong.
    pub fn parse(text: &str) -> Result<Constraint, ConstraintError> {
        let (operator, version_text) = match text.strip_prefix('^') {
            Some(rest) => (Operator::Caret, rest),
            None => match text.strip_prefix('~') {
                Some(rest) => (Operator::Tilde, rest),
                None => match text.strip_prefix(">=") {
                    Some(rest) => (Operator::Floor, rest),
                    None => (Operator::Exact, text),
                },
            },
        };

        // A second operator is not a version. `>1.2.3`, `<=1.2.3` and
        // `>= 1.2.3` all reach here with a character the components reject, and
        // `^1.0.0 || ^2.0.0` reaches it with the wrong number of components.
        // Both come out as the same one-sentence explanation of the grammar,
        // which is what a person needs and what a resolver must not guess at.
        let version =
            Version::parse(version_text).map_err(|source| ConstraintError::BadVersion {
                whole: text.to_string(),
                reason: source.reason().to_string(),
            })?;

        Ok(Constraint { operator, version })
    }

    pub fn operator(&self) -> Operator {
        self.operator
    }

    /// The version the constraint is written against.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The interval this constraint admits.
    pub fn interval(&self) -> Interval {
        match self.operator {
            // An exact version is a single point, which is why the upper bound
            // is inclusive here and nowhere else.
            Operator::Exact => Interval {
                floor: self.version,
                upper: Some(self.version),
                upper_inclusive: true,
            },
            Operator::Caret => Interval {
                floor: self.version,
                upper: Some(caret_ceiling(self.version)),
                upper_inclusive: false,
            },
            Operator::Tilde => Interval {
                floor: self.version,
                upper: Some(Version {
                    major: self.version.major,
                    minor: self.version.minor + 1,
                    patch: 0,
                }),
                upper_inclusive: false,
            },
            Operator::Floor => Interval {
                floor: self.version,
                upper: None,
                upper_inclusive: false,
            },
        }
    }

    /// Whether a version is inside the constraint. `caf contract resolve` prints
    /// the same answer.
    pub fn satisfies(&self, version: &Version) -> bool {
        if self.operator == Operator::Exact {
            return self.version.compare(version) == Ordering::Equal;
        }

        let interval = self.interval();
        if version.compare(&interval.floor) == Ordering::Less {
            return false;
        }
        match interval.upper {
            None => true,
            Some(upper) => version.compare(&upper) == Ordering::Less,
        }
    }

    /// Whether two constraints have a version in common.
    ///
    /// This is the filter semantic, and it is deliberately not a string
    /// comparison: `^0.2.0` and `^0.1.0` are both caret ranges, they touch at
    /// `0.2.0`, and neither contains the other — because before 1.0 the minor is
    /// the breaking surface. The intersection is `[max(floors), min(ceilings))`,
    /// with an exact constraint's ceiling counted as inclusive.
    pub fn intersects(&self, other: &Constraint) -> bool {
        let mine = self.interval();
        let theirs = other.interval();

        let floor = if mine.floor.compare(&theirs.floor) == Ordering::Less {
            theirs.floor
        } else {
            mine.floor
        };

        // The tighter of the two ceilings wins. When the ceilings are the same
        // version the interval is inclusive only if BOTH are, which is exactly
        // the case where both constraints are that one version.
        let (upper, upper_inclusive) = match (mine.upper, theirs.upper) {
            // Both open above: everything from the floor up is common.
            (None, None) => return true,
            // One open above: the other's ceiling decides, and so does its
            // inclusivity — `>=0.2.0` and `0.2.0` share exactly the point
            // 0.2.0, because an exact version's ceiling is inside its range.
            (None, Some(right)) => (Some(right), theirs.upper_inclusive),
            (Some(left), None) => (Some(left), mine.upper_inclusive),
            (Some(left), Some(right)) => match left.compare(&right) {
                // Whichever constraint supplies the tighter ceiling also
                // supplies its inclusivity: an exact version's ceiling is a
                // point that IS in the range, while a caret's is a version that
                // is NOT. Taking the tighter bound and the looser bound's
                // inclusivity would drop the last version of every range.
                Ordering::Less => (Some(left), mine.upper_inclusive),
                Ordering::Greater => (Some(right), theirs.upper_inclusive),
                Ordering::Equal => (Some(left), mine.upper_inclusive && theirs.upper_inclusive),
            },
        };

        let Some(upper) = upper else {
            return true;
        };

        match floor.compare(&upper) {
            Ordering::Less => true,
            Ordering::Equal => upper_inclusive,
            Ordering::Greater => false,
        }
    }

    /// The sentence `caf contract resolve` prints after the answer. On its own
    /// `no` sends a person back to the grammar; saying which edge of the range
    /// they are on sends them to the fix.
    pub fn rationale(&self, version: &Version) -> String {
        if self.operator == Operator::Exact {
            return if self.satisfies(version) {
                format!("{version} is exactly {}", self.version)
            } else {
                format!("{version} is not exactly {}", self.version)
            };
        }

        if self.operator == Operator::Floor {
            return if self.satisfies(version) {
                format!("{version} is at or above {}", self.version)
            } else {
                format!("{version} is below {}", self.version)
            };
        }

        let interval = self.interval();
        let Some(upper) = interval.upper else {
            return format!("{version} is at or above {}", self.version);
        };

        if self.satisfies(version) {
            format!("{version} is in [{}, {})", interval.floor, upper)
        } else {
            format!("{version} is not in [{}, {})", interval.floor, upper)
        }
    }
}

/// The inclusive ceiling only an exact constraint has. `intersects` reads it
/// off the [`Interval`] rather than off the version, because a bare `Version`
/// cannot say whether it was the ceiling of a point or of a range.
impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.operator.prefix(), self.version)
    }
}

/// A caret allows every change to the right of the left-most non-zero
/// component. That is what makes `^0.1.0` mean `[0.1.0, 0.2.0)` and not
/// `[0.1.0, 1.0.0)`, which core's conventions call out by name: before 1.0 the
/// minor *is* the breaking surface.
fn caret_ceiling(version: Version) -> Version {
    match (version.major, version.minor) {
        (major, _) if major > 0 => Version {
            major: major + 1,
            minor: 0,
            patch: 0,
        },
        (_, minor) if minor > 0 => Version {
            major: 0,
            minor: minor + 1,
            patch: 0,
        },
        _ => Version {
            major: 0,
            minor: 0,
            patch: version.patch + 1,
        },
    }
}

fn component_number(component: &str, whole: &str) -> Result<u64, VersionError> {
    if component.len() > 1 && component.starts_with('0') {
        return Err(VersionError::LeadingZero {
            component: component.to_string(),
            whole: whole.to_string(),
        });
    }

    component
        .parse::<u64>()
        .map_err(|_| VersionError::NotAVersion(whole.to_string()))
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VersionError {
    #[error(
        "{0} is not a core version: want MAJOR.MINOR.PATCH, with no prefix and no leading zeros"
    )]
    NotAVersion(String),

    #[error(
        "{component:?} in {whole:?} is not a number: a leading zero is a second spelling of the same version"
    )]
    LeadingZero { component: String, whole: String },
}

impl VersionError {
    /// The clause that goes inside "is not a cafaye core constraint (...)".
    /// Split out so the grammar sentence is written once and every rejection
    /// carries it — a message that only says why *this* string failed leaves a
    /// person to guess what would have worked.
    fn reason(&self) -> &'static str {
        match self {
            VersionError::NotAVersion(_) => "a version is three dot-separated integers",
            VersionError::LeadingZero { .. } => {
                "a leading zero is a second spelling of the same version"
            }
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConstraintError {
    #[error(
        "{0:?} is not a cafaye core constraint: want MAJOR.MINOR.PATCH, optionally \
         prefixed by ^ (compatible), ~ (pin the minor) or >= (floor)"
    )]
    NotAConstraint(String),

    #[error(
        "{whole:?} is not a cafaye core constraint ({reason}): want MAJOR.MINOR.PATCH, \
         optionally prefixed by ^ (compatible), ~ (pin the minor) or >= (floor)"
    )]
    BadVersion { whole: String, reason: String },
}
