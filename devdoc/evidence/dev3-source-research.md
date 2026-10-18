# DEV3 소스 조사 기록

2026-10-18 · **실제 로컬 소스 열람 / 코드 도입·build·제품 시험·성능 비교 아님**

[DEV3 상세 설계](../contracts/disk-storage-design.md) · [구현 계획](../plans/dev3-implementation-plan.md) · [공개 소스 경로 정본](../reference/opensource-references.md)

## 조사 identity와 한계

사용자가 지정한 로컬 checkout을 읽기 전용으로 조사했다. 아래 revision은 이번 조사에서 `git rev-parse HEAD`로 다시 확인했고, 6개 checkout 모두 `git status --porcelain --untracked-files=no`가 빈 출력이었다. 파일 SHA-256은 읽은 작업 트리 bytes의 지문이다. 전체 구현 분석·빌드·시험이나 해당 제품과 같은 SQL/내구성/성능을 보장하는 조사가 아니다. 소스 파일·Git 이력을 XADB에 복사하거나 원본 checkout을 수정하지 않았다.

공개 소스 R1–R6의 경로는 해당 checkout 기준 상대 경로다. 파일별 원래 경로·revision·SHA-256·열람 line range를 가진 inventory는 `/home/et16/work/xadb-source-archive/dev3-20261018-r1/source-inventory.json`이다. 이 archive는 제품 빌드 의존성·Git 배포 산출물이 아니다. 원본을 익명으로 재배포한 것이 아니며 향후 코드 도입 시 출처·라이선스·notice 의무는 별도로 처리한다.

Inventory SHA-256: `22caadc58346c03c0e202aef4ffc3e8d0f3eab861c943471f6e7635be9e89225`. 이 지문은 조사 identity이며 실행 증거가 아니다.

| ID | 대상 | 실제 HEAD |
| --- | --- | --- |
| R1 | PostgreSQL | `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` |
| R2 | MariaDB | `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` |
| R3 | CUBRID | `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` |
| R4 | SQLite | `43a48f77585a88940c727f33682151418f544059` |
| R5 | DuckDB | `1b4c3e54ba3625bac56ad184f0ce642a8d2bfbd5` |
| R6 | RocksDB | `54732b0c8f6f8af34f973d7fae0a234022fcf1fa` |

## 읽은 범위와 파일 지문

아래 line range는 실제 확인한 부분이다. source의 주석과 코드에서 관찰한 책임을 분리해 판단했으며 전체 파일의 모든 branch를 검증했다는 뜻이 아니다.

| ID | 파일 또는 중립 책임명 | 열람 행 | 파일 SHA-256 |
| --- | --- | --- | --- |
| R1 | `src/backend/storage/buffer/README` | 1–125 | `f0058977fadd7b8e8b9b180e56d41c467d5233645899f2679a03e67ae0a551d1` |
| R1 | `src/backend/storage/buffer/freelist.c` | 239–302 | `276156fbe9ee0f85c711cfbebc99c2c1c0e62ce2926fb4e80d8c751bd3d364ba` |
| R1 | `src/backend/access/nbtree/README` | 1–58 | `ffd2ff6f5eae1a81e59fe0f4aa023c0184e807b1bae90f9289d3bb7e7d444b62` |
| R2 | `storage/innobase/buf/buf0flu.cc` | 775–820, 885–920 | `68d9f54b0d42d3319dab37b7cb5a83401522fe953ef39ba89c982f747e606678` |
| R3 | `src/storage/page_buffer.c` | 245–273, 2980–3025, 10755–10805 | `1e137f87e5958902b9197dc8de40b547a52750d25d265aced0dcd7ed0589eb77` |
| R4 | `src/wal.c` | 75–140, 340–390 | `11ab3050c715b8d4b05f20c9030ea96ed70475080ba62fe3f60e441ac765eb47` |
| R4 | `src/btreeInt.h` | 105–178 | `903f2d3ba81ea9cba1dbb6fafef824688adef56a8b4d0fe7a8411261c2b4910e` |
| R5 | `src/storage/standard_buffer_manager.cpp` | 365–425, 455–485 | `4d2b50f04c9b7719fb138a1ec07e373f72403dd6731db9fc8806b034986eee2a` |
| R6 | `db/version_set.cc` | 6308–6390, 7028–7085 | `f5005f1009d24479b9e62875766b8882295974dcf4dc281f36f47870872c7eaf` |

