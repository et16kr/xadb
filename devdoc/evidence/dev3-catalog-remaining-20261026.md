# DEV3 Catalog CREATE 남은 메모리 사례 실행

2026-10-26 · [Catalog CREATE 구현 중간 저장](dev3-catalog-live-milestone-20261025.md)에서 NOT_RUN으로 남긴 9개 사례(MC03, MT01–MT07, MF14)를 진행했다. **9개 모두 source별 PASS다.** MT02는 사용자 결정에 따른 제품 보완 뒤 r36에서 통과했다. 첫 Row INSERT는 시작하지 않았다. 이 기록은 전체 DEV3·S5 또는 runtime F05 완료가 아니다.

## 결과

각 사례는 고정 source·ELF에서 한 번씩 실행했다. 실패는 원 revision의 결과로 보존하고 소급 재분류하지 않는다. [source별 기록](dev3-catalog-remaining-20261026.json)에 결과 파일·해시·검토를 연결한다.

| 사례 | 결과 revision | 내용 |
| --- | --- | --- |
| MC03 | r33 PASS | 실패한 Begin이 local1을 소비한 뒤 local2 성공·Commit, 새 runtime 복구·조회 |
| MT01 | r33 FAIL → r34 PASS | q0 공개 직전 gap snapshot은 Missing, ACK 뒤 old snapshot 동안 Prune Busy/WAL0, 해제 후 C05..C10 |
| MT02 | r33 FAIL → r36 PASS | 같은 epoch의 두 번째 CREATE U. Snapshot1에서 T found·U Missing, H1, old snapshot 동안 U Prune Busy/WAL0, C15..C20 뒤 Snapshot2에서 T·U found |
| MT03 | r34 FAIL → r35 PASS | Commit Pending 중 old snapshot 조회가 비상주 Undo를 실제 LOAD(home read)하고 Missing 판정, Commit ACK |
| MT04 | r33 FAIL → r34 PASS | reader의 Catalog TxnPin+UndoPin 동안 CLR Busy/WAL0, 자기 rollback pin 반환, 이후 R05..R11 |
| MT05 | r34 FAIL → r35 PASS | Pending 중 원 flusher가 Root/Jobs/Status를 clean으로 만든 뒤 설치가 recLSN9·requirement9를 새로 설치 |
| MT06 | r34 FAIL → r35 PASS | flush capture 뒤 Commit의 Drain·설치가 I/O claim을 기다리지 않고, 이전 flush finalize는 새 dirty를 보존 |
| MT07 | r34 FAIL → r35 PASS | Drain 중 새 reader InstallationPending, 외부 fence로 ACK0, durable Commit9을 새 복구가 winner로 완료 |
| MF14 | r34 PASS | CLR stage 뒤 첫 page 설치에서 unwind, 표지·이전된 자기 pin 유지, 새 복구가 LSN9부터 CLR10..TerminalFinish16 완료·Missing |

r33의 MT01/MT04 FAIL은 아래 제품 결함 때문이다. r34의 MT03/MT05/MT06/MT07 FAIL은 시험 구성 오류였다(flusher 역할, Pending 중 snapshot 등록, `read()`의 Root 형식). r33의 MT02 FAIL은 아래 pair victim 준비 공백 때문이다. 단위 메모리 실행 누계는 124.5초로 예산 600초 안이다.

## 제품 변경

