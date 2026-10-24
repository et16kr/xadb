//! Flat bounded records for the independent DEV4 lifecycle model.
//! Native layout is deliberately distinct from the XADBLM02 byte format.

/// An unknown tag in the frozen model vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTag {
    /// Name of the affected enum, for structural diagnostics.
    pub vocabulary: &'static str,
    /// The exact unrecognized byte.
    pub value: u8,
}

macro_rules! vocabulary {
    ($name:ident { $first:ident = $initial:literal, $($variant:ident = $value:literal,)* }) => {
        #[doc = concat!("Frozen `", stringify!($name), "` tags for the independent model.")]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum $name {
            #[default]
            #[doc = concat!("The `", stringify!($first), "` contract value.")]
            $first = $initial,
            $(#[doc = concat!("The `", stringify!($variant), "` contract value.")]
            $variant = $value,)*
        }
        impl TryFrom<u8> for $name {
            type Error = InvalidTag;
            fn try_from(value: u8) -> Result<Self, InvalidTag> {
                match value {
                    $initial => Ok(Self::$first),
                    $($value => Ok(Self::$variant),)*
                    _ => Err(InvalidTag { vocabulary: stringify!($name), value }),
                }
            }
        }
    };
}
vocabulary! { IdentityKind {
    None = 0,
    Engine = 1,
    Session = 2,
    Statement = 3,
    Result = 4,
    Context = 5,
    Transaction = 6,
    Binding = 7,
    Receipt = 8,
    Resource = 9,
    Obligation = 10,
    Fact = 11,
    Anchor = 12,
} }
vocabulary! { EngineAdmission {
    Admitting = 0,
    Fenced = 1,
    RecoveryConfirmedRetired = 2,
} }
vocabulary! { EngineCause {
    None = 0,
    UndoIncomplete = 1,
    CommitUnknown = 2,
    CommittedPublicationPending = 3,
    PeerFence = 4,
} }
vocabulary! { Shell {
    Open = 0,
    Closed = 1,
} }
vocabulary! { Autocommit {
    On = 0,
    Off = 1,
} }
vocabulary! { ReopenKind {
    Initial = 0,
    HealthySameEngine = 1,
    RecoveredFreshEngine = 2,
} }
vocabulary! { SlotState {
    Vacant = 0,
    ReadyUnbound = 1,
    ReadyBound = 2,
    ResultOpen = 3,
    Invalid = 4,
    Closed = 5,
} }
vocabulary! { Shape {
    Select = 0,
    Dml = 1,
} }
vocabulary! { Metadata {
    SelectN0 = 0,
    SelectN1Utf8NotNullMax1 = 1,
    DmlN0 = 2,
    DmlN1Utf8NotNullMax1 = 3,
} }
vocabulary! { StatementLife {
    Current = 0,
    CurrentClosed = 1,
    Retired = 2,
} }
vocabulary! { Retirement {
    None = 0,
    Reexecute = 1,
    StatementClose = 2,
    Rollback = 3,
    SessionClose = 4,
    EngineFence = 5,
    SlotReuse = 6,
} }
vocabulary! { ResultState {
    Open = 0,
    Eof = 1,
    Error = 2,
    Closed = 3,
} }
vocabulary! { ContextStatus {
    ParkedFetch = 0,
    ParkedCommit = 1,
    Completed = 2,
} }
vocabulary! { Deadline {
    Absent = 0,
    FutureAtEntry = 1,
    ReachedAtEntry = 2,
} }
vocabulary! { CompletionPhase {
    NotCompleted = 0,
    RejectedBeforeAdmission = 1,
    AdmittedCompleted = 2,
    CachedOrIdempotentCompleted = 3,
} }
vocabulary! { TransactionOrigin {
    ExplicitBegin = 0,
    OffStarted = 1,
    ImplicitReadOnly = 2,
    ImplicitReadWrite = 3,
} }
vocabulary! { Access {
    ReadOnly = 0,
    ReadWrite = 1,
} }
vocabulary! { TransactionLife {
    Active = 0,
    Ended = 1,
    RecoveryPending = 2,
} }
vocabulary! { CommitProgress {
    None = 0,
    BeforeAttempt = 1,
    Attempted = 2,
    DurableConfirmed = 3,
    Published = 4,
    NoWriteValidated = 5,
} }
vocabulary! { Decision {
    NotApplicable = 0,
    NotAttempted = 1,
    Aborted = 2,
    NotCommittedRecoveryRequired = 3,
    Unknown = 4,
    Committed = 5,
} }
vocabulary! { ClientKnowledge {
    NoDecisionObserved = 0,
    KnownAborted = 1,
    KnownNotCommittedRecoveryRequired = 2,
    Unknown = 3,
    KnownCommitted = 4,
} }
vocabulary! { Delivery {
    NotApplicable = 0,
    Delivered = 1,
    NotDelivered = 2,
} }
vocabulary! { ObligationReason {
    KnownNotCommittedUndoPending = 0,
    CommitDecisionUnknown = 1,
    KnownCommittedPublicationPending = 2,
    PeerActiveTransactionAtFence = 3,
} }
vocabulary! { OwnerDisposition {
    Held = 0,
    Transferred = 1,
    Released = 2,
} }
vocabulary! { Settlement {
    Outstanding = 0,
    RecoveryConfirmed = 1,
    DrainedConfirmed = 2,
} }
vocabulary! { BindingValue {
    Empty = 0,
    A61 = 1,
    B62 = 2,
} }
vocabulary! { BindingLife {
    Current = 0,
    Replaced = 1,
    Released = 2,
} }
vocabulary! { ReceiptKind {
    NoWriteCompletion = 0,
    DurableWrite = 1,
} }
vocabulary! { ResourceKind {
    Plan = 0,
    Binding = 1,
    Snapshot = 2,
    Scratch = 3,
    Temp = 4,
    Undo = 5,
    Locks = 6,
    AbortReserve = 7,
    Recovery = 8,
} }
vocabulary! { FactKind {
    Execution = 0,
    FetchWorkOrCleanup = 1,
    CommitAtomic = 2,
    CommitProgress = 3,
    CommitFinish = 4,
    RollbackCleanup = 5,
    SessionCleanup = 6,
    RecoveryConfirmed = 7,
    LocalCleanup = 8,
} }
vocabulary! { Cleanup {
    NotConsumed = 0,
    Complete = 1,
    Incomplete = 2,
    LocalReleaseAndHandoff = 3,
    RecoveryConfirmed = 4,
} }
vocabulary! { NextWork {
    NotConsumed = 0,
    Rows = 1,
    Eof = 2,
    ReadError = 3,
    Conflict = 4,
    RowBufferTooSmall = 5,
} }
vocabulary! { Api {
    Prepare = 0,
    BindAll = 1,
    Execute = 2,
    Fetch = 3,
    CloseResult = 4,
    CloseStatement = 5,
    Begin = 6,
    SetAutocommit = 7,
    CommitAtomic = 8,
    Rollback = 9,
    CloseSession = 10,
    ReopenSession = 11,
    InspectStatement = 12,
    InspectSession = 13,
    SignalContext = 14,
    ParkFetch = 15,
    ResumeFetch = 16,
    BeginCommit = 17,
    CommitFact = 18,
    FinishCommit = 19,
    RecoveryConfirmedFreshEngine = 20,
} }
vocabulary! { ReturnCode {
    Ok = 0,
    Eof = 1,
    InvalidHandle = 2,
    HandleClosed = 3,
    SessionState = 4,
    Busy = 5,
    EngineFenced = 6,
    ResultBusy = 7,
    SlotBusy = 8,
    TxnState = 9,
    BindCount = 10,
    StalePlan = 11,
    Type = 12,
    NotNull = 13,
    Range = 14,
    SyntaxUtf8 = 15,
    Limit = 16,
    Cancelled = 17,
    Timeout = 18,
    ValueError = 19,
    ReadError = 20,
    Conflict = 21,
    RecoveryRequired = 22,
    ResultUnknown = 23,
    RowBufferTooSmall = 24,
    IgnoredCompleted = 25,
    InvalidContext = 28,
    NoPendingOperation = 29,
    ContinuationKind = 30,
} }
vocabulary! { Phase {
    NotApplicable = 0,
    Entry = 1,
    Prepare = 2,
    Bind = 3,
    Execute = 4,
    Fetch = 5,
    Close = 6,
    Commit = 7,
    Rollback = 8,
    Recovery = 9,
} }
vocabulary! { StatementEffect {
    NoEffect = 0,
    StatementRestored = 1,
    StatementApplied = 2,
    CursorReleased = 3,
    RecoveryRequired = 4,
} }
vocabulary! { Certainty {
    NotApplicable = 0,
    Known = 1,
    Unknown = 2,
} }
vocabulary! { Retry {
    NotApplicable = 0,
    Never = 1,
    Operation = 2,
    AfterRecovery = 3,
    WholeTransactionDecision = 4,
} }
vocabulary! { OriginCause {
    None = 0,
    ValueError = 1,
    ReadError = 2,
    Conflict = 3,
    Cancelled = 4,
    Timeout = 5,
    UndoIncomplete = 6,
    CommitUnknown = 7,
    PublicationPending = 8,
    PeerFence = 9,
} }
vocabulary! { OwnershipSummary {
    None = 0,
    AllHeld = 1,
    AllTransferred = 2,
    AllReleased = 3,
    Mixed = 4,
} }

