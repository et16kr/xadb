//! Test-only interpretation of the frozen, hand-authored lifecycle literals.
//!
//! This reader shares record declarations only. It never calls a model, resolver,
//! checker, or codec to manufacture an expected state or input.

use super::input_schema::*;
use super::schema::*;
use serde_json::{Map, Value};

type ReadResult<T> = Result<T, String>;

/// Original hand-authored logical source, independent of the byte packing recipe.
pub(super) const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/fixtures/dev4-lifecycle-model/literals.json"
));

/// Fixture text and its exact base/delta expansion; no transition rules live here.
pub(super) struct Literals {
    source: Value,
}

impl Literals {
    /// Read the fixed source; its hash is checked by the selecting tests.
    pub(super) fn new() -> ReadResult<Self> {
        Self::from_json(SOURCE)
    }

    /// Read isolated reader-control text without modifying the pinned fixture.
    pub(super) fn from_json(source: &str) -> ReadResult<Self> {
        Ok(Self {
            source: serde_json::from_str(source).map_err(|e| e.to_string())?,
        })
    }

    /// Original IDs, without treating an inherited state as a model transition.
    pub(super) fn state_names(&self) -> ReadResult<Vec<&str>> {
        Ok(object(&self.source["states"])?
            .keys()
            .map(String::as_str)
            .collect())
    }

    /// Populate an existing heap workspace directly from one complete literal.
    pub(super) fn state(&self, name: &str, out: &mut State) -> ReadResult<()> {
        let value = self.named("states", name, &mut Vec::new())?;
        let expanded = self.expand(&value, &mut Vec::new())?;
        read_state(&expanded, out).map_err(|e| format!("state {name}: {e}"))
    }

    /// Interpret one frozen input, optionally applying its explicit vector override.
    pub(super) fn input(&self, name: &str, put: Option<&Value>) -> ReadResult<ResolvedInput> {
        let mut value = self.named("inputs", name, &mut Vec::new())?;
        if let Some(put) = put {
            apply_put(&mut value, put)?;
        }
        let expanded = self.expand(&value, &mut Vec::new())?;
        read_input(&expanded).map_err(|e| format!("input {name}: {e}"))
    }

    fn named(&self, section: &str, name: &str, stack: &mut Vec<String>) -> ReadResult<Value> {
        let key = format!("{section}/{name}");
        if stack.contains(&key) {
            return Err(format!("cyclic literal reference {key}"));
        }
        stack.push(key.clone());
        let result = (|| {
            let source = self.source[section]
                .get(name)
                .ok_or_else(|| format!("missing literal {key}"))?;
            let value = if let Some(base) = source.get("base") {
                // The base is already expanded, while every enclosing named
                // reference remains on this same stack until expansion finishes.
                let mut value = self.named(section, string(base)?, stack)?;
                let delta = object(source)?;
                if delta
                    .keys()
                    .any(|k| !["base", "put", "remove"].contains(&k.as_str()))
                {
                    return Err(format!("unexpected delta field in {key}"));
                }
                if let Some(removes) = delta.get("remove") {
                    for pointer in array(removes)? {
                        let (parent, field) = pointer_parent(&mut value, string(pointer)?)?;
                        if parent.remove(&field).is_none() {
                            return Err(format!("remove of missing field {pointer}"));
                        }
                    }
                }
                if let Some(put) = delta.get("put") {
                    apply_put(&mut value, put)?;
                }
                value
            } else {
                source.clone()
            };
            self.expand(&value, stack)
        })();
        stack.pop();
        result
    }

    fn expand(&self, value: &Value, stack: &mut Vec<String>) -> ReadResult<Value> {
        match value {
            Value::String(s) if s.starts_with('@') => self.source["identities"]
                .get(&s[1..])
                .cloned()
                .ok_or_else(|| format!("unknown identity alias {s}")),
            Value::Array(items) => items
                .iter()
                .map(|v| self.expand(v, stack))
                .collect::<ReadResult<Vec<_>>>()
                .map(Value::Array),
            Value::Object(fields) if fields.contains_key("$value") => {
                exact_fields(value, &["$value"])?;
                let name = string(&fields["$value"])?;
                let key = format!("values/{name}");
                if stack.contains(&key) {
                    return Err(format!("cyclic value reference {key}"));
                }
                stack.push(key);
                let expanded = self.source["values"]
                    .get(name)
                    .ok_or_else(|| format!("missing value {name}"))
                    .and_then(|definition| self.expand(definition, stack));
                stack.pop();
                expanded
            }
            Value::Object(fields) if fields.contains_key("envelope_ref") => {
                exact_fields(value, &["envelope_ref"])?;
                let name = string(&fields["envelope_ref"])?;
                // named pushes/pops the envelope key on this shared stack,
                // including all nested base/value/envelope references.
                self.named("envelopes", name, stack)
            }
            Value::Object(fields) => fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), self.expand(v, stack)?)))
                .collect::<ReadResult<Map<_, _>>>()
                .map(Value::Object),
            _ => Ok(value.clone()),
        }
    }
}

