# DEV3 WAL·undo 재설계 조사와 검토

2026-10-18 · **실제 소스/공식 문서 열람·설계 검토 / 제품 구현·시험·성능 측정 NOT_RUN**

[새 소유 계약](../contracts/disk-storage-design.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [기존 조사](dev3-source-research.md) · [공개 소스 catalog](../reference/opensource-references.md)

## 조사 범위

사용자는 Oracle 유사 방향·ARIES 참고와 COW/GC 재설계를 요청했고, 선행 리뷰 문제를 먼저 고치도록 순서를 지정했다. REVIEW-002를 먼저 반영·검토하고 DESIGN-003을 작성했다. 공개 소스는 비교 입력이며 동일한 내부 알고리즘/SQL·성능을 보장하지 않는다. 원본 checkout 수정·Git 이력/코드 도입·DBMS build는 하지 않았다. 이전 조사에서 COW 유지로 판단한 부분은 당시의 기록이며 이번 채택의 정본이 아니다.

실제 HEAD와 tracked 작업 트리 상태를 다시 확인했다. 아래 6개는 tracked clean이다. 전체 파일 bytes의 SHA-256·revision·열람 행 inventory는 `/home/et16/work/xadb-source-archive/dev3-20261018-r2/source-inventory.json`이며 SHA-256은 `831f3ef0fae41a029e700a8de4c004dc0a08aa9d2061ca2acd71e33872fe4458`이다. archive는 제품 빌드 의존성이 아니다.

| ID | 대상 | 실제 revision |
| --- | --- | --- |
| R1 | PostgreSQL | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| R2 | MariaDB | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| R3 | CUBRID | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| R4 | SQLite | `43a48f77585a88940c727f33682151418f544059` |
| R5 | DuckDB | `1b4c3e54ba3625bac56ad184f0ce642a8d2bfbd5` |
| R6 | RocksDB | `54732b0c8f6f8af34f973d7fae0a234022fcf1fa` |

| ID·열람 경로/중립 책임 | 실제 읽은 행 | 관측과 XADB 판단 |
| --- | --- | --- |
| R1 `src/backend/storage/buffer/README`, `freelist.c` | 1–125 / 238–312 | pin과 content lock, bounded victim 탐색. mutable page pin을 snapshot 보존으로 사용하지 않음 |
| R1 `src/backend/storage/buffer/bufmgr.c` | 4556–4596 | pageLSN까지 WAL flush. XADB WAL-before-page 채택 근거 |
| R1 `src/backend/access/nbtree/README` | 1–65 | 구조 변경·탐색의 동시성. XADB는 초기 tree latch와 stable anchor로 단순화하며 같은 B-link 구현이라고 하지 않음 |
| R2 `storage/innobase/buf/buf0flu.cc` | 760–830, 880–930 | log flush와 doublewrite. XADB는 선택된 pending-slot control까지 명시한 별도 보수적 설계 |
| R3 `src/storage/page_buffer.c` | 10730–10820 | dirty page flush 전 로그 영속화 |
| R3 `src/transaction/log_recovery.c` | 146–170, 265–325, 825–900 | compensation과 analysis/redo/undo. XADB CLR 진행점과 재장애 검증에 반영 |
| R4 `src/wal.c`, `src/btree.c` | 65–145 / 10760–10860 | 고정 snapshot WAL 경계·integrity 순회. XADB 전체 audit와 정상 recovery 비용을 분리 |
| R5 `src/storage/standard_buffer_manager.cpp` | 365–425 | pin 뒤 재검사·중복 예약 방지 |
| R5 `src/storage/single_file_block_manager.cpp` | 1330–1440 | checkpoint 데이터/header sync·공간 수명. checkpoint 장벽과 파일 재사용을 혼동하지 않음 |
| R6 `db/db_impl/db_impl_files.cc` | 210–310 | pending/live 파일의 회수 제외. COW/LSM 파일 회수 책임의 비교이며 LSM 채택 근거가 아님 |

## Oracle·ARIES와 공개 설명

- [Oracle process architecture](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/process-architecture.html): LGWR의 commit redo 영속화, DBW의 data flush와 순서. commit마다 전체 data page sync를 요구하지 않는 방향을 선택했다.
- [Oracle concurrency](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/data-concurrency-and-consistency.html): statement read consistency·undo·row lock. XADB는 명시 ReadCommitted를 추가하고 기본 Serializable은 보수적 table validation으로 유지한다. Oracle과 같은 isolation 구현이라고 하지 않는다.
- [Oracle integrity](https://docs.oracle.com/en/database/oracle/oracle-database/26/cncpt/data-integrity.html), [constraint](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/constraint.html): composite UNIQUE의 all-null 예외와 부분 NULL 충돌, immediate constraint의 statement 종료 검사. REVIEW-002의 NULL/최종 집합 검사를 유지한다.
- [ARIES 원 논문](https://cs.stanford.edu/people/chrismre/cs345/rl/aries.pdf): §§4.2–4.4, 5.4, 6.2 및 CLR 논의의 pageLSN·ATT/DPT·recLSN·fuzzy checkpoint·UndoNxtLSN을 참고했다. XADB는 bounded streaming redo와 영속 UndoRef chain을 사용하므로 원형 ARIES의 그대로 구현이 아니다.

이번의 핵심 판단은 GC라는 이름을 지우는 것이 아니다. immutable pack을 만들어 이전 root를 모두 추적하는 비용을 없애고, page allocator와 영속 undo·삭제 index/row·transaction 상태 각각의 수명 책임을 둔다. undo aging은 여전히 필요하다. doublewrite 추가 I/O, CLR/undo 관리 복잡성, 보수적 serializable conflict·offline backup 중단시간은 남는 비용이며 PF09 실측 전 우월성을 선언하지 않는다.

<a id="review"></a>
## 작업 순서·검토 사실

기준 HEAD `b4d1d8cdd86bfa7d502948708445878c712c5a79`, 착수 dirty manifest SHA-256 `e79b53f41cd4f36a2c2766357a5bf4969c41b256430dd10fac2e567c28273b1d`. 같은 archive의 `base-dirty.json`에 보존했다. 진행 중 HEAD가 `2a0c3ec3aee1bda39af4d394eba2bb3ae3c70df1`로 바뀌고 별도 DEV2 성능 작업이 나타났다. 해당 product code·실행 evidence·build 변경을 되돌리거나 이번 결과로 승인하지 않았다. 동시 DEV4 문서는 직접 수정하지 않는다. 공통 구현 계약에서 새 DEV3 소비 경계를 지정하고 배치 책임 문구만 최신 원문에서 좁게 맞춘다.

1. **REVIEW-002 선행 보완:** partial-null UNIQUE, statement-final 검사·key swap, global-q 한계, reopen 비용, 파일 수 기반 scratch·compaction 점유를 먼저 세 파일에 반영했다. `/root/dev3_redesign_review`가 원본 대비 실제 diff를 검토해 PASS·통합 차단 P1/P2 없음. `/root` 확인. snapshot은 archive `phase1/`에 원래 bytes로 보존했다.
2. **DESIGN-003 초안:** mutable page·WAL·undo/CLR·DWB·checkpoint/aging을 작성했다. 같은 별도 검토자가 P1 세 건(recovery CLR 발견 경로, DWB bootstrap, stale DWB image)과 P2 두 건(checkpoint capture/commit 상태, undo suffix 참조)을 지적했다.
3. **수정:** CLR 전에 recovery epoch/WRITER 영속 선택, 독립 FILEMAP·pending-only DWB control과 retirement, capture LSN/commit coordinator 일치, logged chain cut·참조 count/rollback pin을 명시했다. 해당 반례를 인수에 연결했다.
4. **최종 검토:** `/root/dev3_redesign_review`가 최신 본문·계획·상위 profile·DS/RC/KU/stage 인수·색인을 재검토해 **PASS**, 통합 차단 P1/P2 없음. 추가로 no-force의 새 zero home을 allocation-authorized 완전 INIT+연속 redo로 구성하는 경계를 보강하고 재확인했다. 복원된 pageLSN보다 큰 인증 baseLSN도 인수에 연결했다. checkout HEAD/tracked clean, source hash, archive와 선행 snapshot 지문을 별도 확인해 일치했다. `/root`가 반영 확인 후 DESIGN-003을 ACCEPTED로 기록했다.
5. **검증 범위:** 이번 변경 경로의 `git diff --check` PASS. 제품 시험과 문서 자동 검사는 실행하지 않았으며 document-validation.json을 재생성하지 않았다. 문서 검사는 push gate에서만 한다. F01–F07·제품 구현/복구 시험은 여전히 미완료다. 동시 DEV4·DEV2 성능 변경은 이번 독립 승인 범위에서 제외했다.

| 선행 snapshot | SHA-256 |
| --- | --- |
| `disk-storage-design.md` | `d02e011b69cb76f7f36ed5d32e9c3ed455ef1fc4ccccd8d9141795f4f8b4c2b3` |
| `dev3-implementation-plan.md` | `9bd4dc12aae2e8c202acbd705e9db2bffe578a428add4cf68f8214a02c29a478` |
| `disk-test-matrix.json` | `b18397bbf0fdafc788b171489113e676d2b2abe6256590d6c6253cc22f1b2206` |

선행 COW scratch/compaction 보완은 DESIGN-003에서 대체했고 UNIQUE 의미와 측정/반례 의무는 유지한다. 이 순서는 사용자의 우선순위에 따른 실제 작업 기록이며 제품의 두 구현을 완료했다는 뜻이 아니다.
