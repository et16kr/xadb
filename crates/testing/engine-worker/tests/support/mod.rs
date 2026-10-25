use test_protocol::engine::{Action, Cell, Column, DataType, Observation, Response, TableSpec};

pub fn initialize() -> Action {
    Action::Initialize {
        database: [2; 16],
        timeline: 1,
        epoch: 1,
        resident_bytes: 1024 * 1024,
        snapshot_pins: 32,
    }
}

pub fn generated(seed: u64, requests: u64, mut send: impl FnMut(u64, Action) -> Response) {
    assert_eq!(send(1, initialize()).observation, Observation::Ready {});
    let spec = TableSpec {
        name: "t".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    };
    let Observation::Table { identity, .. } =
        send(2, Action::Create { table: 1, spec }).observation
    else {
        panic!("table")
    };
    assert_eq!(identity.object, 0x0000_0001_0000_0001);
    let Observation::Inserted { identity } = send(
        3,
        Action::Insert {
            table: 1,
            row: 1,
            values: vec![Cell::Int64(0)],
        },
    )
    .observation
    else {
        panic!("row")
    };
    assert_eq!(identity.row, 0x0000_0001_0000_0001);
    let mut random = seed;
    let mut expected = 0i64;
    let mut id = 4;
    for cycle in 0..(requests - 4) / 4 {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = random as i64;
        let valid = cycle % 2 == 0;
        let values = vec![if valid {
            Cell::Int64(value)
        } else {
            Cell::Utf8("wrong type".into())
        }];
        let response = send(
            id,
            Action::Replace {
                table: 1,
                row: 1,
                values,
            },
        );
        assert_eq!(
            (
                response.run,
                response.request,
                response.actor,
                response.version
            ),
            ([1; 16], id, 0, 1)
        );
        if valid {
            assert_eq!(response.observation, Observation::Done {});
            expected = value;
        } else {
            assert_eq!(
                response.observation,
                Observation::ProductError {
                    code: "XADB-ENGINE-0002".into()
                }
            );
        }
        id += 1;
        let snapshot = cycle + 1;
        assert_eq!(
            send(id, Action::Snapshot { snapshot }).observation,
            Observation::Done {}
        );
        id += 1;
        let response = send(
            id,
            Action::Read {
                snapshot,
                table: 1,
                row: 1,
            },
        );
        assert_eq!(
            (response.request, response.actor, response.run),
            (id, 0, [1; 16])
        );
        let Observation::Row { value: Some(row) } = response.observation else {
            panic!("read")
        };
        assert_eq!(row.identity, identity);
        assert_eq!(
            row.values,
            vec![Cell::Int64(expected)],
            "seed {seed} cycle {cycle}"
        );
        id += 1;
        assert_eq!(
            send(id, Action::ReleaseSnapshot { snapshot }).observation,
            Observation::Done {}
        );
        id += 1;
    }
    assert_eq!(id, requests);
    let Observation::Metrics { counters } = send(id, Action::Metrics {}).observation else {
        panic!("metrics")
    };
    assert_eq!(counters.snapshot_pins, 0);
}