fn pointer_parent<'a>(
    value: &'a mut Value,
    pointer: &str,
) -> ReadResult<(&'a mut Map<String, Value>, String)> {
    if !pointer.starts_with('/') {
        return Err(format!("not an absolute literal pointer: {pointer}"));
    }
    let (parent, field) = pointer
        .rsplit_once('/')
        .ok_or("missing pointer component")?;
    let target = value
        .pointer_mut(parent)
        .ok_or_else(|| format!("missing parent {parent}"))?;
    let target = target
        .as_object_mut()
        .ok_or_else(|| format!("non-object parent {parent}"))?;
    Ok((target, field.replace("~1", "/").replace("~0", "~")))
}

fn apply_put(value: &mut Value, put: &Value) -> ReadResult<()> {
    for (pointer, replacement) in object(put)? {
        let (parent, field) = pointer_parent(value, pointer)?;
        parent.insert(field, replacement.clone());
    }
    Ok(())
}

fn object(value: &Value) -> ReadResult<&Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| format!("expected object, got {value}"))
}

fn array(value: &Value) -> ReadResult<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("expected array, got {value}"))
}

fn string(value: &Value) -> ReadResult<&str> {
    value
        .as_str()
        .ok_or_else(|| format!("expected string, got {value}"))
}

fn exact_fields(value: &Value, names: &[&str]) -> ReadResult<()> {
    let fields = object(value)?;
    if fields.len() != names.len() || names.iter().any(|name| !fields.contains_key(*name)) {
        return Err(format!(
            "expected fields {names:?}, got {:?}",
            fields.keys().collect::<Vec<_>>()
        ));
    }
    Ok(())
}

trait FromLiteral: Sized {
    fn read(value: &Value) -> ReadResult<Self>;
}

fn field<T: FromLiteral>(value: &Value, name: &str) -> ReadResult<T> {
    T::read(value.get(name).ok_or_else(|| format!("missing {name}"))?)
        .map_err(|e| format!("{name}: {e}"))
}

impl FromLiteral for u8 {
    fn read(value: &Value) -> ReadResult<Self> {
        value
            .as_u64()
            .and_then(|n| n.try_into().ok())
            .ok_or_else(|| format!("expected u8: {value}"))
    }
}
impl FromLiteral for u16 {
    fn read(value: &Value) -> ReadResult<Self> {
        value
            .as_u64()
            .and_then(|n| n.try_into().ok())
            .ok_or_else(|| format!("expected u16: {value}"))
    }
}
impl FromLiteral for bool {
    fn read(value: &Value) -> ReadResult<Self> {
        value
            .as_bool()
            .ok_or_else(|| format!("expected bool: {value}"))
    }
}
impl<T: FromLiteral> FromLiteral for Option<T> {
    fn read(value: &Value) -> ReadResult<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::read(value).map(Some)
        }
    }
}

macro_rules! literal_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        impl FromLiteral for $name {
            fn read(value: &Value) -> ReadResult<Self> {
                match string(value)? {
                    $(stringify!($variant) => Ok(Self::$variant),)+
                    other => Err(format!("unknown {} literal {other}", stringify!($name))),
                }
            }
        }
    };
}

