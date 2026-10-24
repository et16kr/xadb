# DEV4 P6 — 실제 공급과 연결 준비 수합 — 2026-10-24

2026-10-24 · 공급 대조 `/root/frontend_oracle` · 통합 `/root` · **읽기 수합·독립 사실 검토 수용 / 최소 engine 공급 미충족**

DEV3 `bc93de1bc119739e99521c452b5a2e1be1775638`는 plan4 초기 page의 원 공유 버퍼 clean 소비를 제한 공급한다. 원 Mutable CAS·DWB loan·하나의 SharedPool을 유지하는 실제 clean load/교체와 빈 Catalog 조회·Free64 큐 census가 새 진전이다. **공개 disk 거래·live 정리 큐·Catalog CREATE/CLR·첫 행 ACK/reopen은 아직 DEV4에 인계되지 않았다.** P1–P4 독립 frontend 산출물은 이 차이를 대신하지 않는다.

원 draft.md (로컬 `../../artifacts/dev4-supply-handoff-20261024/draft.md`)와 draft.json (로컬 `../../artifacts/dev4-supply-handoff-20261024/draft.json`), P5 v1/v2/v3는 변경하지 않았다. 이번 동결 관측 draft-v2.json (로컬 `../../artifacts/dev4-supply-handoff-20261024/draft-v2.json`)은 실제 revision, 파일 bytes/SHA256, HEAD blob SHA256, dirty 목록, 원 영수증 연결과 아래 판단을 담는다. 관측 작성자는 제품·모델·시험·compiler·문서 verifier·commit을 실행하지 않았다. 통합 담당은 이 관측과 후속 리뷰를 문서로 수합한다. 외부 DBMS 자료를 새로 조사하지 않았다.

## 관측 기준과 보존

JSON 수집 시각은 **21:05:40 KST**이며 시작/종료 HEAD는 위 `bc93de1`로 같다. 엔진 inventory는686파일이다. 원 JSON의681파일과 비교하면 추가5·내용 변경37·삭제0이며, 이 수치는 파일 hash 대조이지686파일의 의미 검토나 시험 분모가 아니다. 같은 짧은 수집 구간에서 status와 선택 근거 파일 hash 변화는 없었다. 다른 작업자를 정지시키지 않았으므로 원자적 snapshot은 아니다.

읽기 초반 engine dirty는 `wal_io.rs`였으나 수집 시점에는36파일이다. `WalRuntime`과 `InstalledStorageCore` 분리 및 기존 소비자의 private field 경로 변경이 진행 중이며, G5 `plan4_pages*`도 포함한다. `lib.rs`의 private storage 경계와 기존 공개 거래 포트는 유지된다. 이 미commit 작업의 전체 의미·컴파일·실행을 인수하지 않았고, 원 G5 영수증을 현재 dirty 전체의 PASS로 승계하지 않는다. JSON은 현재 파일과 HEAD 파일 hash를 각각 기록한다.

원 초안의 HEAD는 `b3098c1ffe0d555c0021ceb9a4ee5ce958dd7634`다. 원 Markdown에는 engine HEAD diff가 없다고 적혀 있지만 원 JSON의 `engine_diff_from_head`에는7개 engine 경로가 있다. 원본의 관측 불일치를 그대로 보존하며, 그 문장을 v2의 clean 근거로 사용하지 않는다. JSON의 `preserved_baseline`에 두 원 파일 hash와 원 목록을 기록했다.

## DEV3의 실제 제공 범위

