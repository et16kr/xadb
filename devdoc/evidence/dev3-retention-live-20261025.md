# DEV3 plan4 빈 거래 live·prefix 선택 검증

2026-10-25 · **필수75개 source별 PASS, 최종 독립 D1 제한 수용**. [소유 계약](../contracts/dev3-retention-live.md)·[시험 계획](../plans/dev3-test-plan.md#retention-live)·[기계 색인](dev3-retention-live-20261025.json)을 따른다. 원 MutableStorageOwner와 SharedPool을 쓰는 fresh N=0 수명·flush·재적재와 관측용 tag3 prefix를 구현했다. 복구 owner·native 수명·전체 DEV3/S5는 미완료다.

## 실제 범위와 원 결과

live 필수32는 static2와 memory30(A4/L4/R4/W6/F6/D6), prefix 필수43은 독립 literal envelope의 visible header·SHA-256 header/body digest·연결·EOF 경계다. 75개 source별 결과 (로컬 `../../artifacts/dev3/plan4-g6-recovery-20261024/fresh-mandatory-results-r0.json`)는 완료 반복0과 원 실패6을 함께 보존한다. live memory 실행은36개 시도이며 charged wall21.529초(worker17.537765009/감사3.991234991)다. semantic 상한210초(worker180/감사30), 함수·사례30초, 출력8MiB 안이다. static은 무효과이고 prefix43도 파일 효과 없는 검산이다. 후속 native 수명 인수로 세지 않는다.

| 원 source | 새 PASS | 원 실패/범위 |
| --- | ---: | --- |
| live-tests-r2 | static2 | actual memory/native 타입의 source-value 크기 |
| live-tests-r4 | A4 | F01 LoadFrameInvariant |
| live-tests-r5/r6/r7 | 0 | F01 동기화 seam 오류2회·시험 adapter RW 거절1회 |
| live-tests-r8 | F6+D6 | L01 PrepareSemanticInvalidInput |
| live-tests-r9 | L4+R2 | R03 공통 fence 뒤 foreign owner 기대값 오류 |
| live-tests-r10 | R2+W6 | 잔여8 PASS |
| prefix-tests-r0 | prefix43 | 전부 PASS |

모든 source·ELF·실행 입력·원 stdout/stderr hash는 기계 색인에 고정했다. 최종 fresh source는 `1604d737b279919d05e71a80f51cc8496ecbc61243dff60d6c1252023e6a4172`, ELF는 `94f76e3b5afe5ad997e3e3d5bfea449c8a0465a64b372badd9fc2724b1a63525`다. 이전 source의 PASS를 최종 source 재실행으로 바꾸지 않는다. 마지막8 receipt는 `c2dacca0955dafc6792040f2a54af1c51cc9cf05df851bfb37823336993c7c48`, prefix43 receipt는 `f870a9525b1c605ff96f6105db948422fe04f82c5fc3cd653119f87460cfd554`다.

## 수정·자원·품질

LSN0 clean page의 counter를0으로 유지하고 Begin에만 이전 채택 local을 전달하도록 실제 writer/replay를 수정했다. 실제 capture 뒤 pin/latch 반환과 실제 RAM install 뒤·WAL force 전에 시험용 one-shot seam을 두어 late RAM과 retirement 후 Busy를 재현한다. 원 오류 seam의 timeout을 숨기지 않는다. memory port는 새 live fixture에만 RW 허용을 켰으며 기존 genesis 기본값은 유지한다. R03은 같은 session fence를 공유하는 다른 core도 Fenced/CallerMustResolve를 반환함을 확인한다.

원 static2의 memory/native peak63,520/63,656byte, metadata123,424/123,560byte, whole188,960/189,096byte는 actual generic 타입의 source-value 상계다. caller backing·machine stack·RSS와 다르며 새 recovery runtime에 전용하지 않는다. 최초 static2의88,620/89,780byte 실패는 phase 수명 분리 전 결과로 보존했다. 이후 scalar 수정·test seam·lint 변경은 독립 lineage 검토로 연결하고 static2를 반복하지 않았다.

fresh r10와 prefix source의 변경 파일 fmt 및 engine all-targets/all-features Clippy는 PASS다. fresh r3 compile 실패, r5 Clippy 실패와 r0 stale cache의 fresh=true HOLD도 보존한다. helper는 private mutable cache를 lock 안에서 정리한 뒤 fresh=false compile을 요구하며 원 source/ELF와 inode를 공유하지 않는다. original ELF를 보존하기 위한 host ELF 상한512MiB는 제품64KiB 한도를 바꾸지 않는다. source160MiB/compile256MiB/records16MiB/fixture20MiB/native0·최소 free256MiB를 유지했다.

## 인수 경계

S5 고유 추가269(live32/prefix237)는 NOT_RUN이며 최종 S5에는 필수75도 포함한다. 완료 하위·pure codec·workspace wrapper는 실행하지 않았다. 원 raw34파일550,160byte를 재사용하되 완료 시험 함수를 호출하거나 제품 encoder를 oracle로 사용하지 않았다. 기존 prefix 입력의 EOF alias8 표기는7로 교정했고 원 입력도 보존했다.

독립 source D1은39개 owned 경로와 OCR 제외/foreign 항목을 회계했다. 최종 결과·문서·commit 조립의 독립 D1을 fresh live·prefix 범위로 제한 수용했다. 복구 CURRENT 선택·공개, REBASE/LOG_OPEN·loser 정리·VERIFY/STABILIZE·동일 pool 재진입·3PID와 Catalog CREATE/CLR·첫 행 ACK는 후속이다. AI 완료 소비자1/3·DEV3 미완료를 유지한다. 문서 verifier·push는 실행하지 않았다.

최종 독립 D1 (로컬 `../../artifacts/dev3/plan4-g6-recovery-20261024/review/fresh-final-d1-r0/final-d1.md`) SHA `da80736110ebf2cf7587b0932758c7db279f5db593037db85858a8e71eebdc17`를 fresh live·prefix 범위로 수용했다. 현재 host 상한576MiB는 이후 별도 recovery static ELF도 보존하는 한도이며 fresh 실행 당시512MiB와 구별한다.