literal_enum!(EngineAdmission {
    Admitting,
    Fenced,
    RecoveryConfirmedRetired
});
literal_enum!(EngineCause {
    None,
    UndoIncomplete,
    CommitUnknown,
    CommittedPublicationPending,
    PeerFence
});
literal_enum!(Shell { Open, Closed });
literal_enum!(Autocommit { On, Off });
literal_enum!(ReopenKind {
    Initial,
    HealthySameEngine,
    RecoveredFreshEngine
});
literal_enum!(SlotState {
    Vacant,
    ReadyUnbound,
    ReadyBound,
    ResultOpen,
    Invalid,
    Closed
});
literal_enum!(Shape { Select, Dml });
literal_enum!(Metadata {
    SelectN0,
    SelectN1Utf8NotNullMax1,
    DmlN0,
    DmlN1Utf8NotNullMax1
});
literal_enum!(StatementLife {
    Current,
    CurrentClosed,
    Retired
});
literal_enum!(Retirement {
    None,
    Reexecute,
    StatementClose,
    Rollback,
    SessionClose,
    EngineFence,
    SlotReuse
});
literal_enum!(ResultState {
    Open,
    Eof,
    Error,
    Closed
});
literal_enum!(ContextStatus {
    ParkedFetch,
    ParkedCommit,
    Completed
});
literal_enum!(Deadline {
    Absent,
    FutureAtEntry,
    ReachedAtEntry
});
literal_enum!(CompletionPhase {
    NotCompleted,
    RejectedBeforeAdmission,
    AdmittedCompleted,
    CachedOrIdempotentCompleted
});
literal_enum!(TransactionOrigin {
    ExplicitBegin,
    OffStarted,
    ImplicitReadOnly,
    ImplicitReadWrite
});
literal_enum!(Access {
    ReadOnly,
    ReadWrite
});
literal_enum!(TransactionLife {
    Active,
    Ended,
    RecoveryPending
});
literal_enum!(CommitProgress {
    None,
    BeforeAttempt,
    Attempted,
    DurableConfirmed,
    Published,
    NoWriteValidated
});
literal_enum!(Decision {
    NotApplicable,
    NotAttempted,
    Aborted,
    NotCommittedRecoveryRequired,
    Unknown,
    Committed
});
literal_enum!(ClientKnowledge {
    NoDecisionObserved,
    KnownAborted,
    KnownNotCommittedRecoveryRequired,
    Unknown,
    KnownCommitted
});
literal_enum!(Delivery {
    NotApplicable,
    Delivered,
    NotDelivered
});
literal_enum!(ObligationReason {
    KnownNotCommittedUndoPending,
    CommitDecisionUnknown,
    KnownCommittedPublicationPending,
    PeerActiveTransactionAtFence
});
literal_enum!(OwnerDisposition {
    Held,
    Transferred,
    Released
});
literal_enum!(Settlement {
    Outstanding,
    RecoveryConfirmed,
    DrainedConfirmed
});
literal_enum!(BindingValue { Empty, A61, B62 });
literal_enum!(BindingLife {
    Current,
    Replaced,
    Released
});
literal_enum!(ReceiptKind {
    NoWriteCompletion,
    DurableWrite
});
literal_enum!(ResourceKind {
    Plan,
    Binding,
    Snapshot,
    Scratch,
    Temp,
    Undo,
    Locks,
    AbortReserve,
    Recovery
});
literal_enum!(FactKind {
    Execution,
    FetchWorkOrCleanup,
    CommitAtomic,
    CommitProgress,
    CommitFinish,
    RollbackCleanup,
    SessionCleanup,
    RecoveryConfirmed,
    LocalCleanup
});
literal_enum!(Cleanup {
    NotConsumed,
    Complete,
    Incomplete,
    LocalReleaseAndHandoff,
    RecoveryConfirmed
});
literal_enum!(NextWork {
    NotConsumed,
    Rows,
    Eof,
    ReadError,
    Conflict,
    RowBufferTooSmall
});
literal_enum!(Api {
    Prepare,
    BindAll,
    Execute,
    Fetch,
    CloseResult,
    CloseStatement,
    Begin,
    SetAutocommit,
    CommitAtomic,
    Rollback,
    CloseSession,
    ReopenSession,
    InspectStatement,
    InspectSession,
    SignalContext,
    ParkFetch,
    ResumeFetch,
    BeginCommit,
    CommitFact,
    FinishCommit,
    RecoveryConfirmedFreshEngine
});
literal_enum!(ReturnCode {
    Ok,
    Eof,
    InvalidHandle,
    HandleClosed,
    SessionState,
    Busy,
    EngineFenced,
    ResultBusy,
    SlotBusy,
    TxnState,
    BindCount,
    StalePlan,
    Type,
    NotNull,
    Range,
    SyntaxUtf8,
    Limit,
    Cancelled,
    Timeout,
    ValueError,
    ReadError,
    Conflict,
    RecoveryRequired,
    ResultUnknown,
    RowBufferTooSmall,
    IgnoredCompleted,
    InvalidContext,
    NoPendingOperation,
    ContinuationKind
});
literal_enum!(Phase {
    NotApplicable,
    Entry,
    Prepare,
    Bind,
    Execute,
    Fetch,
    Close,
    Commit,
    Rollback,
    Recovery
});
literal_enum!(StatementEffect {
    NoEffect,
    StatementRestored,
    StatementApplied,
    CursorReleased,
    RecoveryRequired
});
literal_enum!(Certainty {
    NotApplicable,
    Known,
    Unknown
});
literal_enum!(Retry {
    NotApplicable,
    Never,
    Operation,
    AfterRecovery,
    WholeTransactionDecision
});
literal_enum!(OriginCause {
    None,
    ValueError,
    ReadError,
    Conflict,
    Cancelled,
    Timeout,
    UndoIncomplete,
    CommitUnknown,
    PublicationPending,
    PeerFence
});
literal_enum!(OwnershipSummary {
    None,
    AllHeld,
    AllTransferred,
    AllReleased,
    Mixed
});

