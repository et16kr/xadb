//! Root-bound Aging codec inputs; the generic work-body boundary stays closed.
use super::*;
use crate::storage::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    obligation::{InventoryClaim, OwnerChain, RemainingBudget, ResourcePeak, SystemProgress},
    retention_page::tests::{Fixture, cases, cell, gen_value, namespace, put64, read64, resign},
};

fn literal_spec(f: &Fixture, phase: &str) -> DescriptorSpec {
    let raw = f.descriptor(&format!("aging-{phase}.descriptor"));
    let root = cell(f.page(&format!("root-{phase}.page")), 0);
    let mut remaining = [RemainingBudget::default(); 5];
    if matches!(phase, "begin" | "abort") {
        remaining[3] = RemainingBudget {
            steps: 1,
            wal: 55112,
            protected: 55112,
            peak: ResourcePeak {
                frames: read64(raw, 1088),
                memory: read64(raw, 1096),
                locks: read64(raw, 1104),
                registry: read64(raw, 1112),
                files: read64(raw, 1120),
                doublewrite: read64(raw, 1128),
            },
            ..RemainingBudget::default()
        };
    }
    DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference: SystemWorkRef::new(namespace(), SystemWorkKind::Aging, 1, gen_value(1))
                .unwrap(),
            phase: WorkPhase::Running,
            sequence: read64(root, 64),
            cursor: read64(root, 72),
            limit: 64,
        }),
        chain: OwnerChain {
            apply: read64(raw, 72),
            previous: read64(raw, 80),
            birth: 3,
            groups: read64(root, 64) - 1,
        },
        profile: super::profile_reference(f.profile),
        inventory: InventoryClaim {
            change_lsn: 3,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: if phase == "finish" { 74784 } else { 19672 },
        consumed_control: 0,
        remaining,
        minimum_wal_lsn: 3,
        last_mutation_sequence: 0,
    }
}

#[test]
fn aging_descriptor_inputs() {
    cases("A", 12, |n| {
        let f = Fixture::standard();
        match n {
            1 => {
                let mut out = [0xa5; 1536];
                encode(
                    &literal_spec(&f, "start"),
                    f.profile,
                    f.page("root-start.page"),
                    &mut out,
                )
                .unwrap();
                assert_eq!(&out, f.descriptor("aging-start.descriptor"));
            }
            2 | 3 => {
                let phase = if n == 2 { "begin" } else { "finish" };
                let raw = f.descriptor(&format!("aging-{phase}.descriptor"));
                let value = decode(raw, f.profile, f.page(&format!("root-{phase}.page"))).unwrap();
                assert_eq!(value.bytes(), raw);
                assert_eq!(value.descriptor(), literal_spec(&f, phase));
            }
            4 => {
                let expected = literal_spec(&f, "begin");
                assert_eq!(
                    obligation::decode_descriptor(
                        f.descriptor("aging-begin.descriptor"),
                        expected.owner.reference(),
                        expected.profile
                    )
                    .err(),
                    Some(ObligationError::UnsupportedWorkBody)
                );
            }
            5..=12 => {
                let mut raw = f.raw("aging-begin.descriptor").to_vec();
                let expected = match n {
                    5 => {
                        raw[416] ^= 1;
                        AgingObligationError::IdentityMismatch
                    }
                    6 => {
                        raw[448] ^= 1;
                        AgingObligationError::IdentityMismatch
                    }
                    7 => {
                        raw[384] = 6;
                        AgingObligationError::Descriptor(ObligationError::UnsupportedWorkBody)
                    }
                    8 => {
                        put64(&mut raw, 360, 4);
                        put64(&mut raw, 392, 4);
                        AgingObligationError::IdentityMismatch
                    }
                    9 => {
                        put64(&mut raw, 608, 19673);
                        AgingObligationError::BudgetMismatch
                    }
                    10 => {
                        put64(&mut raw, 1040, 55111);
                        put64(&mut raw, 1136, 55111);
                        put64(&mut raw, 1328, 55111);
                        AgingObligationError::BudgetMismatch
                    }
                    11 => {
                        put64(&mut raw, 1336, 2);
                        AgingObligationError::OrderMismatch
                    }
                    12 => AgingObligationError::Descriptor(ObligationError::Corrupt),
                    _ => unreachable!(),
                };
                if n == 12 {
                    raw.truncate(1535);
                } else {
                    resign(&mut raw);
                }
                let before = raw.clone();
                assert_eq!(
                    decode(&raw, f.profile, f.page("root-begin.page")).err(),
                    Some(expected)
                );
                assert_eq!(raw, before);
            }
            _ => unreachable!(),
        }
    });
}
