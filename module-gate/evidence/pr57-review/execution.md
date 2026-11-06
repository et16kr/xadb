# PR57 추가 검증 — 2026-11-06

대상은 PR #57, base `ceb2fd8955217763107d6416b307a6a24f6e3495`, 원 head
`7ca3b53ec2657f48303fb6294134fd6d8fa12097`이다. 작성자는 `/root`다.
제품 코드·형식·자원 상한을 수정하지 않고 기존 unlink replay 시험 파일에 신규 5개를 추가했다.
최종 파일과 직접 사용하는 fixture/helper의 SHA256은 [inputs.sha256](inputs.sha256)에 보존한다.

## 선택과 결과

기존 시험이나 전체 workspace를 다시 실행하지 않았다. 모두 U2의 새 scope4 unlink replay 경로에
직접 닿는 신규 시험이다. arena·Job·bank의 실제 바이트, 기존 claim, cursor를 직접 비교하며
허용된 after-image scratch 변경은 canonical 상태 변화로 간주하지 않는다.

| 신규 시험 (`pr57_unlink_` 접두어) | 확인 |
| --- | --- |
| length_boundaries_reject_without_consuming_the_cut | 두 opcode의 envelope/dispatch/group/descriptor/core/native/redo/digest 경계 46개 truncated·resized frame 거절과 후속 정상 수락 |
| all_native_digests_and_last_redo_are_compared_before_publication | wire 문법을 통과하는 native digest 10개·마지막 redo payload 2개 변조를 canonical 비교에서 거절; 앞 anchor redo가 먼저 공개되지 않음 |
| rejects_wrong_stream_cut_and_duplicate_frame | 두 opcode 각각 previous LSN·epoch·timeline 불일치 및 적용 후 동일 frame 재적용 거절 |
| adopts_successor_epoch_only_from_a_seeded_base | 두 opcode의 base seed 없는 epoch 채택·이전 epoch 거절, 새 epoch 수락, 행·claim 보존, raw capture 재해석 |
| refuses_a_valid_nonempty_leaf_cut | 손상되지 않은 독립 ROOT fixture의 nonempty leaf11에 RootCollapse frame을 주면 NotNeeded/IdentityMismatch, 상태 불변 |

실행 환경: Linux x86_64, Rust 1.95.0, debug libtest, `RUST_MIN_STACK=33554432`,
`CARGO_BUILD_JOBS=4`, `CARGO_TARGET_DIR=/workspace/scratch/pr57-target`.
`CARGO_HOME=/workspace/.xadb-env/cargo`, `RUSTUP_HOME=/workspace/.xadb-env/rustup`를 사용했다.

1. `cargo test -p xadb-engine --lib --locked pr57_unlink -- --nocapture`:
   [초기 로그](initial.log), 3 PASS / 1 FAIL. 첫 미커밋 시험 초안의 epoch 전환 replay는 통과했으나
   시험 helper `capture_after`가 seed의 source/issued epoch를 그대로 복사해 Base4 decode에서
   `Base(IdentityMismatch)`가 났다. 이는 제품 결함이 아닌 신규 시험 데이터의 결함이다.
   초기 초안은 별도 commit하지 않았으며 초기 결과를 최종 소스의 결과로 주장하지 않는다.
2. 새 시험의 capture header에서 source epoch(144)·issued epoch(1184)를 successor로 고치고
   reseal했다. native unchanged digest의 동일 mutant 중복도 없앴다. nonempty leaf 시험을 더했다.
   `cargo test -p xadb-engine --lib --locked -- pr57_unlink_all_native pr57_unlink_adopts pr57_unlink_refuses_a_valid --nocapture`:
   [후속 로그](followup.log), 수정·신규 3 PASS / 0 FAIL.
   앞에서 통과한 무변경 길이·stream/중복 시험 2개는 반복하지 않았다. 최종 신규 5개 모두 PASS다.

## 판정과 한계

제품 수정이 필요한 결함은 발견하지 못했다. U2의 canonical 재계산·모든 target 검증 뒤
arena/bank를 바꾸는 순서, 퇴역 page의 Allocated·claim 보존, split/unlink 길이 분기와
opcode 재검증은 계약에 맞는다. source census의 호출·단계별 정적 inventory도 독립 검토했다.
부등식만으로 작은 비지배 항의 누락까지 검출한다고 주장하지 않는다.

anchor의 퇴역 목록은 현재 extent/profile에 맞춘 제한된 단계 설계다. 실제 Free/reuse와
일반적인 큰 tree의 축소까지 구현됐다는 의미는 아니다. live writer·drain은 U3,
native fault cut은 U4의 미완료 범위다. 이 검증은 순수 메모리 replay와 raw Base4 capture이며
실제 디스크 crash/reopen 또는 제품 rebase 인수를 대신하지 않는다. 08-1 완료를 적립하지 않는다.
원 PR head의 두 GitHub Actions workflow는 조회 시 success였으며, 새 commit의 CI와 구분한다.

독립 판정은 [검토 기록](review.md)에 둔다. push 시 commit export에서 fmt·Clippy·문서 검사와
module gate를 수행하며 그 실제 결과는 push 로그로 별도 보존한다.