fn identity_parts(value: &Value) -> ReadResult<[u16; 7]> {
    let parts = array(value)?;
    if parts.len() != 7 {
        return Err("identity must have seven fields".into());
    }
    let mut result = [0; 7];
    for (out, part) in result.iter_mut().zip(parts) {
        *out = u16::read(part)?;
    }
    for part in &result[..3] {
        u8::try_from(*part).map_err(|_| "identity byte exceeds u8")?;
    }
    Ok(result)
}

impl FromLiteral for Identity {
    fn read(value: &Value) -> ReadResult<Self> {
        let p = identity_parts(value)?;
        Ok(Self {
            kind: IdentityKind::try_from(p[0] as u8).map_err(|e| format!("{e:?}"))?,
            actor: p[1] as u8,
            slot: p[2] as u8,
            engine: p[3],
            session: p[4],
            nonce: p[5],
            generation: p[6],
        })
    }
}

impl FromLiteral for RawIdentity {
    fn read(value: &Value) -> ReadResult<Self> {
        // RawIdentity is itself bytes. These fixed field offsets are the literal
        // Identity16 definition, not a call into the codec under test.
        let p = identity_parts(value)?;
        let mut bytes = [0; 16];
        bytes[..3].copy_from_slice(&[p[0] as u8, p[1] as u8, p[2] as u8]);
        for (offset, part) in [4, 6, 8, 10].into_iter().zip(&p[3..]) {
            bytes[offset..offset + 2].copy_from_slice(&part.to_le_bytes());
        }
        Ok(Self(bytes))
    }
}

macro_rules! literal_ref {
    ($name:ident, $kind:ident) => {
        impl FromLiteral for $name {
            fn read(value: &Value) -> ReadResult<Self> {
                let id = Identity::read(value)?;
                if id.kind != IdentityKind::$kind || id.nonce == 0 {
                    return Err(format!(
                        "expected nonzero {} identity: {id:?}",
                        stringify!($kind)
                    ));
                }
                Ok(Self(id.nonce))
            }
        }
    };
}
literal_ref!(EngineRef, Engine);
literal_ref!(SessionRef, Session);
literal_ref!(StatementRef, Statement);
literal_ref!(ResultRef, Result);
literal_ref!(ContextRef, Context);
literal_ref!(TransactionRef, Transaction);
literal_ref!(BindingRef, Binding);
literal_ref!(ReceiptRef, Receipt);
literal_ref!(ResourceRef, Resource);
literal_ref!(ObligationRef, Obligation);
literal_ref!(FactRef, Fact);
literal_ref!(AnchorRef, Anchor);

impl FromLiteral for HolderRef {
    fn read(value: &Value) -> ReadResult<Self> {
        let id = Identity::read(value)?;
        Ok(Self {
            kind: id.kind,
            nonce: id.nonce,
        })
    }
}

