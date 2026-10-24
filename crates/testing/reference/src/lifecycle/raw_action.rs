//! Frozen raw action numbering, independent of state, admission, and environment assembly.
use super::schema::{Api, Autocommit, Shape};
use super::target_selector::TargetSelector;

/// Number of raw action slots in the adopted lifecycle profile.
pub const RAW_ACTION_COUNT: u16 = 1533;

/// Prepare-only dimensions in the raw alphabet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrepareChoice {
    /// Caller-local prepared slot, zero or one.
    pub slot: u8,
    /// Select or DML metadata shape.
    pub shape: Shape,
    /// Parameter count, zero or one.
    pub parameter_count: u8,
}

/// One raw coverage slot before target selection or full input assembly.
/// Fields unused by its API must be `None`; equivalent effects do not merge slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawAction {
    /// Frozen API family, in numeric tag order.
    pub api: Api,
    /// Actor zero or one; recovery alone uses 255.
    pub actor: u8,
    /// Required only by the statement/result/context selector families.
    pub selector: Option<TargetSelector>,
    /// Required only by Prepare.
    pub prepare: Option<PrepareChoice>,
    /// Required only by SetAutocommit.
    pub autocommit: Option<Autocommit>,
    /// Exact family-local outcome tag, including distinct Count0 and Count2.
    pub outcome: u8,
}

/// A malformed raw index or descriptor, not an API rejection or environment exclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawActionError {
    /// The requested index lies outside zero through 1532.
    OutOfRange {
        /// Original out-of-range index, without modulo reduction.
        index: u16,
    },
    /// The descriptor has a missing, unused, or out-of-range family field.
    InvalidFields,
}

#[derive(Clone, Copy)]
enum Dimensions {
    Prepare,
    Target,
    Actor,
    Autocommit,
    Recovery,
}
struct Family {
    api: Api,
    first: u16,
    count: u16,
    outcomes: u16,
    dimensions: Dimensions,
}
// Slowest dimensions precede fastest ones: family, actor, slot/selector, shape, N, outcome.
// Resume's outcome is 2*next_work+cleanup; its ten tags retain that exact order.
const FAMILIES: [Family; 21] = [
    Family {
        api: Api::Prepare,
        first: 0,
        count: 48,
        outcomes: 3,
        dimensions: Dimensions::Prepare,
    },
    Family {
        api: Api::BindAll,
        first: 48,
        count: 352,
        outcomes: 11,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::Execute,
        first: 400,
        count: 352,
        outcomes: 11,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::Fetch,
        first: 752,
        count: 320,
        outcomes: 10,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::CloseResult,
        first: 1072,
        count: 96,
        outcomes: 3,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::CloseStatement,
        first: 1168,
        count: 96,
        outcomes: 3,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::Begin,
        first: 1264,
        count: 6,
        outcomes: 3,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::SetAutocommit,
        first: 1270,
        count: 12,
        outcomes: 3,
        dimensions: Dimensions::Autocommit,
    },
    Family {
        api: Api::CommitAtomic,
        first: 1282,
        count: 20,
        outcomes: 10,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::Rollback,
        first: 1302,
        count: 8,
        outcomes: 4,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::CloseSession,
        first: 1310,
        count: 8,
        outcomes: 4,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::ReopenSession,
        first: 1318,
        count: 4,
        outcomes: 2,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::InspectStatement,
        first: 1322,
        count: 32,
        outcomes: 1,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::InspectSession,
        first: 1354,
        count: 2,
        outcomes: 1,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::SignalContext,
        first: 1356,
        count: 64,
        outcomes: 2,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::ParkFetch,
        first: 1420,
        count: 64,
        outcomes: 2,
        dimensions: Dimensions::Target,
    },
    Family {
        api: Api::ResumeFetch,
        first: 1484,
        count: 20,
        outcomes: 10,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::BeginCommit,
        first: 1504,
        count: 4,
        outcomes: 2,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::CommitFact,
        first: 1508,
        count: 10,
        outcomes: 5,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::FinishCommit,
        first: 1518,
        count: 10,
        outcomes: 5,
        dimensions: Dimensions::Actor,
    },
    Family {
        api: Api::RecoveryConfirmedFreshEngine,
        first: 1528,
        count: 5,
        outcomes: 5,
        dimensions: Dimensions::Recovery,
    },
];