/// Exact identity fields; no alpha-renaming or nonce merging is permitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    /// Defining namespace, excluding None in a valid state.
    pub kind: IdentityKind,
    /// Original actor, or 255 for engine-wide identities.
    pub actor: u8,
    /// Original prepared slot, or 255 when inapplicable.
    pub slot: u8,
    /// Original engine nonce.
    pub engine: u16,
    /// Original session nonce, or zero for engine-wide identities.
    pub session: u16,
    /// Original kind-local issuance nonce.
    pub nonce: u16,
    /// Handle generation; zero for non-handle identities.
    pub generation: u16,
}
impl Identity {
    /// Numeric canonical order, intentionally different from LE byte ordering.
    pub fn key(self) -> (u8, u16, u16, u8, u8, u16, u16) {
        (
            self.kind as u8,
            self.engine,
            self.session,
            self.actor,
            self.slot,
            self.nonce,
            self.generation,
        )
    }
}
macro_rules! reference {
    ($($name:ident),* $(,)?) => {$(
        #[doc = concat!("Original kind-local nonce for `", stringify!($name), "`; zero is only an unvalidated workspace placeholder.")]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(pub u16);
    )*};
}
reference!(
    EngineRef,
    SessionRef,
    StatementRef,
    ResultRef,
    ContextRef,
    TransactionRef,
    BindingRef,
    ReceiptRef,
    ResourceRef,
    ObligationRef,
    FactRef,
    AnchorRef
);