macro_rules! literal_record {
    ($name:ident { $($field:ident),+ $(,)? }) => {
        impl FromLiteral for $name {
            fn read(value: &Value) -> ReadResult<Self> {
                exact_fields(value, &[$(stringify!($field)),+])?;
                Ok(Self { $($field: field(value, stringify!($field))?),+ })
            }
        }
    };
}

literal_record!(EngineRecord {
    id,
    admission,
    first_cause,
    known_decision,
    cause_transaction,
    first_fence_context,
    successor_engine,
    recovery_fact
});
literal_record!(SessionRecord {
    id,
    engine,
    actor,
    lifecycle,
    reopen_kind,
    previous_session,
    closing_context
});
literal_record!(ActorRecord {
    actor,
    shell,
    autocommit,
    session,
    engine,
    transaction,
    parked_context,
    active_public_result
});
literal_record!(PreparedSlot {
    actor,
    slot,
    state,
    next_statement_generation,
    next_result_generation,
    statement,
    binding,
    descriptor_result
});
literal_record!(StatementRecord {
    id,
    issue_ordinal,
    shape,
    parameter_count,
    metadata,
    life,
    retirement,
    session,
    creation_context,
    retirement_context
});
literal_record!(ResultRecord {
    id,
    issue_ordinal,
    statement,
    retirement,
    public_generation_live,
    creation_context,
    retirement_context
});
literal_record!(Descriptor {
    result,
    statement,
    state,
    public_generation_live,
    blocked_by_fence,
    position,
    transaction,
    terminal
});
literal_record!(TransactionRecord {
    id,
    origin,
    access,
    life,
    prior_successful_write,
    progress,
    decision,
    client_knowledge,
    delivery,
    birth_session,
    receipt,
    parked_context,
    obligation,
    completion_fact
});
literal_record!(ObligationRecord {
    id,
    engine,
    transaction,
    origin_session,
    creation_reason,
    latest_reason,
    reason_mask,
    disposition,
    known_decision,
    settlement,
    first_fact,
    latest_fact,
    recovery_fact
});
literal_record!(BindingRecord {
    id,
    statement,
    issue_ordinal,
    value,
    life,
    replaced_by,
    creation_context
});
literal_record!(ReceiptRecord {
    id,
    transaction,
    engine,
    kind,
    delivery,
    anchor,
    source_fact,
    delivery_context
});
literal_record!(OwnerRecord {
    id,
    resource_kind,
    disposition,
    transfer_count,
    original_holder,
    current_holder,
    previous_holder,
    birth_context,
    last_fact
});
literal_record!(FactRecord {
    id,
    kind,
    api,
    outcome_class,
    cleanup,
    decision,
    delivery,
    next_work,
    consumed_parts,
    engine,
    obligation_count,
    owner_count,
    context,
    transaction,
    receipt,
    fresh_engine,
    anchor
});
literal_record!(FactObligation {
    fact,
    obligation,
    known_decision,
    disposition,
    owner_count
});
literal_record!(FactOwner {
    fact,
    resource,
    disposition,
    obligation,
    before_holder,
    after_holder
});
literal_record!(InputObligation {
    obligation,
    known_decision,
    after_ownership_summary,
    associated_owner_count
});
literal_record!(InputOwner {
    resource,
    resource_kind,
    after_disposition,
    obligation,
    before_holder,
    after_holder
});

impl FromLiteral for Diagnostic {
    fn read(value: &Value) -> ReadResult<Self> {
        exact_fields(
            value,
            &[
                "code",
                "phase",
                "effect",
                "certainty",
                "retry",
                "engine_cause",
                "origin_cause",
                "span_start",
                "span_end",
            ],
        )?;
        if field::<u16>(value, "span_start")? != 0 || field::<u16>(value, "span_end")? != 0 {
            return Err("model diagnostic requires literal zero span".into());
        }
        Ok(Self {
            code: field(value, "code")?,
            phase: field(value, "phase")?,
            effect: field(value, "effect")?,
            certainty: field(value, "certainty")?,
            retry: field(value, "retry")?,
            engine_cause: field(value, "engine_cause")?,
            origin_cause: field(value, "origin_cause")?,
        })
    }
}

