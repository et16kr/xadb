//! U2-C4c D4 selectors: the base4 decoder's cause carrier (`row_base`), the
//! CURRENT fallback predicate, and the exact Profile3 gate of recovery
//! selection. Base inputs are the frozen base4-closure literals (sha256-pinned,
//! read at run time); no product output is a golden or a before image.
use super::support::*;
use super::*;
use crate::storage::{
    control::{ControlError, ControlRef, ProfileSpec, SelectedControl, row_profile},
    retention_page::RetentionError,
};
use std::path::Path;

const FIXTURES: &str = "tests/fixtures/module-row-format";
const GENESIS: (&str, &str) = (
    "base4-closure/genesis.xrb",
    "9b7c330e6af39c640dfc55b9836d4c7a42c7846c0139af6111b7b30be05e8c32",
);
const VARIANTS: (&str, &str) = (
    "base4-closure/variants.bin",
    "9a3a127435d053f7b4f250f10befb8815f211d67a72c4c981ca4fc6c44abef89",
);
const ENVELOPES: (&str, &str) = (
    "base4-closure/envelopes.bin",
    "7f9331527ec54a7c7e3e332b1ccafd9395d539d265c3d4793468a0ff47e06653",
);
const NO_INVENTORY: (&str, &str) = (
    "base4/initial.xrb",
    "9473833616a56fbf74fcfc51d14c5eeb2ff96a4ce7cc3889d3f3eb3e809f4df0",
);
const PROFILES: (&str, &str) = (
    "envelopes/profiles.bin",
    "e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a",
);
/// Profile2 ID1 and Profile1 ID1 envelopes of the support namespace.
const PROFILE2: (&str, &str) = (
    "tests/fixtures/dev3-catalog-create/native/raw/b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24.bin",
    "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24",
);
const PROFILE1: (&str, &str) = (
    "tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin",
    "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4",
);

fn pinned_at(path: &Path, sha: &str) -> Vec<u8> {
    let raw = std::fs::read(path).expect("frozen input");
    assert!(raw.len() <= base::row::BASE_BYTES, "{}", path.display());
    let digest: String = content_digest(&raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, sha, "pin {}", path.display());
    raw
}
fn top() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixture((name, sha): (&str, &str)) -> Vec<u8> {
    pinned_at(&top().join(FIXTURES).join(name), sha)
}
fn repository((path, sha): (&str, &str)) -> Vec<u8> {
    pinned_at(&top().join(path), sha)
}
/// The base4-closure variant run lists over their three parent literals; only
/// the Genesis-parented gate variants are materialized here.
fn genesis_variants(genesis: &[u8]) -> Vec<(String, Vec<u8>)> {
    let raw = fixture(VARIANTS);
    let u32_at = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()) as usize;
    let mut out = Vec::new();
    let mut at = 0;
    while at < raw.len() {
        let (parent, length) = (raw[at] as usize, raw[at + 1] as usize);
        let name = std::str::from_utf8(&raw[at + 2..at + 2 + length])
            .unwrap()
            .to_owned();
        at += 2 + length;
        let runs = u32_at(at);
        at += 4;
        let mut bytes = (parent == 0).then(|| genesis.to_vec());
        for _ in 0..runs {
            let (offset, size) = (u32_at(at), u32_at(at + 4));
            if let Some(bytes) = &mut bytes {
                bytes[offset..offset + size].copy_from_slice(&raw[at + 8..at + 8 + size]);
            }
            at += 8 + size;
        }
        if let Some(bytes) = bytes {
            out.push((name, bytes));
        }
    }
    out
}
fn expected(profile: control::Profile, genesis: &[u8]) -> base::Expected {
    base::Expected {
        profile,
        filemap: SelectedControl {
            generation: 1,
            reference: ControlRef::decode(&genesis[384..448]).unwrap(),
        },
        bootstrap_digest: genesis[464..496].try_into().unwrap(),
    }
}

