# DEV3 빈 거래·정리 큐 순수 의미 처리 선택 검증

2026-10-24 · **필수140개(static1·semantic139) source별 PASS, 최종 독립 D1 제한 수용**. [소유 계약](../contracts/dev3-empty-retention-semantics.md)·[선택 계획](../plans/dev3-test-plan.md#empty-retention-semantics)·[기계 색인](dev3-empty-retention-semantics-20261024.json)을 따른다. G6 실제 live owner·DWB·복구 연결과 DEV3/S5는 미완료이며 AI 완료 소비자는1/3이다.

Root/Job page와 Aging descriptor의 정규 표현, 빈 거래의 Begin·AbortEnd·Finish 및 AgingStart를 순수 생산/관측 함수로 구현했다. 새 tag3 인계 envelope와 기존 tag2 시스템 의미 envelope를 구분하며 기존 legacy/tag2 진입점은 tag3을 거절한다. 원 descriptor 검산은 고정 macro 확장으로 공유해 기존 진입점의 값·전달 계층을 유지한다. 실제 pin/예약·append·durable finish·파일 선택 권한은 이 순수 관측에서 발급하지 않는다.

## 원 입력과 실행

독립 offset식 generator가 고정 P1/ordinal0의 raw24개398,608bytes와 원 plan4 reference10개151,552bytes를 생성/복사했다. manifest/index를 포함한36개561,762bytes를 SHA로 동결했다. 최초 생성은 중복 keyword 문법 오류로 실패했으며, 메타데이터 이름만 `status_location`으로 보정한 후속 생성이 통과했다. 원 실패와 원 generator를 보존했고 완료 vector를 재생성하지 않았다. 두 시도의 생성·감사·기록 관측 합은0.431207초/571,873bytes로10초/2MiB 한도 안이다.

| 보존 source | SHA256 | 실제 범위 |
| --- | --- | --- |
| product-r0 | `2e0d075ba3058cf069ae2e49c88046540e669dd03d8bc04115f321b67b9bea2d` | 최초 lib check PASS, 실행0; 이후 리뷰에서 redo index·terminal before 검산 보완 |
| tests-r0 | `5ea1b84e82082cf1a6fe3aeda1858dc3322b9190d11fc18443130154e93764b5` | no-run build PASS, ST01 PASS, semantic137 PASS·N07/N08 FAIL |
| tests-r1 | `9ef3f736f692d3ea697bc1f465d8c91b031aae506cdd230eb828dde4411447aa` | 두 시험 입력 도달성만 보정; no-run build·N07/N08 PASS |
| quality-r0 | `4138701660220c70281bcccf78c2dc84822e2e3fc5f56c26920595e8b2224632` | 소비 연결 대기 lint annotation; owned9 fmt PASS, Clippy FAIL, 실행0 |
| quality-r1 | `e435298eab1cf2c831e3b2e07fccc032de5b33f70b8340cfcf08207ed14036f4` | 시험의 동등 범위 표현·독립 frame 상한 annotation; 변경 파일 fmt·engine lib/tests Clippy PASS, 실행0 |

tests-r0 ELF는 `81ad54a7ab01fb48fb88195cc36df195e75aec47ed017da7488ed55f6e111b38`(44,349,928bytes), tests-r1 ELF는 `d5e15c039450b43a67cfa8c3115e13140c9f014d8c6d0fa2440f9e7bf14330a7`(44,351,640bytes)다. 원 실행 입력은 `95073c99c77a8ed8654c3fd0bb01b34317762298c9c3bb9a493f262422a68b7d`, 두 실패 후속 입력은 `73bc4c1c8567b2f21d0da9d463aa2b25621778f24f3a11aa863e4d1bcbb50bb8`이다.

N07은 Free generation2를 담은 Job page의 LSN0 제약 때문에 의도한 semantic guard 전에 거절됐다. pageLSN3와 generic decode 성공 확인을 추가해 기존 generation 오류를 검사했다. N08은 Free Status의 incoming1이 일반 decoder에서 거절됐다. AbortEnd의 Occupied/Aborting Status를 사용하고 generic decode를 확인해 기존 incoming 오류를 검사했다. 제품은 바꾸지 않았고 원 실패 둘을 삭제하거나 PASS로 바꾸지 않았다. 후속 source에서는 해당 두 ID만 실행했으며 완료137 및 정적1은 반복하지 않았다.

필수 결과 수합 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/mandatory-results.json`)과 독립 읽기 감사 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/review/mandatory-readonly-audit-r0.json`)는 고유 semantic139·실제141회(원 FAIL2 포함)와 완료 PASS 중복0을 구별한다. semantic 전체22.280초 중 process1.558012초·감사/기록20.721988초이며120/30/전체150초 한도 안이다. 보존 출력383,946bytes는4MiB 안이다. checker는 새 marker8·phase accounting4만 실행해12 PASS이며 제품 시험 분모에 넣지 않는다.

