mod support;

use support::frontend::{assert_case, observe};
use xadbtest_reference::frontend::literals::{
    MERGE_SEEDS, generated_literals, lexer_literals, limit_literals,
};

#[test]
fn literal_lexer_vectors() {
    let cases = lexer_literals();
    assert_eq!(cases.len(), 289);
    for case in &cases {
        assert_case(case);
    }
}

#[test]
fn source_and_token_limits() {
    let cases = limit_literals();
    assert_eq!(cases.len(), 10);
    for case in &cases {
        assert_case(case);
    }
}

#[test]
fn generated_literal_sequences() {
    for seed in MERGE_SEEDS {
        assert_case(&generated_literals(seed, 128));
    }
}

#[test]
#[ignore = "expanded 256 seeds x 1024 operations; select only for relevant expanded acceptance"]
fn extended_generated_literal_sequences() {
    for seed in 0..=255 {
        assert_case(&generated_literals(seed, 1024));
    }
}

#[test]
#[ignore = "expanded malformed lexer fuzz profile; minimum 30 minutes when explicitly selected"]
fn extended_malformed_fuzz_30_minutes() {
    use std::time::{Duration, Instant};
    let mut state = 0x5841_4442_4c45_5801u64;
    let mut rounds = 0u64;
    let corpus = lexer_literals();
    for case in &corpus {
        assert_case(case);
    }
    let mut raw = vec![0u8; 1_048_577];
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30 * 60) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let length = if rounds.is_multiple_of(32) {
            raw.len()
        } else {
            ((state >> 32) as usize) % 4097
        };
        for byte in &mut raw[..length] {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *byte = (state >> 32) as u8;
        }
        // Every second iteration mutates a preserved valid/malformed literal;
        // the other iterations cover arbitrary bytes and source-limit inputs.
        let mut source = if rounds.is_multiple_of(2) {
            corpus[(rounds as usize / 2) % corpus.len()].source.clone()
        } else {
            raw[..length].to_vec()
        };
        if rounds.is_multiple_of(2) && !source.is_empty() {
            let position = (state as usize) % source.len();
            source[position] = (state >> 24) as u8;
        }
        let observed = observe(&source);
        assert!(observed.complete, "incomplete fuzz round {rounds}");
        assert!(
            observed.tokens.len() <= 65_537,
            "token budget fuzz round {rounds}"
        );
        for token in &observed.tokens {
            assert!(
                token.start <= token.end && token.end <= source.len(),
                "invalid span fuzz round {rounds}"
            );
        }
        if let Some(error) = observed.diagnostic {
            assert!(
                error.start <= error.end && error.end <= source.len(),
                "invalid diagnostic fuzz round {rounds}"
            );
        }
        rounds += 1;
        if rounds.is_multiple_of(128) {
            assert_case(&generated_literals((rounds / 128) % 64, 128));
        }
    }
    assert!(rounds > 0);
    eprintln!(
        "lexer fuzz completed rounds={rounds} duration={:?} seed=0x584144424c455801",
        start.elapsed()
    );
}