impl FromLiteral for ContextRecord {
    fn read(value: &Value) -> ReadResult<Self> {
        let status = field::<ContextStatus>(value, "status")?;
        let data = if status == ContextStatus::Completed {
            exact_fields(value, &["id", "issue_ordinal", "status"])?;
            ContextData::Completed
        } else {
            exact_fields(
                value,
                &[
                    "id",
                    "issue_ordinal",
                    "status",
                    "api",
                    "original_deadline",
                    "cancel_supplied_or_signalled",
                    "deadline_reached",
                    "cancel_observed_at_completion",
                    "deadline_observed_at_completion",
                    "completion_phase",
                    "statement",
                    "result",
                    "transaction",
                ],
            )?;
            let parked = ParkedContext {
                api: field(value, "api")?,
                original_deadline: field(value, "original_deadline")?,
                cancel_supplied_or_signalled: field(value, "cancel_supplied_or_signalled")?,
                deadline_reached: field(value, "deadline_reached")?,
                cancel_observed_at_completion: field(value, "cancel_observed_at_completion")?,
                deadline_observed_at_completion: field(value, "deadline_observed_at_completion")?,
                completion_phase: field(value, "completion_phase")?,
                statement: field(value, "statement")?,
                result: field(value, "result")?,
                transaction: field(value, "transaction")?,
            };
            match status {
                ContextStatus::ParkedFetch => ContextData::ParkedFetch(parked),
                ContextStatus::ParkedCommit => ContextData::ParkedCommit(parked),
                ContextStatus::Completed => unreachable!(),
            }
        };
        Ok(Self {
            id: field(value, "id")?,
            issue_ordinal: field(value, "issue_ordinal")?,
            data,
        })
    }
}

// Logical state tables are maps. Their keys are original numeric nonces or
// actor:slot pairs; unlike negative input lists, maps have no source array order.
fn ordered_map(value: &Value) -> ReadResult<Vec<&Value>> {
    let mut rows = object(value)?
        .iter()
        .map(|(key, value)| {
            let key = key
                .split(':')
                .map(|part| part.parse::<u16>().map_err(|e| e.to_string()))
                .collect::<ReadResult<Vec<_>>>()?;
            Ok((key, value))
        })
        .collect::<ReadResult<Vec<_>>>()?;
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows.into_iter().map(|(_, value)| value).collect())
}

fn table<T: FromLiteral, const N: usize>(value: &Value, out: &mut Records<T, N>) -> ReadResult<()> {
    out.clear();
    for (index, row) in ordered_map(value)?.into_iter().enumerate() {
        out.try_push(T::read(row).map_err(|e| format!("row {index}: {e}"))?)
            .map_err(|_| "literal table exceeds fixed capacity")?;
    }
    Ok(())
}

fn members<T: FromLiteral, const N: usize>(
    value: &Value,
    facts: &Value,
    out: &mut Records<T, N>,
) -> ReadResult<()> {
    out.clear();
    let mut groups = object(value)?
        .iter()
        .map(|(key, rows)| Ok((key.parse::<u16>().map_err(|e| e.to_string())?, key, rows)))
        .collect::<ReadResult<Vec<_>>>()?;
    groups.sort_by_key(|group| group.0);
    for (_, key, rows) in groups {
        let id = facts
            .get(key)
            .and_then(|f| f.get("id"))
            .ok_or_else(|| format!("missing membership parent fact {key}"))?;
        for row in array(rows)? {
            let mut fields = object(row)?.clone();
            if fields.insert("fact".into(), id.clone()).is_some() {
                return Err("membership already supplies grouped fact field".into());
            }
            out.try_push(T::read(&Value::Object(fields))?)
                .map_err(|_| "membership table exceeds fixed capacity")?;
        }
    }
    Ok(())
}