/// Typed holder namespace and original nonce; the codec checks allowed kinds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HolderRef {
    /// Engine, Statement, Result, Transaction, or Obligation.
    pub kind: IdentityKind,
    /// Original nonce within that namespace.
    pub nonce: u16,
}
/// Complete diagnostic payload retained only by terminal error descriptors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    /// Return code, including its original classification.
    pub code: ReturnCode,
    /// Phase at which the diagnostic was produced.
    pub phase: Phase,
    /// Statement-local effect, distinct from transaction certainty.
    pub effect: StatementEffect,
    /// Certainty reported to the caller.
    pub certainty: Certainty,
    /// Contract retry category.
    pub retry: Retry,
    /// Engine fence cause, if present.
    pub engine_cause: EngineCause,
    /// Original operation cause, if present.
    pub origin_cause: OriginCause,
}
/// Capacity exhaustion in a fixed, allocation-free record table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableFull;

/// Fixed native storage with a checked initialized prefix and no nested heap allocation.
#[derive(Clone, Copy, Debug)]
pub struct Records<T, const N: usize> {
    rows: [T; N],
    len: usize,
}
impl<T: Copy + Default, const N: usize> Default for Records<T, N> {
    fn default() -> Self {
        Self {
            rows: [T::default(); N],
            len: 0,
        }
    }
}
impl<T: PartialEq, const N: usize> PartialEq for Records<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl<T: Eq, const N: usize> Eq for Records<T, N> {}
impl<T, const N: usize> Records<T, N> {
    /// Initialized records only; unused native slots never contribute to equality.
    pub fn as_slice(&self) -> &[T] {
        &self.rows[..self.len]
    }
    /// Mutable initialized prefix for model transitions and decode workspaces.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.rows[..self.len]
    }
    /// Number of initialized records.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether the initialized prefix is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Discard the initialized prefix without allocating or publishing a state.
    pub fn clear(&mut self) {
        self.len = 0;
    }
    /// Append without allocation, leaving the table unchanged when full.
    pub fn try_push(&mut self, row: T) -> Result<(), TableFull> {
        if self.len == N {
            return Err(TableFull);
        }
        self.rows[self.len] = row;
        self.len += 1;
        Ok(())
    }
}
impl<T: Copy, const N: usize> Records<T, N> {
    /// Remove a record, preserving the relative order of the remaining prefix.
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        let row = self.rows[index];
        self.rows.copy_within(index + 1..self.len, index);
        self.len -= 1;
        Some(row)
    }
    /// Reuse this table's storage; no third state workspace is created.
    pub fn copy_from(&mut self, other: &Self) {
        self.rows[..other.len].copy_from_slice(other.as_slice());
        self.len = other.len;
    }
}

