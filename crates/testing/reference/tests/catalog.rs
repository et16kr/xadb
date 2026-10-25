mod support {
    pub mod catalog_cases;
}
use support::catalog_cases::*;
use test_protocol::engine::{Action, Cell, Column, DataType, Observation, Row};
use test_reference::catalog::{CatalogResult, check_catalog};

#[test]
fn literal_catalog_scalar_identity_old_cursor_and_pin_vectors() {
    let trace = literal_catalog();
    assert_eq!(check_catalog(&trace.pairs), CatalogResult::Accepted);
    for index in [1, 2, 3, 12, 13, 14, 15, 17, 18, 19, 22] {
        let mut bad = trace.pairs.clone();
        bad[index].1.observation = Observation::Ready {};
        assert!(matches!(check_catalog(&bad), CatalogResult::Rejected {pair,..} if pair == index));
    }
    for mutate in 0..4 {
        let mut bad = trace.pairs.clone();
        if let Observation::Page {
            rows,
            eof,
            canonical_bytes,
        } = &mut bad[15].1.observation
        {
            match mutate {
                0 => *eof = true,
                1 => *canonical_bytes += 1,
                2 => rows[0].identity.table.timeline += 1,
                _ => rows[0].values[0] = Cell::Utf8(String::new()),
            }
        }
        assert!(matches!(
            check_catalog(&bad),
            CatalogResult::Rejected { pair: 15, .. }
        ));
    }
}

#[test]
fn failed_schema_and_rows_consume_aliases_but_preserve_root_and_id_issuance() {
    let mut trace = Trace::new();
    trace.create();
    trace.insert(1, 1, 42);
    trace.add(
        Action::Create {
            table: 2,
            spec: schema(),
        },
        error("XADB-ENGINE-0003"),
    );
    let mut invalid = schema();
    invalid.columns[0].name = String::new();
    trace.add(
        Action::Create {
            table: 3,
            spec: invalid,
        },
        error("XADB-ENGINE-0001"),
    );
    trace.add(
        Action::Insert {
            table: 1,
            row: 2,
            values: vec![Cell::Null],
        },
        error("XADB-ENGINE-0002"),
    );
    trace.insert(3, 2, 7);
    trace.add(Action::Snapshot { snapshot: 1 }, Observation::Done {});
    trace.add(Action::Delete { table: 1, row: 1 }, Observation::Done {});
    trace.add(
        Action::Replace {
            table: 1,
            row: 1,
            values: vec![Cell::Null],
        },
        error("XADB-ENGINE-0006"),
    );
    trace.add(
        Action::Read {
            snapshot: 1,
            table: 1,
            row: 1,
        },
        Observation::Row {
            value: Some(Row {
                identity: row_id(1, 1),
                values: vec![Cell::Int64(42)],
            }),
        },
    );
    trace.add(Action::Snapshot { snapshot: 2 }, Observation::Done {});
    trace.add(
        Action::Read {
            snapshot: 2,
            table: 1,
            row: 1,
        },
        Observation::Row { value: None },
    );
    assert_eq!(check_catalog(&trace.pairs), CatalogResult::Accepted);
    let mut bad = trace.pairs.clone();
    bad[6].1.observation = Observation::Inserted {
        identity: row_id(1, 3),
    };
    assert!(matches!(
        check_catalog(&bad),
        CatalogResult::Rejected { pair: 6, .. }
    ));
    for (action, reason) in [
        (
            Action::Create {
                table: 2,
                spec: schema(),
            },
            "invalid/reused alias",
        ),
        (
            Action::Insert {
                table: 1,
                row: 2,
                values: vec![Cell::Int64(1)],
            },
            "invalid/reused alias",
        ),
        (
            Action::Read {
                snapshot: 1,
                table: 2,
                row: 1,
            },
            "inactive alias",
        ),
    ] {
        let mut next = Trace {
            pairs: trace.pairs.clone(),
        };
        next.add(action, Observation::Done {});
        assert!(
            matches!(check_catalog(&next.pairs), CatalogResult::Invalid {reason:actual,..} if actual == reason)
        );
    }
}

#[test]
fn malformed_envelopes_duplicate_missing_and_retired_capabilities_never_pass() {
    let good = generated(0, 128);
    for mutation in 0..5 {
        let mut bad = good.clone();
        match mutation {
            0 => bad[0].0.run = [0; 16],
            1 => bad[4].0.request = 4,
            2 => bad[4].0.actor = 1,
            3 => {
                bad.remove(4);
            }
            _ => {
                bad.insert(4, bad[4].clone());
            }
        }
        assert!(matches!(check_catalog(&bad), CatalogResult::Invalid { .. }));
    }
    let mut bad = good.clone();
    bad[5].1.actor = 1;
    assert!(matches!(
        check_catalog(&bad),
        CatalogResult::Rejected { pair: 5, .. }
    ));
    assert!(matches!(check_catalog(&[]), CatalogResult::Invalid { .. }));
    let mut trace = Trace::new();
    trace.create();
    trace.add(Action::Snapshot { snapshot: 1 }, Observation::Done {});
    trace.add(
        Action::ReleaseSnapshot { snapshot: 1 },
        Observation::Done {},
    );
    trace.add(
        Action::Lookup {
            snapshot: 1,
            table: 2,
            name: "t".into(),
        },
        Observation::Done {},
    );
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid { pair: 4, .. }
    ));
}

