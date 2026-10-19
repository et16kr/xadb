mod soak_support;
mod support;
use std::time::Duration;

#[test]
fn preserved_codec_corpus_and_mutation_generator_match_independent_oracle() {
    assert_eq!(soak_support::corpus_replay(), 13);
    let mut state = soak_support::SEED;
    for iteration in 0..256 {
        soak_support::codec_case(&soak_support::generated_codec(&mut state, iteration));
    }
}

#[test]
#[ignore = "V3 actual thirty-minute parser mutation campaign"]
fn codec_fuzz_thirty_minutes() {
    soak_support::codec_soak(Duration::from_secs(1800));
}
