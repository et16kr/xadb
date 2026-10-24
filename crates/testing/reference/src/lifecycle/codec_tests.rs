//! Hand-authored malformed structures and boundaries; frozen golden byte vectors live in literal tests.
use super::*;
use crate::lifecycle::input_schema::{
    BindValue, Environment, InputObligation, InputOwner, RawIdentity,
};
use crate::lifecycle::schema::*;

fn id(kind: IdentityKind, nonce: u16, actor: u8, slot: u8) -> Identity {
    Identity {
        kind,
        actor,
        slot,
        engine: 1,
        session: if actor == 255 {
            0
        } else {
            u16::from(actor) + 1
        },
        nonce,
        generation: if matches!(kind, IdentityKind::Statement | IdentityKind::Result) {
            1
        } else {
            0
        },
    }
}
fn initial() -> State {
    let mut s = State {
        current_engine: EngineRef(1),
        next_nonce: [1; 12],
        ..State::default()
    };
    s.next_nonce[0] = 2;
    s.next_nonce[1] = 3;
    s.engines
        .try_push(EngineRecord {
            id: id(IdentityKind::Engine, 1, 255, 255),
            ..EngineRecord::default()
        })
        .unwrap();
    for actor in 0..2 {
        s.session_incarnations
            .try_push(SessionRecord {
                id: id(IdentityKind::Session, u16::from(actor) + 1, actor, 255),
                engine: EngineRef(1),
                actor,
                ..SessionRecord::default()
            })
            .unwrap();
        s.actors
            .try_push(ActorRecord {
                actor,
                session: SessionRef(u16::from(actor) + 1),
                engine: EngineRef(1),
                ..ActorRecord::default()
            })
            .unwrap();
        for slot in 0..2 {
            s.prepared_slots
                .try_push(PreparedSlot {
                    actor,
                    slot,
                    next_statement_generation: 1,
                    next_result_generation: 1,
                    ..PreparedSlot::default()
                })
                .unwrap();
        }
    }
    s
}
fn open_result() -> State {
    let mut s = initial();
    for kind in [
        IdentityKind::Statement,
        IdentityKind::Result,
        IdentityKind::Context,
        IdentityKind::Transaction,
        IdentityKind::Binding,
    ] {
        s.next_nonce[kind as usize - 1] = 2;
    }
    s.contexts
        .try_push(ContextRecord {
            id: id(IdentityKind::Context, 1, 0, 255),
            issue_ordinal: 1,
            data: ContextData::Completed,
        })
        .unwrap();
    s.issued_statements
        .try_push(StatementRecord {
            id: id(IdentityKind::Statement, 1, 0, 0),
            issue_ordinal: 1,
            session: SessionRef(1),
            creation_context: Some(ContextRef(1)),
            ..StatementRecord::default()
        })
        .unwrap();
    s.bindings
        .try_push(BindingRecord {
            id: id(IdentityKind::Binding, 1, 0, 255),
            statement: StatementRef(1),
            issue_ordinal: 1,
            creation_context: Some(ContextRef(1)),
            ..BindingRecord::default()
        })
        .unwrap();
    s.transactions
        .try_push(TransactionRecord {
            id: id(IdentityKind::Transaction, 1, 0, 255),
            birth_session: SessionRef(1),
            ..TransactionRecord::default()
        })
        .unwrap();
    s.issued_results
        .try_push(ResultRecord {
            id: id(IdentityKind::Result, 1, 0, 0),
            issue_ordinal: 1,
            statement: StatementRef(1),
            public_generation_live: true,
            creation_context: Some(ContextRef(1)),
            ..ResultRecord::default()
        })
        .unwrap();
    s.private_descriptors
        .try_push(Descriptor {
            result: ResultRef(1),
            statement: StatementRef(1),
            public_generation_live: true,
            transaction: Some(TransactionRef(1)),
            ..Descriptor::default()
        })
        .unwrap();
    let p = &mut s.prepared_slots.as_mut_slice()[0];
    p.state = SlotState::ResultOpen;
    p.next_statement_generation = 2;
    p.next_result_generation = 2;
    p.statement = Some(StatementRef(1));
    p.binding = Some(BindingRef(1));
    p.descriptor_result = Some(ResultRef(1));
    s.actors.as_mut_slice()[0].transaction = Some(TransactionRef(1));
    s.actors.as_mut_slice()[0].active_public_result = Some(ResultRef(1));
    s
}
fn encode(s: &State) -> Vec<u8> {
    let mut out = vec![0; MAX_STATE_BYTES];
    let n = encode_state(s, &WireIdentity::default(), &mut out).unwrap();
    out.truncate(n);
    out
}
fn resign(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
fn rejected(bytes: &[u8]) {
    let mut out = State::default();
    assert!(decode_state(bytes, &WireIdentity::default(), &mut out).is_err());
}
fn native_rejected(s: &State) {
    let mut bytes = [0; MAX_STATE_BYTES];
    assert!(encode_state(s, &WireIdentity::default(), &mut bytes).is_err());
}
fn section_offset(bytes: &[u8], section: usize) -> usize {
    let mut at = 256;
    for i in 0..section {
        let n = u16::from_le_bytes([bytes[170 + 2 * i], bytes[171 + 2 * i]]);
        for _ in 0..n {
            at += usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        }
    }
    at
}

#[test]
fn codec_header_checksum_and_length_rejections() {
    let base = encode(&initial());
    assert_eq!(base.len(), 444); // Sum of hand-authored minimal record sizes, fixing the rejection inventory.
    // Every truncated prefix must fail before publishing a state.
    for n in 0..base.len() {
        rejected(&base[..n]);
    }
    for at in [0, 8, 10, 12, 16, 48, 80, 112, 202, 255] {
        let mut changed = base.clone();
        changed[at] ^= 1;
        resign(&mut changed);
        rejected(&changed);
    }
    let mut bad_hash = base.clone();
    bad_hash[256] ^= 1;
    rejected(&bad_hash);
    let mut trailing = base.clone();
    trailing.push(0);
    rejected(&trailing);
    let mut oversized = vec![0; MAX_STATE_BYTES + 1];
    oversized[..8].copy_from_slice(b"XADBLM02");
    rejected(&oversized);
    for capacity in [0, 255, 287, base.len() - 1] {
        let mut short = vec![0; capacity];
        assert!(encode_state(&initial(), &WireIdentity::default(), &mut short).is_err());
    }
}
#[test]
fn codec_record_tags_counts_reserved_and_options() {
    let base = encode(&initial());
    // Initial engine record: length26, id at258, admission274, reserved277, options278..281.
    for (at, value) in [
        (256, 1),
        (274, 255),
        (261, 1),
        (270, 1),
        (277, 1),
        (278, 2),
        (170, 8),
        (174, 1),
        (176, 5),
    ] {
        let mut changed = base.clone();
        changed[at] = value;
        resign(&mut changed);
        rejected(&changed);
    }
    let mut some_zero = base.clone();
    some_zero[278] = 1;
    resign(&mut some_zero);
    rejected(&some_zero);
    let mut short_record = base.clone();
    short_record[256] = 25;
    resign(&mut short_record);
    rejected(&short_record);
}
#[test]
fn codec_identity_order_nonce_and_foreign_links() {
    let base = initial();
    encode(&base);
    let mut s = initial();
    s.session_incarnations.as_mut_slice().swap(0, 1);
    native_rejected(&s);
    let mut s = initial();
    s.session_incarnations.as_mut_slice()[1].id.nonce = 1;
    native_rejected(&s);
    for (kind, next) in [(0, 1), (1, 2), (2, 0), (8, 56)] {
        let mut s = initial();
        s.next_nonce[kind] = next;
        native_rejected(&s);
    }
    for mutate in [0, 1, 2, 3, 4] {
        let mut s = open_result();
        match mutate {
            0 => s.issued_statements.as_mut_slice()[0].id.generation = 0,
            1 => s.issued_results.as_mut_slice()[0].id.slot = 2,
            2 => s.actors.as_mut_slice()[0].session = SessionRef(2),
            3 => s.issued_statements.as_mut_slice()[0].creation_context = Some(ContextRef(6)),
            _ => s.bindings.as_mut_slice()[0].id.actor = 1,
        };
        native_rejected(&s);
    }
}
#[test]
fn codec_completed_payload_and_parked_links() {
    let base = open_result();
    let bytes = encode(&base);
    let at = section_offset(&bytes, 7);
    assert_eq!(&bytes[at..at + 2], &21u16.to_le_bytes());
    let mut resurrected = bytes.clone();
    resurrected.insert(at + 21, 0);
    resurrected[at..at + 2].copy_from_slice(&22u16.to_le_bytes());
    let len = resurrected.len() as u32;
    resurrected[12..16].copy_from_slice(&len.to_le_bytes());
    resign(&mut resurrected);
    rejected(&resurrected);
    let mut parked = open_result();
    parked.contexts.as_mut_slice()[0].data = ContextData::ParkedFetch(ParkedContext {
        api: Api::ParkFetch,
        statement: Some(StatementRef(1)),
        result: Some(ResultRef(1)),
        transaction: Some(TransactionRef(1)),
        ..ParkedContext::default()
    });
    parked.actors.as_mut_slice()[0].parked_context = Some(ContextRef(1));
    parked.transactions.as_mut_slice()[0].parked_context = Some(ContextRef(1));
    encode(&parked);
    for mutation in 0..7 {
        let mut s = State::default();
        s.copy_from(&parked);
        if let ContextData::ParkedFetch(ref mut p) = s.contexts.as_mut_slice()[0].data {
            match mutation {
                0 => p.api = Api::BeginCommit,
                1 => p.statement = None,
                2 => p.result = None,
                3 => p.transaction = None,
                4 => p.cancel_observed_at_completion = true,
                5 => p.deadline_reached = true,
                _ => p.completion_phase = CompletionPhase::AdmittedCompleted,
            }
        }
        native_rejected(&s);
    }
    let mut s = State::default();
    s.copy_from(&parked);
    s.actors.as_mut_slice()[0].parked_context = None;
    native_rejected(&s);
    let mut s = State::default();
    s.copy_from(&parked);
    s.next_nonce[4] = 3;
    let mut second = s.contexts.as_slice()[0];
    second.id.nonce = 2;
    second.issue_ordinal = 2;
    s.contexts.try_push(second).unwrap();
    native_rejected(&s);
    for fetch in [true, false] {
        let mut linked = State::default();
        linked.copy_from(&parked);
        if !fetch {
            linked.contexts.as_mut_slice()[0].data = ContextData::ParkedCommit(ParkedContext {
                api: Api::BeginCommit,
                transaction: Some(TransactionRef(1)),
                ..ParkedContext::default()
            });
            linked.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound;
            linked.actors.as_mut_slice()[0].active_public_result = None;
            linked.private_descriptors.as_mut_slice()[0].state = ResultState::Eof;
            linked.transactions.as_mut_slice()[0].access = Access::ReadWrite;
            linked.transactions.as_mut_slice()[0].progress = CommitProgress::BeforeAttempt;
            encode(&linked);
        }
        for transaction_link in [None, Some(ContextRef(6))] {
            let mut broken = State::default();
            broken.copy_from(&linked);
            broken.transactions.as_mut_slice()[0].parked_context = transaction_link;
            native_rejected(&broken);
        }
    }
    let mut completed = State::default();
    completed.copy_from(&parked);
    completed.contexts.as_mut_slice()[0].data = ContextData::Completed;
    completed.actors.as_mut_slice()[0].parked_context = None;
    native_rejected(&completed); // A retained transaction cannot point at a completed call.
}
#[test]
fn codec_slot_descriptor_and_terminal_payload() {
    encode(&open_result());
    for mutation in 0..9 {
        let mut s = open_result();
        match mutation {
            0 => s.prepared_slots.as_mut_slice()[0].descriptor_result = None,
            1 => s.prepared_slots.as_mut_slice()[0].statement = None,
            2 => s.private_descriptors.as_mut_slice()[0].statement = StatementRef(6),
            3 => s.private_descriptors.as_mut_slice()[0].terminal = Some(Diagnostic::default()),
            4 => {
                s.private_descriptors.as_mut_slice()[0].state = ResultState::Error;
                s.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound;
                s.actors.as_mut_slice()[0].active_public_result = None;
            }
            5 => s.private_descriptors.as_mut_slice()[0].position = 7,
            6 => s.actors.as_mut_slice()[0].active_public_result = None,
            7 => s.prepared_slots.as_mut_slice()[0].next_result_generation = 1,
            _ => s.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound,
        };
        native_rejected(&s);
    }
    for terminal in [ResultState::Eof, ResultState::Error, ResultState::Closed] {
        let mut s = open_result();
        s.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound;
        s.actors.as_mut_slice()[0].active_public_result = None;
        s.private_descriptors.as_mut_slice()[0].state = terminal;
        if terminal == ResultState::Error {
            s.private_descriptors.as_mut_slice()[0].terminal = Some(Diagnostic {
                code: ReturnCode::ReadError,
                ..Diagnostic::default()
            });
        }
        encode(&s);
    }
    let mut fenced = open_result();
    fenced.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound;
    fenced.actors.as_mut_slice()[0].active_public_result = None;
    fenced.engines.as_mut_slice()[0].admission = EngineAdmission::Fenced;
    fenced.engines.as_mut_slice()[0].first_fence_context = Some(ContextRef(1));
    fenced.issued_results.as_mut_slice()[0].public_generation_live = false;
    fenced.issued_results.as_mut_slice()[0].retirement = Retirement::EngineFence;
    fenced.issued_results.as_mut_slice()[0].retirement_context = Some(ContextRef(1));
    fenced.private_descriptors.as_mut_slice()[0].public_generation_live = false;
    fenced.private_descriptors.as_mut_slice()[0].blocked_by_fence = true;
    encode(&fenced);
}

#[test]
fn codec_receipt_anchor_and_owner_shapes() {
    for durable in [false, true] {
        let mut s = open_result();
        s.next_nonce[7] = 2;
        if !durable {
            s.next_nonce[11] = 2;
        }
        s.receipts
            .try_push(ReceiptRecord {
                id: id(IdentityKind::Receipt, 1, 0, 255),
                transaction: TransactionRef(1),
                engine: EngineRef(1),
                kind: if durable {
                    ReceiptKind::DurableWrite
                } else {
                    ReceiptKind::NoWriteCompletion
                },
                anchor: if durable {
                    None
                } else {
                    Some(id(IdentityKind::Anchor, 1, 255, 255))
                },
                ..ReceiptRecord::default()
            })
            .unwrap();
        native_rejected(&s);
    }
    for disposition in [
        OwnerDisposition::Held,
        OwnerDisposition::Transferred,
        OwnerDisposition::Released,
    ] {
        let mut s = open_result();
        s.next_nonce[8] = 2;
        s.owners
            .try_push(OwnerRecord {
                id: id(IdentityKind::Resource, 1, 0, 255),
                resource_kind: ResourceKind::Plan,
                disposition,
                original_holder: HolderRef {
                    kind: IdentityKind::Statement,
                    nonce: 1,
                },
                current_holder: if disposition == OwnerDisposition::Released {
                    Some(HolderRef {
                        kind: IdentityKind::Statement,
                        nonce: 1,
                    })
                } else {
                    None
                },
                ..OwnerRecord::default()
            })
            .unwrap();
        native_rejected(&s);
    }
}
#[test]
fn codec_input_preserves_raw_negative_payloads() {
    let mut input = ResolvedInput {
        api: Api::RecoveryConfirmedFreshEngine,
        actor: 255,
        outcome: 4,
        target: Some(RawIdentity([255; 16])),
        context: Some(RawIdentity([254; 16])),
        ..ResolvedInput::default()
    };
    let mut e = Environment {
        kind: FactKind::RecoveryConfirmed,
        fact: RawIdentity([231; 16]),
        cleanup: 4,
        ..Environment::default()
    };
    for value in [255, 1, 1] {
        e.obligations
            .try_push(InputObligation {
                obligation: RawIdentity([value; 16]),
                ..InputObligation::default()
            })
            .unwrap();
    }
    e.owners
        .try_push(InputOwner {
            resource: RawIdentity([255; 16]),
            after_disposition: OwnerDisposition::Released,
            after_holder: Some(RawIdentity([254; 16])),
            ..InputOwner::default()
        })
        .unwrap();
    input.environment = Some(e);
    let mut bytes = [0; MAX_INPUT_BYTES];
    let n = encode_input(&input, &mut bytes).unwrap();
    let mut output = ResolvedInput::default();
    decode_input(&bytes[..n], &mut output).unwrap();
    assert_eq!(output, input);
    let mut binding = ResolvedInput {
        api: Api::BindAll,
        outcome: 6,
        ..ResolvedInput::default()
    };
    binding
        .bind_values
        .try_push(BindValue {
            tag: 0,
            byte_length: 1,
            bytes: [255, 0, 0, 0, 0, 0, 0, 0],
        })
        .unwrap();
    let n = encode_input(&binding, &mut bytes).unwrap();
    decode_input(&bytes[..n], &mut output).unwrap();
    assert_eq!(output, binding);
}
#[test]
fn codec_input_structural_caps_and_tags() {
    let mut bytes = [0; MAX_INPUT_BYTES];
    let input = ResolvedInput::default();
    let n = encode_input(&input, &mut bytes).unwrap();
    for at in [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
        let mut bad = bytes[..n].to_vec();
        if at < bad.len() {
            bad[at] = 253;
            assert!(
                decode_input(&bad, &mut ResolvedInput::default()).is_err(),
                "offset {at}"
            );
        }
    }
    for len in 0..n {
        assert!(decode_input(&bytes[..len], &mut ResolvedInput::default()).is_err());
    }
    let mut input = ResolvedInput {
        api: Api::BindAll,
        ..ResolvedInput::default()
    };
    input
        .bind_values
        .try_push(BindValue {
            tag: 0,
            byte_length: 9,
            ..BindValue::default()
        })
        .unwrap();
    assert!(encode_input(&input, &mut bytes).is_err());
    let mut maximum = ResolvedInput {
        api: Api::RecoveryConfirmedFreshEngine,
        actor: 255,
        ..ResolvedInput::default()
    };
    let mut env = Environment {
        kind: FactKind::RecoveryConfirmed,
        cleanup: 4,
        ..Environment::default()
    };
    for _ in 0..7 {
        env.obligations
            .try_push(InputObligation::default())
            .unwrap();
    }
    for _ in 0..54 {
        env.owners.try_push(InputOwner::default()).unwrap();
    }
    maximum.environment = Some(env);
    let n = encode_input(&maximum, &mut bytes).unwrap();
    let mut decoded = ResolvedInput::default();
    decode_input(&bytes[..n], &mut decoded).unwrap();
    assert_eq!(maximum, decoded);
    // Env1 without optional identities starts after the 16-byte minimal input prefix/presence.
    let mut bad = bytes[..n].to_vec();
    bad[45..47].copy_from_slice(&8u16.to_le_bytes());
    assert!(decode_input(&bad, &mut decoded).is_err());
}

// Structurally connected retained records. This is not a reachable-model trace oracle.
fn drained_structure() -> State {
    let mut s = open_result();
    s.actors.as_mut_slice()[0].active_public_result = None;
    s.prepared_slots.as_mut_slice()[0].state = SlotState::ReadyBound;
    s.private_descriptors.as_mut_slice()[0].state = ResultState::Eof;
    s.engines.as_mut_slice()[0].admission = EngineAdmission::Fenced;
    s.engines.as_mut_slice()[0].first_fence_context = Some(ContextRef(1));
    let t = &mut s.transactions.as_mut_slice()[0];
    t.access = Access::ReadWrite;
    t.life = TransactionLife::Ended;
    t.progress = CommitProgress::Published;
    t.receipt = Some(ReceiptRef(1));
    t.obligation = Some(ObligationRef(1));
    s.next_nonce[7] = 2;
    s.next_nonce[8] = 5;
    s.next_nonce[9] = 2;
    s.next_nonce[10] = 2;
    s.next_nonce[11] = 2;
    s.receipts
        .try_push(ReceiptRecord {
            id: id(IdentityKind::Receipt, 1, 0, 255),
            transaction: TransactionRef(1),
            engine: EngineRef(1),
            kind: ReceiptKind::DurableWrite,
            anchor: Some(id(IdentityKind::Anchor, 1, 255, 255)),
            source_fact: Some(FactRef(1)),
            ..ReceiptRecord::default()
        })
        .unwrap();
    s.obligations
        .try_push(ObligationRecord {
            id: id(IdentityKind::Obligation, 1, 0, 255),
            engine: EngineRef(1),
            transaction: TransactionRef(1),
            origin_session: SessionRef(1),
            creation_reason: ObligationReason::PeerActiveTransactionAtFence,
            latest_reason: ObligationReason::PeerActiveTransactionAtFence,
            reason_mask: 8,
            disposition: OwnershipSummary::AllReleased,
            settlement: Settlement::DrainedConfirmed,
            recovery_fact: Some(FactRef(1)),
            ..ObligationRecord::default()
        })
        .unwrap();
    s.facts
        .try_push(FactRecord {
            id: id(IdentityKind::Fact, 1, 255, 255),
            kind: FactKind::CommitProgress,
            api: Api::CommitFact,
            outcome_class: 2,
            consumed_parts: 8,
            engine: EngineRef(1),
            transaction: Some(TransactionRef(1)),
            receipt: Some(ReceiptRef(1)),
            anchor: Some(AnchorRef(1)),
            obligation_count: 1,
            owner_count: 4,
            ..FactRecord::default()
        })
        .unwrap();
    s.fact_obligations
        .try_push(FactObligation {
            fact: FactRef(1),
            obligation: ObligationRef(1),
            disposition: OwnershipSummary::AllReleased,
            owner_count: 4,
            ..FactObligation::default()
        })
        .unwrap();
    for (i, kind) in [
        ResourceKind::Undo,
        ResourceKind::Locks,
        ResourceKind::AbortReserve,
        ResourceKind::Recovery,
    ]
    .into_iter()
    .enumerate()
    {
        let nonce = i as u16 + 1;
        let holder = HolderRef {
            kind: IdentityKind::Transaction,
            nonce: 1,
        };
        s.owners
            .try_push(OwnerRecord {
                id: id(IdentityKind::Resource, nonce, 0, 255),
                resource_kind: kind,
                disposition: OwnerDisposition::Released,
                original_holder: holder,
                previous_holder: Some(holder),
                last_fact: Some(FactRef(1)),
                ..OwnerRecord::default()
            })
            .unwrap();
        s.fact_owners
            .try_push(FactOwner {
                fact: FactRef(1),
                resource: ResourceRef(nonce),
                disposition: OwnerDisposition::Released,
                obligation: Some(ObligationRef(1)),
                before_holder: Some(holder),
                after_holder: None,
            })
            .unwrap();
    }
    s
}
#[test]
fn codec_fact_membership_and_drained_settlement() {
    let base = drained_structure();
    encode(&base);
    for mutation in 0..10 {
        let mut s = State::default();
        s.copy_from(&base);
        match mutation {
            0 => s.facts.as_mut_slice()[0].owner_count = 3,
            1 => s.fact_owners.as_mut_slice()[1].resource = ResourceRef(1),
            2 => s.fact_owners.as_mut_slice().swap(0, 1),
            3 => s.fact_obligations.as_mut_slice()[0].owner_count = 3,
            4 => s.fact_owners.as_mut_slice()[0].obligation = None,
            5 => s.obligations.as_mut_slice()[0].recovery_fact = None,
            6 => s.facts.as_mut_slice()[0].outcome_class = 1,
            7 => s.facts.as_mut_slice()[0].fresh_engine = Some(EngineRef(1)),
            8 => {
                s.owners.as_mut_slice()[0].disposition = OwnerDisposition::Held;
                s.owners.as_mut_slice()[0].current_holder = Some(HolderRef {
                    kind: IdentityKind::Transaction,
                    nonce: 1,
                });
            }
            _ => s.facts.as_mut_slice()[0].consumed_parts = 32,
        };
        native_rejected(&s);
    }
    let mut s = State::default();
    s.copy_from(&base);
    let mut duplicate = s.obligations.as_slice()[0];
    duplicate.id.nonce = 2;
    s.next_nonce[9] = 3;
    s.obligations.try_push(duplicate).unwrap();
    native_rejected(&s);
}
#[test]
fn codec_reuses_output_storage_and_preserves_caller_tail() {
    let state = open_result();
    let mut encoded = [0x5a; MAX_STATE_BYTES];
    let n = encode_state(&state, &WireIdentity::default(), &mut encoded).unwrap();
    assert!(encoded[n..].iter().all(|b| *b == 0x5a));
    let mut out = State::default();
    decode_state(&encoded[..n], &WireIdentity::default(), &mut out).unwrap();
    assert_eq!(out, state);
    let bytes = encode(&initial());
    decode_state(&bytes, &WireIdentity::default(), &mut out).unwrap();
    assert!(
        out.issued_statements.is_empty()
            && out.contexts.is_empty()
            && out.private_descriptors.is_empty()
    );
    // A failed decode makes the whole workspace unvalidated; a later valid call replaces all tables.
    assert!(decode_state(&encoded[..n - 1], &WireIdentity::default(), &mut out).is_err());
    decode_state(&bytes, &WireIdentity::default(), &mut out).unwrap();
    assert_eq!(out, initial());
}