| 경계 | 실제 공급과 호출 경계 | 아직 필요한 인계 |
| --- | --- | --- |
| plan4 clean owner | private `MutableStorageOwner::bind_plan4_pages` → `Plan4Pages::load_into/residency_step`. 원 owner/CAS/pool과 operation114,688byte·DWB4MiB loan 유지. root/jobs/anchor/leaf의 초기8 payload page만 허용 | dirty issuer·flush·일반 page·recovered mutable owner와 공개 SQL 소비 수명 |
| Catalog | 실제 allocator+anchor+leaf3pin 검산과 leaf `lower_bound`; `Plan4CatalogLookup` 결과는 `NotFound`뿐. canonical Catalog/XCB1 historical before codec도 존재 | live snapshot metadata, 선언 byte 길이, table/index identity·schema epoch, CREATE/DROP와 Undo/CLR, DDL precommit·두 outcome |
| 정리 큐 | plan4 object4의 root1/jobs2–6 실제 설치. 원 pool의4/4/3pin 창으로64개 Free cell·revision1을 읽고 digest 검산 | enqueue/claim/terminal/aging/horizon 소유, atomic Begin 결합, dirty·WAL flush/replay, 압력하 진행 |
| 공개 거래 | 기존 `Engine`/`DurableTransaction` 존재. `Shared.core`는 `VolatileDatabase`; lookup/schema/read/scan/statement는 기존 Transaction에 위임 | 새 DEV3 disk statement token·영속 undo mark/예약·q/horizon, statement rollback 실패와 복구 owner, 정확 effect/certainty 전달 |
| row/index·첫 행 | private canonical row/version/index/tree codecs와 제한 빈 거래 Begin/RollbackStart/AbortEnd | 실제 disk bounded scan/point/range, own writes/undo 가시성, statement-final UNIQUE·partial NULL·key lock, 첫 행 durable ACK와 변경 행 reopen |
| 복구·maintenance | genesis 검사와 원 source의 닫힌 빈 거래/mixed 복구. 기존 durable snapshot checkpoint/backup/restore API도 존재 | 일반 DEV3 row/catalog commit·UNKNOWN·fence/reopen, fuzzy checkpoint·consistent backup/restore·retention의 실제 공개 공급 |
| temp·자원 | private 첫 extent reserve/cancel와 정해진 backing/loan | SQL temp namespace·quota·cleanup effect와 memory/disk/WAL/FD/rollback 예산 인계 |

근거는 [G5 계약](../contracts/dev3-plan4-clean-owner.md), [G5 구현](../../crates/engine/src/storage/wal_io/plan4_pages.rs), [실제 read 소비자](../../crates/engine/src/storage/wal_io/plan4_pages_read.rs), [공개 거래](../../crates/engine/src/database.rs)와 JSON의 `references`다. `storage`는 private이고 관련 G5 API는 `pub(crate)`다. feature `test-support`의 native 조립은 일반 SQL 소비 포트가 아니다.

기존 `Column`은 이름/type/nullable만 갖고 선언 byte 길이가 없다. 기존 `execute_statement`는 private root 복원·누적256변경 상한을 사용하고 취소가 거래 전체를 abort할 수 있다. `DurableTransaction::rollback`은 private state를 버리고 `()`를 반환한다. 이 API를 DEV4 영속 statement undo·cleanup 실패 계약에 그대로 대응시킬 수 없다. 기존 commit/maintenance가 무기능이라는 뜻이 아니라, 현재 필요한 **새 DEV3 disk owner 공급과 다른 경로**라는 구분이다.

`Plan4GenesisSelection`은 initial closure를 검사할 뿐 analysis/redo/mutable pool/READY 권한을 만들지 않는다. `retention_genesis`의 Free bytes와 Catalog before 값 역시 live 변경 capability가 아니다. 정리 큐의 현재 census 성공을 큐 실행 완료로, 빈 Catalog read를 CREATE/CLR 완료로 분류하지 않는다.

## 새 G5 원 영수증과 한계

[G5 증거](../evidence/dev3-plan4-clean-owner-20261024.md)와 독립 D1 (로컬 `../../artifacts/dev3/plan4-clean-owner-20261024/review/final-d1.md`)은 `ACCEPTED_LIMITED_D1`이다. 필수78은 memory75+static2+native1이며 memory는 원 r2의4건, r3의32건, r4의39건을 합친 고유 범위다. 현재 source 전체를 재실행한 결과로 바꾸지 않는다.