#[test]
fn fixed_profile_and_logical_eight_mib_boundary_are_checked_before_copy() {
    for extra in [0, 1] {
        let mut trace = Trace::new();
        trace.pairs[0].1.observation = error(&"x".repeat(8 * 1024 * 1024 - 512 + extra));
        let result = check_catalog(&trace.pairs);
        assert_eq!(matches!(result, CatalogResult::Invalid { .. }), extra == 1);
        if extra == 0 {
            assert!(matches!(result, CatalogResult::Rejected { .. }));
        }
    }
    let mut trace = Trace::new();
    trace.create();
    trace.add(
        Action::Lookup {
            snapshot: 1,
            table: 2,
            name: "x".repeat(130),
        },
        Observation::Done {},
    );
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid {
            pair: 2,
            reason: "lookup name profile"
        }
    ));
    let mut trace = Trace::new();
    trace.create();
    for snapshot in 1..=9 {
        trace.add(Action::Snapshot { snapshot }, Observation::Done {});
    }
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid {
            pair: 10,
            reason: "snapshot profile"
        }
    ));
    let mut trace = Trace::new();
    trace.create();
    for row in 1..=129 {
        trace.insert(row, row, 0);
    }
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid {
            pair: 130,
            reason: "live row profile"
        }
    ));
}

#[test]
fn catalog_v2_64_seeds_execute_128_pairs_and_reject_mixed_snapshots() {
    for seed in 0..64 {
        let good = generated(seed, 128);
        assert_eq!(check_catalog(&good), CatalogResult::Accepted);
        let mut bad = good;
        let fresh = bad[8].1.observation.clone();
        bad[5].1.observation = fresh;
        assert!(matches!(
            check_catalog(&bad),
            CatalogResult::Rejected { pair: 5, .. }
        ));
    }
}
#[test]
fn catalog_maximum_1024_pairs_preserves_fixed_snapshots() {
    assert_eq!(
        check_catalog(&generated(u64::MAX, 1024)),
        CatalogResult::Accepted
    );
}
#[test]
#[ignore = "explicit V3 independent catalog profile"]
fn catalog_v3_256_seeds_execute_1024_pairs() {
    for seed in 0..256 {
        assert_eq!(
            check_catalog(&generated(seed, 1024)),
            CatalogResult::Accepted
        );
    }
}

#[test]
fn canonical_page_budget_orders_all_128_rows_and_rejects_permutation() {
    let mut trace = Trace::new();
    let mut spec = schema();
    spec.columns = (0..4)
        .map(|index| Column {
            name: index.to_string(),
            data_type: DataType::Binary,
            nullable: false,
        })
        .collect();
    trace.add(
        Action::Create {
            table: 1,
            spec: spec.clone(),
        },
        Observation::Table {
            identity: table_id(1),
            spec,
        },
    );
    let values = vec![Cell::Binary(vec![0; 1024]); 4];
    for row in 1..=128 {
        trace.add(
            Action::Insert {
                table: 1,
                row,
                values: values.clone(),
            },
            Observation::Inserted {
                identity: row_id(1, row),
            },
        );
    }
    trace.add(Action::Snapshot { snapshot: 1 }, Observation::Done {});
    trace.add(
        Action::OpenScan {
            snapshot: 1,
            table: 1,
            cursor: 1,
        },
        Observation::Done {},
    );
    for (start, end, bytes, eof) in [
        (1, 62, 258912, false),
        (63, 124, 258912, false),
        (125, 128, 16704, true),
    ] {
        trace.add(
            Action::NextPage {
                cursor: 1,
                limit: 1024,
            },
            Observation::Page {
                rows: (start..=end)
                    .map(|row| Row {
                        identity: row_id(1, row),
                        values: values.clone(),
                    })
                    .collect(),
                canonical_bytes: bytes,
                eof,
            },
        );
    }
    assert_eq!(check_catalog(&trace.pairs), CatalogResult::Accepted);
    let page = trace.pairs.len() - 3;
    if let Observation::Page { rows, .. } = &mut trace.pairs[page].1.observation {
        rows.swap(0, 1);
    }
    assert!(matches!(check_catalog(&trace.pairs),CatalogResult::Rejected {pair,..} if pair==page));
}