/// D4: `row_base` keeps the base codec's own cause as `Base` (layout and the
/// shared Catalog body, so base version 0/1/2/3/5/65535 stays
/// `Base(Unsupported)`) and carries every Row-graph cause as `RowBase`; the
/// CURRENT fallback admits exactly Missing, ContentMismatch, `Base(Corrupt)`
/// and `RowBase(Corrupt)`.
#[test]
fn row_base_carrier_and_current_fallback_predicate() {
    use base::row::RowBaseError as Row;
    let may = selection::current_may_fall_back;

    // Every carrier shape, constructed.
    for cause in [
        base::Error::Unsupported,
        base::Error::Corrupt,
        base::Error::IdentityMismatch,
        base::Error::OrderMismatch,
        base::Error::Retention(RetentionError::UnsupportedFormat),
    ] {
        let mapped = row_base(Row::Base(cause));
        assert!(matches!(mapped, Plan4RecoveryCause::Base(c) if c == cause));
        assert_eq!(may(&mapped), cause == base::Error::Corrupt, "{cause:?}");
    }
    let row_causes = || {
        [
            Row::Corrupt,
            Row::Identity,
            Row::Order,
            Row::Unsupported,
            Row::Queue(RetentionError::OrderMismatch),
        ]
    };
    for (cause, again) in row_causes().into_iter().zip(row_causes()) {
        let corrupt = cause == Row::Corrupt;
        let mapped = row_base(cause);
        assert!(matches!(&mapped, Plan4RecoveryCause::RowBase(c) if *c == again));
        assert_eq!(may(&mapped), corrupt, "{again:?}");
    }
    assert!(may(&Plan4RecoveryCause::Missing));
    assert!(may(&Plan4RecoveryCause::ContentMismatch));
    for cause in [
        Plan4RecoveryCause::Unsupported,
        Plan4RecoveryCause::IdentityMismatch,
        Plan4RecoveryCause::Capacity,
        Plan4RecoveryCause::Control(ControlError::UnsupportedFormat),
        Plan4RecoveryCause::Control(ControlError::IdentityMismatch),
    ] {
        assert!(!may(&cause), "{cause:?}");
    }

    // The exact base4 decoder's own outputs over the frozen literals.
    let genesis = fixture(GENESIS);
    let profiles = fixture(PROFILES);
    let p3 = row_profile::decode_profile(&profiles[..4096], None).unwrap();
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    let decoded = |bytes: &[u8], profile, scratch: &mut [[u8; PAGE_BYTES]; 4]| {
        row_base(
            base::row::decode(bytes, expected(profile, &genesis), scratch)
                .err()
                .expect("rejected base"),
        )
    };
    base::row::decode(&genesis, expected(p3, &genesis), &mut scratch).unwrap();
    let (mut versions, mut queues) = (0, 0);
    for (name, bytes) in genesis_variants(&genesis) {
        let mapped = decoded(&bytes, p3, &mut scratch);
        if name.starts_with("base-version-") {
            versions += 1;
            assert!(
                matches!(mapped, Plan4RecoveryCause::Base(base::Error::Unsupported)),
                "{name}: {mapped:?}"
            );
        } else if name.starts_with("root-version-") || name.starts_with("job-version-") {
            queues += 1;
            assert!(
                matches!(
                    mapped,
                    Plan4RecoveryCause::Base(base::Error::Retention(
                        RetentionError::UnsupportedFormat
                    ))
                ),
                "{name}: {mapped:?}"
            );
        } else {
            continue;
        }
        assert!(!may(&mapped), "{name} never falls back");
    }
    // base 0/1/2/3/5/65535 and Root/Job version 2/4.
    assert_eq!((versions, queues), (6, 4));
    // A version1/2 Profile binding is the base codec's Unsupported.
    let envelopes = fixture(ENVELOPES);
    for index in [1, 2] {
        let profile =
            control::decode_bootstrap_profile(&envelopes[index * 4096..(index + 1) * 4096], None)
                .unwrap();
        assert_eq!(profile.format(), index as u16);
        let mapped = decoded(&genesis, profile, &mut scratch);
        assert!(matches!(
            mapped,
            Plan4RecoveryCause::Base(base::Error::Unsupported)
        ));
        assert!(!may(&mapped));
    }
    // A missing XOI1 inventory is the shared body's Corrupt: fallback admitted.
    let mapped = decoded(&fixture(NO_INVENTORY), p3, &mut scratch);
    assert!(matches!(
        mapped,
        Plan4RecoveryCause::Base(base::Error::Corrupt)
    ));
    assert!(may(&mapped));
    println!("ROW_BASE PASS base_versions=6 queue_versions=4 profiles=1,2 authority=NONE");
}

