# DEV8 — Oracle·ARIES·로컬 소스 조사와 설계 검토

2026-10-19 · **문서/소스 열람 / 독립 D2 설계 검토 PASS / 비교 실행·제품 시험 NOT_RUN**

[설계](../contracts/dev8-operations-design.md) · [구현·검증 계획](../plans/dev8-implementation-plan.md) · [공개 소스 catalog](../reference/opensource-references.md)

<a id="scope"></a>
## 1. 조사 범위와 provenance

사용자가 DEV8 설계와 Oracle·ARIES·작업 디렉터리의 여러 DBMS 및 추가 로컬 소스 참고를 요청했다. 공개 소스 4개와 중립 식별자 `LOCAL-R01`의 관련 파일을 읽었다. 실제 코드·라이선스 문구를 복사하거나 dependency를 도입하지 않았고 source checkout을 수정하거나 실행하지 않았다. 설계상의 XADB 선택과 관측을 아래에서 구분한다. 전체 소스 감사나 다른 DBMS와의 의미/성능 동등성 증거가 아니다.

사용자의 표기 요구에 따라 추가 소스는 중립 ID·revision·파일 hash로만 이 문서에 식별한다. 원래 root/파일 경로·열람 행 범위는 저장소 밖 `$HOME/.local/state/xadb/research-archive/<inventory SHA-256>/source-inventory.json`에 보존한다. inventory SHA-256은 `82948d22aa53bd8a8862bdfcfa1f4c42bac68b3b71b2ac7558dfabe6c28dc126`이다. 이 hash는 조사 inventory이며 제품 source fingerprint나 실행 PASS가 아니다. 향후 코드 도입은 원 출처/라이선스/notice를 확인하는 DP04 별도 작업이다.

공개 checkout은 catalog의 origin과 실제 HEAD를 다시 확인했고, 사용한 5개 checkout의 tracked 변경은 0이었다. 사용 파일의 실제 bytes hash를 inventory에 기록했다. catalog의 RocksDB 경로는 현재 없었으며 조사/채택 근거로 사용하지 않았다. 나머지 catalog 항목 전체를 이번에 읽었다고 주장하지 않는다.

<a id="primary"></a>
## 2. Oracle와 ARIES 원문

