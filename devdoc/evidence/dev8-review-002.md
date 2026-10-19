# DEV8 — 운영 설계 2차 검토

2026-10-19 · **DEV8-DESIGN-002 / D2 / ACCEPTED·독립 수정본 검토 PASS / 제품 구현·시험·OPS1 NOT_RUN**

[설계 정본](../contracts/dev8-operations-design.md) · [필수 반례](../plans/dev8-implementation-plan.md#review-002-variants) · [최초 조사·검토](dev8-source-research.md)

<a id="scope"></a>
## 1. 범위와 판단 기준

사용자의 DEV8 재검토·문제 즉시 수정 요청에 따라 Oracle·ARIES와 로컬 PostgreSQL·MariaDB·CUBRID·MySQL·LOCAL-R01의 관련 부분을 직접 읽었다. 담당/계약/통합은 `/root`, 독립 검토는 별도 AI 실행 `/root/dev8_independent_review`다. 독립 검토자는 DEV8 원문과 DEV3/DEV7 소유 계약에서 출발해 반례를 찾았다. 이미 DESIGN-001이 해결한 standby의 조기 undo, protected XA, snapshot 이전 열린 거래, PITR clone의 외부 endpoint 격리는 다시 발견한 문제로 세지 않는다.

이 기록은 문서·소스 정적 검토다. 외부 DBMS 실행, production 구현·fault 검증·성능 비교는 수행하지 않았다. 기존 작업 트리의 제품 변경·실행 증거와 DESIGN-001의 당시 검토 결과를 이번 결과로 바꾸지 않는다. 미래 signature/codec/provider의 동결 의무도 그대로 남는다.

<a id="findings"></a>
## 2. 발견한 문제와 반영

행 번호는 검토 시작 시 DESIGN-001 문서 기준이다. 현재 계약은 안정 anchor로 연결한다.

| ID·중요도·원래 위치 | 반례와 영향 | 수정·필수 인수 |
| --- | --- | --- |
| R2-01 · P1 · §6 104행, §7 119행 | source 거래가 T1/T2를 바꾸고 두 disjoint 구독이 각각 받으면, OriginCommit만 사용하는 ledger는 두 번째 projection을 중복으로 버리거나 다른 digest로 오인한다 | [capture](../contracts/dev8-operations-design.md#capture): ApplyDomainId/projection/OriginCommit key, 원본·projection digest 구분, 여러 경로의 동일 효과 중복 제거, 겹치는 domain 거절, snapshot 기반 세대 인계. F02/03/04·V01 |
| R2-02 · P1 · §7 121행 | local sequence 예약 뒤 NEXTVAL을 반환하고 standby 도착 전 primary 상실. COMMIT 없는 SELECT·rollback된 거래에서는 COMMIT 보호가 발급값의 재사용을 막지 못한다 | [동기화](../contracts/dev8-operations-design.md#synchronization): 값 공개 전 범위/epoch 원격 보호, 실패 범위 반환 금지, async 거래와 발급 보호 분리, M1 발급 소유/분리 범위, 승격 후 발급 admission. F01/03·V02 |
| R2-03 · P2 · §7 119행 | P0만 구독하는데 UPDATE가 P0→P1이면 stale 행이 남고, P1→P0이면 target에 없는 행을 UPDATE한다 | [동기화](../contracts/dev8-operations-design.md#synchronization): 전후 membership 네 조합, 진입 시 완전 image, 이탈 시 before-version, 동일 거래 이동/rollback 원자성. F01/04·V03 |
| R2-04 · P2 · §2 34행, §5 73행, §10–11 | seed 뒤 file 생성/확장 WAL은 있어도 해당 FILEMAP/길이/control 세대가 없으면 replay 불가. 나중 donor control이나 같은 q의 더 늦은 fuzzy backup을 쓰면 target cut 이후 상태가 섞인다 | [정체성](../contracts/dev8-operations-design.md#identity)·[PITR](../contracts/dev8-operations-design.md#pitr): prefix별 인증된 dependency와 namespace 설치 후 ACK, target-owned control, archive에 history 보존, 정확한 LSN cut 비교. F01/02/05/06·V04 |

R2-04는 기존 F01이 FILEMAP/control 형식을 이미 소유함을 인정하고 **cut별 선택과 전달 불변식**을 보완한 것이다. namespace 의무가 전혀 없었다고 주장하지 않는다. 승격 뒤 새 peer 없이 strict write-ready가 될 수 없다는 설명도 추가했지만, 이는 기존 지정 peer ACK 조건에서 이미 도출되므로 별도 blocker로 세지 않는다.

<a id="primary"></a>
## 3. Oracle·ARIES 대조

모든 원문은 2026-10-19 열람했다. 관측과 XADB의 선택을 구별하며 문장·코드의 장문 전사는 하지 않는다.

| 자료·열람 범위 | 관측과 적용 |
| --- | --- |
| [Oracle Data Guard Protection Modes](https://docs.oracle.com/en/database/oracle/oracle-database/26/sbydb/oracle-data-guard-protection-modes.html), 26ai, SYNC/AFFIRM·NOAFFIRM·보호 mode | 수신과 disk 보호·가용성 우선 전환의 보장이 다르다. XADB의 strict ACK·명시 보호 실패를 유지하고, sequence의 외부 공개 경계는 DEV7 약속에서 별도로 도출했다. Oracle 전체 의미를 복제한 것이 아니다 |
| [Oracle DBPITR](https://docs.oracle.com/en/database/oracle/oracle-database/26/bradv/rman-performing-flashback-dbpitr.html), 26ai, §17.1.2–17.1.3·17.8.1 | backup과 endpoint 사이 redo 적용 및 계보를 구별한다. XADB는 q를 실제 WAL 끝으로 매핑하고 target 뒤 control/image를 배제한다 |
| [Oracle Backup](https://docs.oracle.com/en/database/oracle/oracle-database/26/bradv/backing-up-database.html), 26ai, §9.3.1·9.7.5–9.7.6 | backup 끝까지 로그를 갖추는 이유, 새 datafile base, change tracking의 보존 범위를 확인했다. XADB의 full-scan fallback과 FILEMAP/INIT·archive closure를 유지한다 |
| [ARIES 서지](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), Mohan 외, TODS17(1), 1992; [논문](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf) §8, 인쇄 131–133쪽 | fuzzy image에 미완료 변경이 있을 수 있고 checkpoint 기반 media redo 시작점·pageLSN·redo 후 undo가 필요하다. XADB의 독립 UndoRef·control inventory·고정 예약은 자체 계약이며 논문에서 그대로 제공되는 기능이 아니다 |

ARIES PDF를 HTTP로 받아 텍스트로 §8을 읽었다. SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. 도구에서 열지 못한 URL을 읽었다고 표시하지 않는다.

<a id="sources"></a>
## 4. 실제 로컬 소스와 provenance

사용한 5개 checkout의 실제 HEAD와 tracked 변경 0을 확인했다. 아래는 선택한 함수/행 관측이며 전체 소스 감사가 아니다. 원래 경로·revision·파일 hash·열람 범위는 저장소 밖 `$HOME/.local/state/xadb/research-archive/<inventory SHA-256>/source-inventory.json`에 보존한다. 이번 inventory SHA-256은 `33655f7ca5af2bb5df2b498d9d1c6ccda30902c7174e39975376ce4383dab12a`다. 사용자 표기 요청에 따라 추가 소스는 중립 ID로만 기록한다. 소스 checkout을 수정하거나 코드를 도입하지 않았다.

| 소스·실제 revision | 읽은 부분·관측 | XADB 판단 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [pgoutput.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/replication/pgoutput/pgoutput.c) 1253–1301: UPDATE 전후 membership 네 조합과 INSERT/DELETE 변환 | R2-03의 partition projection 반례를 대조. 일반 predicate filter 기능 전체를 새 범위로 채택하지 않음 |
| 같은 PostgreSQL revision | [subscriptioncmds.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/commands/subscriptioncmds.c) 954–965: subscription 정체성으로 replication origin 생성 | 진행점 scope를 source ID 하나와 동일시하지 않음. XADB domain/M1 중복 의미는 자체 설계 |
| 같은 PostgreSQL revision | [sequence.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/commands/sequence.c) 800–867: sequence WAL 선행 범위와 거래 commit의 flush/syncrep 연결 | 외부에 반환하는 시점과 거래 종료 시점을 분리해야 함. XADB의 값 발급 전 보호 약속과 동일하다고 해석하지 않음 |
| 같은 PostgreSQL revision | [xlogrecovery.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/access/transam/xlogrecovery.c) 2761–2845의 LSN/transaction target; [slot.c](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/backend/replication/slot.c) 1310–1374의 last-saved restart floor | 정확한 정지 경계와 durable 재개 지점을 별개로 검증 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [semisync_master.cc](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/sql/semisync_master.cc) 745–797: after binlog sync / after storage commit 대기 hook | local 가시성·원격 보호 대기의 차이를 유지. 이 경로만으로 nontransactional 발급 보호를 추정하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [log_applier.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/transaction/log_applier.c) 6562–6605: required LSA, apply flush, 적용 위치 갱신, commit 순서 | 재시작 위치·적용 효과·관측 위치를 구별. XADB 원자 ledger는 독립 시험 의무 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | [rpl_replica.cc](https://github.com/mysql/mysql-server/blob/3b99be40f8a31d9396cad10accfd3ba51647d613/sql/rpl_replica.cc) 1530–1588: relay log보다 resume metadata가 앞설 때 생기는 gap 반례 | control/dependency·payload의 실제 영속화보다 ACK/frontier를 앞세우지 않음 |

LOCAL-R01 revision은 `0682bdd8610c3728810df616c95a858cda675706`이다. [이전 inventory](dev8-source-research.md#local-neutral)의 A–D와 동일 bytes를 재확인하고 다음 부분을 다시 읽었다.

| 중립 ID·SHA-256 | 관측과 선택 |
| --- | --- |
| LOCAL-R01-A `b37488224ccf8a710e62170cb2d6036fb43c3590416bd0956b8e65b88016d08a` | 열린 거래·child sender의 최소 restart 위치. domain/전달 경로별 의무를 합의하기 전 GC하지 않는 근거 |
| LOCAL-R01-B `2d09b62608e6ab74ebbaed6515c00b156eb2a328a77647365d672ba7765ac401` | sender restart/processed 위치의 분리. setter 호출만으로 영속 인계를 증명하지 않음 |
| LOCAL-R01-C `b339442e5bacd86bc50ab5fc1def42d6e6449a3990693d0b12253fabc6634d4d` | receiver의 commit/abort/savepoint/LOB 분류, snapshot 중 update 제약, 적용 commit sync 설정. projection과 공개/내구성의 별도 계약 필요 |
| LOCAL-R01-D `0780ebe98baf2ccd0506a3fe71ebc6efada9f66bb7d5aa467a9bd13da959359e` | 증분 base 존재와 archive 삭제 시작점 계산. XADB의 disk-only backup DAG·복구 closure 의무를 유지하며 원본의 저장 매체 결합을 가져오지 않음 |

<a id="review"></a>
## 5. 독립 검토와 실행 상태

독립 검토자는 R2-02/03을 원문·소스에서 도출했다. `/root`가 제시한 R2-01/04는 독립 검토자가 원래 계약과 반례를 확인했으며, R2-04의 기존 F01 의무와 새 cut별 불변식을 구분했다. 독립 검토자의 외부 소스 직접 열람은 PostgreSQL pgoutput.c이며 Oracle·ARIES·나머지 소스 관측은 `/root`의 조사다. `/root`가 설계·계획·완료 계약과 색인을 수정했다.

수정본 재검토에서 이전 projection 세대의 queued payload가 새 target에 도달하는 추가 P2 경계를 지적했다. 영속 선택된 세대만 apply, old apply drain/fence, 늦은 payload의 RetiredHistory, domain identity 재사용 금지·tombstone 복원을 본문과 V01에 반영했다. 최초 4개 지적과 같은 R2-01의 세대 인계 보완이며 별도 제품 실행을 뜻하지 않는다.

독립 검토자는 M1 교체 시 donor snapshot 이후 target local commit이 유실되는 반례도 확인했다. M1의 모든 affected writer drain/fence·원점별 cut과 미전달 효과 수집·canonical 상태 독립 비교를 본문과 V01에 추가했다. L1의 snapshot 교체를 M1에 그대로 적용하지 않는다.

최종 독립 재검토 결론은 **PASS — 남은 필수 P1/P2 지적 없음**이다. 검토자는 실제 수정본의 domain/세대·sequence 범위 소모·partition membership·control history·PITR LSN과 V01–V04의 완료 조건 연결을 확인했다. 설계·계획·완료 계약·색인은 작업 전 사본과 대조했고, `development-stages.json`은 작업 전 사본이 없어 현재 DEV8 참조·NOT_RUN 상태만 확인했다. 검토자는 파일 수정·문서 검증기·제품 시험을 실행하지 않았다. 계약/통합 담당 `/root`가 이 결과를 반영하여 DESIGN-002를 ACCEPTED로 기록했다.

D8-R2-V01–V04는 필수 시험 **명세**이며 실행 결과가 아니다. F01–F10은 OPEN_NOT_IMPLEMENTED, SC12–15·RC-T24/25·PF08/14·OPS1은 NOT_RUN이다. 문서 자동 검사는 저장소의 pre-push gate에서 수행하므로 이번 편집 중 실행하지 않았다. 제품 코드 변경·제품 시험·commit/push도 수행하지 않았다.