/// MBC40-V01 at the recovery selection gate (`select.rs` profile step): only
/// Profile3 generation1 timeline1 passes it. The genesis-only namespace passes
/// the gate and reads FILEMAP before its own later `Unsupported` (no source
/// closure yet; `faults` S01). Profile1/Profile2 envelopes are the Row
/// decoder's `Control(UnsupportedFormat)` and a Profile3 of generation2 or
/// timeline2 is `Unsupported`, all three before FILEMAP is touched. Every
/// rejection leaves the namespace byte-identical.
#[test]
fn recovery_selection_binds_profile3_generation_timeline() {
    let files = fresh_memory(0);
    // Returns whether the selection reached FILEMAP.
    let reject = |files: &Images, expected, check: &dyn Fn(&Plan4RecoveryCause) -> bool| {
        let d = clone_directory(files);
        let state = d.state();
        clear(&state);
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        match Plan4Recovery::acquire(&d, expected, &mut maintenance, &mut dwb, &mut slot) {
            Ok(_) => panic!("selection accepted"),
            Err(Plan4RecoveryAcquireFailure::Before { error, .. }) => {
                assert!(check(&error.cause), "{:?}", error.cause);
                assert_eq!(error.database_effect, Effect::Unchanged);
            }
            Err(Plan4RecoveryAcquireFailure::Acquired(failed)) => {
                let error = failed.error();
                assert!(check(&error.cause), "{:?}", error.cause);
                assert_eq!(
                    (error.phase, error.database_effect),
                    (Plan4RecoveryPhase::Selection, Effect::Unchanged)
                );
                assert_eq!(failed.observation().mutations_attempted, 0);
            }
        }
        assert_eq!(&images(&state), files);
        assert_closed(&state);
        let s = state.lock().unwrap();
        assert!(s.events.iter().any(|e| e.name == "d3-profile"));
        s.events.iter().any(|e| e.name == "FILEMAP")
    };
    let unsupported = |cause: &Plan4RecoveryCause| matches!(cause, Plan4RecoveryCause::Unsupported);
    assert!(
        reject(&files, Some(namespace()), &unsupported),
        "gate passed"
    );
    let unsupported_format = |cause: &Plan4RecoveryCause| {
        matches!(
            cause,
            Plan4RecoveryCause::Control(ControlError::UnsupportedFormat)
        )
    };
    for envelope in [repository(PROFILE2), repository(PROFILE1)] {
        let mut map = files.clone();
        *map.get_mut("d3-profile").unwrap() = envelope;
        assert!(!reject(&map, Some(namespace()), &unsupported_format));
    }
    let current = row_profile::decode_profile(&files["d3-profile"], Some(namespace())).unwrap();
    for (generation, timeline) in [(2, 1), (1, 2)] {
        let spec = ProfileSpec {
            namespace: Namespace {
                timeline,
                ..namespace()
            },
            generation,
            ..current.spec()
        };
        let mut envelope = vec![0; files["d3-profile"].len()];
        row_profile::encode_profile(spec, &mut envelope).unwrap();
        let mut map = files.clone();
        *map.get_mut("d3-profile").unwrap() = envelope;
        // A supplied namespace would refuse timeline2 as a foreign root first.
        let expected = (timeline == 1).then(namespace);
        assert!(
            !reject(&map, expected, &unsupported),
            "generation{generation} timeline{timeline} stops at the profile gate"
        );
    }
    println!("MBC40-V01 SELECTION PASS profiles=1,2 generation=2 timeline=2 authority=NONE");
}