| ID·원문 / 버전·열람 | 실제 읽은 범위 | XADB에 적용할 판단 |
| --- | --- | --- |
| OR-01 [Data Guard Protection Modes](https://docs.oracle.com/en/database/oracle/oracle-database/26/sbydb/oracle-data-guard-protection-modes.html), 26ai, 2026-10-19 | maximum performance/availability/protection, SYNC/AFFIRM과 NOAFFIRM의 ACK 차이와 장애 시 동작 | receive와 durable 보호를 분리. XADB strict는 자동 async 성공 강등 없이 local outcome과 protection을 분리; D8-T05–10 |
| OR-02 [RMAN DBPITR](https://docs.oracle.com/en/database/oracle/oracle-database/21/bradv/rman-performing-flashback-dbpitr.html), 21c, 2026-10-19 | §18.1.2–18.1.3의 backup 이후 redo, SCN/time target, RESETLOGS/계보 설명 | XADB target q·실제 선택 시점·새 timeline과 backup consistency end를 명시. Oracle SCN/명령/전체 flashback 구현 복제 아님; T26–28 |
| OR-03 [Bounded Recovery](https://docs.oracle.com/en/middleware/goldengate/core/19.1/reference/br.html), 19.1, 2026-10-19 | 열린 거래·부분 상태의 disk persistence, restart checkpoint와 old transaction log 의존, fallback/손상 조건 | capture spool+checkpoint로 restart 의무를 인계. XADB가 원본 WAL을 삭제했다면 불가능한 fallback을 허용하지 않음; T12/13/18 |
| OR-04 [Backing Up the Database](https://docs.oracle.com/en/database/oracle/oracle-database/26/bradv/backing-up-database.html), 26ai, 2026-10-19 검색 발췌 | §9.7 differential/cumulative, level0 parent, block change tracking의 목적 | parent와 변경 추적 최적화를 분리. XADB는 digest full-scan 정확성 경로와 누락 없는 추적을 별도 검증; T24. 전체 본문 완독 아님 |
| AR-01 [ARIES 논문 정보](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), Mohan 외, TODS17(1), 1992, DOI10.1145/128765.128770 | IBM 서지/초록과 [논문 사본](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf)의 §3 CLR/UndoNxtLSN, checkpoint·restart redo, §8 fuzzy image/media recovery | WAL-before-page·redo 이력·CLR 재장애·image copy의 redo 시작점/closure를 확인. XADB의 UndoRef·reservation·timeline/복제 정책은 별도 설계; T23/26/27 |

ARIES PDF는 웹 도구의 timeout 뒤 HTTP download로 받아 `pdftotext`로 해당 절을 읽었다. PDF SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. 논문 전체를 검증하거나 ARIES 구현을 실행하지 않았다. 원문 장문 인용 대신 책임과 선택을 요약했다.

<a id="sources"></a>
## 3. 공개 로컬 소스의 실제 관측

공개 링크는 열람한 revision에 고정했다. 아래 파일의 전부가 아닌 표시한 함수/범위를 읽었다.

| ID·소스 / 실제 HEAD | 관측 | XADB 채택/기각·시험 |
| --- | --- | --- |
| PG-01 PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851`, [syncrep.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/replication/syncrep.c), 1–47·270–347행 | write/flush/apply 대기 구분, local commit 뒤 sync wait cancel이 거래 abort가 아니라는 처리 | XADB의 typed local outcome/protection과 client 부정 사례; T05/06 |
| PG-02 같은 revision, [slot.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/replication/slot.c), 1310–1374행 | persistent slot의 WAL floor에 last-saved restart LSN을 반영 | volatile cursor만으로 GC하지 않음; T03/13/18 |
| PG-03 같은 revision, [basebackup.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/backup/basebackup.c), 645–666행 | manifest에 start/end WAL·timeline 정보를 추가하고 backup 종료 | 파일 목록과 WAL closure를 함께 검증; T23/25/28. 해당 제품 page-copy 알고리즘 전체를 채택한 것은 아님 |
| MA-01/02 MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4`, [semisync_master.h](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/semisync_master.h) 700–725, [semisync_master.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/semisync_master.cc) 730–810행 | binlog sync 뒤와 storage commit 뒤의 wait hook을 구분 | XADB는 local durable 이후 보호 대기·관측 가능성을 명시. binlog+별도 engine commit 구조를 그대로 가져오지 않음; T06 |
| CU-01 CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5`, [log_applier.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/transaction/log_applier.c), 285–307·6525–6605·6648–6755행 | processed/committed/required 위치와 apply info·commit·메모리 확인이 구별됨 | apply frontier·restart pin·자원 ledger를 분리하고 원자 apply+진행점은 XADB 계약으로 검증; T14/18/34. 관측만으로 외부 구현의 모든 실패 원자성을 인증하지 않음 |
| MY-01 MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613`, [rpl_replica.cc](https://github.com/mysql/mysql-server/blob/3b99be40f8a31d9396cad10accfd3ba51647d613/sql/rpl_replica.cc), 1530–1588행 | relay data보다 source-position metadata가 먼저 저장될 때 재시작 누락이 생기는 이유와 flush 순서 | payload durable→resume metadata→frontier 순서를 고정; T08/13/14 |

<a id="local-neutral"></a>
## 4. 추가 로컬 자료 LOCAL-R01

실제 revision은 `0682bdd8610c3728810df616c95a858cda675706`이다. 외부 archive의 원래 경로와 아래 hash를 통해 재확인한다. 이 소스의 기존 제품명·경로 inventory를 XADB 문서에 전사하지 않는다.

| 중립 파일 ID·SHA-256 | 읽은 책임·관측 | 설계 반영 |
| --- | --- | --- |
| LOCAL-R01-A `b37488224ccf8a710e62170cb2d6036fb43c3590416bd0956b8e65b88016d08a` | sender restart 계산에서 열린 거래의 최초 위치, 없을 때 commit 위치, child sender 최소 위치를 고려 | 장기 거래와 여러 consumer의 retention을 구별. durable spool 없이 읽은 위치를 곧바로 restart 위치로 삼지 않음; T12/18 |
| LOCAL-R01-B `2d09b62608e6ab74ebbaed6515c00b156eb2a328a77647365d672ba7765ac401` | sender restart 위치를 별도 상태에 위임 | sent/read/committed/restart를 운영 view에서 각각 제공; T34. 이 함수만으로 해당 상태의 영속성을 주장하지 않음 |
| LOCAL-R01-C `b339442e5bacd86bc50ab5fc1def42d6e6449a3990693d0b12253fabc6634d4d` | receiver의 commit/abort/savepoint/LOB/ACK 이벤트 분류와 commit-write-wait 설정 | logical capture/apply의 부분 rollback·LOB·remote durability·충돌 거래를 명시; T12/14/17. 기존 mode/설정을 그대로 이식하지 않음 |
| LOCAL-R01-D `0780ebe98baf2ccd0506a3fe71ebc6efada9f66bb7d5aa467a9bd13da959359e` | incremental backup의 base 존재 검사와 archive 삭제 기준 계산 | parent chain·복구 closure 검증, backup/archive owner별 보존을 채택; T24/25. 매체별 기존 결합은 디스크 전용 XADB에 가져오지 않음 |

<a id="choices"></a>
## 5. 조사에서 도출한 설계 선택

1. ACK는 위치·mode·실패 효과를 포함한다. source local commit과 원격 보호를 하나의 boolean으로 합치지 않는다.
2. 장기 거래의 undo 독립성과 capture restart의 독립성은 각각 증명한다. spool·backup·archive가 소유권을 인계하기 전 source WAL을 회수하지 않는다.
3. HA 물리 복제와 선택적 논리/M1 복제는 동일 wire payload를 가정하지 않는다. 원점·schema·제약·중복 원자성의 별도 계약을 둔다.
4. fuzzy backup은 consistency end와 redo closure를 갖는다. 이미 더 미래인 page image에 단순 replay 중지만 적용하는 PITR는 거절한다.
5. failover의 안전성은 heartbeat나 timeout이 아니라 실제 writer 차단·선택한 durable 이력·새 branch의 검증에 둔다.
6. 운영 도구·보안·성능은 30개 요구와 실제 test obligations에 배정하고 단일 HA 성공으로 전체 OPS1을 닫지 않는다.

<a id="review"></a>
## 6. 독립 검토와 실행 상태

DEV8-DESIGN-001의 담당·계약·통합은 `/root`, 독립 검토자는 별도 AI 실행 `/root/dev8_design_review`다. 2026-10-19에 설계·계획·조사 문서, DEV3/5/6/7 소유 계약, capability/stage JSON·색인을 직접 대조했다. 구현자의 결론만 전달받은 승인이 아니다. 검토자는 파일 수정·제품 시험·문서 검증기를 실행하지 않았다.

| 발견한 반례·누락 | 반영한 계약·시험 의무 | 재검토 |
| --- | --- | --- |
| H1 bootstrap/restart에 일반 restore를 사용하면 source active 거래를 undo하거나 별도 writer 계보를 만들 수 있음 | `install_standby`/`follow_redo`에서 source identity·ATT/UndoRef/예약을 보존하고 승격 전 loser undo 금지; F01/F03·T08/09/23 | 반영 확인 |
| 최종 COMMIT ACK만 복제하면 외부에 보낸 XA PREPARED vote·coordinator 결정이 failover에서 사라질 수 있음 | vote·decision·의무 해제의 durable closure 보호, 미지원 조합 사전 거절, async obligation 유실 승격 차단; F01/F03·T05/08/09/17 | 반영 확인 |
| prepared 백업과 PITR 복제본이 외부 TM/RM의 현재 의무와 충돌할 수 있음 | DEV7 prepared-backup profile 선행, 일반 PITR clone의 외부 endpoint 격리·원 identity 인계 검증; F05/F06·T23/26–28 | 반영 확인 |
| snapshot 이전 열린 거래·retired origin ledger·노드별 row version 차이로 변경 누락/오판 가능 | capture 활성화 drain·옛 WAL/spool closure, bounded ledger와 RetiredHistory, portable version mapping/tombstone; F04·T12/15/16/20 | 반영 확인 |
| page 보호만으로 WAL/undo/archive/spill/진단의 정보 보호까지 주장할 수 있음 | 매체별 protection scope와 key closure, 미지원 범위 표시; F07·T32/34 | 반영 확인 |

수정본 재검토 결론은 **독립 D2 설계 검토 PASS, 남은 필수 P1/P2 blocker 없음**이다. 검토자는 R11–R15의 30개 요구와 SC12–15·RC-T24/25·PF08/14 연결, NOT_RUN·detail 확장 의무의 보존을 확인했다. 계약/통합 담당은 이 결과를 반영하여 DEV8-DESIGN-001을 ACCEPTED로 기록했다. F01–F10의 실제 형식·provider·수치·fault inventory와 DEV3 REBASE 확장 증명은 구현 전 동결 의무로 남으며 이 설계 PASS가 이를 대체하지 않는다.

제품 구현·DBMS 비교 실행·SC12–15·RC-T24/25·PF08/14·OPS1은 NOT_RUN이다. 문서 자동 검사는 pre-push gate이고 이번 편집 중 실행하지 않는다. 이전 DEV2/COMM의 dirty 변경과 실행 증거를 이번 조사 결과로 바꾸지 않는다.

후속 [REVIEW-002](dev8-review-002.md)는 새로운 반례와 DESIGN-002 보완을 기록한다. 위 DESIGN-001의 당시 독립 검토 결과·source inventory를 소급 수정하지 않는다.