## 관찰·설계 판단·필요한 검증

다음 XADB 선택은 원본의 성능 결론이 아니라 기존 XADB 계약과 대조한 설계 판단이다.

| 출처 | 직접 확인한 책임 | XADB에서 채택한 원칙 | 가져오지 않은 전제·검증 의무 |
| --- | --- | --- | --- |
| R1 | buffer README는 pin과 내용 lock을 구분한다. clock 선택은 pin된 frame을 건너뛰고 후보 부족을 처리한다. nbtree README는 right-link/high key 및 물리 tie-breaker를 설명한다 | PageGuard와 snapshot pin 분리, bounded clock, 동일 key의 RowId 순서 | concurrent mutable B-link 알고리즘은 이식하지 않음. COW scan은 root stack으로 세대 혼합을 막음. DS01/04 |
| R2 | page flush 코드가 page LSN과 flushed LSN을 비교하고 log_write_up_to 또는 doublewrite 경로를 거친다 | WAL-first 대안은 data-before-commit과 별도 복구 계약이라는 근거 | 해당 flush 순서·doublewrite를 COW 경로에 섞지 않음. DS02/RC-T12 |
| R3 | dirty/flushing/이미 할당된 victim 제외 flag, unfix 뒤 교체 가능성, page flush 전 log flush 경로 확인 | 교체 상태·I/O 중 pin·실패 후 buffer 상태를 명시적으로 검사 | 현재 XADB는 공유 dirty frame 없이 private pack을 sync. 해당 lock/MVCC 구현을 복사하지 않음. DS03/04 |
| R4 | WAL 주석의 고정 mxFrame snapshot·reader read-mark와 checkpoint 제한, btree header·정렬 slot pointer·cell 영역 | 고정 snapshot 기준 scan, checkpoint와 reader의 별도 보존 책임, slotted page 경계 검사 | WAL-index/shared-memory 형식·동기화 방식을 채택하지 않음. DS01/04/05 |
| R5 | Pin은 memory eviction/reservation 뒤 lock을 다시 얻어 동시 load를 재검사하고 중복 예약을 해제한다. Unpin은 reader 감소와 eviction queue를 연결한다 | load 경쟁의 이중 예약 방지, owning guard 반환/Drop의 lock 경합 검사 | 분석용 block 크기·columnar 경로를 행 저장 기본값으로 가져오지 않음. DS03/RC-T14 |
| R6 | manifest record 작성→SyncManifest→CURRENT 선택, 선택 실패 때 old manifest 보존; 별도 rewrite 경로에서 rename 후 directory sync | manifest·namespace의 독립 영속 장벽, 불명 선택의 old/new 합집합 보존 | LSM·compaction 구조 또는 실제 crash 시험 결과를 승계하지 않음. DS05/RC-T13 |

## 비교에서 얻은 구체 결정

1. 여러 소스에서 pin·교체·I/O 상태를 구분한다. XADB도 frame pin과 디스크 root 보존을 하나의 refcount로 뭉치지 않는다.
2. in-place 엔진의 WAL-before-page와 XADB의 COW page-before-commit은 다른 전제를 가진다. 이번에는 기존 COW 계약을 유지하고 비용 측정 후만 대안을 검토한다.
3. index leaf의 논리 key와 행 identity를 분리한다. XADB는 RowUID를 유지하며 row tree를 bounded locator로 사용한다.
4. checkpoint와 최신 root만 보존하면 충분하다는 가정을 배제한다. retained journal의 중간 root·previous·snapshot·backup closure를 함께 검사한다.
5. 파일 수·inventory·recovery workspace도 buffer 제한의 일부다. 전체 데이터만 disk에 두고 나머지를 무제한 RAM에 올리는 구현을 DS03/RC-T14에서 거절한다.
6. source 관찰은 성능 비교가 아니다. 작은 pack의 파일 수·sync 비용, COW 쓰기 증폭·full recovery 검증·offline compaction 추가 공간을 PF09에서 측정한다.

## 조사 완료와 남은 실행

6개 로컬 checkout에서 위 범위를 확인하고 지문을 기록했다. source build·benchmark·실제 XADB disk 구현·fault suite는 실행하지 않았다. DEV3 상태는 NOT_RUN이며 이 조사로 DEV2의 미완료 또는 S1 의무가 해소되지 않는다. XADB 코드에 특정 소스 구현을 복사하지 않았으며 외부 원본 경로·지문 archive를 보존했다.