/// `engines` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `admission` field of the wire contract.
    pub admission: EngineAdmission,
    /// Exact `first_cause` field of the wire contract.
    pub first_cause: EngineCause,
    /// Exact `known_decision` field of the wire contract.
    pub known_decision: Decision,
    /// Exact `cause_transaction` field of the wire contract.
    pub cause_transaction: Option<TransactionRef>,
    /// Exact `first_fence_context` field of the wire contract.
    pub first_fence_context: Option<ContextRef>,
    /// Exact `successor_engine` field of the wire contract.
    pub successor_engine: Option<EngineRef>,
    /// Exact `recovery_fact` field of the wire contract.
    pub recovery_fact: Option<FactRef>,
}

/// `session_incarnations` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `engine` field of the wire contract.
    pub engine: EngineRef,
    /// Exact `actor` field of the wire contract.
    pub actor: u8,
    /// Exact `lifecycle` field of the wire contract.
    pub lifecycle: Shell,
    /// Exact `reopen_kind` field of the wire contract.
    pub reopen_kind: ReopenKind,
    /// Exact `previous_session` field of the wire contract.
    pub previous_session: Option<SessionRef>,
    /// Exact `closing_context` field of the wire contract.
    pub closing_context: Option<ContextRef>,
}

/// `actors` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorRecord {
    /// Exact `actor` field of the wire contract.
    pub actor: u8,
    /// Exact `shell` field of the wire contract.
    pub shell: Shell,
    /// Exact `autocommit` field of the wire contract.
    pub autocommit: Autocommit,
    /// Exact `session` field of the wire contract.
    pub session: SessionRef,
    /// Exact `engine` field of the wire contract.
    pub engine: EngineRef,
    /// Exact `transaction` field of the wire contract.
    pub transaction: Option<TransactionRef>,
    /// Exact `parked_context` field of the wire contract.
    pub parked_context: Option<ContextRef>,
    /// Exact `active_public_result` field of the wire contract.
    pub active_public_result: Option<ResultRef>,
}

