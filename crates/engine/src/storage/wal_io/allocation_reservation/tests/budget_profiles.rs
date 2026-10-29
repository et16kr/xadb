//! New shared producer/validator resource-context cases. No allocation stage/I/O
//! operation is executed; with_owner supplies only the selected initial context.
use super::*;
use operation::OperationBudget;

const DIRTY_INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);

fn fixture(budget: OperationBudget, part: &str) -> Vec<u8> {
    match budget {
        OperationBudget::Exclusive81920 => literal(1, 1, part),
        OperationBudget::Dirty98304 => {
            std::fs::read(format!("{DIRTY_INPUT}/p1-n1-{part}")).unwrap()
        }
        OperationBudget::StatusInit114688 | OperationBudget::MixedInit114688 => {
            unreachable!("separate INIT input")
        }
    }
}

fn page_fixture(budget: OperationBudget, part: &str) -> Box<[u8; PAGE_BYTES]> {
    fixture(budget, part)
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| panic!("wrong independent page fixture size"))
}

#[test]
fn allocation_resource_profiles_exact_context() {
    with_owner(1, |owner, state| {
        let before_files = files(state);
        let original_prefix = owner.installed.fresh.core.wal.prefix().unwrap();
        let original_observation = owner.observe().unwrap();
        let profile = owner.installed.fresh.core.plan.profile;
        let identity = owner.state.identity;
        let mut before = boxed::<PAGE_BYTES>();
        before.copy_from_slice(owner.state.frame);
        let batch = Batch {
            owner: SystemWorkRef::new(
                profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                1,
                Generation::new(1).unwrap(),
            )
            .unwrap(),
            mask: 2,
            birth: 1,
        };
        let reserve_plan = operation::Plan {
            profile,
            identity,
            prefix: original_prefix,
            batch,
            cancel: false,
            before_descriptor: None,
        };
        let length = operation::frame_length(1);
        let mut case_count = 0;
        for budget in [OperationBudget::Exclusive81920, OperationBudget::Dirty98304] {
            let opposite = match budget {
                OperationBudget::Exclusive81920 => OperationBudget::Dirty98304,
                OperationBudget::Dirty98304 => OperationBudget::Exclusive81920,
                OperationBudget::StatusInit114688 | OperationBudget::MixedInit114688 => {
                    unreachable!("separate INIT input")
                }
            };
            let mut output = boxed::<FRAME_WORK_BYTES>();
            let mut after = boxed::<PAGE_BYTES>();
            // The old public-private wrapper now delegates to the changed
            // trusted-context implementation; the new case covers that diff.
            match budget {
                OperationBudget::Exclusive81920 => {
                    operation::prepare(reserve_plan, &before, &mut output[..], &mut after).unwrap();
                }
                OperationBudget::Dirty98304 => {
                    operation::prepare_with_budget(
                        reserve_plan,
                        &before,
                        &mut output[..],
                        &mut after,
                        budget,
                    )
                    .unwrap();
                }
                OperationBudget::StatusInit114688 | OperationBudget::MixedInit114688 => {
                    unreachable!("separate INIT input")
                }
            }
            let reserve_frame = fixture(budget, "reserve.frame");
            let reserve_page = page_fixture(budget, "reserve.page");
            let reserve_descriptor = fixture(budget, "reserve.descriptor");
            assert_eq!(&output[..length], reserve_frame);
            assert_eq!(&after[..], &reserve_page[..]);
            assert_eq!(&output[352..1888], reserve_descriptor);
            operation::validate_with_budget(
                reserve_plan,
                &before,
                &reserve_frame,
                &reserve_page,
                budget,
            )
            .unwrap();
            assert_eq!(
                operation::validate_with_budget(
                    reserve_plan,
                    &before,
                    &reserve_frame,
                    &reserve_page,
                    opposite,
                ),
                Err(SemanticError::BudgetMismatch)
            );
            case_count += 1;
            println!("BUDGET_PROFILE_CASE {budget:?} reserve_full_bytes_and_context");

            let reserved = allocation_page::decode_allocation_page(
                &reserve_page[..],
                identity,
                profile.spec().namespace,
            )
            .unwrap();
            operation::validate_live_with_budget(
                &reserved,
                profile,
                Some(batch),
                &reserve_descriptor,
                1,
                budget,
            )
            .unwrap();
            assert_eq!(
                operation::validate_live_with_budget(
                    &reserved,
                    profile,
                    Some(batch),
                    &reserve_descriptor,
                    1,
                    opposite,
                ),
                Err(SemanticError::StateMismatch)
            );
            if budget == OperationBudget::Exclusive81920 {
                operation::validate(reserve_plan, &before, &reserve_frame, &reserve_page).unwrap();
                operation::validate_live(&reserved, profile, Some(batch), &reserve_descriptor, 1)
                    .unwrap();
            }
            case_count += 1;
            println!("BUDGET_PROFILE_CASE {budget:?} live_reserved_context");

            // This is an explicit codec plan for the independently specified
            // LSN1 predecessor. The live session remains pristine; no stage runs.
            let cancel_prefix = WalPrefix {
                complete_lsn: 1,
                complete_offset: 4096 + length as u64,
                synced_lsn: 1,
                synced_offset: 4096 + length as u64,
                ..original_prefix
            };
            let cancel_plan = operation::Plan {
                prefix: cancel_prefix,
                cancel: true,
                before_descriptor: Some(&reserve_descriptor),
                ..reserve_plan
            };
            match budget {
                OperationBudget::Exclusive81920 => {
                    operation::prepare(cancel_plan, &reserve_page, &mut output[..], &mut after)
                        .unwrap();
                }
                OperationBudget::Dirty98304 => {
                    operation::prepare_with_budget(
                        cancel_plan,
                        &reserve_page,
                        &mut output[..],
                        &mut after,
                        budget,
                    )
                    .unwrap();
                }
                OperationBudget::StatusInit114688 | OperationBudget::MixedInit114688 => {
                    unreachable!("separate INIT input")
                }
            }
            let cancel_frame = fixture(budget, "cancel.frame");
            let cancel_page = page_fixture(budget, "cancel.page");
            let finished_descriptor = fixture(budget, "cancel.descriptor");
            assert_eq!(&output[..length], cancel_frame);
            assert_eq!(&after[..], &cancel_page[..]);
            assert_eq!(&output[352..1888], finished_descriptor);
            operation::validate_with_budget(
                cancel_plan,
                &reserve_page,
                &cancel_frame,
                &cancel_page,
                budget,
            )
            .unwrap();
            assert_eq!(
                operation::validate_with_budget(
                    cancel_plan,
                    &reserve_page,
                    &cancel_frame,
                    &cancel_page,
                    opposite,
                ),
                Err(SemanticError::StateMismatch)
            );
            case_count += 1;
            println!("BUDGET_PROFILE_CASE {budget:?} cancel_full_bytes_and_before_budget");

            let opposite_descriptor = fixture(opposite, "reserve.descriptor");
            let cross_plan = operation::Plan {
                before_descriptor: Some(&opposite_descriptor),
                ..cancel_plan
            };
            // Mismatched hash fails before semantic budget comparison.
            assert_eq!(
                operation::validate_with_budget(
                    cross_plan,
                    &reserve_page,
                    &cancel_frame,
                    &cancel_page,
                    budget,
                ),
                Err(SemanticError::OrderMismatch)
            );
            // The opposite-context complete literal has a coherent descriptor
            // hash/checksum, but trusted caller context still rejects its before.
            assert_eq!(
                operation::validate_with_budget(
                    cross_plan,
                    &reserve_page,
                    &fixture(opposite, "cancel.frame"),
                    &cancel_page,
                    budget,
                ),
                Err(SemanticError::StateMismatch)
            );
            match operation::prepare_with_budget(
                cross_plan,
                &reserve_page,
                &mut output[..],
                &mut after,
                budget,
            ) {
                Err(error) => assert_eq!(error, SemanticError::StateMismatch),
                Ok(_) => panic!("producer accepted opposite-context before descriptor"),
            }
            case_count += 1;
            println!("BUDGET_PROFILE_CASE {budget:?} cancel_hash_and_coherent_cross_context");

            let finished = allocation_page::decode_allocation_page(
                &cancel_page[..],
                identity,
                profile.spec().namespace,
            )
            .unwrap();
            // Finished rows have no remaining memory claim. They intentionally
            // contain no wire tag from which a caller context could be inferred.
            for context in [budget, opposite] {
                operation::validate_live_with_budget(
                    &finished,
                    profile,
                    None,
                    &finished_descriptor,
                    1,
                    context,
                )
                .unwrap();
            }
            if budget == OperationBudget::Exclusive81920 {
                operation::validate(cancel_plan, &reserve_page, &cancel_frame, &cancel_page)
                    .unwrap();
                operation::validate_live(&finished, profile, None, &finished_descriptor, 1)
                    .unwrap();
            }
            case_count += 1;
            println!(
                "BUDGET_PROFILE_CASE {budget:?} finished_zero_remaining_not_context_authority"
            );
        }
        assert_eq!(case_count, 10);
        assert_eq!(
            owner.installed.fresh.core.wal.prefix().unwrap(),
            original_prefix
        );
        assert_eq!(owner.observe().unwrap(), original_observation);
        assert_eq!(&owner.state.frame[..], &before[..]);
        assert_eq!(files(state), before_files);
        assert!(state.lock().unwrap().events.is_empty());
        println!("BUDGET_PROFILE_COMPLETE 10 product_stage=0 product_sync=0");
    });
}
