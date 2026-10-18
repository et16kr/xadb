use xadbtest_engine_worker::Session;
use xadbtest_protocol::engine::{
    Action as A, Cell, Column, DataType, Observation as O, Request, TableSpec,
};

fn initialize() -> A {
    A::Initialize {
        database: [7; 16],
        timeline: 9,
        epoch: 2,
        resident_bytes: 64 * 1024 * 1024,
        snapshot_pins: 32,
    }
}
fn spec(name: &str) -> TableSpec {
    TableSpec {
        name: name.into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Int64,
            nullable: true,
        }],
    }
}
fn request(id: u64, action: A) -> Request {
    Request {
        version: 1,
        run: [3; 16],
        request: id,
        actor: 0,
        action,
    }
}
struct Run {
    session: Session,
    next: u64,
}
impl Run {
    fn new() -> Self {
        let mut run = Self {
            session: Session::new(),
            next: 1,
        };
        assert_eq!(run.call(initialize()), O::Ready {});
        run
    }
    fn call(&mut self, action: A) -> O {
        let id = self.next;
        self.next += 1;
        let response = self.session.dispatch(request(id, action)).unwrap();
        assert_eq!(
            (
                response.run,
                response.request,
                response.actor,
                response.version
            ),
            ([3; 16], id, 0, 1)
        );
        response.observation
    }
    fn create(&mut self, alias: u64, name: &str) -> u64 {
        let O::Table {
            identity,
            spec: actual,
        } = self.call(A::Create {
            table: alias,
            spec: spec(name),
        })
        else {
            panic!("create")
        };
        assert_eq!(actual, spec(name));
        assert_eq!(
            (
                identity.database,
                identity.timeline,
                identity.generation,
                identity.schema_epoch
            ),
            ([7; 16], 9, 1, 1)
        );
        identity.object
    }
}
fn error(code: &str) -> O {
    O::ProductError { code: code.into() }
}

#[test]
fn adapter_uses_actual_crud_and_old_snapshot_capabilities_after_drop_recreate() {
    let mut run = Run::new();
    assert_eq!(run.create(1, "t"), 0x0000_0002_0000_0001);
    let O::Inserted { identity } = run.call(A::Insert {
        table: 1,
        row: 1,
        values: vec![Cell::Int64(7)],
    }) else {
        panic!("insert")
    };
    assert_eq!(identity.row, 0x0000_0002_0000_0001);
    assert_eq!(identity.table.object, 0x0000_0002_0000_0001);
    assert_eq!(run.call(A::Snapshot { snapshot: 1 }), O::Done {});
    assert_eq!(
        run.call(A::OpenScan {
            snapshot: 1,
            table: 1,
            cursor: 1
        }),
        O::Done {}
    );
    assert_eq!(
        run.call(A::Replace {
            table: 1,
            row: 1,
            values: vec![Cell::Int64(8)]
        }),
        O::Done {}
    );
    assert_eq!(run.call(A::Delete { table: 1, row: 1 }), O::Done {});
    run.call(A::Snapshot { snapshot: 2 });
    assert_eq!(
        run.call(A::Read {
            snapshot: 2,
            table: 1,
            row: 1
        }),
        O::Row { value: None }
    );
    assert_eq!(
        run.call(A::Delete { table: 1, row: 1 }),
        error("XADB-ENGINE-0006")
    );
    assert_eq!(run.call(A::DropTable { table: 1 }), O::Done {});
    assert_eq!(run.create(2, "t"), 0x0000_0002_0000_0002);
    run.call(A::Snapshot { snapshot: 3 });
    assert_eq!(
        run.call(A::Read {
            snapshot: 3,
            table: 1,
            row: 1
        }),
        error("XADB-ENGINE-0005")
    );
    let O::Row { value: Some(row) } = run.call(A::Read {
        snapshot: 1,
        table: 1,
        row: 1,
    }) else {
        panic!("old row")
    };
    assert_eq!(row.values, vec![Cell::Int64(7)]);
    assert_eq!(row.identity, identity);
    run.call(A::ReleaseSnapshot { snapshot: 1 });
    let O::Page {
        rows,
        eof,
        canonical_bytes,
    } = run.call(A::NextPage {
        cursor: 1,
        limit: 1024,
    })
    else {
        panic!("page")
    };
    assert!(eof);
    assert_eq!(rows, vec![row]);
    assert_eq!(canonical_bytes, 69); // 56-byte RowUID + 4 column count + tag + i64.
    run.call(A::CloseScan { cursor: 1 });
    run.call(A::ReleaseSnapshot { snapshot: 2 });
    run.call(A::ReleaseSnapshot { snapshot: 3 });
    let O::Metrics { counters } = run.call(A::Metrics {}) else {
        panic!("metrics")
    };
    assert_eq!(counters.snapshot_pins, 0);
}