/// `prepared_slots` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparedSlot {
    /// Exact `actor` field of the wire contract.
    pub actor: u8,
    /// Exact `slot` field of the wire contract.
    pub slot: u8,
    /// Exact `state` field of the wire contract.
    pub state: SlotState,
    /// Exact `next_statement_generation` field of the wire contract.
    pub next_statement_generation: u16,
    /// Exact `next_result_generation` field of the wire contract.
    pub next_result_generation: u16,
    /// Exact `statement` field of the wire contract.
    pub statement: Option<StatementRef>,
    /// Exact `binding` field of the wire contract.
    pub binding: Option<BindingRef>,
    /// Exact `descriptor_result` field of the wire contract.
    pub descriptor_result: Option<ResultRef>,
}

/// `issued_statements` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatementRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `issue_ordinal` field of the wire contract.
    pub issue_ordinal: u16,
    /// Exact `shape` field of the wire contract.
    pub shape: Shape,
    /// Exact `parameter_count` field of the wire contract.
    pub parameter_count: u8,
    /// Exact `metadata` field of the wire contract.
    pub metadata: Metadata,
    /// Exact `life` field of the wire contract.
    pub life: StatementLife,
    /// Exact `retirement` field of the wire contract.
    pub retirement: Retirement,
    /// Exact `session` field of the wire contract.
    pub session: SessionRef,
    /// Exact `creation_context` field of the wire contract.
    pub creation_context: Option<ContextRef>,
    /// Exact `retirement_context` field of the wire contract.
    pub retirement_context: Option<ContextRef>,
}

/// `issued_results` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `issue_ordinal` field of the wire contract.
    pub issue_ordinal: u16,
    /// Exact `statement` field of the wire contract.
    pub statement: StatementRef,
    /// Exact `retirement` field of the wire contract.
    pub retirement: Retirement,
    /// Exact `public_generation_live` field of the wire contract.
    pub public_generation_live: bool,
    /// Exact `creation_context` field of the wire contract.
    pub creation_context: Option<ContextRef>,
    /// Exact `retirement_context` field of the wire contract.
    pub retirement_context: Option<ContextRef>,
}

/// `private_descriptors` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Descriptor {
    /// Exact `result` field of the wire contract.
    pub result: ResultRef,
    /// Exact `statement` field of the wire contract.
    pub statement: StatementRef,
    /// Exact `state` field of the wire contract.
    pub state: ResultState,
    /// Exact `public_generation_live` field of the wire contract.
    pub public_generation_live: bool,
    /// Exact `blocked_by_fence` field of the wire contract.
    pub blocked_by_fence: bool,
    /// Exact `position` field of the wire contract.
    pub position: u16,
    /// Exact `transaction` field of the wire contract.
    pub transaction: Option<TransactionRef>,
    /// Some exactly for Error. No row, SQL text, snapshot, or receipt payload.
    pub terminal: Option<Diagnostic>,
}

/// `contexts` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContextRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// One-based issue ordinal; Issued[k] selects original ordinal k+1, without renumbering.
    pub issue_ordinal: u16,
    /// Completed payload is physically absent; parked calls retain their original payload.
    pub data: ContextData,
}

/// `transactions` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransactionRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `origin` field of the wire contract.
    pub origin: TransactionOrigin,
    /// Exact `access` field of the wire contract.
    pub access: Access,
    /// Exact `life` field of the wire contract.
    pub life: TransactionLife,
    /// Exact `prior_successful_write` field of the wire contract.
    pub prior_successful_write: bool,
    /// Exact `progress` field of the wire contract.
    pub progress: CommitProgress,
    /// Exact `decision` field of the wire contract.
    pub decision: Decision,
    /// Exact `client_knowledge` field of the wire contract.
    pub client_knowledge: ClientKnowledge,
    /// Exact `delivery` field of the wire contract.
    pub delivery: Delivery,
    /// Exact `birth_session` field of the wire contract.
    pub birth_session: SessionRef,
    /// Exact `receipt` field of the wire contract.
    pub receipt: Option<ReceiptRef>,
    /// Exact `parked_context` field of the wire contract.
    pub parked_context: Option<ContextRef>,
    /// Exact `obligation` field of the wire contract.
    pub obligation: Option<ObligationRef>,
    /// Exact `completion_fact` field of the wire contract.
    pub completion_fact: Option<FactRef>,
}

