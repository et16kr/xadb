//! Direct interpretation of frozen selector read-set inputs, without model or codec rules.

use super::input_schema::RawIdentity;
use super::raw_test_support::{array, byte, text, word};
use super::schema::{
    ActorRecord, ContextData, ContextRecord, EngineRef, Identity, IdentityKind, ParkedContext,
    PreparedSlot, Records, ResultRecord, Retirement, SessionRef, State, StatementLife,
    StatementRecord,
};
use super::target_selector::{SelectorError, TargetKind, TargetSelector};
use serde_json::Value;

fn fields(value: &Value, names: &[&str]) {
    let object = value.as_object().expect("literal object");
    assert_eq!(object.len(), names.len(), "exact read-set fields");
    for name in names {
        assert!(object.contains_key(*name), "missing literal field {name}");
    }
}

fn identity(value: &Value) -> Identity {
    let parts = array(value);
    assert_eq!(parts.len(), 7);
    Identity {
        kind: IdentityKind::try_from(byte(&parts[0])).expect("representable identity kind"),
        actor: byte(&parts[1]),
        slot: byte(&parts[2]),
        engine: word(&parts[3]),
        session: word(&parts[4]),
        nonce: word(&parts[5]),
        generation: word(&parts[6]),
    }
}

fn retirement(value: &Value) -> Retirement {
    match text(value) {
        "None" => Retirement::None,
        "SessionClose" => Retirement::SessionClose,
        "EngineFence" => Retirement::EngineFence,
        other => panic!("unrepresented literal retirement {other}"),
    }
}

#[derive(Clone, Copy)]
enum Poison {
    Engines,
    Sessions,
    Owners,
    Facts,
    Receipts,
}

/// Typed values prepared outside the small worker; no full-state validity is asserted.
pub(super) struct ReadSet {
    current_engine: EngineRef,
    next_nonce: [u16; 12],
    actors: Vec<ActorRecord>,
    slots: Vec<PreparedSlot>,
    statements: Vec<StatementRecord>,
    results: Vec<ResultRecord>,
    contexts: Vec<ContextRecord>,
    poison: Vec<Poison>,
}

impl ReadSet {
    fn read(value: &Value) -> Self {
        fields(
            value,
            &[
                "current_engine",
                "next_nonce",
                "actors",
                "prepared_slots",
                "issued_statements",
                "issued_results",
                "contexts",
                "poison_default_record_tables",
            ],
        );
        fields(&value["next_nonce"], &["Statement", "Result", "Context"]);
        let mut next_nonce = [0; 12];
        next_nonce[2] = word(&value["next_nonce"]["Statement"]);
        next_nonce[3] = word(&value["next_nonce"]["Result"]);
        next_nonce[4] = word(&value["next_nonce"]["Context"]);
        Self {
            current_engine: EngineRef(word(&value["current_engine"])),
            next_nonce,
            actors: array(&value["actors"])
                .iter()
                .map(|v| {
                    fields(v, &["actor", "engine", "session"]);
                    ActorRecord {
                        actor: byte(&v["actor"]),
                        engine: EngineRef(word(&v["engine"])),
                        session: SessionRef(word(&v["session"])),
                        ..ActorRecord::default()
                    }
                })
                .collect(),
            slots: array(&value["prepared_slots"])
                .iter()
                .map(|v| {
                    fields(
                        v,
                        &[
                            "actor",
                            "slot",
                            "next_statement_generation",
                            "next_result_generation",
                        ],
                    );
                    PreparedSlot {
                        actor: byte(&v["actor"]),
                        slot: byte(&v["slot"]),
                        next_statement_generation: word(&v["next_statement_generation"]),
                        next_result_generation: word(&v["next_result_generation"]),
                        ..PreparedSlot::default()
                    }
                })
                .collect(),
            statements: array(&value["issued_statements"])
                .iter()
                .map(|v| {
                    fields(v, &["id", "issue_ordinal", "life", "retirement"]);
                    StatementRecord {
                        id: identity(&v["id"]),
                        issue_ordinal: word(&v["issue_ordinal"]),
                        life: match text(&v["life"]) {
                            "Current" => StatementLife::Current,
                            "Retired" => StatementLife::Retired,
                            other => panic!("life {other}"),
                        },
                        retirement: retirement(&v["retirement"]),
                        ..StatementRecord::default()
                    }
                })
                .collect(),
            results: array(&value["issued_results"])
                .iter()
                .map(|v| {
                    fields(
                        v,
                        &[
                            "id",
                            "issue_ordinal",
                            "retirement",
                            "public_generation_live",
                        ],
                    );
                    ResultRecord {
                        id: identity(&v["id"]),
                        issue_ordinal: word(&v["issue_ordinal"]),
                        retirement: retirement(&v["retirement"]),
                        public_generation_live: v["public_generation_live"]
                            .as_bool()
                            .expect("literal bool"),
                        ..ResultRecord::default()
                    }
                })
                .collect(),
            contexts: array(&value["contexts"])
                .iter()
                .map(|v| {
                    fields(v, &["id", "issue_ordinal", "context_data"]);
                    ContextRecord {
                        id: identity(&v["id"]),
                        issue_ordinal: word(&v["issue_ordinal"]),
                        data: match text(&v["context_data"]) {
                            "Completed" => ContextData::Completed,
                            "ParkedFetchDefault" => {
                                ContextData::ParkedFetch(ParkedContext::default())
                            }
                            other => panic!("context data {other}"),
                        },
                    }
                })
                .collect(),
            poison: array(&value["poison_default_record_tables"])
                .iter()
                .map(|v| match text(v) {
                    "engines" => Poison::Engines,
                    "session_incarnations" => Poison::Sessions,
                    "owners" => Poison::Owners,
                    "facts" => Poison::Facts,
                    "receipts" => Poison::Receipts,
                    other => panic!("poison table {other}"),
                })
                .collect(),
        }
    }

