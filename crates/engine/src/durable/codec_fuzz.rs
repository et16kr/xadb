//! Bounded current-build parser mutation campaign. Round-trip properties are
//! parser self-checks; independent disk/history verdicts remain in the Python
//! oracle. This is deterministic mutation fuzzing, not coverage-guided fuzzing.
use super::codec::*;
use crate::api::{Column, DataType, DatabaseId, TableSpec};
use std::{
    fs,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};
use xadb_foundation::{
    codec::encode_frame,
    identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch},
    value::Value,
};

const GENERATOR: &str = "dev2-disk-record-mutation-v1";
const SEED: u64 = 0x584144424449534b;
const OPERATORS: usize = 11;

fn corpus() -> Vec<(Identity, Record)> {
    let id = Identity {
        database: DatabaseId::new([71; 16]).unwrap(),
        timeline: 9,
        epoch: 2,
    };
    let table = ObjectHandle {
        object: ObjectId::new((1 << 32) | 1).unwrap(),
        generation: Generation::new(1).unwrap(),
        schema_epoch: SchemaEpoch::new(1).unwrap(),
    };
    let schema = TableImage {
        identity: table,
        spec: TableSpec {
            name: "items".into(),
            columns: vec![
                Column {
                    name: "n".into(),
                    data_type: DataType::Int64,
                    nullable: true,
                },
                Column {
                    name: "b".into(),
                    data_type: DataType::Bool,
                    nullable: false,
                },
                Column {
                    name: "i".into(),
                    data_type: DataType::Int64,
                    nullable: false,
                },
                Column {
                    name: "s".into(),
                    data_type: DataType::Utf8,
                    nullable: false,
                },
                Column {
                    name: "v".into(),
                    data_type: DataType::Binary,
                    nullable: false,
                },
            ],
        },
    };
    let row = RowImage {
        table,
        row: RowId::new((2 << 32) | 1).unwrap(),
        values: vec![
            Value::Null,
            Value::Bool(true),
            Value::Int64(i64::MIN),
            Value::Utf8("한글\0é".into()),
            Value::Bytes(vec![0, 255, 1]),
        ],
    };
    let prior = Link {
        epoch: 1,
        segment: 1,
        end_offset: 256,
        file_size: 256,
        q: 3,
        prefix_digest: [1; 32],
    };
    let reference = ManifestRef {
        generation: 2,
        digest: [2; 32],
    };
    let manifest = Manifest {
        generation: 2,
        cut: 3,
        epoch_high_water: 2,
        anchor: Anchor {
            epoch: 2,
            segment: 1,
            header_digest: [3; 32],
            offset: 128,
            prefix_digest: [4; 32],
            q: 3,
        },
        tables: vec![schema.clone()],
        chunks: vec![ChunkRef {
            index: 0,
            bytes: 400,
            digest: [5; 32],
            rows: 1,
        }],
        provenance: None,
    };
    let records = vec![
        Record::Header(Header {
            segment: 1,
            start_q: 0,
            origin: Origin::Genesis,
            previous: None,
            resume_digest: [0; 32],
        }),
        Record::Header(Header {
            segment: 2,
            start_q: 3,
            origin: Origin::Rotation,
            previous: Some(Link {
                epoch: 2,
                ..prior.clone()
            }),
            resume_digest: [0; 32],
        }),
        Record::Header(Header {
            segment: 1,
            start_q: 3,
            origin: Origin::Resume,
            previous: Some(prior.clone()),
            resume_digest: [6; 32],
        }),
        Record::Header(Header {
            segment: 1,
            start_q: 3,
            origin: Origin::Restore,
            previous: None,
            resume_digest: [0; 32],
        }),
        Record::Batch(Batch {
            txn_local: 1,
            previous_q: 0,
            q: 1,
            changes: vec![Change::Create(schema)],
        }),
        Record::Batch(Batch {
            txn_local: 2,
            previous_q: 1,
            q: 2,
            changes: vec![Change::Drop(table)],
        }),
        Record::Batch(Batch {
            txn_local: 3,
            previous_q: 2,
            q: 3,
            changes: vec![Change::Put(row.clone())],
        }),
        Record::Batch(Batch {
            txn_local: 4,
            previous_q: 3,
            q: 4,
            changes: vec![Change::Delete {
                table,
                row: row.row,
            }],
        }),
        Record::Seal {
            end_offset: 256,
            q: 3,
            prefix_digest: [7; 32],
        },
        Record::Resume {
            segment: 1,
            previous: prior,
        },
        Record::Writer {
            segment: 1,
            header_digest: [8; 32],
        },
        Record::Current {
            current: reference.clone(),
            previous: None,
        },
        Record::Current {
            current: reference,
            previous: Some(ManifestRef {
                generation: 1,
                digest: [9; 32],
            }),
        },
        Record::Manifest(manifest.clone()),
        Record::Manifest(Manifest {
            provenance: Some(Provenance {
                database: id.database,
                timeline: 8,
                generation: 2,
                digest: [10; 32],
            }),
            ..manifest
        }),
        Record::Chunk {
            generation: 2,
            index: 0,
            rows: vec![row],
        },
    ];
    records.into_iter().map(|record| (id, record)).collect()
}