/// `obligations` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObligationRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `engine` field of the wire contract.
    pub engine: EngineRef,
    /// Exact `transaction` field of the wire contract.
    pub transaction: TransactionRef,
    /// Exact `origin_session` field of the wire contract.
    pub origin_session: SessionRef,
    /// Exact `creation_reason` field of the wire contract.
    pub creation_reason: ObligationReason,
    /// Exact `latest_reason` field of the wire contract.
    pub latest_reason: ObligationReason,
    /// Exact `reason_mask` field of the wire contract.
    pub reason_mask: u8,
    /// Derived aggregate only; exact per-resource ownership remains in owners/fact_owners. Mixed is explicit.
    pub disposition: OwnershipSummary,
    /// Exact `known_decision` field of the wire contract.
    pub known_decision: Decision,
    /// Exact `settlement` field of the wire contract.
    pub settlement: Settlement,
    /// Exact `first_fact` field of the wire contract.
    pub first_fact: Option<FactRef>,
    /// Exact `latest_fact` field of the wire contract.
    pub latest_fact: Option<FactRef>,
    /// Settlement evidence: RecoveryConfirmed=1 refers fresh-engine recovery fact; DrainedConfirmed=2 refers confirming Published progress fact, without clearing engine fence.
    pub recovery_fact: Option<FactRef>,
}

/// `bindings` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BindingRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `statement` field of the wire contract.
    pub statement: StatementRef,
    /// Exact `issue_ordinal` field of the wire contract.
    pub issue_ordinal: u16,
    /// Exact `value` field of the wire contract.
    pub value: BindingValue,
    /// Exact `life` field of the wire contract.
    pub life: BindingLife,
    /// Exact `replaced_by` field of the wire contract.
    pub replaced_by: Option<BindingRef>,
    /// Exact `creation_context` field of the wire contract.
    pub creation_context: Option<ContextRef>,
}

/// `receipts` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiptRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `transaction` field of the wire contract.
    pub transaction: TransactionRef,
    /// Exact `engine` field of the wire contract.
    pub engine: EngineRef,
    /// Exact `kind` field of the wire contract.
    pub kind: ReceiptKind,
    /// Exact `delivery` field of the wire contract.
    pub delivery: Delivery,
    /// DurableWrite Some; NoWriteCompletion None. Supplied fact identity, not derived q/LSN.
    pub anchor: Option<Identity>,
    /// Exact `source_fact` field of the wire contract.
    pub source_fact: Option<FactRef>,
    /// Exact `delivery_context` field of the wire contract.
    pub delivery_context: Option<ContextRef>,
}

/// `owners` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnerRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `resource_kind` field of the wire contract.
    pub resource_kind: ResourceKind,
    /// Exact `disposition` field of the wire contract.
    pub disposition: OwnerDisposition,
    /// Exact `transfer_count` field of the wire contract.
    pub transfer_count: u16,
    /// Exact `original_holder` field of the wire contract.
    pub original_holder: HolderRef,
    /// Exact `current_holder` field of the wire contract.
    pub current_holder: Option<HolderRef>,
    /// Exact `previous_holder` field of the wire contract.
    pub previous_holder: Option<HolderRef>,
    /// Exact `birth_context` field of the wire contract.
    pub birth_context: Option<ContextRef>,
    /// Exact `last_fact` field of the wire contract.
    pub last_fact: Option<FactRef>,
}