    /// Assign the literal to existing storage. No resolver, invariant, or codec call occurs.
    pub(super) fn put(&self, out: &mut State) {
        out.current_engine = self.current_engine;
        out.next_nonce = self.next_nonce;
        macro_rules! clear {
            ($($field:ident),+ $(,)?) => { $(out.$field.clear();)+ };
        }
        clear!(
            engines,
            session_incarnations,
            private_descriptors,
            transactions,
            obligations,
            bindings,
            receipts,
            owners,
            facts,
            fact_obligations,
            fact_owners
        );
        put_records(&self.actors, &mut out.actors);
        put_records(&self.slots, &mut out.prepared_slots);
        put_records(&self.statements, &mut out.issued_statements);
        put_records(&self.results, &mut out.issued_results);
        put_records(&self.contexts, &mut out.contexts);
        for table in &self.poison {
            match table {
                Poison::Engines => out.engines.try_push(Default::default()).unwrap(),
                Poison::Sessions => out
                    .session_incarnations
                    .try_push(Default::default())
                    .unwrap(),
                Poison::Owners => out.owners.try_push(Default::default()).unwrap(),
                Poison::Facts => out.facts.try_push(Default::default()).unwrap(),
                Poison::Receipts => out.receipts.try_push(Default::default()).unwrap(),
            }
        }
    }
}

fn put_records<T: Copy, const N: usize>(values: &[T], table: &mut Records<T, N>) {
    table.clear();
    for row in values {
        table.try_push(*row).expect("literal fits fixed table");
    }
}

fn expected_error(value: &Value) -> SelectorError {
    match text(value) {
        "InvalidActor" => SelectorError::InvalidActor,
        "InvalidSelector" => SelectorError::InvalidSelector,
        "MissingActor" => SelectorError::MissingActor,
        "DuplicateActor" => SelectorError::DuplicateActor,
        "InvalidCallerScope" => SelectorError::InvalidCallerScope,
        "InvalidIssuedRecord" => SelectorError::InvalidIssuedRecord,
        "DuplicateOrdinal" => SelectorError::DuplicateOrdinal,
        "DuplicateNonce" => SelectorError::DuplicateNonce,
        "InvalidNextNonce" => SelectorError::InvalidNextNonce,
        "MissingSlot" => SelectorError::MissingSlot,
        "DuplicateSlot" => SelectorError::DuplicateSlot,
        "InvalidGeneration" => SelectorError::InvalidGeneration,
        other => panic!("unknown literal selector error {other}"),
    }
}

fn expected_bytes(value: &Value) -> RawIdentity {
    let hex = text(&value["expected_raw_hex"]);
    assert_eq!(hex.len(), 32);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let mut bytes = [0; 16];
    for (i, part) in bytes.iter_mut().enumerate() {
        *part = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
    }
    // Independently cross-check the two frozen representations. The expected
    // returned bytes remain the literal hex, never a product packing result.
    let tuple = array(&value["expected_raw_tuple"]);
    assert_eq!(tuple.len(), 7);
    assert_eq!(bytes[0], byte(&tuple[0]));
    assert_eq!(bytes[1], byte(&tuple[1]));
    assert_eq!(bytes[2], byte(&tuple[2]));
    for (offset, field) in [(4, 3), (6, 4), (8, 5), (10, 6)] {
        assert_eq!(
            u16::from_le_bytes([bytes[offset], bytes[offset + 1]]),
            word(&tuple[field])
        );
    }
    assert_eq!(bytes[3], 0);
    assert_eq!(&bytes[12..], &[0; 4]);
    RawIdentity(bytes)
}

/// One exact selector input and literal expected response, prepared outside its worker.
pub(super) struct SelectorCase {
    pub(super) id: String,
    pub(super) actor: u8,
    pub(super) kind: TargetKind,
    pub(super) selector: TargetSelector,
    pub(super) expected: Result<RawIdentity, SelectorError>,
    pub(super) read_set: Option<ReadSet>,
}

impl SelectorCase {
    /// Read frozen data directly. This method does not resolve or validate a target.
    pub(super) fn read(value: &Value) -> Self {
        assert_eq!(value["state_must_be_unchanged"], true);
        let input = &value["input"];
        fields(input, &["actor", "kind", "selector"]);
        let selector = input["selector"].as_object().expect("literal selector");
        assert_eq!(selector.len(), 1);
        let (name, index) = selector.iter().next().unwrap();
        let selector = match name.as_str() {
            "Issued" => TargetSelector::Issued(byte(index)),
            "FrozenUnissued" => TargetSelector::FrozenUnissued(byte(index)),
            other => panic!("selector {other}"),
        };
        let read_set = if let Some(name) = value.get("full_literal") {
            assert_eq!(text(name), "initial");
            assert!(value.get("read_set").is_none());
            None
        } else {
            Some(ReadSet::read(&value["read_set"]))
        };
        let expected = if value["expected_error"].is_null() {
            Ok(expected_bytes(value))
        } else {
            assert!(value["expected_raw_hex"].is_null() && value["expected_raw_tuple"].is_null());
            Err(expected_error(&value["expected_error"]))
        };
        Self {
            id: text(&value["id"]).to_owned(),
            actor: byte(&input["actor"]),
            kind: match text(&input["kind"]) {
                "Statement" => TargetKind::Statement,
                "Result" => TargetKind::Result,
                "Context" => TargetKind::Context,
                other => panic!("target kind {other}"),
            },
            selector,
            expected,
            read_set,
        }
    }
}