## 정적 상계와 품질

원 ST01의 실제 순수 source 상계는46,696bytes로64KiB 이하다. semantic35,224bytes에는 Aging16,128bytes와 borrowed carrier2,024bytes가 포함되며, page 추가11,472bytes를 더했다. caller writable loan은114,688bytes(3page49,152+frame65,536), readonly loan은121,856bytes로 별도 계상한다. 이는 source-value 계산이며 machine stack/RSS 측정이나 live/native F05 승인이 아니다.

두 시험만 수정한 r1의 파생 정적 수용 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/static-admission-r1.json`)은 원 측정 함수·제품·타입 불변을 읽기 대조한 실행 전제다. 새 ELF의 ST01 실행/PASS로 표시하지 않는다. 마지막 lint 수정은 동일한 정수 범위 조건과 독립 frame 상한의 annotation뿐이며 제품 동작·타입·자원 식은 유지했다. 원 Clippy 실패와 동등성 근거 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/lint-repair-source-proof.json`)를 보존한다.

준비 상한은 source160MiB/ELF128MiB/shared compile256MiB/records16MiB/fixture2MiB/native0이다. 최소 free256MiB와 다음 준비 출력 예측량을 먼저 확보하고 공유 cache의 compiler lock을 사용했다. 보존 source/ELF와 mutable cache의 inode 교집합은0이다. 원 core/G5/operational 증거는 보호하며 다른 COMM·DEV4 및 G6 control/base/live 초안은 이번48개 경로에서 제외했다.

## 남은 범위

이번 순수 단위의 S5 추가475개는 NOT_RUN이다. 최종 S5에는 필수140개도 포함한다. 실제 queue/Status의 pin·dirty·flush와 capture@L, 원 runtime의 append/force, base/epoch/CURRENT 선택, loser 정리·같은 pool의 live 재진입, 실제3PID 인수, Catalog CREATE/CLR·첫 행 ACK는 후속 구현/검증이다. 전체 lower/workspace wrapper·이미 완료한 시험·문서 verifier는 실행하지 않았다. 문서 검증은 push 직전 gate이며 이번 단위에서 push하지 않는다.

## 최종 독립 검토

최종 D1 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/review/final-d1.md`)·기계 기록 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/review/final-d1.json`)은61개 소유 경로를 모두 검토해 `ACCEPTED_LIMITED_D1`으로 수용했다. OCR125개 중 소유61개를 검토하고 foreign64개를 제외했으며, 그중 reviewable20개는13개 검토/7개 제외로 계상했다. 공유 문서에 도착한 별도 COMM/DEV4 변경 때문에 최초 working-hash 회계가 실패한 원본을 보존하고, 불변 scoped Git blob과 foreign 추가를 구분한 후속 회계로 닫았다. 실제 제품/소유 source는 변하지 않았다.

최종 전 준비 회계 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/preparation-before-final-d1.json`)는 source113,543,131·ELF88,701,568·compile124,667,672·records3,335,525·fixture561,762bytes/native0을 확인했다. 선행 core 보존 대조 (로컬 `../../artifacts/dev3/plan4-g6-empty-retention-20261024/preservation-after-quality-r1.json`)는 원98개 artifact·38개 source·2개 ELF가 그대로임을 확인한 읽기 감사다. 이 검토로 live/native/F05·G6·DEV3/S5 또는 AI 소비자를 완료로 승격하지 않는다.