- **조회의 Live Undo 읽기 결함 수정.** lookup은 참조 Undo root를 범용 `read()`로 읽었는데, 이 함수의 Undo 분기는 원 container의 초기 Free generation1 page만 받아들인다. Forward 뒤 Live generation2 Undo는 항상 `Undo(IdentityMismatch)`로 fence됐다. 완료된 조회 시험은 모두 Prune 또는 rollback 뒤 상태여서 이 경로를 지나지 않았다. `read.rs`에 `UndoShape`를 두고 lookup만 page decode·한 slot·pageLSN을 검사하는 `read_catalog_undo`를 쓴다. 레코드의 정확한 기대값 검사는 lookup에 그대로 남는다.
- **Writer 준비 분리.** 이전 중간 저장에서 작업 트리에 남긴 `CatalogPreparation` 초안을 포함했다. Live와 split `RetentionWriter`가 같은 검사를 공유하며, reader snapshot이 살아 있는 동안 writer가 Prune/CLR을 준비할 수 있다. Clippy 지적 4곳을 고쳤고 source census는 +56B, peak 63938이다.
- **두 번째 CREATE의 pair victim 준비.** 같은 epoch의 두 번째 CREATE에서 새 pair 두 page의 victim은 첫 table의 dirty pair page뿐이었다. 준비 단계가 이를 알리지 않아 Reserve가 `Busy`만 반환했다. 사용자의 2026-10-26 결정에 따라 `prepare_catalog_create_targets`가 Reserve와 같은 규칙으로 victim을 미리 판정해 dirty victim에 `FlushRequired`를 반환하게 했다. 판정은 권고이며 Reserve의 거절은 그대로다. victim이 없을 때의 오류는 이제 Begin 전에 나온다. 범위 해석은 [D3-CATALOG-CREATE-002](../contracts/dev3-catalog-create.md#sequential-create)에 기록했다.
- **시험 전용 관측.** horizon·reference pin·physical pin·pool 관측과 `install_after_capture` 재노출은 모두 `cfg(test)` 읽기 전용이며 권한을 만들지 않는다.

## 독립 입력과 검토

MF14 복구 기대값은 기존 corpus에 없어 별도 작성자가 [memory-retire corpus](../../tests/fixtures/dev3-catalog-create/memory-retire/provenance.md)를 1회 생성·1회 감사했다(106개 raw, 16행, checkpoint 3개). memory-gap과 달리 생성 전 독립 reader 검토가 없었다. r34 독립 검토가 원 R/N02/MC04 행과 바이트를 대조한 사후 검토이다.

독립 검토는 r33·r34·r35·r36 각각의 실행 전에 받았다. 검토 원문은 각 revision의 `admission-evidence/`에 있다. r34 검토는 flusher 역할 강제, Pending 중 snapshot 등록 거절, `read()`의 Root v1 decoder를 놓쳤고 r35 검토가 이를 정정했다. r34 검토를 해당 schedule의 검증 근거로 인용하지 않는다. 순차 CREATE의 범위 기록 D3-CATALOG-CREATE-002는 별도 독립 검토의 필수 수정 6건과 선택 1건을 반영한 뒤 확정했다.

## 계획 대비 차이

- MT04는 CLR이 Aborting을 요구하므로 RollbackStart9 뒤 reader가 pin을 잡는다. RollbackStart9는 Status만 바꾸므로 pin과 Missing 판정은 같다.
- MT01/MT02의 H 관측은 version hold 없는 `cfg(test)` registry 읽기이다. 실제 H 판정은 Busy로 거절된 Prune이다. Prune scratch로 서로 겹치지 않는 reader loan을 빌려 쓴다.
- MT05/MT06은 Commit을 별도 thread의 두 번째 Foreground Owner로 준비·실행한다. 유일한 전용 flusher Owner는 thread를 옮길 수 없다. Begin과 Commit의 Owner가 달라도 되는지는 의미상 거래 owner를 설계할 때 다시 정한다.
- MT03은 old snapshot을 Commit 전에 등록한다. snapshot 등록은 apply admission이 필요해 Pending 중 거절된다. FlushRequired는 실패한 사전조건으로 처리한다.
- MT07의 content guard 대상은 Root 대신 Status이다.

## 남은 범위

- **정적 검토만 한 분기.** victim이 없는 준비 단계 오류와 recLSN이 없는 Flush 후보의 Invariant는 실행하지 않았다. 완료된 사례는 victim이 clean 또는 fresh여서 판정이 `None`이고, 변경 코드 기준에 따라 MT02만 실행했다.
- **`read()` Root/Jobs 결함 OPEN.** v1 decoder만 써서 유효한 v2 page에서 fence한다. 현재 제품 소비자는 없지만 사용 전에 수정·검토가 필요하다.
- MF01/MF13은 UNREACHABLE/NOT_RUN 그대로이다. runtime F05 전체 인수와 DEV3/S5는 미완료이다.

## 정리

r33–r36의 source·ELF·로그·결과·입장 기록을 보존하고, 해시를 확인한 compiler target만 제거했다(각 약 99MB). 다른 세션이 동시에 수정 중인 `crates/testing/controller`는 r34–r36 overlay에서 제외했다. 작업 트리에서 fmt와 engine Clippy `-D warnings`를 통과했다. 최종 commit의 포맷·Clippy·문서 검사는 pre-push 훅에서 수행한다.

## 2026-10-26 검토 후 해결

위 남은 범위는 r36 당시 기록이다. 후속 [읽기 수정·오류 검증](dev3-catalog-review-fix-20261026.md)에서 Root/Jobs 결함을 해결하고 새 pair 오류 선택4개를 실행했다. recLSN 누락은 상위 Flush 후보까지 도달하기 전에 하위 frame invariant가 차단한다. 원 r33–r36 증거와 이력의 OPEN/정적 검토 표기는 당시 상태로 보존한다.
