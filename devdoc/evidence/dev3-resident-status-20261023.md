# DEV3 여러 Status의 실제 상주 수명 — 선택 검증

2026-10-23 · **D2 ACCEPTED / LOCAL_SELECTED_VERIFICATION / 제한 단위 최종 D1 수용**.
작성·통합 `/root`, 독립 검토 `/root/pool_design_review`, 방식 `open-code-review-delegate`.
[source·receipt 색인](dev3-resident-status-20261023.json)과
[소유 계약](../contracts/dev3-task-05-buffer-flush.md#resident-status-pool)이 정확 범위를 정한다.

## 공급 범위

기존 `SharedPool`의 directory·pin·DPT·WAL 권한을 그대로 사용하면서 caller의 fresh2..64 frame을
받는다. private closed binding의 consuming activation 뒤 여러 Status의 실제 INIT·주소별 read/
flush·clean 퇴출·지정 슬롯 home load를 공급한다. selected slot은 INIT 목적지이며 다른 상주 주소의
읽기·flush·load 권한을 대신하지 않는다. frame0 allocator는 고정한다.

필수 이력은3frame에서 allocator와 Status A/B/C의4주소다. A/B를 함께 dirty로 만들고 A를 flush/
퇴출한 자리에 C를 INIT한 뒤 B를 flush하고 그 자리에 A를 다시 읽는다. 현재 allocator membership,
원 slot/generation/stamp, load permit의 별도 index, INIT claim/key/completion의 실제 index를 검산한다.
기존 정확 두-frame binder·owner·flush pipeline은 같은 내부 코어를 사용한다.

loaded Status의 live Begin/undo·자동 victim/pressure·큰 backing·다음 extent·일반 READY·tree/행 거래와
전체 DEV3/S5는 미완료다. 첫 extent의 Status는1..63이므로 N64는 allocator+63 Status 경계이며,
N64+1 주소 인수는 다음 extent 공급 후의 잔여다. 영속 wire 형식 변경은 없다.

## Source별 검증

| source | 결과와 보존 범위 |
| --- | --- |
| r0/r1 | 공통 authority wrapper·private helper 접근·타입 식 compile FAIL 원본 보존, runtime NOT_RUN |
| r2 | compile·초기 layout PASS. producer 임시값을 빠뜨린 자원식은 불완전하므로 최종 F05로 사용하지 않음 |
| compiler 첫 source | 시험의 non-Clone snapshot 접근으로 baseline FAIL. 원 진단·미실행 목록 보존 |
| compiler 수정 source `4820e1ef…` | baseline1·positive2·negative6의9건 PASS, 63.747초. 제품 runtime은 실행하지 않음 |
| r4 | load permit 시험에서 Owner보다 먼저 storage를 drop한 compile FAIL 보존 |
| r5 | 수정 compile PASS, mock/native 실제 타입의 새 static2 PASS. hot196200/load110992/whole261736 |
| r6/r7 | 추가 시험의 consumed prepared view 접근 compile FAIL 후 view를 먼저 보존해 compile PASS. r7 fmt PASS, 시험 tuple의 Clippy type-complexity FAIL 보존 |
| r8 `78ef7d12…` | tuple alias·native retention/build-log 검산 보완. runtime34 중32PASS/2FAIL. 두 실패는 두 번째 Maintenance owner를 요청한 시험 설정 오류. 새 checker2·fmt/Clippy PASS |
| r9 `855746ca…` | 두 번째 owner를 Foreground로 수정한 실패2만 PASS(0.192초). native 최종 정리 시간 검산 checker1·fmt/Clippy PASS. 새 native 이력1 PASS |
| r10 `586bafd3…` | 독립 리뷰의 뒤쪽 Status 의미 실패 정리 신규1 PASS(0.103초). fmt/Clippy PASS. 완료된 runtime/native/compiler를 반복하지 않음 |

r8 ELF `15423376…`의 원34 선택은3.506초이며 신규30·직접 관련4다. 이미 PASS인32개를 r9에
반복하지 않았다. 관련4는 바뀐 공통 binder, INIT/reader/flush 소비자와 indexed loader/permit의
기존 slot1 의미다. 전체 wrapper·무변경 하위 회귀·workspace 제품 시험은 실행하지 않았다.
[동반 manifest](../../crates/engine/tests/fixtures/dev3-resident-status/manifest.json)는 최종 신규31·관련4와
각 changed-path 관계를 기록한다. `FROZEN_INPUTS_NOT_EXECUTION_EVIDENCE`는 실행 PASS가 아니다.

compiler9는 정상 release·activation Before retry, retained Err의 Owner 재대여, held guard 재대여,
Sync, consumed binding 재사용, split 중 prepare, frame alias를 구별한다. 정확 단일 진단의 code/
primary path/line와 실제 실행 helper의 capture 전후 hash를 감사했다. 이후 test-only/helper 또는
자원식·permit index 변경을 이유로 변하지 않은 lifetime probe를 반복하지 않았다.
checker는 초기 borrow2·native3, retention/build-log 새2, 마지막 정리 시간 새1로 총8개다.
각 receipt는 원 source를 소유하며 다른 source의 PASS로 다시 표기하지 않는다.

## 실제 디스크 복구

r9의 동일 build ELF `7b486413…`에서 producer221960→recovery221965→reentry221967의 세 PID가
새 history 하나를 실행했다. producer의 dirty allocator/C와 clean A/B home, 전체 WAL·descriptor·DWB,
원 inode, allocator와 Status3의 최종 bytes/LSN/REDO 수를 독립 literal과 대조했다. 재진입에는 새 home
write가 없다. 기존 recovery worker는 새 입력의 소비자로 사용했으며 완료된 이전 history를 반복하지 않았다.

최종 inventory/정리까지1.933초로 history30초·합120초 안이다. DB24,344,000바이트는24MiB/
보존96MiB 안이며 case 출력303,273바이트다. 각 process의 peak FD5/final0, 최종 handles0을 확인했다.
실패/timeout도 actual path·bytes·hash·inode를 보존하고 최종 시간/공간 초과를 FAIL로 기록하는
runner를 추가했다. native PASS를 crash/fault 전수나 live 거래 인수로 확대하지 않는다.

## 자원과 독립 검토

원 operation114688·flush4MiB·page16KiB/frame·dirty196608·whole262144·component65536를 유지한다.
activation63296, wide chain25704, 작은 admission Result72, 큰 read Result2112다.
retained와 각 Owner의 거절 Result를 포함한 read phase180488과 producer의 plans4096/temp16384를
포함한 phase196200 중 큰 값을 사용한다. producer와 active wide reader가 각각 Owner를 차지하므로
그 phase의 남은 retained Result는32개다. read/light phase는34개를 보수적으로 센다. gate 배제만으로
이 비용을 생략하지 않는다. whole261736의 여유는408바이트며 layout 변경 시 다시 검산해야 한다.
논리 source-value 상계이고 실제 compiler stack/RSS 또는 일반 OOM 복구 보장이 아니다.

OCR delegate 독립 D2와 실행 전 F05/runtime gate를 통과한 선택만 실행했다. 원 리뷰는 typed
Fenced/Poisoned cause, allocator→payload lock 순서, producer 임시값 회계, unwind mutex poison
oracle, native 실패 inventory/build-log binding/최종 시간 검산을 지적했다. 각 수정과 원 pending/
조건부 판정을 함께 보존한다. 뒤 Status의 typed semantic 실패 정리 시험도 후속으로 추가했다.
최종 D1은 `D1_ACCEPTED_LIMITED_RESIDENT_STATUS_LIFECYCLE`이며 미해결 차단 지적0건이다.
원문 `review/final-d1.json`의 SHA-256은
`5d7bb7dd9a9a82ba1482358332bb156b10c08550409abc3425d6da08c730be73`이다.
정확 staged tree `13ef30452f32f406e87623f78f19552b987f77ed`의69경로(검증한 r10과 같은 source/fixture58·문서11)를
검토했다. OCR 전체74는 검토69·무관 제외5이며 reviewable24/24를 검토했다. 이후 변경은 이 판정과
receipt의 상태 메타데이터 동기화뿐이며 제품·시험 재실행은 없다.
리뷰·원 FAIL·source/ELF·compiler 진단·도구 보존 경로/크기는 JSON과
`artifacts/dev3/resident-status-20261023/`의 원 receipt를 따른다.

## 남은 인수

S5 확대는 profile2/3×N4/8/64의6 ignored selector다. 모든 victim146개와 victim×late-content8200개를
작성했으며 **NOT_RUN**이다. 입력은 기존 독립 literal generator로 profile/capacity별 생성하고
활성8MiB를 지킨다. 중간 신규31·static2·compiler semantic8·checker8·native history1도 최종 S5
inventory에 포함하며 compiler baseline은 별도 제어 사례다. 관련4는 원 inventory와 중복 집계하지 않는다.
문서 자동 검사는 push 전용이므로 실행하지 않았다. push·DEV3/S5 완료 승격은 하지 않았다.
