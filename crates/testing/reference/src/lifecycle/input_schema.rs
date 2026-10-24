//! Raw resolved inputs, retaining malformed targets and negative environment vectors.
use super::schema::{
    Api, Deadline, Decision, FactKind, OwnerDisposition, OwnershipSummary, Records, ResourceKind,
};

/// Exact raw identity bytes, including intentionally invalid kind and reserved bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawIdentity(pub [u8; 16]);

/// One bounded raw binding value; invalid UTF-8 remains an input, not a decode failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BindValue {
    /// Utf8=0, Int64=1, TypedNullUtf8=2; structural decoder rejects other tags.
    pub tag: u8,
    /// Exact payload length, at most eight bytes in this exploration profile.
    pub byte_length: u16,
    /// Original bytes; only byte_length bytes belong to the value.
    pub bytes: [u8; 8],
}
/// Supplied full identity for one recovery obligation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputObligation {
    /// Exact supplied bytes, including negative recovery identities.
    pub obligation: RawIdentity,
    /// Supplied decision, to be checked after API admission.
    pub known_decision: Decision,
    /// Summary of the associated supplied owner rows.
    pub after_ownership_summary: OwnershipSummary,
    /// Number of associated rows within this environment envelope.
    pub associated_owner_count: u16,
}
/// Supplied ownership transition; state membership is a separate, short-reference format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputOwner {
    /// Exact resource identity.
    pub resource: RawIdentity,
    /// Supplied retained resource category.
    pub resource_kind: ResourceKind,
    /// Supplied disposition after the action.
    pub after_disposition: OwnerDisposition,
    /// Associated recovery obligation, if any.
    pub obligation: Option<RawIdentity>,
    /// Current holder immediately before this action, or None for a new resource.
    pub before_holder: Option<RawIdentity>,
    /// Holder after this action, absent for Released.
    pub after_holder: Option<RawIdentity>,
}
/// Aggregate environment envelope; semantic admission is separate from structural decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Environment {
    /// Fact category supplied by the raw action family.
    pub kind: FactKind,
    /// Supplied exact fact identity.
    pub fact: RawIdentity,
    /// None=0, Execute=1, Fetch=2, Resume=3.
    pub work_kind: u8,
    /// Family-local supplied work class, or 255 when absent.
    pub work_class: u8,
    /// Complete=1 through RecoveryConfirmed=4, or absent=255.
    pub cleanup: u8,
    /// Commit class 0..8, or absent=255.
    pub commit_class: u8,
    /// Supplied transaction identity.
    pub transaction: Option<RawIdentity>,
    /// Supplied receipt identity.
    pub receipt: Option<RawIdentity>,
    /// Supplied durable anchor identity.
    pub anchor: Option<RawIdentity>,
    /// Supplied fresh engine identity.
    pub fresh_engine: Option<RawIdentity>,
    /// Original ordered rows; do not sort or deduplicate negative inputs.
    pub obligations: Records<InputObligation, 7>,
    /// Original ordered owner rows.
    pub owners: Records<InputOwner, 54>,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            kind: FactKind::Execution,
            fact: RawIdentity::default(),
            work_kind: 0,
            work_class: 255,
            cleanup: 255,
            commit_class: 255,
            transaction: None,
            receipt: None,
            anchor: None,
            fresh_engine: None,
            obligations: Records::default(),
            owners: Records::default(),
        }
    }
}
/// Fully resolved raw action bytes; no reference to a mutable model state is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedInput {
    /// API family.
    pub api: Api,
    /// Actor 0/1, or 255 for recovery.
    pub actor: u8,
    /// Exact family-local outcome tag, including otherwise equivalent N0 Bind outcomes.
    pub outcome: u8,
    /// Prepare slot, or 255 when unused.
    pub prepare_slot: u8,
    /// Prepare shape, or 255 when unused.
    pub prepare_shape: u8,
    /// Prepare parameter count, or 255 when unused.
    pub prepare_n: u8,
    /// Requested autocommit mode, or 255 when unused.
    pub set_autocommit: u8,
    /// Raw caller-supplied target, including malformed sentinel identities.
    pub target: Option<RawIdentity>,
    /// Fresh caller or original continuation identity.
    pub context: Option<RawIdentity>,
    /// Original deadline, unchanged by subsequent signal actions.
    pub original_deadline: Deadline,
    /// Supplied or signalled cancellation bit.
    pub cancel: bool,
    /// Deadline reached bit, separate from original_deadline.
    pub deadline_reached: bool,
    /// Raw values in original order; count is the initialized length.
    pub bind_values: Records<BindValue, 2>,
    /// Supplied facts and memberships, checked only at the semantic environment gate.
    pub environment: Option<Environment>,
}
impl Default for ResolvedInput {
    fn default() -> Self {
        Self {
            api: Api::Prepare,
            actor: 0,
            outcome: 0,
            prepare_slot: 255,
            prepare_shape: 255,
            prepare_n: 255,
            set_autocommit: 255,
            target: None,
            context: None,
            original_deadline: Deadline::Absent,
            cancel: false,
            deadline_reached: false,
            bind_values: Records::default(),
            environment: None,
        }
    }
}
