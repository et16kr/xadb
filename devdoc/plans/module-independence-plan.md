# XADB 모듈 독립 개발 구조 전환 작업 계획

2026-10-28 [Row scope3 frame 검산](../evidence/module-independence-survey-20261026.md#row-scope3-wire): main `2fdff66`에서 별도14opcode·다중 statement core·native/redo 입구를 구현하고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row actor의 실제 페이지 관계 검산](../evidence/module-independence-survey-20261026.md#row-actor-graph): 사용자의 전체 계획 완료 요청에 따라 `main` `d27bfa4`에서 재개했다. 한 actor의 Base4/Status/Undo/Row graph를 구현했고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [CREATE 원 flush 대여](../evidence/module-independence-survey-20261026.md#c3-create-lease): main `3fa5f6f`에서 예약의 disjoint 원 자원·연속 flush lease를 구현했다. r1 compile FAIL을 보존하고 r2 compiler3/runtime10/nativeN01을 통과했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 admission·M4/M5는 미완료다.

2026-10-28 [CREATE 원 자원의 borrow 본문](../evidence/module-independence-survey-20261026.md#c3-create-parts): main `8613c53`의 원6자원을 빌리는 private view로 실제 본문을 분리한다. r1 fixture FAIL을 보존하고 r1 compiler2/runtime5 및 r2 수정·미실행 runtime2/nativeN01을 검증했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 CREATE와 M4/M5는 미완료다.

2026-10-28 [공유 pool CREATE claim](../evidence/module-independence-survey-20261026.md#c3-pair-shared): main `c845ea7`에서 exact 후보·공유 종료·ordinary gate를 구현한다. r1 compiler4/runtime12/nativeN01 3PID PASS, 최종 독립 D1/D2 한정 수용이며 actual shared reservation과 M4/M5는 미완료다.

2026-10-28 [CREATE 기존 대상 key 접근](../evidence/module-independence-survey-20261026.md#c3-pair-existing): main `4e68a4d`에서 typed subset·실제 Reserve/Forward 준비/Cancel 연결을 진행한다. r2 compiler5/runtime8/nativeN01 3PID PASS·r1 주입 FAIL 보존, 최종 독립 D1/D2 한정 수용이며 shared admission·M4/M5는 미완료다.

2026-10-28 main `ef95a8a`의 [exclusive CREATE pair 원자 leaf](../evidence/module-independence-survey-20261026.md#c3-pair-atomic)를 진행한다. r3 runtime8/nativeN01 3PID PASS·원2개 FAIL 보존, 최종 독립 D1/D2 한정 수용이며 shared admission·C2/C3–C5·M4/M5는 미완료다.

2026-10-28 main `9c334fe`의 [Commit 준비·적용 단일 예약](../evidence/module-independence-survey-20261026.md#c2-commit-call)은 source별 runtime11/compiler4·원 N03 2PID를 통과했다. 원3개 FAIL을 보존하며 최종 독립 D1/D2 한정 수용다. CREATE/일반 floor/Row·M4/M5는 미완료다.

2026-10-28 · **D2 ACCEPTED / M0–M3 완료 / M4 C0·C1·C2·C3 부분공급(C2 compact source·statement 읽기·Catalog Commit writer 창, C3 InstallGrant·TerminalWal·Prepared) / M5 Row 순수 codec·ledger 부분구현 / 전체 M4·M5 제품 인수 미완료 / Catalog Commit writer 창을 main에 통합**

[개발 문서 진입점](../README.md) · [기존 병렬 정책](../policies/parallel-development-policy.md) · [저장소 배치](../policies/repository-build-plan.md) · [DEV3 작업 분해](dev3-implementation-plan.md#remaining-work) · [변경 코드 시험 지시](../../AGENTS.md)

2026-10-28 main `af78960`의 후속 [Begin·AgingStart progress](../evidence/module-independence-survey-20261026.md#c2-start-progress)는 두 시작 경로를 같은 원 gate/source에 연결한다. 선택 비용과 issued-local gap을 유지하며 r2 compiler3/runtime5·같은 ELF native N02(3PID) 각1회 PASS와 r1 compile FAIL을 보존했다. CREATE/일반 floor/Row·M4/M5는 미완료다.

2026-10-28 main `bf62459`의 후속 [Catalog lifecycle progress](../evidence/module-independence-survey-20261026.md#c2-lifecycle-progress)는 RollbackStart/AbortEnd/CancelReserved/ReleaseRole의 공유 동기 호출과 원 ordinary application view를 구현한다. r3 compiler4/runtime12·같은 ELF native N02(3PID) 각1회 PASS와 r1/r2 compile FAIL을 보존한다. 나머지 C2/Row/C3–C5와 M4/M5는 미완료다.

2026-10-28 main `c217fbf`의 후속 [Catalog 회수 progress 공급](../evidence/module-independence-survey-20261026.md#c2-retirement-progress)은 CLR/Prune/RootFree/TerminalFinish를 원 gate·고정 source ledger에 연결했다. Maintenance 역할과 장기 snapshot horizon을 보존하며 source별 선택7+6 PASS와 원 FAIL을 구분한다. 이어 main `fa0f033`의 [공유 회수 준비 body](../evidence/module-independence-survey-20261026.md#c2-retirement-preparation)를 같은 gate/source에 연결하고 선택12를 검증했다. 후속 main `582ccbf`의 [동기 회수 호출](../evidence/module-independence-survey-20261026.md#c2-retirement-call)은 준비·적용을 한 원 gate/source 안에 묶어 회수 Prepared의 외부 보유 gap을 닫는다. compiler4/runtime13·같은 ELF native N02(3PID)를 각1회 PASS했다. 남은 공급은 일반 진입/반환 floor·Commit Prepared lifetime·나머지 writer/Row의 ledger 참여와 C3–C5이며 M4/M5 전체 인수는 계속 미완료다.

<a id="purpose"></a>
## 1. 목적과 이번 인계 범위

**기능 담당자가 확정된 하위 계약을 사용해 다른 기능의 내부 구현을 수정하지 않고 개발·검증할 수 있는 구조로 전환한다.** 필요한 하위 의존성과 최종 통합 순서는 유지하면서, 파일·공유 상태·미확정 API 때문에 생기는 불필요한 공동 수정과 대기를 줄인다.

사용자는 초기부터 “모듈별로 구성하고, 병렬로 개발해도 충돌을 최소화”하기를 요구했다. 현재 평가를 확인한 뒤 현재 구조를 변경하고 이 목적대로 개발되도록 에이전트가 문서를 수정할 작업 계획서를 요청했다. 따라서 후속 문서 담당은 현 구조를 설명하거나 병렬 운영 안내만 추가하는 데 그치지 않고, **목표 구조·내부 API·소유 계약·개발 순서를 수정안으로 구체화**한다.

이번 산출물은 그 문서 수정과 후속 구조 전환의 계획이다. 작성·리뷰만으로 진행 중인 다른 채팅·프로세스가 중지됐거나 소유 계약·제품 코드가 바뀐 것으로 기록하지 않는다. 최초 계획 작성은 계획서와 탐색 색인의 작성·리뷰에 한정했다. 후속 전체 실행 지시에 따른 현재 상태는 [실행 추적](#execution)에 둔다. M0–M3를 완료했고 M4 C0의 borrowed codec·생산자 타입과 C1의 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급을 진행했으며 C2의 [동기 Catalog 조회 본문 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)도 부분공급했고 M4 전체·M5는 미완료다.

2026-10-28 사용자는 `codex/module-independence-finish`를 `main`에 병합하고 이후 main에서 진행하도록 지시했다. [앞선 통합 기록](../evidence/module-independence-survey-20261026.md#merge-main-20261028) 뒤 별도 잔여 작업 branch의 `af54a4e`도 리뷰·사용자 지시에 따라 main으로 통합했다. 현재 재개 기준은 [Catalog Commit writer 창 병합 기록](../evidence/module-independence-survey-20261026.md#writer-progress-merge-main-20261028)이며, 위 초기 공급 설명과 아래 source별 기록은 당시 범위로 보존한다. C2 전체 동시 admission·C3 나머지 공급·Row actual 연결과 M4/M5 제품 인수는 미완료다.

우선 조사 범위는 DEV3의 WAL·페이지/버퍼·할당·거래/undo·복구·Catalog 연결이다. 실제 첫 전환 범위는 M2에서 선정하는 두 소비 기능과 그 기능에 필요한 공통 경계로 닫는다. foundation/platform·SQL·COMM은 이미 확보한 경계를 보존하면서 수신 계약에 영향이 있는 부분만 대조한다. 이후 단계도 영향을 받는 선행·인계 문구를 갱신하되, 전체 DBMS를 다시 설계하는 작업으로 확대하지 않는다.

<a id="baseline"></a>
## 2. 출발 근거와 상태 재확인

| 관측 | 근거와 해석 |
| --- | --- |
| 초기 요구가 이미 정책에 존재 | [병렬 정책](../policies/parallel-development-policy.md#section-1)의 책임별 파일 구성·공통 계약 최소화·작은 조립 지점. 새 목표를 발명하는 작업이 아님 |
| 큰 모듈의 분리는 실제 작동 | [SQL](../../crates/sql/Cargo.toml)·[transport](../../crates/transport/Cargo.toml)의 제품 의존은 foundation. [DEV4 앞단](../evidence/dev4-frontend-20261024.md)은 DEV3 변경을 제외한 소스와 별도 소유 경로로 개발 |
| DEV3 공통 경로에 변경 집중 | 평가 기준 `9275154`까지 2026-10-20 이후 저장 소스를 바꾼71개 커밋 중33개가 `wal_io.rs`, `bootstrap.rs`, `doublewrite_install.rs`, `mutable_storage.rs` 가운데 하나 이상을 수정. 등록·재노출·명칭 변경도 포함한 Git 경로 집계이며 실제 merge 충돌 수나 생산성 수치가 아님 |
| 기능 연결과 하위 기반 보완이 결합 | `e443115`의 Catalog 연결은 시험을 포함한 Rust101파일 변경. [원 구현 범위](../evidence/dev3-catalog-live-milestone-20261025.md)는 첫 연결의 정상적인 필요와 미완료 공급을 구분. 파일 수만으로 잘못된 설계라고 판정하지 않음 |
| 타입 재노출 사슬의 실제 비용 | 같은 Catalog 권한 타입을 여러 중간 모듈에서 재노출. 위 증거의 일반 빌드 오류는 시험 전용 조건 속성의 잘못된 연결이었고 이미 수정됨. 현재 미수정 결함으로 재보고하지 않음 |
| 최초 평가 이후 상태 변화 | 계획 작성 시작 HEAD는 `107667d3cde769bfd20232da1c1dcc288c60c863`. DEV3 후속 수정과 DEV2 변경이 추가됐고 DEV4 병합 중 공용 README·문서 색인에 충돌을 관측. 최초 평가의 미커밋5파일·미완료 목록을 현재 사실로 복사하지 않음 |

후속 담당은 M0에서 최신 HEAD·dirty/index·진행 작업을 다시 확인한다. 위 관측은 출발 근거이며 현재 소스 전체의 인수 결과가 아니다. 기존 정상·FAIL·UNKNOWN·중단·NOT_RUN과 source별 증거는 그대로 유지한다.

계획 작성 중 병합은 다른 작업에서 완료됐고, 탐색 색인 등록 시점의 기준은 `27c2288b6ed7b48e556c179067348c686650b3f9`이며 충돌은 없었다. 이 계획 작업은 병합·기존 제품 수정을 수행하지 않았다.

<a id="principles"></a>
## 3. 구조 전환의 설계 기준

1. **변경 책임을 경계로 삼는다.** 같은 기능의 값·오류·설정·계측·시험은 해당 책임에 둔다. 파일 수·줄 수·crate 수를 목표 지표로 삼지 않는다.
2. **공통 의미와 조립을 분리한다.** 실제 공유 ID·참조·권한·오류 의미는 단일 소유 계약을 유지한다. WAL·buffer·undo·catalog를 함께 사용하는 순서는 명시된 거래/복구 조립 담당에 둔다. 서로 다른 초기화 단계를 중첩한 모듈 경로가 모든 기능의 접근 경로가 되지 않게 한다.
3. **독립 개발은 계약과 좁은 접근 권한으로 확보한다.** 상태 전체나 manager를 넘기는 대신 필요한 값·읽기 view·소유권 이전·검증된 capability를 제공한다. 내부 구조체 필드의 공개 확대, 전역 singleton, raw 포인터, 공통 가변 context로 결합을 숨기지 않는다.
4. **모듈의 컴파일 의존과 실행 조립 관계를 따로 그린다.** 실행 시 서로 협력한다고 제품 모듈이 상대의 내부 구현을 import해야 하는 것은 아니다. trait는 실제 대체 구현·효과 경계가 필요한 곳에 사용하고 모든 함수를 추상화하지 않는다.
5. **개발 선행과 통합 선행을 분리한다.** 하위 전체 단계 완료 대신 사용할 API·불변식·최소 기능 공급을 특정한다. 확정 계약과 필요한 컴파일 경계를 사용해 구현할 수 있는 소비 기능의 실제 로직은 하위 전체 구현을 기다리지 않는다. M2에서 각 소비자의 착수 가능 구현, 허용할 계약 대역, 실제 공급을 기다릴 통합 인수를 나눈다. 대역의 허용 효과·한계·독립 기대값과 실제 공급으로의 연결 의무를 정하며, 실제 저장 효과·완료 판정은 실제 공급과 연결한 뒤에만 인정한다.
6. **공통 기반 수정은 유한한 선행 작업으로 닫는다.** 소비 A/B가 각각 자신의 요구에 맞춰 공통 타입·WAL 의미·lock 순서를 바꾸지 않는다. 필요한 변경을 먼저 합의·통합한 뒤 같은 기준을 받는다. 새 요구가 드러나면 영향받는 작업만 다시 조정하고 무관한 작업은 계속한다.
7. **독립 단위의 인수도 설계한다.** 서버 없는 실제 core 호출, 필요한 최소 fixture·독립 기대값·명시된 오류/자원 경계를 제공한다. 실제 I/O·동시성·복구가 계약에 포함되면 해당 통합 인수를 별도로 유지한다.

보존할 제품 불변식은 [구현 계약의 DEV3 적용 경계](../contracts/implementation-contracts.md), [disk 저장 정본](../contracts/disk-storage-design.md), [버전 정책](../policies/version-compatibility-policy.json)을 따른다. WAL-before-page, Commit/ACK와 UNKNOWN, 원 pin/lease/fence·authority의 유일성, lock 순서, 원자 공개, undo/CLR·재장애, 자원 상한과 회수 진행을 분할 편의 때문에 약화하지 않는다. 런타임 단일 writer 보장과 개발 담당자의 작업 직렬화는 별개로 평가한다.

<a id="target-boundaries"></a>
## 4. 문서 담당이 확정할 목표 책임 경계

아래는 **설계 검토를 시작할 책임 분류**다. 실제 Rust 모듈명·파일 경로·crate 분리·함수 signature는 M1/M2에서 현 코드와 소비자를 대조해 확정한다. 이 표만으로 새 경로를 build inventory에 등록하지 않는다.

| 책임 | 소유할 상태·동작 | 다른 책임에 제공할 최소 경계 | 피해야 할 결합 |
| --- | --- | --- | --- |
| 파일/저장 수명 | namespace·파일 identity·공통 fence·설치/복구 후 수명 | 제한된 파일 효과·검증된 저장 수명 인계 | 설치 전용 wrapper를 모든 정상 연산의 접근 사슬로 사용 |
| WAL | append slot·prefix·물리 write/sync·durable 관측 | stage·force·정확 requirement 확인·실패 effect | Catalog/행 업무 규칙을 WAL 구현에 등록 |
| 페이지/버퍼 | frame·pin·dirty/recLSN·load/evict·DWB/home flush | 실제 page 획득·변경 admission·flush 결과·수명 토큰 | 상위 거래/Catalog의 상태 전체를 보유하거나 직접 실행 |
| 할당/객체 수명 | reserve/cancel·generation·초기화·재사용 권한 | typed 예약·설치/반환 인계·할당 검산 | 임의 caller가 헤더/번호만으로 유효 권한 생성 |
| tree/행/index/Catalog | 각 데이터의 표현·검색·변경 후보·도메인 불변식 | 명시된 페이지/undo/거래 계약을 소비하는 기능 API | 각자 buffer/WAL/commit 의미를 복제하거나 다른 기능 본문 수정 |
| 거래/undo/가시성 | transaction/statement 상태·undo mark·snapshot·권한·공개 | 변경 묶음 준비·적용·commit/abort의 닫힌 수명 | caller에게 부분 commit 순서나 실패 정리를 떠넘김 |
| 복구/checkpoint/회수 | restart 상태·redo/undo 순서·horizon·정리 의무 | 검증된 복원 상태와 원 소유권 인계·회수 요청 | live와 recovery가 비슷한 필드를 쓴다는 이유로 상태 의미 통합 |
| 조립 지점 | 위 모듈의 인스턴스/수명 연결·원자적 실행 순서 | 기능 담당이 사용하는 제한된 실행 경계 | 기능별 거대 dispatch·오류·설정·계측을 중앙에 누적 |

M2 산출물에는 다음 두 그림을 각각 둔다: (a) 허용 Rust import/타입 의존 그래프, (b) 생성→정상 실행→오류/종료→재시작의 소유권·호출 흐름. source 경로와 코드 symbol을 연결하고 순환·예외의 이유를 기록한다. 초기화/복구 수명을 분리하면서 기존 원 owner를 복제하거나 capability 생성자를 넓히지 않는지 별도로 검토한다.

각 경계는 **소유 상태, provider/consumer, 입력/출력 타입, 소유/borrow 수명, mutation 권한, 정상·실패 효과, 취소/정리, 동시성·lock, 자원 회계, 영속/복구 의미, 실제 최소 공급, 선택 시험, 미공급 의무**를 갖는다. 해당하지 않는 항목은 이유를 적는다. 선언만 있는 API와 현재 호출 가능한 API를 구분한다.

<a id="m2-handoff"></a>
### M2 설계 인계의 구체 형식

M2에서는 위 경계를 선정한 A/B의 실제 호출에 대입한다. 아래 항목은 기존 소유 계약·작업 기록의 해당 절에 작성하고 이 계획에서 연결한다. 같은 내용을 별도 문서마다 복제하거나 새 관리 도구를 만들지 않는다. 이 표는 산출물의 작성 기준이다. A/B 선정과 API 설계의 실제 채택 상태·근거는 [실행 추적](#execution)과 [소유 문서 채택 결정](#m3-adoption)에 둔다.

| 산출물 | M2에서 확정할 내용 | 검토자가 확인할 연결 |
| --- | --- | --- |
| A/B 기능별 작업 명세 | 기존 backlog ID, 입력과 관측 가능한 완료 동작, 지원/제외 범위, 담당·전용 경로, 각 담당이 구현할 제품 본문 | 두 기능이 실제로 소비할 선정 경계와 독립 구현 범위. 상대 기능의 미완료 본문·fixture를 선행으로 요구하지 않는지 확인 |
| API 사용 설계 | 현재 symbol→목표 모듈/함수, Rust signature·visibility, 입력/반환 값과 원 owner·borrow·권한의 이동. A/B 각각의 정상 호출 및 관련 오류·취소·종료/복구 흐름을 짧은 호출 예나 순서표로 설명 | 필요한 모든 호출이 허용 경계로 표현되는지, 호출자가 내부 필드를 열거나 원자성·실패 정리를 다시 구현해야 하는 지점이 없는지 확인. 원자 공개·WAL·pin/fence 불변식과 대조 |
| 공통 공급별 이행 명세 | 각 M4 단위의 현재→목표 경로, 제공 API·완료 동작, 직접 선행, 담당, 제외 범위·되돌림 단위. 각 공급이 해소할 A 또는 B의 구체적인 호출 제약을 연결 | A/B의 호출에서 필요한 공급으로 거슬러 올라가고, 각 공급에서 실제 소비자로 되짚어 범위 누락·불필요한 확대를 확인. 한쪽만 필요한 경계 공급도 이유를 남기며 A/B 기능 본문과 구별 |
| 착수·검증 인계 | C0에서 사용할 계약·최소 컴파일 경계·허용 대역, C에서 받을 실제 공급, 단독/합류 시나리오와 기대값 출처, 기존 selector 또는 구현과 함께 추가할 case, 변경에 필요한 자원 검산·실제 I/O/복구 의무 | 공급 수신으로 해소할 미실행 의무와 실제 diff에 따른 선택 범위를 연결. A/B가 선정 경계를 함께 소비하는 합류 시나리오의 관측값을 정하며, 런타임 동시 호출은 제품 계약이 지원·요구하는 범위에서만 배정 |

목표 경로·signature와 아직 작성하지 않은 case는 계획 상태로 표시한다. 호출 예는 문서 설계이며 M2에서 prototype 구현이나 컴파일·제품 시험을 수행하라는 뜻이 아니다. 실제 경계 구현과 관련 검증은 M4/M5의 해당 작업에서 한다.

M2 종료 시 위 연결에서 남은 **선정 범위의 설계·배정 공백**을 열거한다. A/B에 필요한 API·권한 의미나 필수 공급·인수의 담당/완료 조건이 미정이면 M2를 미완료로 두고 해소할 작업을 특정한다. 계약과 배정이 확정된 M4/M5의 미구현 공급·미실행 시험은 후속 의무로 남기며 M2의 실행 선행으로 올리지 않는다. 범위 밖 기능의 미정사항은 제외 근거와 후속 의무로 남긴다. M4 도중 새 공통 공급이 필요하면 이 표의 영향 행과 기존 시도·결과를 보존해 갱신하고 해당 변경을 독립 검토한다. A/B의 기능 본문을 공통 담당에게 옮기거나 검토 없이 공급 완료 조건을 늘리는 방식으로 해결하지 않는다.

<a id="phases"></a>
## 5. 작업 순서와 단계별 완료 조건

각 단계 종료는 정해진 산출물과 독립 검토로 판단하며, 사용자에게 일상 작업마다 재승인을 요청하는 절차를 추가하지 않는다. 이번 계획 작성은 아래 실행을 수행했다는 증거가 아니다.

| 단계 | 작업·담당 | 산출물 | 종료 조건 |
| --- | --- | --- | --- |
| M0 기준선·작업 보존 | 통합 담당 | 현재 revision/dirty/index·관련 untracked·동시 작업·실행 중 job·원 증거·미완료 목록; 영향 범위와 소유자 | 다른 작업을 덮어쓰지 않는 편집 범위, merge 진행/미해결 상태와 재개 지점을 확인. 새 기능 보류의 대상과 실제 확인 상태를 구분 |
| M1 현재 경계 조사 | 구조 문서 담당 | 상태 소유표·실제 의존 그래프·공통 파일 변경 이유·consumer별 차단 표 | 조사 대상의 각 의존을 필수 의미/공통 계약 미완료/불필요한 내부 접근/단순 등록 중 하나로 설명. 판단 보류는 근거·해결 질문과 연결 |
| M2 목표 구조·이행 설계 | 계약 담당+독립 검토자 | [설계 인계표](#m2-handoff)의 현재→목표 매핑, 경계 계약, 두 제품 작업과 공통 공급의 사전 선정, 유한한 이행 단위·제외 범위·되돌림 단위 | 두 작업의 정상/오류 호출 순서를 같은 하위 계약으로 대조. 소비자별 구현 착수/통합 선행, 각 M4 단위의 공급 동작·선택 인수와 M5에 남길 기능 본문 확정. 필수 미정사항 해소, 원자성·수명·복구 반례 검토, 독립 D2 수용 |
| M3 소유 문서 정합화 | 문서 담당+통합 담당 | 선정 경계에 필요한 6절의 실제 Markdown/JSON 수정, 대체 결정·선행 관계·작업 계약·변경 영향 인수 | 현재 계약과 목표 계약의 적용 이행 단위를 구별. 선정 범위의 오래된 금지/직렬 선행과 충돌 없음. 실제 공급과 미구현을 구별. 독립 D2 수용 |
| M4 구조 변경 | 경계별 구현 담당 | M2의 이행 단위별 코드·관련 선택 검증·현재 소비자 연결·D1/D2 기록 | 선정한 공통 공급과 소비 경계를 실제 코드로 제공. 제품 의미/자원 보존, 변경 경로의 실제 복구 의무 충족. A/B 기능 본문은 M5의 담당 범위로 두며 해당 실행 지시가 있을 때 착수 |
| M5 두 제품 기능 병렬 인수 | 제품 담당 A/B+통합+별도 검토 | 같은 기준에서 진행한 서로 다른 두 제품 변경, 각 국소 인수, 실제 합류 결과·충돌/대기 기록 | 8절의 사전 고정 인수 기준 충족. 계획/코드 분할/검증 도구 병행만으로 완료하지 않음. 후속 일반 기능 확대 재개 |

**문서 정비 중 보류 범위:** M0에서 지정한 DEV3 공유 경계에 새 상태 전이·소비 기능을 추가하는 작업을 보류 대상으로 기록한다. 기존 실행 중 작업은 담당의 안전한 경계에서 결과·FAIL·중단·미완료를 보존하고 인계한다. 다른 세션을 무단 종료하거나 서비스/호스트를 재시작하지 않는다. 무관한 독립 조사·문서·검토까지 일괄 중단하지 않는다.

보류 대상의 지정과 실제 보류 확인은 다르다. 각 대상의 담당·기준 source·인계 위치·확인 상태를 기록하고, 확인하지 못한 작업은 진행 가능성이 있는 것으로 취급한다. M0의 목록만으로 다른 채팅에 연락하거나 작업을 중지할 권한을 추정하지 않는다. 충돌 범위의 변경은 인계 확인 또는 편집 범위 분리 뒤 진행한다. M2/M3 채택 전과 M4/M5 착수 전에는 기준 이후의 실제 diff를 대조하여 영향을 받는 설계·계약·시험 선택만 갱신한다. 무관한 HEAD 변경으로 완료한 검토·시험을 처음부터 반복하지 않는다.

**첫 전환의 한도:** M2는 공통 공급의 각 이행 단위에 제공 API·완료 동작·필수 인수·제외 범위를 적는다. 해당 공급에 필요하지 않은 DEV3 잔여 기능이나 4절의 모든 책임 재배치를 M3/M4의 선행으로 추가하지 않는다. 새로운 공통 의미가 필요해 범위가 늘면 원 계획과 이유를 보존하고 영향을 받는 M2 설계를 다시 검토한다. 전체 모듈화가 끝날 때까지 두 소비 기능의 착수를 미루는 포괄적 선행으로 바꾸지 않는다.

**재개는 필요한 공급별로 판단한다.** M3 수용 뒤에는 구조 변경인 M4를 진행할 준비가 된다. 사전 선정한 A/B의 제품 구현은 M2에 정한 착수 계약과 최소 컴파일 경계가 제공된 부분부터 M5 작업으로 시작할 수 있다. 경로·의미가 겹치지 않는 M4의 잔여 공급과 병행하되, 실제 공급이 필요한 인수는 해당 공급의 수신 뒤 수행한다. M4 전체 완료를 모든 소비 구현의 일괄 선행으로 삼지 않는다. 일반 기능 확대는 M5 인수 뒤 해당 경계부터 연다. 전체 DEV0–DEV9의 모듈화 완성이나 DEV3 S5 완료를 M5의 선행으로 요구하지 않는다. M4/M5에는 후속 해당 작업의 실행 지시가 필요하며, 이 계획서 작성만으로 제품 구현·시험을 시작하지 않는다.

<a id="document-map"></a>
## 6. 후속 에이전트의 문서 수정 대상과 방법

현재 소유 문서의 본문과 관련 JSON을 함께 고친다. 종합 계획에 새 원칙을 덧붙이고 실제 owner 계약은 그대로 두는 방식으로 완료하지 않는다. 아래는 우선 읽기/수정 표이며 모든 파일을 기계적으로 수정하라는 목록은 아니다. 각 행에 실제 수정 절 또는 미수정 근거를 남긴다.

| 소유 문서·연결 자료 | 필요한 수정 |
| --- | --- |
| [병렬 정책](../policies/parallel-development-policy.md)·관리 JSON | 독립 개발의 정의, 공통 계약 동결/변경 절차, `owned_paths`와 의미 중복 검토, 공유 등록 지점, PD01–03의 실제 인수 연결. 측정 전 충돌 감소를 선언하지 않음 |
| [코딩 컨벤션](../policies/coding-convention.md#section-4)·관리 JSON | 기능 소유 경계, 최소 visibility, 조립과 공통 의미, 부모 재노출 사슬의 판단 기준. 일률적 crate/trait/파일 분할 규칙 도입 금지 |
| [저장소 배치](../policies/repository-build-plan.md)·관리 JSON | 목표 역할·허용 의존·테스트 의존·공유 조립 책임. 실제 경로·package·source fingerprint·export 목록은 M4 구현과 함께 등록 |
| [구현 계약](../contracts/implementation-contracts.md)·[disk 저장 설계](../contracts/disk-storage-design.md#ownership) | transaction/storage/WAL/recovery의 실제 소유·호출 방향. 불변식·원자성은 유지하면서 원 owner의 생성/이전과 모듈 접근 경계를 재설계 |
| [저장 세션](../contracts/dev3-storage-session.md#consumer-driven-refinement)·[WAL I/O](../contracts/dev3-wal-io.md)·[Commit continuation](../contracts/dev3-commit-continuation.md) | 설치→정상→복구 owner 관계, 원 fence/lease/slot/pin 권한, 상태 토큰·정확 durability 관측. 중첩 모듈 구조를 유지해야 한다는 가정을 검토·대체 |
| [DEV3 작업별 설계](../contracts/dev3-task-designs.md#catalogue)의 해당 3.02–3.14 | 각 기능의 provider/consumer, 허용 API, 오류/자원/수명, 독립 구현 가능 범위와 실제 통합 인수. 소유 계약과 다르게 이중 정의하지 않음 |
| [Catalog CREATE](../contracts/dev3-catalog-create.md)·[plan4 복구 owner](../contracts/dev3-plan4-recovery-owner.md) 등 선택 경계의 직접 계약 | 현 코드의 symbol·권한 생성자·공유 버퍼·복구 인계와 목표 구조 대응. 옮기는 책임과 유지하는 제품 의미를 별도 표시 |
| [DEV3 구현 계획](dev3-implementation-plan.md#parallel-development)·[공급 설계도](../contracts/dev3-task-designs.md#dependency-map) | 전체 기능 번호순 선행을 최소 공급/독립 준비/실제 합류로 구분. M0–M5와 일치하는 착수·재개, A/B 실제 담당·파일 범위·통합 지점 배정 |
| [하위 계층 개발](bottom-up-development.md)·[개발 단계](development-stages.md)·단계/완료 JSON | bottom-up 실제 통합·인수 순서는 보존. 개발 착수의 독립 가능 범위를 명시하고 새 구조 인수를 제품 단계 완료나 S5 면제로 바꾸지 않음 |
| [DEV3 시험 계획](dev3-test-plan.md)·[모듈 시험 목록](../contracts/module-test-matrix.json) | 새 경계의 좁은 인수와 실제 연결 의무, diff→case 선택, 원 시험 ID 이동 매핑. 기존 전수 inventory·원 FAIL·원 source 증거 유지 |
| [DEV4 독립 앞단](../contracts/dev4-independent-frontend.md)·[준비 계획](dev4-preparation-plan.md)·[구현 계획](dev4-implementation-plan.md) | SQL에서 받을 실제 disk API와 최소 공급을 새 경계에 연결. 독립 frontend/model 진행과 엔진 인수를 구분. 최신 DEV4 병합 상태를 다시 수신 |
| [AI 작업 절차](../policies/ai-development-method.md#section-8)·[작업 계약 예시](../examples/task-contract.example.json)·[AI 효율 계획](dev3-ai-throughput-improvement.md) | “같은 상태이므로 한 담당이 계속 구현”을 상시 해법으로 삼지 않음. 공통 공급 동결→제품 A/B→합류를 실제 작업 입력에 반영. 독립 검토와 구현 병렬을 구별 |
| [AGENTS.md](../../AGENTS.md)·[문서 진입점](../README.md)·폴더 README·[문서 색인](../documents.json) | 유효한 구조 전환 방향·보류/재개 범위와 읽기 경로를 짧게 연결. 시험 제한·예외의 적용 범위·push gate를 그대로 유지 |
| [구현 상태](../implementation/README.md)·해당 구현 문서 | 실제 코드가 바뀐 범위만 갱신. 문서 설계만 고친 단계는 “목표 구조 미구현”으로 표시. 과거 실행 증거는 재작성하지 않음 |

후속 문서 담당은 `문서/절 → 기존 규칙 → 변경 문구/계약 → 이유 → 영향 API·소비자 → 독립 검토 → 구현/인수 상태` 표를 이 계획의 실행 기록 또는 기존 결정 절에 남긴다. 장문 규범은 소유 문서 한 곳에 두고 이 계획은 작업 순서·진행 참조를 유지한다.

### 기존 결정과의 대체 관계

- [D3-DIRECTION-002](dev3-implementation-plan.md#direction-refinement)와 [저장 세션의 국소 개선](../contracts/dev3-storage-session.md#consumer-driven-refinement)은 당시 첫 행 연결을 우선하여 전체 재배치를 선행하지 않도록 했다. **이번 사용자 방향은 모듈 독립성을 위한 구조 변경을 먼저 설계하는 것**이다. M3는 해당 “국소 변경만 허용/구조 정리를 선행하지 않음” 범위에 한정해 명시적인 대체 관계를 기록한다. 기존 결정을 지우거나 제품 의미·품질·첫 행 최종 목표까지 폐기하지 않는다.
- 단일 작성자는 공통 계약 변경과 원자적 합류의 담당이다. 모든 기능 본문을 한 사람이 순차 개발해야 한다는 규범으로 확대하지 않는다. 기존 병렬 표의 준비 역할과 실제 제품 A/B 역할을 구분한다.
- 계획 채택 시점에는 기존 owner API를 임의 변경할 수 없다. M2/M3의 구체 D2 결정이 채택되면 해당 이행 단위부터 새 계약을 적용한다. 과거 source의 증거와 호환 정책은 당시 의미를 유지한다.

<a id="handoff"></a>
## 7. 에이전트 작업 입력과 변경 관리

문서 정비는 공통 의미·선행 관계를 바꾸는 D2다. [DP01](../policies/development-policy.md#section-2)에 따라 작성자와 다른 검토자가 실제 원문·diff·반례를 읽고 확인한다. 사용자가 확정한 방향 안의 통상 결정에 반복적인 사람 승인을 추가하지 않는다.

후속 에이전트의 첫 작업 입력은 다음과 같이 고정한다.

> 이 계획의 M0–M3를 수행한다. 최신 소스와 진행 작업을 확인해 영향을 받는 기능 확장의 보류·인계 범위와 실제 확인 상태를 기록한다. DEV3 내부의 현재 상태 소유·의존·접근 경로를 조사하고, 서로 다른 두 제품 기능이 같은 하위 계약을 사용해 독립 개발할 목표 구조와 API를 설계한다. [M2 설계 인계표](#m2-handoff)에 따라 M4의 유한한 공통 공급과 M5에 남길 A/B 기능 본문, C+A·C+B 단독 검증과 실제 합류 인수를 먼저 정한다. 선정 경계에 필요한 6절의 실제 소유 문서·JSON·개발 순서·작업 계약을 수정하고 독립 D2 검토를 받는다. 현 구조의 설명·파일명 변경안만으로 종료하지 않는다. 제품 구현·시험 실행은 이 문서 정비 작업에 포함하지 않는다. 기존 코드·병합·시험 증거는 보존한다.

각 실제 작업에는 다음을 기록한다.

| 구분 | 필수 내용 |
| --- | --- |
| 기준·소유 | HEAD와 dirty/index·관련 untracked digest, 작업/계약/통합 담당, 좁은 독점 경로, 공용 문서·등록 담당, 실제 동시 작업과 보류 확인 상태 |
| 제공·소비 | 공급할 계약/실제 API, 소비자 둘과 최초 호출 경로, 구현 착수의 최소 선행, 허용 대역·한계, 실제 공급을 기다릴 통합 인수 |
| 변경 영향 | 공유 상태·공통 API·형식·lock/자원·오류 의미의 변경 여부, 미공급 기능, 다른 담당이 수정해야 하는 파일과 이유 |
| 인수·인계 | 새/변경 코드의 case 선택 이유, 원 결과·미실행 의무, 별도 검토자, 병합 순서, 다음 담당이 받을 source/계약 identity |

공유 README·문서 색인·manifest·모듈 조립은 한 담당이 최신 원문에 좁게 반영한다. merge 충돌이 진행 중이면 해당 owner에게 인계하거나 분리된 문서 작업 범위로 진행하고, 충돌 표식을 임의 삭제하거나 상대 변경을 옛 사본으로 덮지 않는다. 별도 worktree도 같은 파일/의미를 바꿀 수 있으므로 독립성의 증거로 삼지 않는다.

M4의 각 이행 단위는 변경 전 소비 경로, 이동/추출한 책임, 변경한 API, 영향 case, 인계 revision을 갖는다. 한 단위의 실패는 해당 patch/commit을 기준으로 복구하며 다른 작업을 일괄 reset하지 않는다. 필요하면 수명이 정해진 내부 adapter를 두되 소유자·제거 조건을 기록하고 이중 권한/이중 정답 구현을 만들지 않는다. 이전 사내 DB 호환을 위해 영구 adapter를 추가하지 않는다.

M5 이후에도 같은 작업 입력을 사용한다. 새 소비 기능 때문에 다른 기능의 내부나 공통 의미를 바꿔야 하면 원인과 최소 공급을 먼저 기록하고, 해당 경계의 계약 변경을 별도 검토한다. 공통 파일 수정이 반복되면 기능 구현을 무조건 직렬화하는 대신 경계를 다시 평가한다. 이번에 인수한 경계를 다른 미검토 영역의 독립성 증거로 확대하지 않는다. 이 지속 적용 기준은 M3에서 병렬 정책·AI 작업 절차·작업 계약 예시에 반영한다.

<a id="acceptance"></a>
## 8. 모듈 독립성의 실제 인수

### M2에서 먼저 선정할 두 제품 작업

각 작업은 사용자 기능 또는 저장 기능의 관측 가능한 동작을 실제로 추가·변경해야 한다. 구현+oracle, 구현+리뷰, 문서+시험 도구를 두 제품 작업으로 세지 않는다. 코드 이동만 두 갈래로 수행하는 것도 해당하지 않는다.

후보는 현재 미완료 backlog의 tree/행 연산과 Catalog 조회·변경 등에서 고른다. **행과 index가 반드시 독립이라고 미리 가정하지 않는다.** 공통 tree/undo를 동시에 바꿔야 하면 그 최소 공급을 M4 선행으로 추출하거나, 같은 새 경계를 소비하는 다른 실제 기능을 선정한다. 각 후보의 의미 의존·소유 경로·완료 동작·필수 시험을 비교해 독립 검토자가 적합성을 확인한다. 쉬운 rename이나 관계없는 SQL lexer 작업으로 DEV3 내부 경계의 인수를 대신하지 않는다.

M2 종료 전에 A/B의 실제 기능·기준 계약·하위 공급·허용 변경 경로·공유 수정 목록·합류 소비자를 확정한다. 작업 분해에서 A와 B 각각에 실제 제품 동작을 구현하는 본문과 인수를 배정한다. 기능별 본문을 M4의 공통 담당에게 몰아 구현하고 M5에서 이름·연결·시험만 나누는 방식은 독립 개발 인수가 아니다. 선정할 수 없으면 막는 공통 공급과 한정된 해결 단위를 기록하고 M2를 미완료로 둔다. 후보를 실행 뒤 유리한 결과에 맞춰 교체하지 않는다.

### 단독 개발·검증 기준

실제 구현 착수의 공통 source·동결 계약·최소 컴파일 경계를 **C0**로 기록한다. C0에 대역이나 미공급 효과가 있으면 허용할 구현·시험 범위와 미실행 의무를 표시한다. M4에서 선정 경계의 실제 공급을 수신한 인수 기준을 **C**로 고정하고 C0→C의 공급 변경을 A/B 기능 변경과 구분한다. 기존 최소 공급으로 충분하면 C0와 C는 같을 수 있다.

A 단독 산출물은 **C+A**, B 단독 산출물은 **C+B**로 기록한다. 각 산출물에는 사전 지정한 해당 기능의 최소 연결·시험 코드만 포함하며 상대 기능의 미완료 코드·fixture·빌드 산출물에 의존하지 않는다. C0에서 시작하는 최초 기능 검증부터 상대 기능이 섞이지 않은 source를 사용한다. 대역으로 확인한 범위는 실제 공급의 인수와 구분한다. 별도 worktree 또는 고정 export 등으로 입력을 보존하되 수단 자체를 독립성의 증거로 삼지 않는다.

source·binary·fixture·출력과 필요한 build 경로를 구분하여 상대 작업의 변경이 실행 중 섞이지 않게 한다. 자원 간섭 때문에 시험을 순차 실행할 수 있으며, 제품 구현의 실제 겹침과 시험 동시 실행을 혼동하지 않는다. 합류 source는 **C+A+B+사전 지정 연결 변경**으로 추적한다. 최신 통합 기준을 수신할 때는 C 이후 차이를 분리해 영향만 검토한다. 공통 의미가 바뀌면 원 시도·결과를 보존하고 MI02에 따라 새 기준을 정하며, 조용히 rebase한 결과를 원래 기준의 독립 인수로 표시하지 않는다.

### M5 필수 기준

| ID | 판정할 사실 | 인수 조건 |
| --- | --- | --- |
| MI01 계약으로 독립 착수 | 동일 C0의 동결 계약과 최소 경계를 받은 A/B가 각자의 명세·API·시험 입력으로 구현 | 별도 구현 담당과 겹치는 실제 제품 구현 구간을 기록. 서로의 미완료 제품 본문이나 비공개 필드 수정을 기다리지 않음. C0→C 공급 대기와 실제 통합 의무를 별도 표시 |
| MI02 기능 변경의 국소성 | A/B의 전용 경로와 공용 변경 diff | 상대 기능 내부의 계획 밖 변경0. 공통 변경은 사전 지정한 연결/등록에 한정. 공통 의미 수정이 새로 필요하면 원 시도를 보존하고 M2/M4로 환류 |
| MI03 독립 인수 | C+A, C+B 각각의 실제 core·오류/수명·자원 경계 | 상대 기능이 포함되지 않은 고정 source와 실제 최소 공급에서 독립 기대값으로 선택 검증. 제품 코드와 oracle 신뢰 경계 보존 |
| MI04 실제 합류 | C를 수신한 A/B와 연결 변경을 합쳐 실제 소비 경로 연결 | 최신 통합 기준과의 차이를 기록하고 변경된 연결/신규 인수 코드에 필요한 상호작용·오류·복구를 확인. text merge 성공만으로 합격하지 않음 |
| MI05 보호와 추적 | 공유 파일·증거·반영 순서 | 다른 담당 변경/원 결과 덮어쓰기0. 텍스트 충돌과 의미 충돌, 계약 재개방·대기·재작업 원인을 따로 기록 |
| MI06 별도 판정 | 계약·실제 diff·원 실패·source별 결과 | 독립 D1/D2 검토의 미해결 필수 지적0. SKIP/중단/미공급/미실행을 PASS로 바꾸지 않음 |

MI02의 사전 등록 연결 수정은 통합 담당의 작업으로 남기며 기능 본문 공동 수정을 그 이름으로 숨기지 않는다. 연결 파일의 단순 text conflict는 해결 내용과 비용을 기록해 판정할 수 있지만, 미예정 공통 의미 변경으로 원 계약이 깨진 경우는 MI02를 충족하지 못한다. 판정 기준은 실행 전에 동결한다.

PD01은 이 A/B 사례에 연결한다. PD02/PD03은 작업 중 실제 공통 계약·문서/생성물 동시 변경이 있었다면 해당 사례로 판정하고, 없었다면 NOT_RUN으로 남긴다. M5 완료를 PD01–03 전부의 PASS로 확대하지 않는다. 새 인수는 원 PD 기록·과거 실패를 소급 수정하지 않고 source별 증거를 추가한다.

기록할 비용은 실제 시작/종료·계약 대기·공유 파일 조정·merge 충돌 해결·리뷰 재작업이다. 중첩한 작업자의 시간을 경과 시간으로 더하지 않는다. 대조군 없는 두 작업으로 병렬 속도 배율을 주장하지 않는다. 본 인수의 주장은 **선정한 두 기능의 독립 구현과 실제 합류 가능성**이며 전체 DBMS의 무간섭 보장이 아니다.

<a id="verification"></a>
## 9. 시험·검사와 증거 보존

- M0–M3 문서 작업에서는 제품 코드·시험·측정 실행을 시작하지 않는다. 독립 검토는 소스·계약·diff를 읽어 수행한다. 새 문서와 JSON을 추가했다는 이유로 문서 verifier를 실행하지 않는다.
- M4/M5는 마지막 검증 이후 **실제 코드 diff→영향 계약→선택 case**를 실행 전에 연결한다. 새 경계·이동/변경한 코드와 실제로 영향받은 소비 경로만 검증한다. 명칭·이동·구조 변경이라도 필요한 의미/컴파일 경계 검증은 하되 무관한 기존 시험을 포함하지 않는다.
- 단독 source는 최초 기능 검증부터 사용하며 병렬 인수용 재실행 단계를 추가하지 않는다. C0→C 수신은 실제 바뀐 공급·연결 경로의 인수를 선택하고, 완료한 동일 A/B 시험이나 무변경 C의 시험을 반복하지 않는다. 병합·source fingerprint·상위 단계 변경만으로 반복하지 않으며, 병합에서 코드가 바뀌면 그 diff와 관련 실패/미실행 사례를 선택한다. 새 합류 인수가 필요하면 M2에서 실제 연결을 확인할 신규 시험을 함께 설계한다.
- owner·pin·fence·WAL/flush·recovery의 의미/수명이 바뀌면 해당 변경에 필요한 현재-build 실제 I/O·동시성·재시작/재장애 인수를 배정한다. 무변경 DEV0/1/2 전체 회귀·workspace·하위 wrapper를 구조 전환 인수로 실행하지 않는다.
- 기존 DEV3 최종 S5의 확정 inventory와 전수 예외는 그대로 유지한다. M5는 S5를 대체하지 않으며 새 전수 실행 예외도 아니다. COMM·PF04의 별도 진단/시험 지시를 구조 변경의 무변경 회귀 허가로 확대하지 않는다.
- 문서 검사·전체 fmt/Clippy push gate는 [DOC-002](../policies/documentation-policy.md#push-gate)에 따라 실제 push할 tip commit에서 수행한다. 편집/commit 때 실행하지 않으며, Clippy 정적 검사와 기능 시험 결과를 구별한다.
- old/new source·시험 selector·binary·fixture·원 FAIL/UNKNOWN/NOT_RUN의 연결을 보존한다. 경로를 옮길 때 현재 링크와 역사적 원 경로의 매핑을 기록하고 과거 증거를 현재 결과로 재표기하지 않는다.

<a id="decision"></a>
## 10. 결정·실행 기록

| 항목 | 기록 |
| --- | --- |
| 결정 ID | XADB-MODULE-INDEPENDENCE-001 |
| 분류·상태 | D2 · ACCEPTED / 작업 계획과 문서 인계 범위에 한정 |
| 권한 근거 | 2026-10-26 사용자의 현재 구조 변경 방향 및 후속 에이전트 문서 정비를 위한 작업 계획서 작성 요청 |
| 작성·계약·통합 | `/root`; 구체 경계의 후속 담당과 실제 실행 A/B는 M0/M2에서 배정 |
| 선택 | 필요한 하위 의존·제품 불변식을 유지하면서 실제 소유 계약과 최소 공급을 정비. 준비된 계약 경계부터 두 제품 기능을 독립 개발하고 실제 공급을 수신해 합류를 확인 |
| 비교 대안 | 현 구조에서 인원만 늘리면 공통 수정 대기를 해결하지 못함. 파일/이름 이동만으로는 API와 소유 경계가 유지됨. 모든 역할의 별도 crate·전체 선설계는 검증/이행 범위를 불필요하게 확대. 실제 소비자 두 개를 기준으로 경계를 확정하는 안 선택 |
| 대체 범위 | 기존 초기 독립 개발 요구를 재확인. 현재 계약은 M3의 명시적 후속 결정으로 수정. 국소 정리만 허용하던 작업 순서의 관련 제한을 구조 전환 범위에서 대체할 대상으로 지정; 품질·제품 범위·S5/호환 의무는 유지 |
| 최초 별도 검토 | `/root/module_plan_review` · 2026-10-26 · 최초 계획 전체와 탐색 색인3파일의 변경, DP01/병렬 정책·관련 DEV3 소유/시험/국소 개선 계약 및 실제 WalRuntime·InstalledStorageCore·Plan4Retention 경계를 읽기 대조해 수용. 후속3개 문구 보완도 재확인, 당시 필수 수정0·차단0. `/root` 반영 확인. Git 집계 재산출·제품/문서 검사 실행 없음. 아래 재검토 수정본이나 M2/M3의 구체 API/계약 검토, M4/M5 실행 인수를 대신하지 않음 |
| 실행 | 계획서·색인 작성만. 소유 문서 일괄 개정·제품 구조 변경·시험·병렬 pilot·다른 채팅 중지 모두 미실행 |
| 재검토 조건 | 새 구조가 권한/원자성/복구·자원을 약화, 두 실제 소비자를 선정할 수 없음, 공통 의미 수정이 반복, adapter가 영구화, 무관한 선행/자동화가 확대되거나 후속 사용자 방향이 변경됨 |

후속 실행 기록은 M0–M5별 담당·기준 identity·산출물 링크·판정·잔여를 추가한다. 작업 시작 전은 NOT_RUN, 문서 채택 뒤 제품 미구현, 구조 변경 뒤 pilot 미실행을 각각 구별한다. 이 계획서의 존재를 개발 중지·구조 전환 완료·충돌 감소의 실행 증거로 사용하지 않는다.

<a id="review-002"></a>
### 2026-10-26 사용자 요청 재검토

기준 HEAD는 `27c2288b6ed7b48e556c179067348c686650b3f9`이며 계획서와 기존 색인3파일이 미커밋 상태였다. 사용자의 계획서 리뷰·즉시 수정 지시에 따라 `/root`가 원문과 관련 정책을 다시 대조하고, 최초 검토와 다른 `/root/module_plan_second_review`가 독립 D2 검토를 수행했다. 수정 대상은 이 계획서다.

| 발견한 문제 | 반영한 수정 |
| --- | --- |
| 개발 선행과 통합 선행을 나눈 원칙과 달리, 실행표는 M4 전체 실제 공급 뒤에만 소비 구현을 시작하는 것으로 읽힘 | 소비자별 구현 착수·허용 대역·실제 공급 인수를 M2에서 구분. 준비된 경계는 M4 잔여 공급과 병행하며 착수 C0와 실제 인수 C를 구별 |
| 공통 공급의 종료 범위가 추상적이어서 DEV3 잔여 작업이나 A/B 기능 본문까지 M4에 흡수할 수 있음 | 첫 전환의 유한한 공급 단위·완료 동작·제외 범위를 고정. A/B 각각의 실제 제품 로직은 해당 기능 담당에게 배정 |
| 각각 시험했다는 기록만으로는 상대 기능이 섞인 소스의 숨은 의존을 배제할 수 없음 | 최초 검증부터 단독 source를 사용하고 C+A·C+B 및 합류 source를 구분. 공급 변경·빌드/fixture 입력을 추적하되 완료한 무변경 시험은 반복하지 않음 |
| 보류 대상 지정만으로 실제 작업이 정지했거나 M0 기준이 계속 유효하다고 오해할 여지 | 실제 보류 확인·인계 상태를 별도 기록. 채택/착수 시 관련 변경만 다시 대조하고 미확인 작업과의 충돌 범위를 분리 |

**수정본 D2 수용 · 미해결 필수 수정0 · 차단0.** `/root/module_plan_second_review`가 수정 전후 diff와 수정본 전체, DP01/DP02·병렬 정책·문서 push gate·DEV3 공급/완료 규칙을 읽기 대조했다. C0/C 구분, 경계별 M4/M5 병행, A/B 실제 본문 배정, 단독 source와 실제 공급 인수, 변경 코드 시험 제한의 일관성을 확인했고 `/root`가 반영을 확정했다. 기존 수용 기록을 이번 수정본의 검토 근거로 재사용하지 않았다.

제품·시험·빌드·자동 문서 검사·커밋·다른 채팅 중지는 실행하지 않았으며, 이 재검토로 M0–M5 실행 상태를 승격하지 않는다. 구체 API·구조 구현·실제 병렬 개발의 인수는 후속 작업으로 남는다.

<a id="review-003"></a>
### 2026-10-26 M2 설계 인계 보완

| 항목 | 기록 |
| --- | --- |
| 기준·권한·담당 | HEAD `fc122f5b17ae6b88cd421f3384b00b40287f85eb`, 착수 시 작업 트리/index clean. 사용자의 “계획문서의 보완이 필요하면 해주세요” 요청. 작성·계약·통합 `/root` |
| 분류·상태·관계 | D2 · 보완안 ACCEPTED. XADB-MODULE-INDEPENDENCE-001의 M2 산출물 구체화이며 기존 목표·M0–M5 순서·MI01–06·제품/시험 의무를 유지 |
| 이유·선택 | 기존 원칙의 차단 결함은 발견하지 않았으나 A/B·API·공통 공급·인수 항목이 각 절에 분산되어 실제 설계 인계에서 연결을 누락할 위험이 있음. [설계 인계표](#m2-handoff)로 필요한 호출과 공급을 양방향 대조하고 선정 범위의 미정사항을 종료 조건에 연결 |
| 대안·범위 | 현재 조사 없이 A/B나 API를 미리 확정하는 대안은 배제. 별도 보고 체계 대신 기존 계약·작업 기록을 연결. 계획 본문·단계표·후속 작업 입력과 탐색 README2개만 보완. 문서 ID·경로·제목·역할은 같아 `documents.json`의 기존 등록 유지 |
| 검토·반영 | `/root/module_handoff_review`가 최신3파일 diff·계획 전체·AGENTS·DP01/DP02·코딩/병렬/문서 정책·하위 계층 순서·DEV3 시험 선정 규칙을 읽기 전용 대조하여 D2 수용, 미해결 필수 수정0·차단0. M2의 설계/배정과 M4/M5의 미구현/미실행 의무 구분, C0/C·병행·A/B 본문 배정·범위 한도·불변식 보존을 확인했고 `/root`가 반영을 확정. 과거 review-002의 수용을 재사용하지 않았으며 구체 API 설계·실제 공급·병렬 인수의 검증은 범위 밖 |
| 실행·재검토 | 계획·탐색 색인 보완에 한정. M0–M5 수행·제품 변경·테스트·빌드·자동 문서 검사·commit/push 없음. 표가 무관한 기능 선설계·범위 확대·중복 문서 작성의 선행으로 사용되면 재검토 |

2026-10-26 후속 사용자 지시에 따라 의미 있는 변경 단위마다 필요한 선택 검증·독립 검토를 마치고 커밋한다. 커밋 완료는 해당 부분공급의 저장이며 M4/M5 전체 완료를 뜻하지 않는다.

<a id="execution"></a>
## 11. 2026-10-26 계획 실행

2026-10-28 사용자 지시로 `codex/module-independence-finish`의 기존 작업을 `main`에 병합한다. 이후 이 계획의 개발·재개 기준은 **`/home/et16/work/xadb`의 `main`**이다. 채팅을 새로 열어도 실제 마지막 작업 위치·미커밋 상태·검증 source를 먼저 확인하며 기존 작업을 다른 기준선에서 다시 구현하지 않는다. [병합 기록](../evidence/module-independence-survey-20261026.md#merge-main-20261028)은 두 parent의 고유 변경·원 증거·선택 검증을 구별한다. 아래 과거 브랜치별 실행 기록과 수치는 당시 source에 귀속하며 병합 source의 실행 결과가 아니다. M4/M5 전체는 미완료다.

같은 날 후속 사용자 지시로 남은 작업은 main `854bd8d`에서 만든 별도 branch `claude/module-independence-remaining`에서 단계별 commit으로 진행한다([작업 지침](../../AGENTS.md)). 병합 전 stash 초안은 적용하지 않는다. 각 실행의 build target·임시 export·ELF는 결과 기록 뒤 삭제하고 commit/tree·파일별 hash·로그·결과로 source identity를 보존한다.

사용자의 후속 “이 계획서를 실행해 주세요” 요청으로 M0–M5 실행을 시작한다. 이 요청은 계획 작성만 허용했던 앞선 작업과 구별하며 M4/M5의 실행 지시를 포함한다. 기존 제품 불변식·독립 검토·변경 코드 시험·최종 S5 의무를 유지한다.

| 단계 | 현재 상태·근거 | 남은 종료 조건 |
| --- | --- | --- |
| M0 | [기준선 확인](../evidence/module-independence-survey-20261026.md#baseline) 완료, 별도 worktree 격리. `66ddf61` 재개와 main `b7ce51d` 확인. 다른 세션 중지 없음 | 후속 합류 때 실제 관련 diff 재확인 |
| M1 | [소유·의존 분류와 소비 후보](../evidence/module-independence-survey-20261026.md#survey)·[원 source 독립 검토](../evidence/module-independence-survey-20261026.md#review) 완료 | 선정 경계의 후속 관련 diff가 생기면 영향만 갱신 |
| M2 | 완료·ACCEPTED. [최종 독립 D2](../evidence/module-independence-survey-20261026.md#final-design-review) 수용·필수 수정0을 수신해 API·소비자·유한 공급·인수 설계 동결 | M4/M5에서 새 반례 또는 계약 변경이 생기면 영향 행만 재검토 |
| M3 | 완료·ACCEPTED. 소유 문서·운영/시험 JSON 채택 반영, MBC31 보완의 한정 재확인까지 독립 D2 수용·필수 수정0 | 구현에 따른 실제 공급 상태와 등록을 해당 diff에 맞춰 갱신 |
| M4 | IN_PROGRESS. C0 borrowed codec 선택6 PASS·[독립 수용](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec), 생산자 타입 compiler4 자동 PASS·수명 거절1 독립 수동 수용·원 판정기 FAIL 보존([증거](../evidence/module-independence-survey-20261026.md#c0-producer-types)). [C1 실제 물리 WAL 분리](../evidence/module-independence-survey-20261026.md#c1-physical-wal)·컴파일3 PASS와 [원 owner/port 이전](../evidence/module-independence-survey-20261026.md#c1-owner-ports)·컴파일4/신규 메모리2 PASS에 이어 [control/isolation backing](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)·컴파일3/변경·신규 runtime4 PASS·독립 한정 수용. [epoch backing 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)·compiler3/변경 runtime3 PASS·독립 D1/D2 한정 수용. [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 fresh/최종 복구 seeding·CREATE/Commit을 구현하고 source별 선택 실행·원 FAIL·빈 Commit 범위 정정을 보존했다. [읽기 전용 거래/statement 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)과 장기 credit을 추가하고 source별 compiler·선택 runtime7 결과를 보존했다. [C2 동기 Catalog 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 기존 actual lookup 수신·source별 runtime7을 확인했으며 statement factory·short aggregate는 후속이다. [조회 수명/Commit Drain 연결](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime)은 source별 선택13을 검증했으며 동시 byte 합은 아직 미공급이다. [Catalog 후보 scratch 재사용](../evidence/module-independence-survey-20261026.md#c2-catalog-scratch)은 중복 local buffer를 제거했으나 전체 peak는 그대로이며 aggregate 여유 확보로 승격하지 않는다. [거래·Aging 생성 수명 분리](../evidence/module-independence-survey-20261026.md#c2-transaction-source-phases)는 source별12+7회 선택 PASS와 lookup42850→41552B 감소·C09 설명 정정을 보존하고 최종 독립 D1/D2를 한정 수용했다. aggregate admission은 여전히 미공급이다. [Pending 보유/stage 분리](../evidence/module-independence-survey-20261026.md#c2-pending-carrier)는 원 pin 이동·Drain 후 guard 재구성과 선택11개 PASS·실행 설명 정정을 보존하고 최종 독립 D1/D2를 한정 수용했다. [CatalogFull source 예약](../evidence/module-independence-survey-20261026.md#c2-short-source)은 원 isolation header의 고정 charge와 선택17개를 검증했고 독립 D1/D2를 한정 수용했다. [상주 Catalog class](../evidence/module-independence-survey-20261026.md#c2-resident-read)는 무 I/O 경로·두 reader의 합55976B와 선택9개 PASS·원 FAIL을 보존하며 독립 D1/D2를 한정 수용했다. [최종 source 반환 수명](../evidence/module-independence-survey-20261026.md#c2-source-return)의 선택7개를 검증하고 독립 D1/D2를 한정 수용했다. [Commit phase 관측](../evidence/module-independence-survey-20261026.md#c2-commit-phases)은 cfg(test)1 PASS·원 compiler FAIL을 보존하며 당시 source의 Full/Pending layout 충돌을 확인했다. [Pending image 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image)는 준비 자료568B와 관련12 PASS·원 compile FAIL을 보존한다. [source 진입 수명](../evidence/module-independence-survey-20261026.md#c2-source-entry)은 progress→source→refs와 source 실패 정리·선택5 PASS를 보존한다. [Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source)은 관련3 PASS·import-only compile과 Full37724B를 보존하며 Full/Pending 합69100B는 여전히 한도 초과다. [WAL wait phase 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source)은 cfg(test) 신규1과 반환 경계 보완1의 source별 PASS, Pending5320B/Full 합65300B를 보존하며 제품 charge는 유지한다. [Drained 실제 소비 경계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff)는 원 Drain/guard 재구성을 분리하고 normal Drop 순서·memory 복구 및 관련10 PASS를 보존한다. [Catalog CallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope)는 원 예약을 결과 소비까지 유지하고 selected query를 이동했으며 source별14+1+2·compiler4 PASS와 원 I/O fixture FAIL을 보존한다. 당시 Full38076B로 Pending 관측 합65652B였으며 [budget 행 borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-budget-rows)로 Full36972B·합64548B가 됐다. 같은 source의5+5 선택 PASS와 harness identity 원 FAIL을 보존하며 entry/tail·writer/progress의 전체 aggregate는 미공급이다. [progress Owner borrow 후보](../evidence/module-independence-survey-20261026.md#c2-borrowed-progress-rejected)는 원 identity를 바꿀 수 있는 반례로 기각·원복하고 후보14/compiler3 PASS를 별도 보존했다. [readonly replay source](../evidence/module-independence-survey-20261026.md#c2-readonly-replay-source)는 생산자 배열을 분리해 Full35895B와 source별6+계수 보완1 PASS를 확인했으나 전체 floor/phase 예약은 미완료다. [작은 Catalog 진입/정리](../evidence/module-independence-survey-20261026.md#c2-compact-call-boundary)는 실제 오류/consuming close를 공급하고 runtime25·compiler4 PASS와 Full35619B를 보존한다. 전체 bound floor·writer/progress phase 예약은 미완료다. [replay 작업 공간 대여](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace)는 원 zero tail3328B를 actual observer에 빌려 Full33980B·선택9 PASS를 확인했고 원 compile FAIL/경고와 import-only build를 구별한다. 전체 entry/tail·writer phase는 미완료다. [split 고정 source 계산](../evidence/module-independence-survey-20261026.md#c2-split-source-charges)은 actual entry의 계산을 원 exclusive split으로 옮기고 실제3port·복구 caller와 ticket16B를 계수했다. 선택16 PASS·Full33972B/shared22400B이며 전체 entry/tail·writer phase는 미완료다. [예약 밖 호출 phase 관측](../evidence/module-independence-survey-20261026.md#c2-uncharged-call-phases)은 작은 source 실패 helper·선택4 PASS·원 test compile FAIL을 보존한다. 같은 원 core34입구의 compact floor15057B와 기존 wide tail은 cfg(test) 관측이며 제품 admission 미연결이다. [query 종료 경계](../evidence/module-independence-survey-20261026.md#c2-query-tail-boundaries)는 정상 소비 block·작은 오류 정리와 선택5 PASS, 정상710B/오류1394B 한 호출 관측을 보존하며 전체 floor는 미연결이다. [compact DPT](../evidence/module-independence-survey-20261026.md#c2-compact-dpt)는 원 private 배열3072B를 줄이고 helper660B를 actual source에 연결했다. r3 선택12·r2 원 표현/진단2 PASS와 기존 FAIL을 보존하며 shared19328B/Full33972B/Resident17588B·전체 floor 미공급이다. [LOAD claim borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-load-claims)는 원 conflict predicate·선택4 PASS와 compact floor14776B 관측을 보존하며 admission은 미연결이다. [예약 뒤 원 검산](../evidence/module-independence-survey-20261026.md#c2-admitted-validation)은 내부 입구 우선순위 D2를 한정 재개방하여 pool/snapshot 검산6958B를 원 Full/Resident 예약 안으로 옮겼다. 선택12 PASS·원 test-build FAIL을 보존하며 전체 floor·writer phase는 미공급이다. [statement Catalog 읽기](../evidence/module-independence-survey-20261026.md#c2-statement-reads)는 기존 Full/Resident Scope에 실제 statement를 연결하고 Found/Missing 자동 catalog bit를 공급한다. runtime10/compiler3 PASS·원 test-build FAIL을 보존하며 whole floor/writer는 미완료다. [apply 설치 권한](../evidence/module-independence-survey-20261026.md#c3-install-grant)은 Catalog3 actual producer와 legacy4 소비 bridge를 원 generic installer에 연결했다. source별 runtime3+7/compiler5 PASS·원 legacy fixture/test-build FAIL을 보존하며 독립 D1/D2 한정 수용이다. [Catalog terminal 소유](../evidence/module-independence-survey-20261026.md#c3-terminal)를 apply로 이전하고 실제 Busy와 관련 r3 runtime9/compiler4 PASS·r1/r2 FAIL을 보존했으며 독립 D1/D2 한정 수용이다. [Prepared 경계](../evidence/module-independence-survey-20261026.md#c3-prepared)는 실제 정의를 apply root로 이동하고 build2/compiler4 PASS·독립 D1/D2 한정 수용이다. [Root3 순수 codec](../evidence/module-independence-survey-20261026.md#row-root3-codec)은 신규4의 source별 PASS·원 FAIL을 보존했으며 독립 D1/D2 한정 수용이다. current Profile2를 유지한다. [Job3 cell·census](../evidence/module-independence-survey-20261026.md#row-job3-census)의 신규11 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Profile3·Row page 입구](../evidence/module-independence-survey-20261026.md#row-envelope-codecs)의 신규8 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Base4 raw layout](../evidence/module-independence-survey-20261026.md#row-base4-layout)는 별도 exact 입구의 r1 build2/runtime8 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Root3 Aging](../evidence/module-independence-survey-20261026.md#row-aging)은 별도 descriptor/후보의 r1 build2/runtime5 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Row 거래 ledger](../evidence/module-independence-survey-20261026.md#row-transaction-ledger)는 다중 root/statement의 pure 현재 잔여 검산과 source별9 selector PASS·원 fixture LSN 오류 FAIL을 보존하며 독립 D1/D2를 한정 수용했다. C3 전체는 미완료다. [Catalog Commit writer 창](../evidence/module-independence-survey-20261026.md#c2-writer-progress)은 terminal read gate와 stage/Pending/install/publish 고정 몫을 원 isolation ledger에 연결했고 r1 compiler3·runtime59 PASS·독립 D1/D2 한정 수용이다. 다른 writer·진입/반환 floor·Row progress는 미공급이다. 전체 C0·C1–C5 공급과 제품 인수 미완료 | 동결한 공급 단위의 구조 변경·관련 선택 검증·검토 |
| M5 | IN_PROGRESS. [A/B C0 제품 본문 착수](../evidence/module-independence-survey-20261026.md#consumer-c0-start): INSERT 검산/leaf 후보와 point/cursor 순수 본문을 같은 기준에서 병행하고 상대 source/fixture가 없는 첫 export에서 A10/B3 선택 PASS. actual 공급·단독/합류 제품 인수 NOT_RUN | C+A/C+B 실제 저장·조회·복구 수신, 각 단독/합류 인수·MI01–06 |

최초 실행 위치는 `/home/et16/.codex/worktrees/module-independence/xadb`, 출발 commit은 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`다. 2026-10-27 재개는 깨끗한 main `0852f06`을 확인하고, 기존 작업 공간이 다른 채팅 소유이므로 같은 commit의 `/home/et16/.codex/worktrees/module-independence-finish/xadb`로 격리했다. 이전 작업 공간과 원 artifacts는 읽기만 하며 다른 세션을 중지하지 않았다. 이번 단계의 권한·소유·동시 작업·미확인 세션은 [조사 기록](../evidence/module-independence-survey-20261026.md)에 보존한다. 계획의 전체 목표는 M5 인수까지 유지하며 문서 정비만으로 완료하지 않는다.

채택한 M2 계약은 [원 소유·소비 경계](../contracts/dev3-module-boundaries.md), [두 제품 소비자](../contracts/dev3-module-consumers.md), [Missing Row 참조 이동](../contracts/dev3-visibility-retirement.md#row-missing-traversal), [statement/cursor F02](../contracts/dev3-task-06-tree-rows.md#module-statement-cursor), [두 격리 수준의 공급](../contracts/dev3-task-10-concurrency.md#module-row-isolation), [자원 배정](../contracts/dev3-resource-bounds.md#row-missing-module-budget)에 둔다. 최종 독립 검토와 M2/M3 채택을 수신했으며 실제 공급·인수와 구별한다. 새 경로는 실제 구현 시에만 build inventory에 등록한다.

2026-10-27–28 이번 한 단계는 clean `main`·`4ed0cf9`에서 Aging/Root source 계산을 적응했다. 참고 `c4b9b6f`의 제품5파일과 main 전용 관측/선택 입력을 수신하며 다른 브랜치 전체·과거 실행 결과는 병합하지 않는다. [이번 증거](../evidence/module-independence-survey-20261026.md#c2-aging-source-main)에 원 실패·선택4개와 독립 검토를 구별한다. 다음 공급 의무는 실제 WAL 대기/Drain의 호출·반환 수명 구체화와 writer/progress·전체 동시 source admission이다.

2026-10-28 이번 한 단계는 clean `main`·`e76e8c4`에서 실제 WAL wait 호출·반환 source 관측을 적응했다. 원 core 포함 main 값에서 Full+Pending 합의65536B 초과를 확인했다. 제품 charge/admission은 변경하지 않으며 [선택1개 PASS와 합 기준 FAIL](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source-main)을 구별한다. 다음은 Drain 완료와 content guard 인계 수명 분리, 이후 writer/progress·entry/return 전체 합의 공급이다.

2026-10-28 이번 한 단계는 clean `main`·`d2baeb9`에서 [Drain 완료/content guard 인계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff-main)를 분리했다. 참고 `00feccd`의 필요한 코드만 main4phase/core 회계에 적응했으며 원 pins/Installing을 private Drained로 이동한다. 정상 미완료 Drop의 fence-before-pins와 선택11개1회 PASS를 확인했고 OCR 전체18파일의 독립 D1/D2 한정 수용·지적1건 수정·필수 수정0을 반영했다. 기존 Full+Pending 합 기준 FAIL은 보존한다. 다음은 writer/progress source 예약·entry/return 전체 합과 초과 해소이며 M4/M5는 미완료다.

2026-10-28 이번 한 단계는 clean `main`·`ae042ae`에서 [CatalogCallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope-main)를 적응해 조회 결과 소비까지 원 예약/Drain 표시를 유지했다. 참고 `847def2`의 필요한 코드만 main의 core·반환 Result 회계에 맞췄다. 선택 query 수신·runtime18/compiler4 각1회 PASS와 현재 Full+Pending 합 기준 FAIL을 구별한다. OCR 전체23파일 독립 D1/D2 한정 수용·시험선택 지적1건 보완·필수 수정0을 반영했다. 다음은 Full 중첩 source 비용·진입/반환 전체 합 정리와 writer/progress 예약이며 M4/M5는 미완료다.

2026-10-28 잔여 작업 branch의 첫 단위는 `c855f1b`에서 [Catalog Commit의 terminal read gate와 writer source 창](../evidence/module-independence-survey-20261026.md#c2-writer-progress)을 공급했다. Commit은 새 read를 닫고 admitted read 반환을 기다린 뒤 stage 몫을 예약하며, Pending·publication에서는 read와 합산하고 Drain 뒤에만 install로 증가한다. 사전 독립 검토의 REQUIRED3을 실행 전에 해소했고 r1 compiler3·runtime59 각1회 PASS다. 다음 공급 의무는 나머지 MutationPort writer와 Row progress class의 ledger 참여, 진입 전·반환 뒤 floor이며 M4/M5는 미완료다.

2026-10-28 `b35150c` 이후 [실제 빈 쓰기 capsule](../contracts/dev3-module-boundaries.md#writable-empty-capsule)을 공급한다. 원 Begin actor·빈 statement·원 RollbackStart/AbortEnd를 연결했고 source별 선택 검증과 독립 D1/D2 한정 수용·필수 수정0을 확인해 이 단위를 commit한다. 자동 Status 선택과 own-actor Catalog 읽기·shared CREATE admission·일반 Row mark/rollback/commit·전체 entry/return floor·C3–C5 및 M4/M5 인수는 계속 미완료다. [이번 증거](../evidence/module-independence-survey-20261026.md#c3-write-capsule)를 재개 기준으로 사용한다.

<a id="m3-adoption"></a>
### D3-MODULE-ADOPTION-001 — 선정 경계의 소유 문서 채택

2026-10-26 · **D2 ACCEPTED / M2·M3 완료 / M4 C0 타입·C1 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급 / 공통 공급·제품 인수 미완료**. 검토 기준 `ae6fb6d`, 검토 결과 저장 commit `66ddf61`, 담당·계약·통합 `/root`, 독립 검토 `/root/module_handoff_review`. 사용자의 계획 실행 지시와 DP01의 기존 위임에 따라 [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신하고 소유 계약·JSON의 채택 상태를 반영했다. 당시 PROPOSED 중간 저장과 미해결 기록은 원 증거에 보존한다. 이번 채택은 실제 구현·compiler·runtime·native 인수 결과가 아니다.

선택은 기존 실제 owner를 보존한 C0–C5 경계를 각 정본에 수신하고 두 제품 본문을 별도 담당에게 맡기는 것이다. 기존 구조를 그대로 둔 등록/rename만으로 끝내는 대안과 전체 저장 모듈을 한 번에 재작성하는 대안은 채택하지 않는다. D3-DIRECTION-002의 **국소 개선만 허용한 작업 순서 제한**과 AI 효율 계획의 **두 번째 역할을 준비 작업으로만 제한한 부분**만 선정 범위에서 대체한다. 첫 행 목표·원자성·격리·복구/자원·DEV2 미완료·S5·호환·과거 source 증거는 유지한다.

Root3/Job3/Profile3·Row scope3·base4는 이름 변경의 부수 효과가 아니라 [Row 의미/dispatch](../contracts/dev3-module-consumers.md#format-dispatch)의 별도 D2 구체화다. 기존 [호환 정책](../policies/version-compatibility-policy.json)의 development fresh fixture와 현재-build 복구 의무를 유지하고, 과거 format의 자동 fallback/converter를 만들지 않는다. 기존 pure decoder·원 실행 결과를 보존하며 새 current dispatch/fixture/복구는 M4에서 함께 구현·검증한다.

| §6 소유 행 | 실제 반영 절 또는 미수정 근거 | 영향·검토·실행 상태 |
| --- | --- | --- |
| 병렬 정책·JSON | [계약 인계](../policies/parallel-development-policy.md#module-contract-handoff)와 관리 JSON의 module_contract_handoff | C0/C·제품 구현 중첩·첫 단독 source·MI01–06 수신; ACCEPTED·PD01–03 NOT_RUN |
| 코딩 컨벤션·JSON | 미수정. 기존 §4–5가 공유 의미와 상위 조립, 최소 visibility, API 수명/오류 완결성을 이미 요구 | 새 파일 크기/trait/crate 규칙 없음; 실제 M4 diff에 적용 |
| 저장소 배치·JSON | 미수정. 기존 engine 내부 역할·허용/실제/시험 의존 구별을 유지 | 신규 source 경로·fingerprint·export는 실제 M4 구현과 함께 등록 |
| 구현 계약·disk 저장 | [구현 계약의 모듈 전환](../contracts/implementation-contracts.md#module-transition), [disk 책임/API](../contracts/disk-storage-design.md#ownership) | C의 actual 권한/효과와 A/B 제품 소유 구분; ACCEPTED·actual 공급 미완료 |
| 세션·WAL·Commit | [세션](../contracts/dev3-storage-session.md#consumer-driven-refinement)의 한정 대체/원 소유, [WAL I/O](../contracts/dev3-wal-io.md)의 물리/의미 분리, [continuation](../contracts/dev3-commit-continuation.md)의 exact receipt/whole PinSet | 원 W/File/slot/fence·lease·Drain·epoch/Q 순서 보존; ACCEPTED |
| 해당 3.02–3.14 | [공급 설계도](../contracts/dev3-task-designs.md#dependency-map), 3.06 statement/cursor·3.09 CLR·3.10 격리·3.11 terminal·3.12 복구 및 참조/자원 owner | 기존 일반 allocation/index/checkpoint/회수 의무는 그대로. 직접 owner가 수신한 의미를 모든 task에 중복 정의하지 않음 |
| CREATE·plan4 복구 | [CREATE](../contracts/dev3-catalog-create.md)의 기존 claim/flush lease, [복구 owner](../contracts/dev3-plan4-recovery-owner.md)의 최초 census·typed Row action | 실제 Catalog/Row 상태·오류 공급; 기존 source 결과는 새 API 검증 아님 |
| DEV3 실행·공급 순서 | [한정 대체](dev3-implementation-plan.md#direction-refinement)·[A/B 배치](dev3-implementation-plan.md#parallel-execution-plan), 공급 설계도 | 두 제품 담당·공통 단일 작성·동시 구현2개 한도 보존; ACCEPTED |
| bottom-up·단계/완료 JSON | 미수정. 실제 하위 공급을 받은 통합·기능 인수, DEV2 partial·DEV3/S5 미완료와 최종 전수 예외가 이미 명확 | 내부 모듈 착수·M5를 새 단계 gate나 완료로 표시하지 않음 |
| 시험 계획·모듈 목록 | [MBC/MCA/MCB/MCI 배정](dev3-test-plan.md#module-independence-tests)과 module-test-matrix.json | 실제 diff→case, 첫 C+A/C+B, 새 합류와 원 native/S5 의무; 실행 NOT_RUN |
| DEV4 문서군 | 미수정. 기존 독립 frontend/model 착수와 실제 engine 뒤 Q1의 구분을 유지 | 아직 C의 내부 목표 API만 존재. DEV4 공개 공급/연결이 구현됐다는 변경 없음; 재개 시 실제 DEV3 공급 수신 |
| AI 절차·예시·효율 계획 | [작업 절차](../policies/ai-development-method.md#section-8), [예시](../examples/task-contract.example.json)의 C0/C/단독/합류·validation, [효율 계획](dev3-ai-throughput-improvement.md)의 제품 병행 | 예시는 PROPOSAL_EXAMPLE_NOT_EXECUTED 유지. 원 소비자 수·실측 시간·효율 결과 변경0 |
| AGENTS·진입점·색인 | 미수정. 현재 README에 구조 전환·M2/owner/인수 경로가 있고 새 두 계약은 폴더 README와 documents.json에 등록됨 | 신규 문서 ID/파일 없음. 기존 테스트·검토·push gate 유지 |
| 구현 상태 | [구현 색인](../implementation/README.md)에 C0 codec·생산자 타입과 C1 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급·전체 C0/C1–C5/M5 미완료를 표시 | `66ddf61`까지 제품 diff0. 후속 C0 실제 diff·선택 인수는 별도 기록하며 과거 실행 증거 승격0 |

별도 검토자는 최종 M2를 수용했고, M3의 module-test-matrix.json에서 누락된 MBC31을 추가한 뒤 해당 행을 한정 재확인하여 M3도 수용했다. 두 판정의 미해결 필수 수정은0이며 [검토 기록](../evidence/module-independence-survey-20261026.md#final-design-review)에 입력 identity와 범위를 보존한다. 구체 API가 actual source로 구현 불가능하거나 원 권한/격리/복구·자원을 약화하는 경우, 공통 의미 재개방이 A/B의 독립 착수를 막는 경우에 해당 행을 다시 검토한다. M4/M5의 compiler·runtime·native·source census와 MI01–06는 실제 구현 뒤의 미완료 의무다.

2026-10-28 `4ac878c` 이후 [첫 Catalog statement의 거래 완료 연결](../contracts/dev3-module-boundaries.md#catalog-capsule-completion)을 한정 수용했다. actual Forward를 등록한 원 capsule의 Commit/full abort와 변경 범위 검증·최종 독립 D1/D2를 마쳤으며, shared CREATE 제품 admission·Row/C4/C5·M4/M5 완료 목표는 축소하지 않는다. [source별 진행 증거](../evidence/module-independence-survey-20261026.md#c3-catalog-capsule)를 따른다.


2026-10-28 `9f36840` 이후 [원 statement의 shared CREATE 제품 예약](../contracts/dev3-module-boundaries.md#catalog-shared-create)을 구현·선택 검증한다. 원 reservation의 source/gate/flush·claim 보존과 private 반환을 공급하며 [source별 결과](../evidence/module-independence-survey-20261026.md#c3-create-admission)를 기록한다. source별 compiler4·신규 runtime9/source1과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 일반 Row/C4/C5·전체 floor·M4/M5는 미완료다.


2026-10-28 `830d053` 이후 [원 writable view의 Catalog own 조회](../contracts/dev3-module-boundaries.md#catalog-own-read)를 연결한다. 실제 Status head와 CREATE Undo를 검산하는 첫 CREATE 한정 공급이며 [source별 검증](../evidence/module-independence-survey-20261026.md#c3-own-catalog-read)과 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 사용자 후속 지시로 이 문제 해결 뒤 새로운 개발은 개시하지 않는다. 일반 Row/CheckedTable/C4/C5·전체 short floor·M4/M5는 미완료다.