#[test]
fn adapter_preserves_schema_scalar_tags_and_missing_table_error() {
    let mut run = Run::new();
    let schema = TableSpec {
        name: "한글".into(),
        columns: [
            DataType::Bool,
            DataType::Int64,
            DataType::Utf8,
            DataType::Binary,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, data_type)| Column {
            name: format!("v{index}"),
            data_type,
            nullable: true,
        })
        .collect(),
    };
    run.call(A::Create {
        table: 1,
        spec: schema.clone(),
    });
    let values = vec![
        Cell::Bool(false),
        Cell::Int64(i64::MIN),
        Cell::Utf8("\0한글".into()),
        Cell::Binary(vec![0, 255]),
    ];
    run.call(A::Insert {
        table: 1,
        row: 1,
        values: values.clone(),
    });
    assert_eq!(
        run.call(A::Insert {
            table: 1,
            row: 2,
            values: vec![Cell::Int64(1)]
        }),
        error("XADB-ENGINE-0002")
    );
    run.call(A::Snapshot { snapshot: 1 });
    let O::Table { spec: stored, .. } = run.call(A::Lookup {
        snapshot: 1,
        table: 2,
        name: "한글".into(),
    }) else {
        panic!("lookup")
    };
    assert_eq!(stored, schema);
    let O::Row { value: Some(row) } = run.call(A::Read {
        snapshot: 1,
        table: 2,
        row: 1,
    }) else {
        panic!("read")
    };
    assert_eq!(row.values, values);
    assert_eq!(
        run.call(A::Lookup {
            snapshot: 1,
            table: 3,
            name: "missing".into()
        }),
        error("XADB-ENGINE-0004")
    );
    assert_eq!(
        run.call(A::Replace {
            table: 1,
            row: 1,
            values: vec![Cell::Null; 4]
        }),
        O::Done {}
    );
}

#[test]
fn protocol_failures_poison_session_before_any_later_effect() {
    for bad in [
        request(2, initialize()),
        request(2, A::DropTable { table: 0 }),
        request(2, A::DropTable { table: 17 }),
        request(1, A::Metrics {}),
        Request {
            run: [4; 16],
            ..request(2, A::Metrics {})
        },
        Request {
            actor: 32,
            ..request(2, A::Metrics {})
        },
        Request {
            version: 2,
            ..request(2, A::Metrics {})
        },
    ] {
        let mut run = Run::new();
        assert!(run.session.dispatch(bad).is_err());
        assert!(
            run.session
                .dispatch(request(
                    3,
                    A::Create {
                        table: 1,
                        spec: spec("t")
                    }
                ))
                .is_err()
        );
    }
    for bad in [
        request(1, A::Metrics {}),
        request(2, initialize()),
        Request {
            actor: 1,
            ..request(1, initialize())
        },
    ] {
        let mut session = Session::new();
        assert!(session.dispatch(bad).is_err());
        assert!(session.dispatch(request(1, initialize())).is_err());
    }
}

#[test]
fn failed_and_released_aliases_are_permanently_consumed() {
    let mut failed = Run::new();
    assert_eq!(
        failed.call(A::Create {
            table: 1,
            spec: spec("")
        }),
        error("XADB-ENGINE-0001")
    );
    assert!(
        failed
            .session
            .dispatch(request(
                3,
                A::Create {
                    table: 1,
                    spec: spec("t")
                }
            ))
            .is_err()
    );
    for action in [
        A::Snapshot { snapshot: 1 },
        A::ReleaseSnapshot { snapshot: 1 },
    ] {
        let mut released = Run::new();
        released.call(A::Snapshot { snapshot: 1 });
        released.call(A::ReleaseSnapshot { snapshot: 1 });
        assert!(released.session.dispatch(request(4, action)).is_err());
    }
    let mut failed_row = Run::new();
    failed_row.create(1, "t");
    assert_eq!(
        failed_row.call(A::Insert {
            table: 1,
            row: 1,
            values: vec![]
        }),
        error("XADB-ENGINE-0002")
    );
    assert!(
        failed_row
            .session
            .dispatch(request(
                4,
                A::Insert {
                    table: 1,
                    row: 1,
                    values: vec![Cell::Null]
                }
            ))
            .is_err()
    );
}

#[test]
fn initialization_failure_returns_product_error_then_permanently_closes() {
    let mut session = Session::new();
    let bad = A::Initialize {
        database: [0; 16],
        timeline: 1,
        epoch: 1,
        resident_bytes: 4096,
        snapshot_pins: 1,
    };
    assert_eq!(
        session.dispatch(request(1, bad)).unwrap().observation,
        error("XADB-ENGINE-0001")
    );
    assert!(session.dispatch(request(2, initialize())).is_err());
    assert!(session.dispatch(request(3, A::Metrics {})).is_err());
}

#[test]
fn pin_quota_and_cursor_limit_are_bounded_without_silent_eviction() {
    let mut run = Run::new();
    run.create(1, "t");
    for snapshot in 1..=32 {
        assert_eq!(run.call(A::Snapshot { snapshot }), O::Done {});
    }
    assert_eq!(
        run.call(A::Snapshot { snapshot: 33 }),
        error("XADB-ENGINE-0008")
    );
    run.call(A::ReleaseSnapshot { snapshot: 2 });
    assert_eq!(run.call(A::Snapshot { snapshot: 34 }), O::Done {});
    for cursor in 1..=32 {
        assert_eq!(
            run.call(A::OpenScan {
                snapshot: 1,
                table: 1,
                cursor
            }),
            O::Done {}
        );
    }
    assert!(
        run.session
            .dispatch(request(
                run.next,
                A::OpenScan {
                    snapshot: 1,
                    table: 1,
                    cursor: 33
                }
            ))
            .is_err()
    );
}
