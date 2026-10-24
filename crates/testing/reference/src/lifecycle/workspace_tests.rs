use super::*;
use std::mem::size_of;

#[test]
fn workspace_budget_stack_and_reuse() {
    std::thread::Builder::new()
        .name("lifecycle-workspace-256k".into())
        .stack_size(256 * 1024)
        .spawn(|| {
            let native = size_of::<State>();
            assert!(native <= MAX_WORKSPACE_BYTES);
            assert!(matches!(
                Workspace::try_new_with_budget(native - 1),
                Err(WorkspaceError::Capacity)
            ));
            let mut first = Workspace::try_new_with_budget(native).unwrap();
            let mut second = Workspace::try_new().unwrap();
            assert_eq!(first.owned_bytes(), native);
            assert!(first.owned_bytes() + second.owned_bytes() <= 2 * MAX_WORKSPACE_BYTES);
            for fact in 1..=6 {
                for resource in 1..=54 {
                    first
                        .state_mut()
                        .fact_owners
                        .try_push(FactOwner {
                            fact: FactRef(fact),
                            resource: ResourceRef(resource),
                            disposition: OwnerDisposition::Released,
                            before_holder: Some(HolderRef {
                                kind: IdentityKind::Obligation,
                                nonce: fact,
                            }),
                            ..FactOwner::default()
                        })
                        .unwrap();
                }
            }
            assert_eq!(first.state().fact_owners.len(), 324);
            assert_eq!(
                first.state_mut().fact_owners.try_push(FactOwner::default()),
                Err(TableFull)
            );
            second.state_mut().copy_from(first.state());
            assert_eq!(first.state(), second.state());
            assert_eq!(second.state().fact_owners.as_slice()[323].resource, ResourceRef(54));
            first.state_mut().fact_owners.clear();
            assert_eq!(second.state().fact_owners.len(), 324);
            second.state_mut().copy_from(first.state());
            assert!(second.state().fact_owners.is_empty());
            assert_eq!(first.owned_bytes(), native);
            println!(
                "native_state_bytes={native}; two_workspace_capacity={}; stack_budget=262144; max_membership_rows=324",
                first.owned_bytes() + second.owned_bytes()
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