/// `facts` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactRecord {
    /// Exact `id` field of the wire contract.
    pub id: Identity,
    /// Exact `kind` field of the wire contract.
    pub kind: FactKind,
    /// Exact `api` field of the wire contract.
    pub api: Api,
    /// Exact `outcome_class` field of the wire contract.
    pub outcome_class: u8,
    /// Exact `cleanup` field of the wire contract.
    pub cleanup: Cleanup,
    /// Exact `decision` field of the wire contract.
    pub decision: Decision,
    /// Exact `delivery` field of the wire contract.
    pub delivery: Delivery,
    /// Exact `next_work` field of the wire contract.
    pub next_work: NextWork,
    /// Raw outcome class remains exact; this records only Work/Cleanup/Receipt/CommitProgress/RecoverySet actually consumed.
    pub consumed_parts: u8,
    /// Exact `engine` field of the wire contract.
    pub engine: EngineRef,
    /// Exact `obligation_count` field of the wire contract.
    pub obligation_count: u16,
    /// Exact `owner_count` field of the wire contract.
    pub owner_count: u16,
    /// Exact `context` field of the wire contract.
    pub context: Option<ContextRef>,
    /// Exact `transaction` field of the wire contract.
    pub transaction: Option<TransactionRef>,
    /// Exact `receipt` field of the wire contract.
    pub receipt: Option<ReceiptRef>,
    /// Exact `fresh_engine` field of the wire contract.
    pub fresh_engine: Option<EngineRef>,
    /// Exact `anchor` field of the wire contract.
    pub anchor: Option<AnchorRef>,
}

/// `fact_obligations` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactObligation {
    /// Exact `fact` field of the wire contract.
    pub fact: FactRef,
    /// Exact `obligation` field of the wire contract.
    pub obligation: ObligationRef,
    /// Exact `known_decision` field of the wire contract.
    pub known_decision: Decision,
    /// Derived aggregate only; exact per-resource ownership remains in owners/fact_owners. Mixed is explicit.
    pub disposition: OwnershipSummary,
    /// Exact `owner_count` field of the wire contract.
    pub owner_count: u16,
}

/// `fact_owners` record in the frozen state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactOwner {
    /// Exact `fact` field of the wire contract.
    pub fact: FactRef,
    /// Exact `resource` field of the wire contract.
    pub resource: ResourceRef,
    /// Exact `disposition` field of the wire contract.
    pub disposition: OwnerDisposition,
    /// Exact `obligation` field of the wire contract.
    pub obligation: Option<ObligationRef>,
    /// Exact `before_holder` field of the wire contract.
    pub before_holder: Option<HolderRef>,
    /// Exact `after_holder` field of the wire contract.
    pub after_holder: Option<HolderRef>,
}

