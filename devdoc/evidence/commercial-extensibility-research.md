# XADB — 상용 수준 확장 한계 조사와 선제 보완

2026-10-19 · **소스 열람·D2 설계 보완 ACCEPTED·독립 검토 완료 / 제품·모델·성능 실행 NOT_RUN**

[확장 가능성 평가](../reference/commercial-dbms-extensibility-assessment.md) · [저장 소유 계약](../contracts/disk-storage-design.md#extensibility-review) · [SQL 소유 계약](../contracts/dev4-sql-design.md#extensibility-review) · [개발 방침](../policies/development-policy.md#section-2)

사용자가 확장 가능성 평가의 한계 점검과 사전 보완, Oracle 및 로컬 DBMS 비교를 요청했다. 구현·설정 상한을 일괄 높이기보다, 아직 구현되지 않은 DEV3/4의 복구·진행·자원 경계와 인수 사례를 보완한다. 기존 DEV2/COMM/성능 작업 트리의 제품 코드는 수정하지 않는다. 외부 소스는 설계 연구 입력이며 코드·시험 정답·Git 이력을 도입하지 않는다. 요청한 추가 로컬 자료는 `SRC-L04`로 식별하며 원래 제품명·절대 경로·remote URL을 이 기록에 포함하지 않는다. 디스크 undo·flush·임시 정렬 책임만 참고했다.

## 1. 기준과 작업 소유

| 항목 | 기록 |
| --- | --- |
| 작업/계약/통합 | `/root`; 독립 검토 `/root/extensibility_review` |
| 기준 commit | `795b4c974f44aa73b366f93eeb2dd2f22654ece7` |
| 편집 직전 tracked diff SHA-256 | `ec8e87223601bf07c67d43c46d1cfa240b4be4012976686df2fa75eb4875fc4c` (`git diff --binary HEAD`의 바이트) |
| untracked 범위 | 편집 직전 28개 파일의 경로·SHA-256 목록을 로컬 기준 사본에 보존. 해당 canonical JSON SHA-256 `d4e4be563614da311086cd98598d89e92e0c991fc7a624725e3f78223a93ff56` |
| 소유 변경 | `contracts/disk-storage-design.md`, `disk-test-matrix.json`, `dev4-sql-design.md`; `plans/dev3-implementation-plan.md`, `dev4-implementation-plan.md`, `stage-completion.json`; 평가 보고서와 이 조사 기록 |
| 공유 변경 | `devdoc/README.md`, `evidence/README.md`, `documents.json`의 이 기록/보완 진입점만 최신 내용에 추가. 진행 중인 제품/COMM 변경·과거 receipt를 덮어쓰지 않음 |
| 반영 순서 | 소유 계약 → 인수 계획/JSON → 평가 요약/색인 → 별도 검토 → 검토 결과 기록. 실행 산출물 재생성 없음 |
| 검증 범위 | 소스/계약/diff의 정적 대조. 제품·모델·Oracle 실행 비교·benchmark 미실행. 문서 자동 검사는 기존 pre-push gate로 유지 |

이 identity는 조사한 작업 트리의 문맥이며 시험 후보 fingerprint나 clean export 검증 결과가 아니다. 이후 동시에 진행된 다른 작업의 상태는 이 조사로 인증하지 않는다.

## 2. 실제 열람 근거

공개 소스 경로/remote는 [로컬 소스 catalog](../reference/opensource-references.md)에서 확인했다. 아래 네 checkout의 실제 HEAD와 tracked 변경 없음을 재확인했다. build·실행·전체 구현 감사를 수행한 것은 아니다. line은 해당 revision의 열람 구간이다.

| 입력 | 실제 revision/문서 버전 | 읽은 책임 | XADB에서 얻은 판단 |
| --- | --- | --- | --- |
| Oracle | Database 19c, 2026-10-19 공식 문서 열람 | [Managing Undo §16.2.2.3](https://docs.oracle.com/en/database/oracle/oracle-database/19/admin/managing-undo.html): retention guarantee와 공간 부족 DML 실패 | 과거 버전 보존과 무제한 쓰기 가용성을 동시에 약속하지 않음. XADB의 live pin 보호·새 write 거절을 원인별 진단/재개 인수로 구체화 |
| Oracle | Database 19c, 같은 열람일 | [Tuning the PGA](https://docs.oracle.com/en/database/oracle/oracle-database/19/tgdba/tuning-program-global-area.html): 작업 영역·one/multi-pass·target와 absolute limit 구분 | XADB는 이미 있는 합산 hard quota를 유지. binding 교체·동시 merge peak와 cleanup 예약을 검증. 외부 제품의 session 종료 정책/수치를 그대로 채택하지 않음 |
| PostgreSQL | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/replication/slot.c:1305–1368`, `ReplicationSlotsComputeRequiredLSN`: persistent slot의 저장된 restart LSN까지 보존 | runtime 소비 위치만으로 WAL을 회수하면 restart 때 필요한 로그가 사라질 수 있음. 아래 후속 소비자 인계 의무에 반영 |
| PostgreSQL | 같은 revision | `src/backend/utils/sort/tuplesort.c:31–72`: sort별 메모리, run 생성·bounded merge·preread | query별 메모리와 전체 동시 실행 peak는 별도. spill pass·실제 I/O 비용과 합산 자원 인수 유지 |
| MariaDB | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `storage/innobase/trx/trx0purge.cc:1445–1481`: oldest view와 history_size 기반 DML 지연 계산 | 오래된 view·purge 진행·쓰기 압력을 함께 관측. 지연 공식을 복제하지 않고 XADB의 예약/명시 거절을 유지 |
| CUBRID | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/transaction/log_page_buffer.c:4043–4168`, `logpb_flush_pages`: sync/group/async 구분·요청 LSA 대기 | group commit과 약한 ACK는 다른 선택. 미래 group commit도 같은 durable 성공/latency 조건으로 평가; 이번에 도입하지 않음 |
| SRC-L04 | `0682bdd8610c3728810df616c95a858cda675706` | `src/sm/sdc/sdcUndoSegment.cpp:2022–2110,2290–2340`: disk undo 할당의 bounded 재시도·최소 disk view·회수 경계 | 긴 reader가 공간 회수에 미치는 영향과 실패 종료를 점검. 외부 segment 알고리즘을 XADB의 reference/aging 모델로 오인하지 않음 |
| SRC-L04 | 같은 revision | `src/sm/sdb/sdbFlushMgr.cpp:325–369,548–577`: 교체/checkpoint 작업 선택과 무진행 busy-loop 수정 주석/조건 | eviction 진행과 checkpoint floor 진행을 따로 인수. hot-page redirty에는 scheduling 공정성만으로 충분하지 않아 pressure/drain 조건 추가 |
| SRC-L04 | 같은 revision | `src/qp/qmc/qmcDiskSortTempTable.cpp:195–224`: 임시 작업 공간 추정과 sort area 제한 | cardinality 추정은 실제 자원 예약을 대체하지 않음. XADB의 merge peak·FD·임시 파일 소유 검증에 연결 |

외부 제품 설명은 해당 구간에서 읽은 사실에 한정한다. XADB에 대한 선택과 반례는 그 사실을 바탕으로 한 자체 판단이며, 외부 제품과 동일한 구현·전체 SQL 호환·성능을 주장하지 않는다.

열람 파일 전체 바이트의 SHA-256은 다음과 같다.

| 소스/파일 | SHA-256 |
| --- | --- |
| PostgreSQL `slot.c` | `b668a8efcfa5c43077850bf65ad1b296a6be27b1a1d6762c98e2d878ce9ddeb0` |
| PostgreSQL `tuplesort.c` | `dc140a5018871fe263b585f4e8a7c9ab364b7d9533a6188b4279ab9ce3135641` |
| MariaDB `trx0purge.cc` | `378c07e8dffa370c8860e5825c43582a9f709504568764621b73fb833f0937de` |
| CUBRID `log_page_buffer.c` | `91452a20ba010f34d169056f2b1438b3036a02b943099a69410f92147dc79658` |
| SRC-L04 `sdcUndoSegment.cpp` | `45edeeca0aaa0f1fa2ca4aff3eb56ad7b7270faea0d93c8e94cbd7c22187ae2d` |
| SRC-L04 `sdbFlushMgr.cpp` | `e07232e71e13fcc8fa5df91efbe0ec4896223b9d7beb33694903f3311a9df487` |
| SRC-L04 `qmcDiskSortTempTable.cpp` | `dcf21171ef06345938418b32e4a3a95c8093ae86a0a9798d57b61bcee90f38ae` |

XADB 현재 구조는 `crates/engine/src/catalog.rs:112–115`의 `TableRoot.rows: Vec<RowEntry>`와 `transaction.rs:821–830`의 global commit position 검증을 확인했다. 열람 바이트 SHA-256은 각각 `8ebe21f20040708d03d7c601c58b67e3f0ae14a8369bd94b6ab85d9fdadec9f1`, `1836db2b7b54608bac9085a6386b4d5beaa74d0038a0c4abab7cd254a747b75a`다. DEV3의 bounded page/undo·table validation 전환은 이미 채택돼 있으므로 이번에 발견한 새 해결책으로 계산하지 않는다.

## 3. 확인한 한계와 실제 보완 범위

| 한계/반례 | 기존 대비 보완 | 정본·인수 |
| --- | --- | --- |
| 신규 거래 한도를 낮춘 reopen에서 이전 loser/CLR 복구 의무를 새 한도로 자를 위험 | 기존 fixed profile을 checksummed identity와 pre-write 설정 거절로 구체화. 표현 상한·새 admission·복구 요구량 분리 | [DEV3 14.1](../contracts/disk-storage-design.md#capacity-profile), DS03/05·F01/02/04/05 |
| 긴 reader가 undo 회수를 막고 쓰기가 거절됨 | 기존 보호/거절 정책을 유지하면서 자원별 owner 진단·실제 해제 후 재개 사례 추가 | [DEV3 13.3](../contracts/disk-storage-design.md#retention-pressure), DS03/04/06 |
| replacement flush는 진행해도 oldest dirty/지속 redirty 때문에 checkpoint WAL floor가 정체 | bounded service와 무진행 감지, pressure에서 새 forward admission 제한·기존 유한 작업/rollback/flush 진행을 명시 | [DEV3 9.2](../contracts/disk-storage-design.md#flush-progress), DS03/05·M06·PF09 |
| 같은 table의 독립 행까지 Serializable conflict; 높은 시도 TPS가 낮은 유효 처리량을 가림 | 독립 table/행·실제 충돌 × native 격리별 비교, 외부 부하와 원요청 retry 비용 분리 | [PF09](../plans/dev3-implementation-plan.md#performance); SQL은 기존 Serializable만 소비 |
| query별 예산만으로 동시 SQL·binding 교체 peak를 놓침 | 기존 DB aggregate quota를 유지하고 old+new peak·cleanup 예약·동시 spill/실패 unlink 사례를 명시 | [SQL 합산 예약](../contracts/dev4-sql-design.md#aggregate-pressure), [T07/08/11/14](../plans/dev4-implementation-plan.md#aggregate-pressure) |

binding 반례의 단위는 MiB다. DB64 중 기존 binding8을 포함해63을 이미 charge하고 새 binding4를 준비하면 peak67로 거절해야 한다. 최종 크기만 `63−8+4=59`로 계산하면 이전 binding을 보존해야 하는 실패 원자성 구간의 한도를 놓친다. 이는 설계 산술 예이며 실제 메모리/OOM 시험 결과가 아니다.

보완은 DEV3-REVIEW-008·DEV4-REVIEW-006의 D2 계약 구체화다. 초기 수치 상향, 공개 SQL 의미 변경, 새로운 product crate, 미래 연산자/복제 구현은 포함하지 않는다. 제품 코드를 지금 변경하지 않은 이유는 핵심 한계의 소유 구현이 DEV3/4이며 선행 DEV2 인수·leaf 동결이 미완료이기 때문이다. 현재 DEV2를 최종 디스크 엔진처럼 키우는 임시 우회보다 기존 하위 계층 순서를 유지한다.

<a id="future-operations"></a>
## 4. 후속 운영 기능에서 다시 닫을 의무

다음은 **후속 설계 입력**이며 현재 archive/RP/온라인 백업 구현·완료 판정이 아니다. 정본은 [복구/회수 계약](../contracts/recovery-logging-design.md#section-10)과 해당 기능 착수 때의 소유 계약이며, DEV3의 offline profile에 durable consumer가 있는 것으로 소급 적용하지 않는다.

- persistent WAL consumer를 도입할 때 runtime 처리 위치·외부 ACK·영속 restart 위치를 구별한다. 초기 pin 등록과 회수 선택을 원자 조정하고, 새 위치까지의 consumer 상태가 crash 후에도 복원된다는 근거를 영속 인계한 뒤만 이전 WAL을 해제한다. 기록 전 crash와 기록 후/회수 전 crash를 모두 시험한다. PostgreSQL의 저장된 restart LSN 처리가 이 질문의 연구 근거다.
- consumer 한도를 넘기면 필수 로그를 삭제한 채 정상 연결이라고 표시하지 않는다. 명시적인 소비 실패/재동기화 상태와 pin 해제의 영속 순서를 정하고, 관리자의 의도와 복원 요구를 인수한다. 느린 consumer의 과거 WAL pin과 DEV3 RECOVERY_REBASE의 반복 crash 고정 공간 증명을 다시 결합해야 한다.
- 온라인 백업/PITR은 base image·필수 redo/undo·catalog/파일 identity·복원 cut의 완결 집합과 보존 인계가 필요하다. 초기 offline exact-cut 복사의 PASS를 이 기능의 증거로 승계하지 않는다.
- 큰 DML은 대상 목록 spill뿐 아니라 관측 버전·최종 UNIQUE·누적 undo/CLR·statement rollback·전체 abort의 bounded 실행을 함께 재설계한다. 한 SQL 문장을 숨은 여러 commit으로 나누지 않는다. 타입/조인/집계/optimizer 확대는 값/비교/codec·독립 SQL 기대값·spill·측정 비용을 해당 DEV7 leaf에서 닫는다.

## 5. 독립 검토와 실행 상태

`/root/extensibility_review`가 DP01의 별도 실행으로 원문·반례·기준 사본 대비 DEV3/4 계약/계획·DS/stage JSON·평가 §8·조사/색인을 읽기 전용 검토했다. 사전 검토에서 이미 존재하는 SQL aggregate quota를 새 발견으로 표현하지 않도록 구분했고, 공정한 flush만으로 hot-page recLSN이 전진하지 않는 반례를 받아 pressure/drain 조건을 추가했다. 최종 **D2 설계 채택 검토 PASS, 통합을 막는 P1/P2 없음**을 확인하여 `/root`가 DEV3-REVIEW-008·DEV4-REVIEW-006을 ACCEPTED로 반영했다.

검토자는 Oracle 두 공식 페이지와 공개3개 checkout의 관련 구간/HEAD를 별도 대조했다. SRC-L04 전면 감사·동시 DEV6/COMM 변경·외부 DBMS 및 제품 실행·모델·문서 자동 검사는 검토 범위에서 제외했다. 독립 검토의 PASS는 설계 diff에만 적용하며 F01–F05 입력 동결이나 제품 시험을 대신하지 않는다. 이 작업이 추가한 문장에는 사용자 지정 제외 명칭이 없음을 내용 대조로 확인했다.

소스 열람과 설계 문서 수정은 실제 제품 보완 코드의 검증이 아니다. DEV2는 partial, DEV3/4의 모델·제품·성능 인수는 NOT_RUN을 유지한다. 조사·설계 편집 중 문서 검사기·외부 제품 빌드를 실행하지 않았다. 이후 사용자가 전체 문서 검사와 commit/push를 요청했으며, 문서 검증은 `scripts/git-hooks`의 pre-push가 전송할 commit의 임시 export에서 수행한다. 그 결과는 해당 commit에만 귀속하며 제품 실행 증거가 아니다.
