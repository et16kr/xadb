use xadbtest_protocol::engine::{
    Action, Cell, Column, DataType, Metrics, Observation, Request, Response, Row, RowIdentity,
    TableIdentity, TableSpec,
};

pub fn initialize() -> Action {
    Action::Initialize {
        database: [2; 16],
        timeline: 7,
        epoch: 3,
        resident_bytes: 64 * 1024 * 1024,
        snapshot_pins: 32,
    }
}
pub fn table_id(object: u64) -> TableIdentity {
    TableIdentity {
        database: [2; 16],
        timeline: 7,
        object: 0x0000_0003_0000_0000 + object,
        generation: 1,
        schema_epoch: 1,
    }
}
pub fn row_id(table: u64, row: u64) -> RowIdentity {
    RowIdentity {
        table: table_id(table),
        row: 0x0000_0003_0000_0000 + row,
    }
}
pub fn schema() -> TableSpec {
    TableSpec {
        name: "t".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    }
}
pub fn error(code: &str) -> Observation {
    Observation::ProductError { code: code.into() }
}
#[derive(Default)]
pub struct Trace {
    pub pairs: Vec<(Request, Response)>,
}
impl Trace {
    pub fn add(&mut self, action: Action, observation: Observation) {
        let id = self.pairs.len() as u64 + 1;
        self.pairs.push((
            Request {
                version: 1,
                run: [1; 16],
                request: id,
                actor: 0,
                action,
            },
            Response {
                version: 1,
                run: [1; 16],
                request: id,
                actor: 0,
                observation,
            },
        ));
    }
    pub fn new() -> Self {
        let mut result = Self::default();
        result.add(initialize(), Observation::Ready {});
        result
    }
    pub fn create(&mut self) {
        self.add(
            Action::Create {
                table: 1,
                spec: schema(),
            },
            Observation::Table {
                identity: table_id(1),
                spec: schema(),
            },
        );
    }
    pub fn insert(&mut self, alias: u64, issued: u64, value: i64) {
        self.add(
            Action::Insert {
                table: 1,
                row: alias,
                values: vec![Cell::Int64(value)],
            },
            Observation::Inserted {
                identity: row_id(1, issued),
            },
        );
    }
}

// A scalar fixture independent from the catalog interpreter and production tree.
// Five setup/tail slots plus seven-operation cycles include a retained snapshot,
// changed live row, two independent reads, and explicit resource release.
pub fn generated(seed: u64, requests: usize) -> Vec<(Request, Response)> {
    let mut trace = Trace::new();
    trace.create();
    trace.insert(1, 1, 0);
    let mut random = seed;
    let mut current = 0;
    let mut snapshot = 0;
    while trace.pairs.len() + 7 <= requests {
        snapshot += 1;
        let old = current;
        trace.add(Action::Snapshot { snapshot }, Observation::Done {});
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        current = random as i64;
        trace.add(
            Action::Replace {
                table: 1,
                row: 1,
                values: vec![Cell::Int64(current)],
            },
            Observation::Done {},
        );
        trace.add(
            Action::Read {
                snapshot,
                table: 1,
                row: 1,
            },
            Observation::Row {
                value: Some(Row {
                    identity: row_id(1, 1),
                    values: vec![Cell::Int64(old)],
                }),
            },
        );
        trace.add(Action::ReleaseSnapshot { snapshot }, Observation::Done {});
        snapshot += 1;
        trace.add(Action::Snapshot { snapshot }, Observation::Done {});
        trace.add(
            Action::Read {
                snapshot,
                table: 1,
                row: 1,
            },
            Observation::Row {
                value: Some(Row {
                    identity: row_id(1, 1),
                    values: vec![Cell::Int64(current)],
                }),
            },
        );
        trace.add(Action::ReleaseSnapshot { snapshot }, Observation::Done {});
    }
    while trace.pairs.len() < requests {
        // These deterministic failed statements still count as real requests.
        trace.add(
            Action::Replace {
                table: 1,
                row: 1,
                values: vec![Cell::Null],
            },
            error("XADB-ENGINE-0002"),
        );
    }
    trace.pairs
}