- 최종 r4 source는 `36d5645c2e5f3b5342b2e168a6acf388c35b729a415a9ee04689d4170612e7b9`, ELF는 `8db64629269cf10ff48300e3f88a3f17a02d4dcb65e095530387baf95735390e`다.
- native는 **원 r2** source `7d620404a786934b5e1c58b8db25d85b250c96223130f12d37df8785e516e664`, ELF `dd861ec37bdaade6a09ec68e418e3d3ef23d897686b83361f11ee0fa3d0eb388`, PID247411의 fresh→clean 소비1process다. G5 mutation0·entropy0이며 same-ELF3process recovery 증거가 아니다.
- r2 oracle abort, r3 empty-frame 선택 제품 FAIL과 수정 후 선택 범위를 보존한다. compiler10개 진단 수용과 원 campaign의 `HOLD_FINAL_RECEIPT_RESERVE_EXCEEDED`는 별개다. 원 preparation backing peak FAIL도 지워지지 않는다.
- S5 추가128은 작성·NOT_RUN, 소비자 완료는1/3이며 DEV3/S5는 미완료다. source-value 자원 수치는 machine stack/RSS 전체 상한이 아니다.

이번 작업은 선택한 G5 영수증·독립 D1 **10파일의 bytes/SHA 일치**를 읽기로 확인했다. 원 바이너리를 실행하거나 과거 모든 로그를 재판정하지 않았다. 이전 plan4 genesis304·Catalog before123·empty/mixed·row leaf 증거는 원 draft의 source별 연결을 보존한다. 특히 mixed 원 동일 ELF3단계 `NOT_MET`를 새 PASS로 바꾸지 않는다. 과거 genesis 단락의 “G5 미완료”는 당시 상태이며 현재 G5 clean의 제한 진전과 구분한다.

## DEV4 P1–P4 산출물 대응

| 선행 단위 | 실제 독립 산출물과 현재 기록 | 엔진 수신 뒤 소비·남은 책임 |
| --- | --- | --- |
| P1 | public Lexer/Name, streaming Splitter/StatementSource. [lexer](../evidence/dev4-frontend-20261024.md)·[splitter](../evidence/dev4-splitter-20261024.md)의 원 source 제한 D1 | prepared 입력/원문 소유와 연결. source/token 성공은 session 또는 storage 효과가 아님 |
| P2 | `Parser::parse`의 owned source/flat AST, INSERT·BEGIN/COMMIT/ROLLBACK·AUTOCOMMIT raw 구문과 marker. [P2 제한 D1](../evidence/dev4-parser-20261024.md) | whole statement binding과 실제 statement/transaction owner. COMMIT parse는 durable ACK가 아님 |
| P3 | ScalarReader decode/copy_sql/copy_native, predicate AST, 명시 ColumnSpec/hint의 PredicateCompiler·borrowed row/SQL parameter eager3VL. [scalar](../evidence/dev4-scalar-20261024.md)·[predicate](../evidence/dev4-predicate-20261024.md)·[typed](../evidence/dev4-typed-predicate-20261024.md) 제한 D1 | live Catalog metadata와 parameter/행 binding. 명시 schema fixture는 실제 Catalog lookup을 대체하지 않음 |
| P4 | raw SELECT/UPDATE/DELETE/CREATE TABLE/INDEX/DROP TABLE/INDEX, Where→Or와 문장 전체 marker 순서. [선택 검증 완료·최종 D1 대기](../evidence/dev4-remaining-parser-20261024.md), 읽은 시점의 dirty 제품 | projection/order/assignment binding, LIMIT/DDL raw 범위·길이 의미, PK/UNIQUE/index 실행, DDL precommit·실제 효과 |