fn next(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}
fn mutated(frame: &[u8], operator: usize, random: usize) -> (Vec<u8>, bool) {
    let mut raw = frame.to_vec();
    match operator {
        0 => {
            raw[random % frame.len()] ^= 1;
            (raw, true)
        }
        1 => {
            raw.truncate(random % frame.len());
            (raw, true)
        }
        2 => {
            raw[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
            (raw, true)
        }
        _ => {
            let mut payload = frame[10..frame.len() - 32].to_vec();
            match operator {
                3 => payload[random % (frame.len() - FRAME_OVERHEAD)] ^= 1 << (random % 8),
                4 => payload.insert(random % payload.len(), (random >> 8) as u8),
                5 => {
                    payload.remove(random % payload.len());
                }
                6 => payload[14] = 255,
                7 => payload[39..43].fill(0),
                8 => payload[4..6].copy_from_slice(&2_u16.to_le_bytes()),
                9 => payload[6..14].copy_from_slice(&u64::MAX.to_le_bytes()),
                10 => payload.push(0),
                _ => unreachable!(),
            }
            (encode_frame(&payload).unwrap(), operator >= 6)
        }
    }
}
fn verify(bytes: &[u8], known_invalid: bool) -> Result<bool, &'static str> {
    match decode(bytes) {
        Err(_) => Ok(false),
        Ok((id, record)) if !known_invalid => {
            let again = encode(id, &record).map_err(|_| "accepted record cannot re-encode")?;
            if again != bytes {
                return Err("accepted record is not canonical");
            }
            if decode(&again).ok() != Some((id, record)) {
                return Err("second decode changed record");
            }
            Ok(true)
        }
        Ok(_) => Err("known-invalid input accepted"),
    }
}
fn run(extended: bool) {
    let corpus = corpus();
    assert_eq!(corpus.len(), 16);
    let frames: Vec<_> = corpus
        .iter()
        .map(|(id, record)| {
            let frame = encode(*id, record).unwrap();
            assert_eq!(decode(&frame).unwrap(), (*id, record.clone()));
            frame
        })
        .collect();
    let seeds = if extended { 256 } else { 64 };
    let per_seed = if extended { 1024 } else { 128 };
    let minimum = if extended { 1800 } else { 0 };
    println!(
        "XADB_DISK_FUZZ_PROFILE {{\"generator\":\"{GENERATOR}\",\"seed\":{SEED},\"seed_count\":{seeds},\"operations_per_seed\":{per_seed},\"min_seconds\":{minimum},\"input_cap\":1048618,\"coverage_guided\":false,\"variants\":16,\"operators\":11}}"
    );
    let started = Instant::now();
    let mut progress = Instant::now();
    let mut seed_visits = [0_u64; 256];
    let mut variants = [0_u64; 16];
    let mut operators = [0_u64; OPERATORS];
    let mut accepted = 0_u64;
    let mut rejected = 0_u64;
    let mut round = 0_u64;
    loop {
        let seed = round as usize % seeds;
        let mut random = SEED ^ ((seed as u64 + 1) << 32) ^ round.wrapping_mul(0x9e3779b97f4a7c15);
        for case in 0..per_seed {
            let variant = case % frames.len();
            let operator = (case / frames.len() + seed) % OPERATORS;
            let (bytes, known_invalid) = mutated(&frames[variant], operator, next(&mut random));
            match catch_unwind(AssertUnwindSafe(|| verify(&bytes, known_invalid))) {
                Ok(Ok(true)) => accepted += 1,
                Ok(Ok(false)) => rejected += 1,
                failure => {
                    let reason = match &failure {
                        Ok(Err(reason)) => *reason,
                        _ => "decoder or encoder panicked",
                    };
                    let parent = std::env::var_os("XADBTEST_V3_FAILURE_DIR")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir);
                    let path =
                        parent.join(format!("disk-codec-{}-{round}-{case}", std::process::id()));
                    fs::create_dir(&path).unwrap();
                    let mut raw = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path.join("input.bin"))
                        .unwrap();
                    raw.write_all(&bytes).unwrap();
                    let mut receipt = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path.join("case.txt"))
                        .unwrap();
                    writeln!(receipt, "{GENERATOR} seed={seed} round={round} case={case} variant={variant} operator={operator} known_invalid={known_invalid} {reason}").unwrap();
                    eprintln!("retained codec failure: {}", path.display());
                    match failure {
                        Err(payload) => resume_unwind(payload),
                        _ => panic!(
                            "{reason}: seed={seed} round={round} case={case} variant={variant} operator={operator}"
                        ),
                    }
                }
            }
            variants[variant] += 1;
            operators[operator] += 1;
            seed_visits[seed] += 1;
        }
        round += 1;
        if round >= seeds as u64 && started.elapsed() >= Duration::from_secs(minimum) {
            break;
        }
        if progress.elapsed() >= Duration::from_secs(30) {
            println!(
                "XADB_DISK_FUZZ_PROGRESS cases={} elapsed_ms={}",
                accepted + rejected,
                started.elapsed().as_millis()
            );
            progress = Instant::now();
        }
    }
    assert!(
        seed_visits[..seeds]
            .iter()
            .all(|visits| *visits >= per_seed as u64)
    );
    assert!(variants.iter().all(|visits| *visits > 0));
    assert!(operators.iter().all(|visits| *visits > 0));
    assert!(accepted > 0 && rejected > 0);
    assert_eq!(accepted + rejected, operators.iter().sum::<u64>());
    assert_eq!(accepted + rejected, seed_visits.iter().sum::<u64>());
    println!(
        "XADB_DISK_FUZZ_RESULT {{\"generator\":\"{GENERATOR}\",\"seed\":{SEED},\"elapsed_ms\":{},\"cases\":{},\"accepted\":{accepted},\"rejected\":{rejected},\"seed_visits\":{:?},\"variant_visits\":{variants:?},\"operator_visits\":{operators:?}}}",
        started.elapsed().as_millis(),
        accepted + rejected,
        &seed_visits[..seeds]
    );
}

#[test]
fn disk_record_mutation_v2_64_seeds() {
    run(false);
}
#[test]
#[ignore = "Requires a retained release binary and at least 1800 seconds"]
fn disk_record_mutation_v3_30_minutes() {
    run(true);
}