pub fn literal_catalog() -> Trace {
    let mut trace = Trace::new();
    let mut spec = schema();
    spec.columns = [
        ("text", DataType::Utf8, true),
        ("flag", DataType::Bool, false),
        ("number", DataType::Int64, false),
        ("bytes", DataType::Binary, false),
    ]
    .into_iter()
    .map(|(name, data_type, nullable)| Column {
        name: name.into(),
        data_type,
        nullable,
    })
    .collect();
    trace.add(
        Action::Create {
            table: 1,
            spec: spec.clone(),
        },
        Observation::Table {
            identity: table_id(1),
            spec: spec.clone(),
        },
    );
    let first = vec![
        Cell::Null,
        Cell::Bool(true),
        Cell::Int64(i64::MIN),
        Cell::Binary(vec![0, 255]),
    ];
    let second = vec![
        Cell::Utf8("가\0".into()),
        Cell::Bool(false),
        Cell::Int64(i64::MAX),
        Cell::Binary(vec![]),
    ];
    for (row, values) in [(1, first.clone()), (2, second.clone())] {
        trace.add(
            Action::Insert {
                table: 1,
                row,
                values,
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
    trace.add(
        Action::ReleaseSnapshot { snapshot: 1 },
        Observation::Done {},
    );
    trace.add(
        Action::Replace {
            table: 1,
            row: 1,
            values: second.clone(),
        },
        Observation::Done {},
    );
    trace.add(Action::Delete { table: 1, row: 2 }, Observation::Done {});
    trace.add(Action::DropTable { table: 1 }, Observation::Done {});
    trace.add(
        Action::Create {
            table: 2,
            spec: spec.clone(),
        },
        Observation::Table {
            identity: table_id(2),
            spec: spec.clone(),
        },
    );
    trace.add(Action::Snapshot { snapshot: 2 }, Observation::Done {});
    trace.add(
        Action::Lookup {
            snapshot: 2,
            table: 3,
            name: "t".into(),
        },
        Observation::Table {
            identity: table_id(2),
            spec,
        },
    );
    trace.add(
        Action::Read {
            snapshot: 2,
            table: 1,
            row: 1,
        },
        error("XADB-ENGINE-0005"),
    );
    trace.add(
        Action::Read {
            snapshot: 2,
            table: 2,
            row: 1,
        },
        error("XADB-ENGINE-0005"),
    );
    trace.add(
        Action::NextPage {
            cursor: 1,
            limit: 1,
        },
        Observation::Page {
            rows: vec![Row {
                identity: row_id(1, 1),
                values: first,
            }],
            eof: false,
            canonical_bytes: 79,
        },
    );
    trace.add(
        Action::NextPage {
            cursor: 1,
            limit: 0,
        },
        error("XADB-ENGINE-0007"),
    );
    trace.add(
        Action::NextPage {
            cursor: 1,
            limit: 1,
        },
        Observation::Page {
            rows: vec![Row {
                identity: row_id(1, 2),
                values: second,
            }],
            eof: true,
            canonical_bytes: 85,
        },
    );
    trace.add(
        Action::NextPage {
            cursor: 1,
            limit: 1024,
        },
        Observation::Page {
            rows: vec![],
            eof: true,
            canonical_bytes: 0,
        },
    );
    trace.add(
        Action::Metrics {},
        Observation::Metrics {
            counters: Metrics {
                operations: 7,
                rows_read: 2,
                snapshot_pins: 2,
                resident_bytes: 1000,
                row_references_copied: 2,
            },
        },
    );
    trace.add(Action::CloseScan { cursor: 1 }, Observation::Done {});
    trace.add(
        Action::ReleaseSnapshot { snapshot: 2 },
        Observation::Done {},
    );
    trace.add(
        Action::Metrics {},
        Observation::Metrics {
            counters: Metrics {
                operations: 7,
                rows_read: 2,
                snapshot_pins: 0,
                resident_bytes: 500,
                row_references_copied: 2,
            },
        },
    );
    trace
}
