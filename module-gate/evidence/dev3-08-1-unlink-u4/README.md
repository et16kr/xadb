# 08-1 U4 live unlink native fault cut 실행 기록

[계약 U4 결과](../../../devdoc/contracts/dev3-tree-smo.md#unlink-u4)의 UNL-N01..N06 native 실행 기록이다. controller
`scripts/check_dev3_unlink_native.py`가 debug test ELF 하나로 사례마다 process를 phase별로 돌렸다. 사례는 cut 여섯 ×
모양 둘(-L LeafUnlink, -R RootCollapse)의 12 이력이다. phase는 produce, recover, reenter이고, N06은 그 사이에 crash가
더 있다. 기대값은 동결 inventory `tests/fixtures/dev3-08-unlink/native-inventory.tsv`와 controller 자신의 규칙에서 온다.

- `run/execution-plan.json`: worker 이름, 사례, source·ELF·input·controller hash, 시간과 log 한도, 사례마다의 기대값.
- `run/results.json`: process마다의 종료 코드와 시간.
- `run/complete.json`: 사례마다의 결과, pid, WAL prefix와 cut이 더한 bytes, 복구 process의 log bytes.
- `run/<사례>/<phase>/receipt.tsv`: worker가 쓴 receipt. produce는 cut 위치, q, unlink 전 WAL prefix의 길이·SHA-256,
  complete LSN, bank high-water, 빈 leaf·root·anchor page다. recover·reenter는 자원 receipt, READY q, route·level·목록,
  high-water, B가 읽은 행 수, (recover만) ack q다.
- `run/<사례>/<phase>/files.json`: process가 끝난 뒤 namespace 파일마다의 크기와 SHA-256.
- `run/source-commit.txt`: 실행한 source commit.
- `review.md`: 독립 D2 검토 기록(최종 회차). `review-r1.md`는 1회차(CONDITIONAL, 필수 2) 기록이다.

source hash는 controller가 실행 때 `git ls-files --cached --others --exclude-standard`의 파일마다 SHA-256을 적은 manifest의
SHA-256이다. 실행 tree는 `run/`과 검토 기록을 더하기 전의 commit(`run/source-commit.txt`)과 같다. 그 commit에서 같은 manifest를 다시 만들면 hash가 같다. ELF,
namespace image, produce의 before image(pool arena·claim), effect trace, test log는 크기 때문에 남기지 않는다. controller가
실행 중에 effect trace와 WAL을 검사했고, image는 `files.json`의 hash로 남는다.

<a id="pr59-review"></a>
## PR #59 추가 검토 — 2026-11-07 KST

기준 PR head는 `8f8cbee3ef0cc8152a1c17ae0d07fdcca508c88a`, base는 `3c9d796da3c54888845ef453f1c77a382316fc29`다.
담당 `/root`가 설계·방향·worker·controller·기존 증거를 대조했다. U4의 process별 crash/recover/reenter,
독립 literal 기대값, route와 retirement 목록의 분리, allocator와 claim 보존 검사는 디스크 DBMS의 복구 우선 방향에 맞는다.
제품 코드 변경은 없으며 전원 단절·복구 중 추가 fault 전수는 이 PR의 증거 범위가 아니다. 일정 적립은 PR 병합 조건부다.
기존 `e3efd16` 전체 파일 manifest를 Git archive로 재구성해 `run/execution-plan.json`의 source hash와 일치함도 확인했다.

판정기에는 다음 결함이 있었다. 모두 검사기 오류이며, 실제 제품 복구가 실패했다는 판정은 아니다.

- N01/N02: WAL read의 요청 길이를 완료 길이로 사용해 short read/EOF도 prefix 도달 증거로 받아들였다.
- N04: cut label만 확인해 sync 직전 ATTEMPT→cut이나 실패한 sync의 trace를 받아들였다.
- N06: publish·sync·dirsync 이름과 횟수만 확인해 실패한 효과나 CURRENT가 아닌 selector도 받아들였다.
- N05 recover: 전체 page를 요청했지만 1 byte만 완료한 home write, 빈 DWB read도 수리 완료로 받아들였다.

D2 선택은 controller가 실제 `OK:n` bytes, 정확한 WAL/selector, 성공한 barrier를 검사하는 것이다.
요청 길이만 강제하는 대안은 정상적인 short I/O 재시도도 막으므로 채택하지 않았다. 같은 파일에 연속해서
완료된 부분 쓰기가 공개 전에 anchor 전체를 덮으면 허용한다. 제품·native adapter의 형식은 바꾸지 않는다.
재검토 조건은 trace 형식, CURRENT 공개 순서, DWB 수리 순서 변경이다.

새 `scripts/test_check_dev3_unlink_native.py`의 8개 테스트는 독립 합성 trace로 부정 사례와 정상 부분 I/O를 대조한다.
수정 전 23개 subtest가 기대한 거절을 하지 않아 FAIL, 수정 후 8개 테스트 모두 PASS다.
명령은 `PYTHONPATH=scripts python3 -B -m unittest -v test_check_dev3_unlink_native`다.
로그는 `review-validation/red.log`, `review-validation/green.log`에 보존한다. 이 새 Python suite는 로컬 선택 실행이며,
기존 CI의 Rust memory twin 셋에 추가된 CI 실행이라고 주장하지 않는다.

보강한 controller로 `UNL-N01-R`, `UNL-N04-R`, `UNL-N05-R`, `UNL-N06-R`의 **4 이력·13 process가 PASS**했다.
N01은 N01/N02 공통 WAL read 분기를, N04는 WAL sync, N05는 DWB 수리, N06은 CURRENT barrier를 검증한다.
RootCollapse 한 모양으로 변경 분기를 모두 검증하며, N01/N02 양쪽과 부분 I/O는 신규 Python 반례로 확인했다.
변경 없는 leaf 모양·기존 memory twin·전체 workspace 시험을 반복하지 않았다. 기존 12 이력 결과는 당시 결과로 유지한다.

`review-validation/source.json`은 실행 코드의 Git blob과 SHA-256, 선택 이유를 기록한다. Rust ELF는 원 PR head와
동일한 Rust 소스로 `cargo test -p xadb-engine --lib --no-run --locked` 빌드했고, Python·문서는 실행 당시 작업 트리다.
`native-execution-plan.json`의 source는 그 시작 시점 전체 파일 manifest의 hash이며 최종 commit hash가 아니다.
전체 manifest와 원 trace·test log·before image는 `/workspace/scratch/pr59-review/native/`에 보존했고,
`native-artifacts.sha256`에 전체 artifact hash를 남겼다. 생성 build target·ELF만 검증 종료 뒤 삭제한다.
`native-results.json`, `native-complete.json`, `native-receipts.json`, `native.log`는 이 선택 실행의 기록이다.

[독립 D2 추가 검토](review-codex.md)는 `/root/pr59_independent_review`의 별도 실행이며,
기존 `review.md`·`review-r1.md`를 대체하거나 과거 source의 판정을 바꾸지 않는다.
