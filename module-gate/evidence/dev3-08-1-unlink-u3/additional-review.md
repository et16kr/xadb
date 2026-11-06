# PR 58 추가 코드 리뷰와 경계 시험

2026-11-06. 대상은 main `92b22bf97f80af99b17ec468348529f41a3e819e` 대비
PR head `319048cb6fd7997a1f400861f0aca2ba9e75ce32`다. 사용자가 설계·방향·코드 리뷰,
새 추가 시험과 발견된 문제의 수정 commit을 요청했다. 담당은 `/root`, 별도 검토자는
`/root/independent_review`다. 기존 U3 검토·실행 기록은 그대로 보존한다.

## 설계와 코드 판단

제품 방향과 [U3 계약](../../../devdoc/contracts/dev3-tree-smo.md#unlink-u3)을 대조했다.
RAM note를 권한이 아닌 힌트로 두고, 현재 anchor에서 다시 route한 뒤 canonical replay와
resident before image·stamp를 검사해 기존 structural shell·설치 본문으로 처리하는 방향은
적절하다. DeletePrune·live INSERT CLR의 설치 뒤 note 생성, ControlCut과 note 쓰기 시점,
LOAD 보호 집합, structural bank·XOI 갱신, stage 뒤 fence, A의 Busy 종료와 flush 재시도를
검토했다. 이 검토에서 수정이 필요한 제품 결함은 확인하지 못했다.

현재 한계도 유지한다. unlink마다 canonical replay·fsync 비용이 들고 퇴역 page는 Allocated로
남는다. CREATE Catalog Job이 남은 제품 table의 note는 버린다. U4 native crash cut은 이
검토에서 실행하지 않았으며, 메모리 namespace의 current-build restart를 native 내구성 인수나
08-1 완료로 확대하지 않는다. A drain census의 14 B 여유는 후속 변경 때 재산정할 항목이다.

## 새 시험과 선택 이유

기존 테스트를 통째로 재실행하는 대신 다음 세 함수를 추가했다.

- `unl_review_refilled_leaf_discards_stale_note_then_can_be_unlinked`: C 정리만 수행해
  오른쪽 빈 leaf의 note를 남긴 뒤 새 INSERT로 같은 leaf를 채운다. 직접 unlink는 NotNeeded,
  파일 이미지·anchor·행은 불변, note만 제거해야 한다. 새 행을 다시 DELETE하면 새 note로
  RootCollapse가 가능하고, 남은 행은 새 statement와 current-build restart에서 같아야 한다.
- `unl_review_four_pending_notes_follow_leaf_unlinks_into_root_collapse`: 4개 leaf의 15행을
  한 거래에서 지우고 drain한다. 두 LeafUnlink, RootCollapse, 새 root의 NotNeeded로 끝나
  unlink 수는 3, 도달 root leaf는 하나, 퇴역 집합은 옛 root와 나머지 leaf 셋이어야 한다.
  그 root에 새 INSERT를 하고 읽기·restart 결과를 대조한다.
- `unl_review_flush_failures_preserve_prior_progress_and_pending_notes`: unlink 하나 성공 뒤
  다음 note에서 try-flush Fatal, continue-flush Fatal, continue-flush Busy 세 경우를 독립
  literal 호출열과 비교한다. 기존 완료 수·대기 note·후속 note가 보존되고 오류 stage와
  must-resolve가 원인에 맞아야 한다.

별도 검토자가 새 diff와 기존 literal harness·live helper를 읽고 선택의 관련성,
독립 기대값, 오류 판정, 실제 결과 검토를 맡았다. 정적 D1 ACCEPT, 필수 지적 0이다.
검토 blob은 다음과 같다.

| 파일 | Git blob |
| --- | --- |
| `crates/engine/src/storage/row/insert/driver/tests.rs` | `8befc822a08545fecc634e2203ac007f6154ebef` |
| `crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs` | `a3880528c4f35353fd412c18a305c0e339432475` |

## 실행 기록

실행 환경은 Linux x86_64, Rust 1.95.0, debug test profile이다. 로컬 로그·소스 manifest·binary
hash는 `/workspace/scratch/pr58-review/`에 보존한다. build target은 기록 후 제거한다.

첫 `cargo test -p xadb-engine --lib --locked unl_review_ -- --nocapture` 호출은 선택과 harness
설정 오류가 있었다. 새 flush 테스트(세 fault)는 PASS, 새 refill 테스트는
`XADB_CATALOG_SOURCE_SHA256` 누락으로 시나리오 진입 전에 FAILED다. prefix가 기존 U1/U2
시험 세 개에도 맞아 의도하지 않은 실행이 있었고 그 셋은 PASS였다. 이를 신규 검증 수에
포함하지 않으며 전체 suite의 결과로 해석하지 않는다. CHAIN은 이 최초 바이너리에 없다.
이후 완료된 flush 시험은 반복하지 않고, 실제 소스 manifest·ELF·입력 hash와 32 MiB test
thread stack을 지정한 바이너리에서 두 live 함수를 각각 `--exact --nocapture`로 선택한다.
원 실패 로그는 `new-tests.log`로 보존한다.

두 live 시험의 source manifest SHA-256은 `a8f6fcbcff7606e23918b0df1c79445eb802026a294c0041491c007e82ff5b84`,
ELF SHA-256은 `0852b2d9fa9c1d3c39b7a57bb7143ca7bc4a2bbdf95a3fc08119d558ad5ce89d`다.

REFILL의 첫 `--exact` 실행도 61.10초에 FAILED였다. `leaves` 시험 helper가 leaf의 첫
RowId(`Option`)로 정렬하므로 빈 leaf는 앞쪽으로 이동하는데, 새 시험이 빈 leaf를 여전히
index 1로 가정했다. 실패한 값은 살아 있는 왼쪽 leaf `[1, 2, 3]`이었다. 제품 코드를
바꾸지 않고 빈 leaf와 refilled leaf를 안정적인 PageRef로 찾아 판정하도록 시험을 고쳤다.
원 로그 `live-1.log`의 SHA-256은
`efcf6466bb0153df2f779de7c6b705363262e109e9e84b2197e0474200e06888`이다.
CHAIN은 최초 검토 blob `a2e5d08cf366acfe03595b99f3c7110238dfa981`의 바이너리에서 실행하며,
REFILL의 위 두 assertion 수정과 무관한 CHAIN 함수는 반복하지 않는다. 수정한 REFILL은
별도 build와 소스·ELF hash를 기록해 실행한다.

수정 REFILL: **PASS**, 1 passed / 0 failed / 2420 filtered, 80.23초.
소스 manifest `b17cba94f62d46b26f532c4ebf61d5557b806ff981298abd849e907ab57a92de`,
ELF `d9cc1d1d591ce868c6d55df2f0659aa2fa26bd7e6ae4a4d079ed4aad53d820cb`,
로그 `refill/live-1.log` SHA-256 `10688b1943e629079093534c2d90c947feaef3df235820203cff81261480f732`.

최초 flush PASS·harness 설정 실패·오선택을 함께 보존한 `new-tests.log` SHA-256은
`0e162d8fb7c0e4b3f656b5ef7561f273f68d0bff2a0a421177f39a37f99d09ec`이다.

CHAIN: **PASS**, 1 passed / 0 failed / 2420 filtered, 429.44초.
로그 `chain/live-1.log` SHA-256 `84dd5236973747f38de0a6fcf5d3781d07bbbe1af4ebc95f7c1544f9543d1a3f`.

최종 신규 검증은 3함수 PASS다(그중 flush 함수는 세 fault를 검증). 제품 소스는 변경하지
않았고, 새 시험의 목록 위치 가정만 수정했다. 별도 검토자가 exact 로그·결과·hash 결속과
module-gate의 최신 시험 blob 및 선택을 확인했다. 최종 독립 D1 ACCEPT, 필수 지적 0이다. 기존 검토의 hash·판정은 수정하지 않고
새 D1 기록을 추가한다. 전체 workspace·기존 영향 batch·native fault cut을 재실행한 결과가 아니다.