/// Original fields retained while a fetch or commit continuation is parked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParkedContext {
    /// ParkFetch or BeginCommit, matching the context variant.
    pub api: Api,
    /// Original deadline class; a signal does not change it.
    pub original_deadline: Deadline,
    /// Original or subsequently signalled cancellation.
    pub cancel_supplied_or_signalled: bool,
    /// Whether the original deadline has subsequently been reached.
    pub deadline_reached: bool,
    /// False until completion; a completed record drops this payload.
    pub cancel_observed_at_completion: bool,
    /// False until completion; a completed record drops this payload.
    pub deadline_observed_at_completion: bool,
    /// NotCompleted while parked.
    pub completion_phase: CompletionPhase,
    /// Exact statement for a parked fetch; None for commit.
    pub statement: Option<StatementRef>,
    /// Exact result for a parked fetch; None for commit.
    pub result: Option<ResultRef>,
    /// Exact transaction kept until the continuation finishes.
    pub transaction: Option<TransactionRef>,
}
/// Projection B removes completed outgoing payloads, retaining exact defining identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContextData {
    /// Finished calls retain only identity and issue ordinal in ContextRecord.
    #[default]
    Completed,
    /// A fetch continuation with original targets and controls.
    ParkedFetch(ParkedContext),
    /// A commit continuation with its original transaction and controls.
    ParkedCommit(ParkedContext),
}
impl ContextData {
    /// Frozen wire tag for this variant.
    pub fn status(self) -> ContextStatus {
        match self {
            Self::Completed => ContextStatus::Completed,
            Self::ParkedFetch(_) => ContextStatus::ParkedFetch,
            Self::ParkedCommit(_) => ContextStatus::ParkedCommit,
        }
    }
}
/// Exact descriptor hashes carried by every canonical state header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireIdentity {
    /// State layout descriptor hash.
    pub layout: [u8; 32],
    /// Exploration profile descriptor hash.
    pub profile: [u8; 32],
    /// Raw action alphabet descriptor hash.
    pub action: [u8; 32],
    /// Environment exclusion descriptor hash.
    pub exclusion: [u8; 32],
}
/// A reusable decoded state. Empty/default values are unvalidated workspaces, not initial states.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Exact current engine nonce.
    pub current_engine: EngineRef,
    /// Next original nonce for kinds 1 through 12, without renumbering.
    pub next_nonce: [u16; 12],
    /// Bounded `engines` section, in numeric canonical order.
    pub engines: Records<EngineRecord, 7>,
    /// Bounded `session_incarnations` section, in numeric canonical order.
    pub session_incarnations: Records<SessionRecord, 8>,
    /// Bounded `actors` section, in numeric canonical order.
    pub actors: Records<ActorRecord, 2>,
    /// Bounded `prepared_slots` section, in numeric canonical order.
    pub prepared_slots: Records<PreparedSlot, 4>,
    /// Bounded `issued_statements` section, in numeric canonical order.
    pub issued_statements: Records<StatementRecord, 6>,
    /// Bounded `issued_results` section, in numeric canonical order.
    pub issued_results: Records<ResultRecord, 6>,
    /// Bounded `private_descriptors` section, in numeric canonical order.
    pub private_descriptors: Records<Descriptor, 4>,
    /// Bounded `contexts` section, in numeric canonical order.
    pub contexts: Records<ContextRecord, 6>,
    /// Bounded `transactions` section, in numeric canonical order.
    pub transactions: Records<TransactionRecord, 6>,
    /// Bounded `obligations` section, in numeric canonical order.
    pub obligations: Records<ObligationRecord, 6>,
    /// Bounded `bindings` section, in numeric canonical order.
    pub bindings: Records<BindingRecord, 6>,
    /// Bounded `receipts` section, in numeric canonical order.
    pub receipts: Records<ReceiptRecord, 6>,
    /// Bounded `owners` section, in numeric canonical order.
    pub owners: Records<OwnerRecord, 54>,
    /// Bounded `facts` section, in numeric canonical order.
    pub facts: Records<FactRecord, 6>,
    /// Bounded `fact_obligations` section, in numeric canonical order.
    pub fact_obligations: Records<FactObligation, 36>,
    /// Bounded `fact_owners` section, in numeric canonical order.
    pub fact_owners: Records<FactOwner, 324>,
}
impl State {
    /// Copy into existing native storage without heap allocation or a third state clone.
    pub fn copy_from(&mut self, other: &Self) {
        self.current_engine = other.current_engine;
        self.next_nonce = other.next_nonce;
        self.engines.copy_from(&other.engines);
        self.session_incarnations
            .copy_from(&other.session_incarnations);
        self.actors.copy_from(&other.actors);
        self.prepared_slots.copy_from(&other.prepared_slots);
        self.issued_statements.copy_from(&other.issued_statements);
        self.issued_results.copy_from(&other.issued_results);
        self.private_descriptors
            .copy_from(&other.private_descriptors);
        self.contexts.copy_from(&other.contexts);
        self.transactions.copy_from(&other.transactions);
        self.obligations.copy_from(&other.obligations);
        self.bindings.copy_from(&other.bindings);
        self.receipts.copy_from(&other.receipts);
        self.owners.copy_from(&other.owners);
        self.facts.copy_from(&other.facts);
        self.fact_obligations.copy_from(&other.fact_obligations);
        self.fact_owners.copy_from(&other.fact_owners);
    }
}
