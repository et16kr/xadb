# DEV3·DEV4 아키텍처 비교와 설계 보완

2026-10-18 · **소스 독해·설계 보완 / D2 독립 검토 PASS·채택 / 모델·제품·성능 실행 NOT_RUN**

[DEV3 보완 결정](../contracts/disk-storage-design.md#architecture-review) · [DEV3 모델·예약 인수](../plans/dev3-implementation-plan.md#state-model) · [DEV4 보완 결정](../contracts/dev4-sql-design.md#architecture-review) · [DEV4 인수](../plans/dev4-implementation-plan.md#acceptance) · [공개 소스 catalog](../reference/opensource-references.md)

## 조사 범위와 판단

사용자가 DEV3·DEV4를 로컬 DBMS checkout과 비교해 체계성·치밀함·아키텍처를 리뷰하도록 요청했고, 후속으로 문서 수정을 지시했다. 앞선 응답에서 실제 소스를 읽었으며 이 수정 작업에서 HEAD·tracked 변경·파일 hash를 다시 확인했다. 원본 checkout은 수정하지 않았고 외부 코드·Git 이력을 도입하지 않았다. 기존 [DEV3 조사](dev3-wal-undo-research.md)·[DEV4 조사](dev4-source-research.md)의 역사적 범위와 독립 검토 결과는 변경하지 않는다.

책임 분리, WAL-before-page, 영속 undo, 논리 undo와 구조 변경의 분리, typed SQL/prepared 경계는 타당한 방향으로 판단했다. 그러나 소스의 일부 책임이 비슷하다는 사실은 XADB의 참조 계수·queue·DWB control 조합의 안전성이나 성능을 증명하지 않는다. DEV3/DEV4는 미구현이며 기존 DBMS와 같은 제품 성숙도·전체 SQL 호환·처리량을 주장하지 않는다.

## 실제 source identity

아래 5개 checkout은 재확인 때 tracked clean이다. untracked 내용·빌드 환경·전체 기능 감사는 하지 않았다. 원래 root/origin/경로·열람 행과 14개 파일 전체 SHA-256은 외부 local archive `/home/et16/work/xadb-source-archive/dev3-dev4-architecture-20261018-r1/source-inventory.json`에 보존했다. inventory SHA-256은 `6ef50788d811dd8d1008d4b93f40738dfd8e1cbc37e397f2d680a7ca583b0399`다. source R7의 원래 경로 inventory는 그 archive에만 두며 제품 빌드 입력이 아니다.

| ID | 대상 | 실제 revision |
| --- | --- | --- |
| R1 | PostgreSQL | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| R2 | MariaDB/InnoDB | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| R3 | CUBRID | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| R4 | SQLite | `43a48f77585a88940c727f33682151418f544059` |
| R7 | 로컬 참고 소스 R7 | `0682bdd8610c3728810df616c95a858cda675706` |

| 소스·읽은 범위 | 관측 | XADB에 적용한 판단 |
| --- | --- | --- |
| R1 `storage/buffer/README` 1–108, `bufmgr.c` 4554–4599 | pin/content lock 구별, pageLSN까지 WAL flush | page pin·snapshot·참조 수명을 분리하고 WAL-before-page 유지 |
| R1 `parser/analyze.c` 109–188, `utils/cache/plancache.c` 688–751 | 분석된 query와 parameter 타입, plan 유효성 재확인 | DEV4 parse/bind/plan/execute·stale 경계 유지 |
| R2 `trx/trx0undo.cc` 1–180 | undo history·purge 및 rollback segment/page latch의 구체적 소유 | XADB 고유 refcount/chain cut은 별도 상태 모델과 잠금 순서 필요. 해당 알고리즘을 그대로 도입하지 않음 |
| R2 `buf/buf0flu.cc` 745–845·875–944, `buf/buf0dblwr.cc` 588–690 | flush 경계와 여러 page의 DWB batch | page별 장벽을 보존한 bounded batch와 비용 인수 추가 |
| R3 `transaction/log_recovery.c` 266–335·4900–4970 | 논리 undo의 보상 기록, CLR은 redo-only, 다음 undo와 system operation 처리 | effect/progress의 원자 CLR, split 뒤 논리 undo·재장애 모델 유지/보강 |
| R4 `vdbeaux.c` 3240–3312·3445–3515 | statement savepoint rollback과 전체 rollback, cleanup 실패 시 오류 승격 | SQL의 이전 statement 보존·rollback 실패/Fenced 구분 유지 |
| R7 undo-segment 1870–1915·2020–2095·2270–2375 | undo page 할당·최소 view SCN·extent recycle | XADB의 모든 자원 예약·reader/rollback 보존을 자체 계약으로 검증 |
| R7 disk-flusher 1770–1885, disk-index 20–88 | I/O buffer 단위 doublewrite·WAL 대기, SMO와 latch 순서 | 초기 version-edit gate와 명시 다중 page 순서, batch 인수 선택 |
| R7 SQL-interface 1635–1765·2390–2476, statement 105–145·255–302 | validate/optimize/execute, 객체 검증·잠금, statement savepoint/view SCN | SQL/engine 경계와 token·cleanup 소유 유지 |

R1 경로는 `src/backend`, R2는 `storage/innobase`, R3/R4는 `src` 아래다. 위 표는 실제 독해 부분이며 파일 전체 또는 제품 전체 분석을 뜻하지 않는다.

공식 [디스크 MVCC·거래 관리 설명](https://manual.altibase.com/7.1/en/admin/administrator/8.-Managing-Transactions/)과 [undo tablespace 설명](https://manual.altibase.com/7.1/en/admin/administrator/6.-Managing-Tablespaces/The-Undo-Tablespace/)도 앞선 리뷰에서 확인했다. disk in-place 변경·undo/TSS 보존과 만료 공간 재사용의 비교 입력이며 resident/mixed 엔진을 XADB 범위에 추가하지 않는다. 벤치마크나 해당 제품의 실행 비교는 하지 않았다.

## 지적과 반영 범위

| 지적 | 반영 | 아직 검증되지 않은 것 |
| --- | --- | --- |
| undo/refcount/aging/rollback 교차 상태가 미구체적 | DEV3 7.1·8.1절: version-edit gate·page 순서, transient reference pin, edge/count/queue 상태, D3-M01–M06 | 모델 코드·전수 탐색·실제 concurrency/recrash |
| reserve 숫자만으로 rollback 진행을 보장할 수 없음 | DEV3 13.1절: opcode별 자원 벡터·동시 부등식, 전체 pin 예약, 진행용 frame/FD/WAL·workspace 분리 | 실제 codec bound 대입·예약표·최악 조건 실행 |
| page별 DWB/control sync 비용 | DEV3 9.1절: 최대64 page batch, 전체 file sync 후 전체 retirement, bootstrap repair 순서; PF09 1/8/64 비교 | byte layout·deadline·fault profile·성능 효과 |
| DEV2 영속 경로와 DEV3 재구현 부담 | DEV3 계획 1.1절: 실제 symbol/test의 재사용/adapter/전용/신규 inventory | 아직 없는 K1–K5 코드를 재사용했다고 주장하지 않음. stage 선행 유지 |
| DEV4 초기 한계가 일반 제품 능력으로 오인될 수 있음 | 처리 범위 표·시도/성공 TPS·거절 원인·gate/flush 비용과 실제 공급 인수 | 큰 DML·고급 optimizer·SQL ReadCommitted는 기존 후속 의무 |
| all-null unique lookup의 단건 오해 위험 | 기존 의미를 명시하고 D4-V25·T05에 다중 결과/단건 종료 mutant 추가 | 제품 lookup 구현·Q1 실행 |

이 작업은 확정된 데이터 손상 버그를 고쳤다는 주장이 아니다. 앞선 설계 리뷰의 미완결·성능·개발 비용 위험을 구현 가능한 순서와 인수 의무로 좁힌다. 새로운 동시성 단순화도 실측 전 선택이며, 모델 PASS와 제품 PASS를 구별한다.

## 작업 소유·동시 변경

| 항목 | 기록 |
| --- | --- |
| 분류·담당 | DEV3-REVIEW-004 / DEV4-REVIEW-003 D2. 작업/계약/통합 `/root`; 읽기 전용 독립 검토 `/root/dev34_architecture_review` |
| 기준 | HEAD `b8b1e60a314d397198e2f2c41205f9e468a6decf`; archive `base-identity.json` SHA-256 `fc06a1073d54c55877a57a41de1f531f452759e1e61527b03f2f0cec62a40f93`; 원문14개 사본 보존 |
| 전용 수정 | DEV3/DEV4 상세 설계·구현 계획, disk/recovery test matrix의 DEV3 항목, 이 조사 문서 |
| 공유 수정 | implementation-contracts의 DEV3 적용 안내, stage-completion/development-stages의 해당 단계 배열·참조, devdoc 및 contracts/plans/evidence README, documents.json 신규 근거 등록 |
| 중복 관측 | 착수 때 COMM 신규 설계와 공통 색인/stage 변경, DEV2 extended/stress/fuzz·현재 구현 문서 변경 존재. 해당 본문·제품 코드·receipt는 이 작업 범위 밖이며 최신 파일의 DEV3/4 위치만 좁게 수정 |
| 반영 순서 | owner 계약→모델/자원/성능 계획→DS/RC/stage 인수→색인→읽기 전용 독립 검토→수정·재확인 |
| 산출물 | Markdown/인수 JSON만 변경. 새 product path·crate·dependency·runner 없음. document-validation.json/과거 evidence 재생성 없음 |
| 검증 방침 | 의미/실제 diff 독해와 별도 D2 검토. 문서 자동 검사는 DOC-002 push gate에서만 실행; 모델·제품·DBMS 실행·PF09/PF10 NOT_RUN |

작업 중 HEAD가 `055f744a33ac33a34fada89cba2660588f07fb4c`로 이동했고 COMM 후속 리뷰·DEV2 extended evidence의 동시 변경을 관측했다. 공유 README의 좁은 patch가 최신 COMM 문구와 달라 적용되지 않은 뒤 최신 원문을 다시 읽어 DEV3/4 위치만 반영했다. 원문 사본 전체를 되쓰지 않았으며 해당 동시 작업의 의미·실행 결과를 이번 검토로 승인하지 않는다. 변경한 tracked 경로의 `git diff --check`는 PASS이며 링크/인수/제품 의미의 자동 검증 결과가 아니다.

## 독립 검토

`/root/dev34_architecture_review`는 기준 사본 대비 실제 변경·색인, 기존 소유 계약, source/base inventory hash를 읽기 전용으로 검토했다. 동시 COMM·DEV2 작업은 승인 범위에서 제외했으며 모델·제품·문서 검사기를 실행하거나 파일을 수정하지 않았다.

첫 검토는 **P1 1건**으로 채택 보류였다. page latch의 try/retry와 공정한 actor scheduling만으로는 reader가 매 try 순간을 선점하는 무한 starvation을 배제하지 못한다는 지적이다. 이에 유한 K 뒤 FIFO progress turn, 구조 변경 차단/준비 단계의 flusher 허용, 전체 pin 준비 뒤 reader content drain과 bounded 적용을 명시했다. 추가 검토에서 발견한 workspace/turn 역순 대기도 막도록 선택된 owner만 공유 workspace를 획득하게 했고, 자원 표에도 rollback을 포함했다. M03/M06·DS03·DEV3 완료 기준에는 depth16 safety와 별도의 fair-cycle/감소 순위 진행 판정, 관련 mutant와 실제 thread 인수를 연결했다.

최종 독립 검토는 최신 수정본을 다시 읽고 **P1 해소·추가 필수 P1/P2 없음·설계 채택 PASS**로 판단했다. `/root`가 수정 반영과 검토 범위를 확인하여 DEV3-REVIEW-004·DEV4-REVIEW-003을 ACCEPTED로 기록했다. 실제 guard·예약 수치·모델·제품·성능 인수는 검토 대상 실행 결과가 아니며 모두 해당 미동결/NOT_RUN 상태를 유지한다.