SQL normal dependency는 현재 foundation뿐이며 engine을 소비하지 않는다. 따라서 이 독립 API를 마무리한 사실은 최소 disk 공급 충족이나 DEV4 Q1 완료 근거가 아니다. P5 v3는 별도 상태/action 설계 초안이며 실제 engine/session 구현 또는 실행 모델 증거가 아니다.

P4 원 source-r1 SHA는 `95a9745a6a4aa32be4ba422bd48c67918a8e8c07d13a185adb19922c08f3c50c`이며 기준 `57db415`에 DEV4 overlay만 넣어 DEV3 dirty를 제외했다. 이번 수집의 제품5파일은 원 r1 hash와 모두 같다. 원 semantic/resource receipt의 exit0을 읽었으며 semantic8함수·resource13함수의 기록이다. 고정659+boundary129=788 ID, 생성8,192를 합친 semantic parse8,980회다. known-bad50/정상 통제12는 제품 호출0이고 확대2함수는 NOT_RUN이다. 이 원 실행 범위는 현재 engine source 검증이 아니다. 기존 P2 dispatch5의 집중 검증 외 완료 suite 재실행을 요구하지 않는다.

## 후속 공급 인수의 입력

[DEV4 최소 공급](../plans/dev4-implementation-plan.md#entry)과 [진단 인계](../plans/dev4-preparation-plan.md#diagnostic-handoff-tests)를 유지한다. 전체 DEV3 S5를 별도의 추가 선행으로 만들지도, 기존 volatile API로 최소 공급을 낮추지도 않는다.

공급 owner가 공개 symbol·call graph·소유 수명·에러 effect/certainty·자원/취소 경계와 실제 source별 durability 영수증을 제시하면 독립적으로 대조한다. 특히 preentry cancel, implicit SELECT cleanup, OFF DML statement rollback, undo 실패 fence, DDL precommit 후 실패를 실제 owner와 연결해야 한다. private storage 우회, SQL UNIQUE 재구현, 한 원자 DDL/DML의 다중 commit 분할은 인계 대안이 아니다.

최소 공급 수신 뒤 binding→첫 INSERT statement→commit/reopen→작은 SELECT/fetch→실패/rollback의 **새 연결 경로**를 선택 검증한다. 관측의 결론은 **최소 engine 공급 미충족·후속 실제 공급 인수 필요**다. 원 v2는 당시 읽기 초안으로 보존하며 새 제품 PASS로 승격하지 않는다.


## 관측 이후의 통합 기록

P4는 이후 최종 독립 D1을 제한 수용하고 `70c9c5f513edc0296bb5bebcd07ef62b11a16164`로
커밋했다. 이 후속 상태는 위21:05:40 관측의 D1 대기 원문·원 source 결과를 변경하지 않는다.
이 커밋은 engine 제품을 변경하지 않았다. P5 수명 모델은 설계 후보 단계이며 모델 구현·탐색은
NOT_RUN이다. P6는 이 시점의 공급/API·잔여를 수합했으며 실제 engine 공급을 인수한 것은 아니다.

최종 독립 사실 검토는21:14:50 KST 이 수합을 수용했다. 차단 사실 오류는0건이며 새 engine
공급 인수는 아니다. 검토자는 원 G5 영수증10파일과 연결 metadata30파일의 해시, source별75개
고유 memory PASS ID 및 static2/native1의 범위를 읽기 대조했다. 제품·시험·compile·문서 verifier
실행은0이다. 원문과 해시는 [수합 색인](dev4-supply-handoff-20261024.json)에 기록한다.

검토 당시 후속 engine dirty는37개였고 원 관측 뒤 allocation_reservation.rs, empty_transactions.rs,
mutable_storage.rs가 바뀌었다. 이 차이도 별도 시각으로 보존하며 원686파일/36dirty 관측이나
과거 제품 PASS를 현재 전체의 검증으로 바꾸지 않는다. 실제 공급이나 공개 API가 바뀌면 그 새
revision·변경 경로·원 증거를 다시 대조한다. 이 문서 자체는 자동 갱신 또는 시험 재실행 지시가 아니다.
