# PR #56 추가 수정 독립 검토 — 2026-11-06

Verdict: ACCEPT — D1, required_open: 0

Reviewer: `/root/pr56_independent_review`; Author: `/root`.
범위: 원본 PR tip `2b399982b77c5e2fa99e6c220d835db2e47f632c` 이후 Rust 3파일과 신규 시험 선택.

## 설계·방향 검토

퇴역 페이지를 Allocated로 유지하고 reached/retired 집합을 분리하는 설계, split/unlink wire grammar 분리,
canonical before 재계산 후 전체 바이트 대조는 일관된다. 원본에서 필수 수정 P2 1건을 발견했다:
Base4가 각 항목의 retire LSN과 Structural high-water를 비교하지 않아 계약의 상한을 보장하지 못했다.

## 수정 검토

`current_tree::observe`는 이제 `item.retire_lsn > high`를 IdentityMismatch로 거절한다.
기존 anchor LSN·retired page LSN·도달/소유 검증은 유지한다. 기존 `high`를 재사용하며 새 지역값·출력 타입·
중첩 호출·할당이 없어 source census 증가는 없다. 필수 수정은 해소됐다.

신규 시험 3개는 관련성과 검출력이 충분하다. 두 unlink fixture에서 high-water 90과 capture/anchor 91을
구성하여 retire 90의 정상 전체 decode와 retire 91의 거절을 확인한다. 연속 LeafUnlink 두 번 → RootCollapse
→ RootSplit은 실제 descriptor와 high-water를 다음 입력으로 넘기고 독립 XTR1 목록·행·allocator 보존과
퇴역 페이지 비재사용을 확인한다. 두 opcode의 core 각 256 byte의 낮은 비트 하나를 뒤집은 512개 입력은
체크섬을 다시 계산한 뒤 거절된다(모든 byte 값의 전수 검사는 아님).

[원 실행 증거](codex-tests.md)를 읽었고 재실행하지 않았다. red3는 제품 반례 RED(None vs IdentityMismatch),
green은 수정 후 1 PASS다. red2의 frame 두 시험은 PASS이며 같은 실행의 경계 시험 실패는 capture fixture
설정 오류다. 초기 harness 실패를 제품 결함 증거로 쓰지 않고 보존하는 방식이 타당하다.

기존 module gate 구조/Base4 mapping에 신규 정확한 case 2개·1개와 head source를 추가한 것을 확인했다.
이전 D2의 역사적 blob·검토자 기록은 보존한다.

## 검토한 Git blob

| 파일 | blob |
| --- | --- |
| `crates/engine/src/storage/plan4_recovery_base/row/current_tree.rs` | `83b6daf7b401fa37055ca9874d4550254c6f7161` |
| `crates/engine/src/storage/plan4_recovery_base/row/graph/current_tree_tests.rs` | `e9ee7ebf3e1f90258676be240a486b4393e767f1` |
| `crates/engine/src/storage/semantic_row/structure/weights/unlink_frame_tests.rs` | `36f43acf6749f88302465230f482f6cfcba24a76` |

원 로그 SHA-256:

- red3: `16f37bf7c77926de64a295b333a6ebcb601a63172bb4176ab086cfe8efeb9400`
- green: `46eb0f5a7c2888ab78abc0a906359a2082aeaed43d36c019c7ef9d0005ae3c6d`
- red2: `2c78892b77aef33093c941a25d8b067988db5b7bc565281f1746b9db1c0c83dd`

## 한계

U2 replay, U3 live, U4 crash는 후속이다. 이 판정은 읽기 검토이며 fmt/Clippy/document/module gate와
원격 CI의 실행 결과를 포함하지 않는다. 제품 완성·내구성 인수 판정도 아니다.
