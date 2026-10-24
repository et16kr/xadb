//! Independent literal oracle for DEV4's borrowed lexical/name front end.
//!
//! This module consumes flat observations and does not depend on SQL or foundation.
//! Expected semantics live in the separately frozen [`literals`] inventory.

pub mod literals;

use literals::{DiagnosticObservation, LexCase, LexObservation, NameCase, NameObservation};

/// Verdict from exact comparison with a complete frozen literal expectation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every observed field equals the independent literal expectation.
    Accepted,
    /// A field or terminal-completeness property disagrees with the fixture.
    Rejected,
}

/// Compare every token, span, diagnostic field and completion flag.
///
/// The case's constructor supplies the answer; this does not scan its source.
pub fn check_lex(case: &LexCase, observed: &LexObservation) -> Verdict {
    if observed.complete && case.expected.complete && observed == &case.expected {
        Verdict::Accepted
    } else {
        Verdict::Rejected
    }
}

/// Compare exact name bytes, metadata and diagnostics without normalizing input.
pub fn check_name(
    case: &NameCase,
    observed: &Result<NameObservation, DiagnosticObservation>,
) -> Verdict {
    if observed == &case.expected {
        Verdict::Accepted
    } else {
        Verdict::Rejected
    }
}