fn read_state(value: &Value, out: &mut State) -> ReadResult<()> {
    exact_fields(
        value,
        &[
            "current_engine",
            "next_nonce",
            "engines",
            "session_incarnations",
            "actors",
            "prepared_slots",
            "issued_statements",
            "issued_results",
            "private_descriptors",
            "contexts",
            "transactions",
            "obligations",
            "bindings",
            "receipts",
            "owners",
            "facts",
            "fact_obligations",
            "fact_owners",
        ],
    )?;
    out.current_engine = field(value, "current_engine")?;
    let kinds = [
        "Engine",
        "Session",
        "Statement",
        "Result",
        "Context",
        "Transaction",
        "Binding",
        "Receipt",
        "Resource",
        "Obligation",
        "Fact",
        "Anchor",
    ];
    exact_fields(&value["next_nonce"], &kinds)?;
    for (index, name) in kinds.into_iter().enumerate() {
        out.next_nonce[index] = field(&value["next_nonce"], name)?;
    }
    macro_rules! fill {
        ($($name:ident),+ $(,)?) => { $(table(&value[stringify!($name)], &mut out.$name).map_err(|e| format!("{}: {e}", stringify!($name)))?;)+ };
    }
    fill!(
        engines,
        session_incarnations,
        actors,
        prepared_slots,
        issued_statements,
        issued_results,
        private_descriptors,
        contexts,
        transactions,
        obligations,
        bindings,
        receipts,
        owners,
        facts
    );
    members(
        &value["fact_obligations"],
        &value["facts"],
        &mut out.fact_obligations,
    )?;
    members(&value["fact_owners"], &value["facts"], &mut out.fact_owners)?;
    Ok(())
}

fn read_environment(value: &Value) -> ReadResult<Environment> {
    exact_fields(
        value,
        &[
            "envelope_schema",
            "fact",
            "kind",
            "work_kind",
            "work_class",
            "cleanup",
            "commit_class",
            "reserved0",
            "transaction",
            "receipt",
            "anchor",
            "fresh_engine",
            "obligations",
            "owners",
        ],
    )?;
    if field::<u8>(value, "envelope_schema")? != 1 || field::<u8>(value, "reserved0")? != 0 {
        return Err("unexpected literal environment version/reserved field".into());
    }
    let mut out = Environment {
        kind: field(value, "kind")?,
        fact: field(value, "fact")?,
        work_kind: match string(&value["work_kind"])? {
            "None" => 0,
            "Execute" => 1,
            "Fetch" => 2,
            "Resume" => 3,
            other => return Err(format!("unknown input work kind {other}")),
        },
        work_class: field(value, "work_class")?,
        cleanup: field(value, "cleanup")?,
        commit_class: field(value, "commit_class")?,
        transaction: field(value, "transaction")?,
        receipt: field(value, "receipt")?,
        anchor: field(value, "anchor")?,
        fresh_engine: field(value, "fresh_engine")?,
        obligations: Records::default(),
        owners: Records::default(),
    };
    for row in array(&value["obligations"])? {
        out.obligations
            .try_push(InputObligation::read(row)?)
            .map_err(|_| "input obligations exceed capacity")?;
    }
    for row in array(&value["owners"])? {
        out.owners
            .try_push(InputOwner::read(row)?)
            .map_err(|_| "input owners exceed capacity")?;
    }
    Ok(out)
}

fn read_input(value: &Value) -> ReadResult<ResolvedInput> {
    exact_fields(
        value,
        &[
            "schema_version",
            "api",
            "api_tag",
            "actor",
            "outcome_tag",
            "prepare_slot",
            "prepare_shape",
            "parameter_count",
            "set_autocommit",
            "target",
            "context",
            "original_deadline",
            "supplied_cancel",
            "deadline_reached",
            "bind_values",
            "environment",
        ],
    )?;
    let api: Api = field(value, "api")?;
    if field::<u8>(value, "schema_version")? != 1 || field::<u8>(value, "api_tag")? != api as u8 {
        return Err("literal input schema version or named/numeric API disagrees".into());
    }
    // All seven frozen input vectors have zero values. Do not invent a JSON
    // binding vocabulary or silently accept unrepresented values for this leaf.
    if !array(&value["bind_values"])?.is_empty() {
        return Err("nonempty binding values are outside these frozen literals".into());
    }
    Ok(ResolvedInput {
        api,
        actor: field(value, "actor")?,
        outcome: field(value, "outcome_tag")?,
        prepare_slot: field(value, "prepare_slot")?,
        prepare_shape: field(value, "prepare_shape")?,
        prepare_n: field(value, "parameter_count")?,
        set_autocommit: field(value, "set_autocommit")?,
        target: field(value, "target")?,
        context: field(value, "context")?,
        original_deadline: field(value, "original_deadline")?,
        cancel: field(value, "supplied_cancel")?,
        deadline_reached: field(value, "deadline_reached")?,
        bind_values: Records::default(),
        environment: if value["environment"].is_null() {
            None
        } else {
            Some(read_environment(&value["environment"])?)
        },
    })
}