/// Decode one raw index without consulting state or allocating.
/// The output describes coverage dimensions only, not a complete `ResolvedInput`.
///
/// # Errors
///
/// Returns `OutOfRange` for indices at or above `RAW_ACTION_COUNT`, preserving the index.
pub fn action_at(index: u16) -> Result<RawAction, RawActionError> {
    let family = FAMILIES
        .iter()
        .find(|f| index >= f.first && index < f.first + f.count)
        .ok_or(RawActionError::OutOfRange { index })?;
    let local = index - family.first;
    let outcome = (local % family.outcomes) as u8;
    let mut outer = local / family.outcomes;
    let mut action = RawAction {
        api: family.api,
        actor: 0,
        selector: None,
        prepare: None,
        autocommit: None,
        outcome,
    };
    match family.dimensions {
        Dimensions::Prepare => {
            let parameter_count = (outer % 2) as u8;
            outer /= 2;
            let shape = if outer.is_multiple_of(2) {
                Shape::Select
            } else {
                Shape::Dml
            };
            outer /= 2;
            let slot = (outer % 2) as u8;
            outer /= 2;
            action.prepare = Some(PrepareChoice {
                slot,
                shape,
                parameter_count,
            });
            action.actor = outer as u8;
        }
        Dimensions::Target => {
            let selector = (outer % 16) as u8;
            action.selector = Some(if selector < 6 {
                TargetSelector::Issued(selector)
            } else {
                TargetSelector::FrozenUnissued(selector - 6)
            });
            action.actor = (outer / 16) as u8;
        }
        Dimensions::Actor => action.actor = outer as u8,
        Dimensions::Autocommit => {
            action.autocommit = Some(if outer.is_multiple_of(2) {
                Autocommit::On
            } else {
                Autocommit::Off
            });
            action.actor = (outer / 2) as u8;
        }
        Dimensions::Recovery => action.actor = 255,
    }
    Ok(action)
}

/// Return the exact raw index for a canonical descriptor, rejecting unused fields.
/// This operation neither classifies an API outcome nor merges equivalent actions.
///
/// # Errors
///
/// Returns `InvalidFields` when any field is absent, unused, or outside its family range.
pub fn index_of(action: &RawAction) -> Result<u16, RawActionError> {
    let family = FAMILIES
        .iter()
        .find(|f| f.api == action.api)
        .ok_or(RawActionError::InvalidFields)?;
    if u16::from(action.outcome) >= family.outcomes {
        return Err(RawActionError::InvalidFields);
    }
    let recovery = matches!(family.dimensions, Dimensions::Recovery);
    if (recovery && action.actor != 255) || (!recovery && action.actor > 1) {
        return Err(RawActionError::InvalidFields);
    }
    let outer = match family.dimensions {
        Dimensions::Prepare => {
            if action.selector.is_some() || action.autocommit.is_some() {
                return Err(RawActionError::InvalidFields);
            }
            let prepare = action.prepare.ok_or(RawActionError::InvalidFields)?;
            if prepare.slot > 1 || prepare.parameter_count > 1 {
                return Err(RawActionError::InvalidFields);
            }
            ((u16::from(action.actor) * 2 + u16::from(prepare.slot)) * 2 + prepare.shape as u16) * 2
                + u16::from(prepare.parameter_count)
        }
        Dimensions::Target => {
            if action.prepare.is_some() || action.autocommit.is_some() {
                return Err(RawActionError::InvalidFields);
            }
            let selector = match action.selector {
                Some(TargetSelector::Issued(n)) if n < 6 => u16::from(n),
                Some(TargetSelector::FrozenUnissued(n)) if n < 10 => 6 + u16::from(n),
                _ => return Err(RawActionError::InvalidFields),
            };
            u16::from(action.actor) * 16 + selector
        }
        Dimensions::Autocommit => {
            if action.selector.is_some() || action.prepare.is_some() {
                return Err(RawActionError::InvalidFields);
            }
            let mode = action.autocommit.ok_or(RawActionError::InvalidFields)?;
            u16::from(action.actor) * 2 + mode as u16
        }
        Dimensions::Actor | Dimensions::Recovery => {
            if action.selector.is_some() || action.prepare.is_some() || action.autocommit.is_some()
            {
                return Err(RawActionError::InvalidFields);
            }
            if recovery { 0 } else { u16::from(action.actor) }
        }
    };
    Ok(family.first + outer * family.outcomes + u16::from(action.outcome))
}
