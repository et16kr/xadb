# DEV4 — DEV3 병행 준비·독립 모듈 개발 계획

2026-10-24 · **D4-PREP-001/002 ACCEPTED / 독립 lexer·이름 제한 구현 / 전체 DEV4·Q1 미완료**

[독립 앞단 계약](../contracts/dev4-independent-frontend.md) · [전체 DEV4 계획](dev4-implementation-plan.md) · [기존 18개 작업](dev4-work-breakdown.md) · [조사·검토](../evidence/dev4-preparation-research-20261024.md)

<a id="entry"></a>
## 1. 목표와 시작 조건

DEV3가 저장·복구를 개발하는 동안 DEV4의 독립 입력 처리·값 의미·검증 도구를 먼저 완성할 수 있도록 한다. 최초 요청에서는 이 계획과 계약만 확정했다. 후속 사용자의 선행 개발 요청으로 [첫 lexer·이름 구현](../implementation/dev4-implementation.md)을 진행했다. DEV3 작업은 별도 소유이며 이후에도 아래 순서를 적용한다.

독립 모듈은 D4-PREP-001의 허용 범위·독립 D2 채택과 해당 leaf 입력 동결 뒤 시작할 수 있다. DEV3 전체 완료나 존재하지 않는 scan/catalog API를 기다리지 않는다. 엔진을 소비하는 모듈은 [기존 entry](dev4-implementation-plan.md#entry)의 실제 DEV3 최소 공급 증거를 요구한다. Q1과 단계 DAG는 유지하며 준비 작업을 새 stage/gate·XADB task로 등록하지 않는다.

<a id="sequence"></a>
## 2. 배정과 순서

아래 P0–P6은 이 계획 안의 배정 묶음이며 DEV2 P0/K0나 새 제품 단계가 아니다. 기존 4.xx ID가 범위의 정체성이다. 최초 확정 당시 모든 상태는 **NOT_RUN**이었다. 현재 P0의 lexer 소비 부분과 P1 lexer/이름·해당 P5 literal 판정을 제한 구현했으며 [source별 증거](../evidence/dev4-frontend-20261024.md)에 실제 소유·검증·잔여를 기록한다. 나머지 상태는 NOT_RUN이며 담당 구현자·실제 소유 경로는 해당 개발 시작 때 배정한다. 본 계획·계약 담당은 `/root`, 독립 검토자는 조사 기록에 남긴다.

| 묶음·기존 ID | 직접 선행 | 산출물·독립 종료 조건 | 병행 가능 범위 |
| --- | --- | --- | --- |
| P0 입력·자원 동결: 4.01.2/3 일부, 4.14.1.1·4.14.2 일부, 4.13.1.2 일부 | 채택 계약·foundation 실제 API | 아래 F01 앞단 입출력과 allocation/취소 profile, lex/parse 진단·독립 fixture schema. 새 코드가 생기면 해당 자원·오류 시험. session/engine adapter를 만들지 않음 | DEV3와 독립; oracle 입력 설계와 병행 |
| P1 lexer·이름·문장 경계: 4.02.1–3 | P0의 해당 동결 | UTF-8/token/span·이름·stream chunk 경계 구현과 T01의 관련 사례. 첫 종료점은 lexer/이름, 그다음 streaming splitter | production 담당과 별도 literal 담당 가능 |
| P2 공통 parser·첫 구문: 4.03.1.1, 4.03.2.1, 관련 4.03.3 | P1·AST/marker 계약 | INSERT·거래 제어 AST, 잘린 입력/후행 token·한도 진단. 실제 INSERT/COMMIT 성공으로 기록하지 않음 | P3의 scalar 입력, 독립 oracle |
| P3 값·predicate: 4.04.1–3, 4.03.1.2 | scalar는 P1, predicate는 P2 공통 표현과 scalar | typed NULL·원래 타입·byte 길이·3값 논리·전체 타입 검사. actual catalog 없이 명시 타입 fixture로 검증 | P4의 독립 구문 추가 |
| P4 나머지 parser: 4.03.1.3, 4.03.2.2/3, 관련 4.03.3 | P2, WHERE 문장은 P3 predicate AST | SELECT/UPDATE/DELETE/네 DDL AST·진단. DDL type_length 원문·precommit 이후 검사 경계 보존 | 각 구문별 파일/시험 소유를 나눌 수 있음 |
| P5 독립 판정·모델: 4.16.1.1/.2, 4.16.3.1 일부/.2 | literal은 각 의미 계약, 모델은 F02 상태/action·해당 F04 동결 | 각 leaf와 함께 literal·부정 판정 공급. 수명 모델은 별도 종료점으로 정상 공간 완주·known-bad 거절 | parser 전체를 기다리지 않음; 모델 전체 완료를 P1의 선행으로 두지 않음 |
| P6 공급 수합·실제 연결 준비: 4.01.1, 각 leaf의 증거 수합 | 해당 P1–P5 결과, 최신 DEV3 공급 기록 | 실제 구현/미구현·원 source receipt·잔여 T/V 목록·API 차이표. engine 연결 착수 전 기존 DEV3 선행 확인 | 설계·수신 목록은 처음부터 갱신; 실제 binding/실행은 별도 |

첫 개발 묶음은 **P0의 lexer 관련 입력 + P1 lexer/이름 + 독립 literal/오류 판정**으로 제한한다. 전체 F02 상태 모델·F03 spill·F05 PF10을 처음 lexer의 장벽으로 삼지 않는다. 다음은 공통 parser와 INSERT/거래 제어 구문, scalar/조건식, 나머지 구문 순이다. 18개 분류 수를 동일 공수로 계산하거나 준비율을 DEV4 기능 완료율로 바꾸지 않는다.

<a id="freeze"></a>
## 3. 이번 확정과 구현 전 실제 입력

| 기존 항목 | 이번에 확정한 것 | 해당 leaf 구현 전에 남는 입력 |
| --- | --- | --- |
| F01 앞단 | 허용 문법 정본·Oracle 차이, token/AST/값의 책임, source/span/typed NULL·DDL 길이 보존·진단·한도 | 실제 Rust signature·token/AST variant·readonly accessor, capacity 산식·arena layout, whitespace/LF/CR/CRLF·line comment·오류 우선순위, error/span literal 전수 목록·fixture hash. 첫 leaf에서는 소비 부분만 동결 |
| F02 | 실제 session/engine API는 선행하지 않는 추상 모델 범위, 앞단은 session outcome을 생성하지 않음. [진단과 호출 효과의 책임](../contracts/dev4-independent-frontend.md#diagnostic-effects) 구분 | 모델 action/parameter 동치류·상태 전이 표·탐색 상태/전이 상한. 실제 binding/handle/transaction API와 phase·cleanup·outcome 조립은 DEV3 공급 뒤 |
| F03 | spill 구현은 이번 독립 제품 범위 밖 | DEV3 sorter 재사용 검토·실제 temp port·codec/manifest·cleanup·FD/disk 산식은 후속 |
| F04 앞단/모델 | 아래 선택 가능한 필수 사례와 독립성·부정 판정 의무 | case별 실제 입력/기대값·명령·mutation recipe·hash, source/runner identity. 모델은 탐색 recipe·완주 상한 먼저 |
| F05 | 기존 SQL 입력/메모리 한도와 앞단 측정 항목 유지 | 앞단 cancel 검사 최대 간격·할당 실패 지점, 측정할 때 host/build/표본 조건. 실제 SQL/native PF10·수치 회귀 예산은 후속 |

계약 동결과 실행 입력 동결을 구분한다. 이 계획에는 실행하지 않은 명령·fixture hash·구현자 이름을 만들어 넣지 않는다. 각 leaf의 새 D2 결정은 독립 검토를 받고, 확정 계약 안의 구현은 D1 독립 검토로 통합한다. 실제 앞단 공통 타입을 변경하면 관련 소비 계약·시험도 함께 조정한다.

<a id="tests"></a>
## 4. 구현과 함께 작성할 선택 시험

다음 FE01–FE16은 독립 앞단 범위의 **필수 case family**이며 실행 건수·완전한 Q1 inventory가 아니다. 실제 세부 case ID를 구현 전에 펼치고 고정한다. family의 일부 PASS를 전체 PASS로 수합하지 않는다. 아래 한도 `L−1/L/L+1`은 다른 한도/메모리가 먼저 거절하지 않도록 fixture를 설계하고, 구조상 불가능한 조합은 근거를 기록한다.

| ID | 담당·원래 인수 | 필수 입력·판정 |
| --- | --- | --- |
| FE01 | P1 / T01 | 모든 지원 token·punctuation, 대소문자·멀티 byte span·EOF·첫 잘못된 UTF-8; comment/quote 안 marker·semicolon 제외 |
| FE02 | P1 / T01,V18 | 미인용/인용 이름·PUBLIC 접두어, 127/128/129 bytes·빈 이름·NUL/C0/C1, `t/T/"T"` 동일·`"t"` 구별·`""` escape. Oracle 차이 목록과 대조 |
| FE03 | P1 / T01,T12 | 단일 API 다중 문장 거절, 빈 API와 빈 batch 구별. quote/comment/UTF-8/escape 경계마다 chunk 분할, 완결/미완결 EOF. splitter 결과를 parse 결과와 별도로 기록 |
| FE04 | P1–P4 / T01,T11 | 원문/token/AST/depth/marker 각 L−1/L/L+1, 긴 NOT·괄호·AND/OR·목록. 실패 cleanup·bounded stack/Drop, comment도 원문 한도 소비 |
| FE05 | P2 / T01 | INSERT tuple/열·BEGIN/COMMIT/ROLLBACK/SET AUTOCOMMIT의 literal AST, marker 순서·금지 위치·잘림·후행 token |
| FE06 | P3 / T01,T02 | NOT/AND/OR 우선순위·괄호·비교·IS NULL의 독립 AST. `!=`·산술·함수·subquery 등 미지원과 일반 Syntax 구별 |
| FE07 | P4 / T01 | SELECT/UPDATE/DELETE/CREATE·DROP TABLE/INDEX 지원 grammar production마다 정상·오류. SELECT clause·projection/order 항목 한도·alias/ordinal 미지원 |
| FE08 | P4 / T01,T03,V21/22 | `VARCHAR(0)`, `VARCHAR(-1)`, i64보다 긴 decimal 길이는 AST 보존. 잘못된 digits/미지원 타입은 진입 거절. **실제 precommit·두 outcome 시험은 NOT_RUN으로 남김** |
| FE09 | P3 / T02,V19/20 | Int64 min/max와 양쪽 한계 밖, empty text typed NULL·native empty text·empty binary·공백·U+0000, 타입 불일치·nullable·65,535/65,536/65,537 bytes |
| FE10 | P3 / T02 | NOT3·AND9·OR9의 독립 표, 6개 비교 연산×지원 타입·NULL 조합, IS NULL/NOT NULL·WHERE T만 통과. NULL/단락 평가 전 타입 불일치 거절. `'a' = 'a '`는 FALSE, `'a' < 'a '`는 TRUE를 literal/literal·명시 타입 row·parameter 조합의 byte 비교로 판정; Oracle literal의 padding 기대값과 혼용 금지 |
| FE11 | P0–P4 / T11,V28 일부 | 각 allocation 실패·중간 취소/deadline·실패 뒤 새 호출 성공, 호출별 제어 수명·긴 token/평가 loop 안 checkpoint. 반환 진단에 추가 allocation·거래 상태/outcome 조립 없음. 아래 진입 전/실행 중 연결 사례를 후속 owner에 인계 |
| FE12 | P0–P4 / T11,V28 및 합산 자원 | 두 이상 caller의 같은 DB quota, session/DB 잔여 bytes−1/동일/+1, old/new AST 공존·뒤 예약 실패·잔여 charge. 성공/오류/drop 뒤 원래 사용량으로 회복; retained 결과는 계속 charge |
| FE13 | P5 / T01,T02,T13 | 잘못된 token/span/AST/typed NULL/3값 결과를 literal 판정기가 거절. bag 중복·순서·incomplete 결과 거절은 해당 판정 구현에 연결. production 함수 공유 금지 |
| FE14 | P5 / T08,T09,T11,T13 | F02 동결 뒤 2 session·prepared 각2·활성 result 각1·길이6 이내 상태 모델. stale generation 수락·늦은 cancel 전파·close=commit·undo 미완료→Idle·implicit fetch terminal 취소→거래/result 유지 mutant 거절. 상한 도달 INCONCLUSIVE |
| FE15 | P1–P4 / T01 | 해당 변경 parser/lexer의 malformed/fuzz corpus·seed·길이/깊이/메모리 상한·유효 입력 보존·panic/무한 loop 금지. 최소/확대 profile을 별도 선택 가능하게 작성 |
| FE16 | P6 / T13,T14 일부 | 실제 dependency/source 경계·원 receipt/fixture/runner identity와 coverage 수합. parse 시간·할당/peak·cancel 검사 간격 관측은 새 경로에 연결. Q1/PF10·내구성 PASS로 확대 금지 |

FE14의 탐색기·모델은 제품 상태 전이 함수를 정답으로 쓰지 않는다. 실제 I/O·thread·engine ACK의 인수를 모델 전수 결과로 대체하지 않는다. FE08의 parser 성공은 DDL 성공이 아니며 FE05의 COMMIT AST는 commit receipt가 아니다.

<a id="diagnostic-handoff-tests"></a>
FE11의 독립 단계에서는 순수 helper의 취소/실패·예약 반환·진단만 검증한다. 아래는 **실제 DEV3 공급 뒤 작성·실행할 연결 시나리오**이며 현재 전부 NOT_RUN이다. 가짜 session/engine으로 종료 증거를 만들지 않는다. 각 owner는 F02/F04에서 세부 case·오류 관측 지점·cleanup witness를 동결하고 해당 연결 코드가 생길 때 선택 실행한다.

| 연결 시나리오 | 담당 기존 작업·인수 | 필수 관측 |
| --- | --- | --- |
| 거래 진입 전 parse/값 입력의 CANCELLED/TIMEOUT | 4.13.1/4.14.2 · T11 | lex/parse/bind phase·NoEffect, 기존 Idle/Active·binding 유지. 실제 거래 진입 뒤 bind 오류와 구분 |
| ON SELECT의 fetch predicate 평가 중 CANCELLED/TIMEOUT | 4.12/4.13/4.14.2 · T09/T11 | fetch phase·cursor/snapshot 해제·implicit 거래 종료. helper의 무효과를 호출 전체 상태 유지로 포장하는 mutant 거절 |
| 명시/OFF DML의 평가 중 취소·값 오류 | 4.09/4.13/4.14.2 · T09/T11 | execute phase·이번 statement 복원·이전 성공 변경 보존·Active. 같은 호출에서 처음 시작한 OFF 거래 포함 |
| 위 실행의 undo/정리 실패 | 4.13.3/4.17 · T11 | 실제 cleanup 실패에 따른 RecoveryRequired/Fenced·원 certainty 보존. helper 오류만 보고 정상 rollback/Idle 응답 금지 |
| DDL precommit 성공 뒤 native DDL 호출 전 Type/Range 오류 | 4.10.2/4.10.3·4.13.1.2 · T03/T09 | prior=Committed·ddl=NotAttempted·Idle과 실제 phase 보존. native DDL을 실행한 Aborted 또는 원래 Active·NoEffect로 바꾸지 않음 |

테스트 실행은 [사용자 지시](../../AGENTS.md)를 따른다. **최초 계획 확정 때는 테스트를 실행하지 않았다.** 후속 개발에서는 새 코드와 수정 코드의 필수 사례를 선택하고 마지막 검증 이후 diff→case 대응을 실행 전에 기록한다. 완료된 무변경 사례·전체 workspace·하위 wrapper 반복은 금지한다. 긴 fuzz·큰 조합은 필수 profile을 줄이지 않고 관련 코드 변경과 함께 선택·계획하며 미실행/중단/FAIL을 보존한다. DEV4에 새 최종 전수 예외를 만들지 않는다. 문서 자동 검사는 push gate 전용이다.

<a id="coordination"></a>
## 5. 파일 소유·DEV3 작업 보호

실제 구현 때 기존 `crates/sql` 계획 안에서 lexer/name/splitter, parser/AST, scalar/predicate, frontend diagnostic/budget 책임을 나눈다. 이는 배치 후보이며 지금 빈 경로나 build 등록을 추가하지 않는다. semantic oracle는 기존 `crates/testing/reference` 책임에 두고 생산 의미 계산과 분리한다. 테스트 단순 입력/관측 schema를 protocol에 추가할 필요는 실제 소비자가 있을 때 결정한다.

DEV3의 `crates/engine`과 저장 계약은 이번 독립 구현의 수정 범위에서 제외한다. 앞단이 요구하는 foundation 기능이 부족하면 범위를 기록하고 최소 공급 변경을 별도 검토한다. DEV3 작업을 막거나 미완료 저장 API를 추정해 foundation으로 올리지 않는다.

공유 Cargo/lockfile·모듈 연결·문서 JSON은 통합 담당 한 명이 최신 내용을 읽고 순차 반영한다. 별도 작업 트리도 의미 중복은 확인한다. source/export 등록은 실제 코드와 함께 하고 기존 실행 증거·dirty 변경을 덮어쓰지 않는다. 각 실제 작업 기록에는 기준 revision/dirty digest·독점/공유 파일·검토자·소비 산출물·선택 시험을 남긴다.

<a id="acceptance"></a>
## 6. 종료와 다음 연결

독립 leaf 종료에는 코드·필수 선택 시험·부정 판정·자원/오류 증거와 별도 D1 검토가 필요하다. 원 source/범위를 보존하고 부모 분류의 미구현·미실행 부분을 남긴다. 본 계획 채택만으로 어떤 leaf도 완료하지 않는다.

DEV3 수신 후 4.01.1이 실제 catalog/DDL·row/index·token/undo·commit/recovery·temp 공급을 대조한다. 기존 DEV4 최소 선행을 충족한 뒤 binding → 첫 INSERT statement → commit/reopen → 작은 SELECT/fetch → 실패/rollback 순으로 연결하며 [기존 공급 순서](dev4-work-breakdown.md#sequence)를 따른다. 실제 adapter의 phase·cleanup·outcome 책임과 [진단 인계 사례](#diagnostic-handoff-tests)를 해당 owner에 연결한다. 이때 변경된 연결 경로의 시험을 수행하고 앞단 기존 PASS를 새 실행으로 재계산하지 않는다.

Q1은 기존 SC02·KU13·PF10와 D4-T01–14의 실제 저장 통합·오류/복구·prepared·CLI 인수를 모두 요구한다. 앞단 일부 완료를 Q1·DEV4 완료로 승격하지 않는다. 기간·속도 향상은 아직 측정하지 않았으며 첫 leaf부터 구현/검토/시험/재작업 시간을 구분해 이후 배정에 사용한다.