#[test]
fn input_budget_accounts_schema_lookup_and_observed_cell_bytes() {
    // Pair bases 3*512 + Create request/response schemas 2*(1+64+1)
    // + Lookup name129 =1797 bytes before the final error string.
    for extra in [0, 1] {
        let mut trace = Trace::new();
        trace.create();
        trace.add(
            Action::Lookup {
                snapshot: 1,
                table: 2,
                name: "x".repeat(129),
            },
            error(&"e".repeat(8 * 1024 * 1024 - 1797 + extra)),
        );
        let expected = if extra == 0 {
            "inactive alias"
        } else {
            "logical input/observation budget"
        };
        assert!(
            matches!(check_catalog(&trace.pairs),CatalogResult::Invalid {reason,..} if reason==expected)
        );
    }
    // One pair512 + returned row128 + cell32 + binary payload, including
    // a bad observation: the preflight must still account every observed byte.
    for extra in [0, 1] {
        let mut trace = Trace::new();
        trace.pairs[0].1.observation = Observation::Row {
            value: Some(Row {
                identity: row_id(1, 1),
                values: vec![Cell::Binary(vec![0; 8 * 1024 * 1024 - 672 + extra])],
            }),
        };
        assert_eq!(
            matches!(check_catalog(&trace.pairs), CatalogResult::Invalid { .. }),
            extra == 1
        );
    }
}

#[test]
fn table_cursor_name_value_and_metric_profile_boundaries_are_explicit() {
    let mut trace = Trace::new();
    for alias in 1..=5 {
        let mut spec = schema();
        spec.name = alias.to_string();
        trace.add(
            Action::Create {
                table: alias,
                spec: spec.clone(),
            },
            Observation::Table {
                identity: table_id(alias),
                spec,
            },
        );
        if alias == 4 {
            assert_eq!(check_catalog(&trace.pairs), CatalogResult::Accepted);
        }
    }
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid {
            pair: 5,
            reason: "live table profile"
        }
    ));
    let mut trace = Trace::new();
    trace.create();
    trace.add(Action::Snapshot { snapshot: 1 }, Observation::Done {});
    for cursor in 1..=9 {
        trace.add(
            Action::OpenScan {
                snapshot: 1,
                table: 1,
                cursor,
            },
            Observation::Done {},
        );
        if cursor == 8 {
            assert_eq!(check_catalog(&trace.pairs), CatalogResult::Accepted);
        }
    }
    assert!(matches!(
        check_catalog(&trace.pairs),
        CatalogResult::Invalid {
            pair: 11,
            reason: "cursor profile"
        }
    ));
    for length in [128, 129, 130] {
        let mut trace = Trace::new();
        let mut spec = schema();
        spec.name = "a".repeat(length);
        let result = if length == 128 {
            Observation::Table {
                identity: table_id(1),
                spec: spec.clone(),
            }
        } else {
            error("XADB-ENGINE-0001")
        };
        trace.add(Action::Create { table: 1, spec }, result);
        assert_eq!(
            matches!(check_catalog(&trace.pairs), CatalogResult::Accepted),
            length <= 129
        );
    }
    for length in [1024, 1025] {
        let mut trace = Trace::new();
        let mut spec = schema();
        spec.columns[0].data_type = DataType::Binary;
        trace.add(
            Action::Create {
                table: 1,
                spec: spec.clone(),
            },
            Observation::Table {
                identity: table_id(1),
                spec,
            },
        );
        trace.add(
            Action::Insert {
                table: 1,
                row: 1,
                values: vec![Cell::Binary(vec![0; length])],
            },
            Observation::Inserted {
                identity: row_id(1, 1),
            },
        );
        assert_eq!(
            matches!(check_catalog(&trace.pairs), CatalogResult::Accepted),
            length == 1024
        );
    }
    for field in 0..4 {
        let mut trace = literal_catalog();
        if let Observation::Metrics { counters } = &mut trace.pairs[22].1.observation {
            match field {
                0 => counters.row_references_copied = 1,
                1 => counters.resident_bytes = 64 * 1024 * 1024 + 1,
                2 => counters.operations += 1,
                _ => counters.rows_read += 1,
            }
        }
        assert!(matches!(
            check_catalog(&trace.pairs),
            CatalogResult::Rejected { pair: 22, .. }
        ));
    }
}

#[test]
fn incremental_input_budget_remains_failed_and_matches_pair_limit() {
    use test_reference::catalog::CatalogInputBudget;
    let pairs = generated(0, 1024);
    let mut budget = CatalogInputBudget::default();
    for (request, response) in &pairs {
        budget.admit(request, response).unwrap();
    }
    assert_eq!(budget.admit(&pairs[0].0, &pairs[0].1), Err("request count"));
    assert_eq!(
        budget.admit(&pairs[0].0, &pairs[0].1),
        Err("input budget already failed")
    );
}
