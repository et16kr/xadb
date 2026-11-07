# PR 58 WAL 실패 경계 후속 검토

2026-11-06. 검토 기준은 main `92b22bf97f80af99b17ec468348529f41a3e819e` 대비
PR head `a374b3ad98a2c02c2d9a6ae90b5ca0e1ecaa21ab`다. 사용자의 설계·방향성·코드 검토와
신규 시험·필요한 수정 commit 요청에 따른 후속이다. 담당 `/root`, 별도 검토자
`/root/independent_review`(이번 세션의 별도 실행). [앞선 추가 검토](additional-review.md)의
REFILL/CHAIN/flush literal 시험과 그 실패·결과·hash는 보존하고 재실행하지 않았다.

## 설계·코드 판단

[제품 방향](../../../devdoc/plans/project-direction.md)과
[U3 계약](../../../devdoc/contracts/dev3-tree-smo.md#unlink-u3)을 대조했다.
RAM note는 제거 권한이 아닌 후보로만 사용하고 현재 anchor에서 다시 route한다. 같은 cut의
canonical replay, resident image와 stamp 검증, 기존 structural shell과 ExistingGroup 설치
본문 재사용은 디스크 엔진의 정확성·복구를 먼저 보장하는 방향과 일치한다.

DeletePrune·live INSERT CLR의 설치 뒤 note 기록, ControlCut과 기록 시점, LOAD 보호 집합,
정리 drain의 Busy/Flush/Fatal, Structural bank·XOI와 퇴역 목록, source census를 검토했다.
독립 검토에서도 수정이 필요한 제품 결함은 발견하지 못했다. 이번 commit은 제품 코드를
바꾸지 않고 아래 실패 경계의 시험·증거·gate 선택을 추가한다.

범위 제한은 유지한다. CREATE Catalog Job을 보유한 제품 table은 아직 unlink하지 않는다.
퇴역 page는 Allocated로 남고, unlink마다 canonical replay와 fsync 비용이 발생한다.
A drain의 source census 여유 14 B는 후속 변경 시 재검산해야 하며 실제 native stack 측정이
아니다. U4 native process fault cut은 미실행이며 08-1 전체 완료나 제품 인수로 승격하지 않는다.

## 새 시험과 독립 기대값

수정한 파일은
`crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/unlink_rows.rs`다.
기존 memory namespace fault port와 A의 실제 cleanup drain, C의 실제 복구, B의 읽기를 사용한다.
새 helper가 8행(두 leaf) 또는 11행(세 leaf)을 만들고 왼쪽 leaf의 세 행을 각각 commit DELETE한다.
C의 Row 정리만 먼저 끝내 note 하나를 남기고 A drain의 unlink에만 fault를 주입한다.

| 새 함수 | unlink 종류 | fault |
| --- | --- | --- |
| `unl_review_leaf_unlink_partial_wal_write_fences_and_recovers` | LeafUnlink | 새 WAL frame의 31 B만 쓰고 오류 |
| `unl_review_leaf_unlink_sync_error_fences_and_recovers` | LeafUnlink | 전체 WAL 쓰기·sync 완료 후 오류 |
| `unl_review_root_collapse_partial_wal_write_fences_and_recovers` | RootCollapse | 새 WAL frame의 31 B만 쓰고 오류 |
| `unl_review_root_collapse_sync_error_fences_and_recovers` | RootCollapse | 전체 WAL 쓰기·sync 완료 후 오류 |

각 사례는 다음을 검산한다.

- fault가 정확히 한 번 도달하며 `Stage::Unlink`, Durable/Wal, CallerMustResolve와 A의
  `must_resolve`가 반환된다. drain은 unlink 성공을 보고하지 않고 note를 유지한다.
- session은 fenced, read gate는 다시 열리고 active read operation은 0이다.
- 실패 세션의 일반 pool 접근은 Fenced/CallerMustResolve로 거절된다. durable wait 전
  RAM 설치는 코드 검토 사실이며, 실패 세션을 healthy API로 읽어 검증하지 않는다.
- home 파일과 기존 WAL prefix는 불변이다. 부분 쓰기는 정확히 31 B만 추가되고 sync 오류는
  전체 frame을 남긴다.
- 새 session의 복구 후 세 DELETE의 ACK와 나머지 행이 보존된다. 부분 frame에서는 원래
  anchor가, 전체 frame에서는 새 root·level·퇴역 목록이 복구되며 RAM note는 모두 사라진다.

기대 행은 입력 순서의 RowId와 독립 tag로 만든다. successor anchor는 원래 트리에서 survivor의 PageRef·root level·퇴역 집합을 직접 계산한다. 제품 frame producer를
기대값 생성에 재사용하지 않는다. 메모리 fault port의 sync 완료 모델은 실제 장치 내구성이나
강제 종료 시험을 대신하지 않는다.

## 실행·독립 검토

새 네 함수만 `--exact --nocapture`로 각각 선택한다. 전체 workspace 또는 기존 324개 gate
선택을 로컬에서 재실행하지 않는다. immutable ELF 하나, 두 프로세스 동시 실행, 사례당
1,200초 상한, 32 MiB test thread stack이다. 계획·전체 소스 manifest·ELF hash·로그는
`/workspace/scratch/pr58-wal-review/`에 남기며 이번 build target만 결과 기록 후 삭제한다.

최초 빌드는 신규 시험의 RetentionPhase import 누락으로 실패했다. 해당 import를 추가했고
미실행을 PASS로 기록하지 않는다. `pr58-build.log`에 원 실패를 보존한다. 빌드 중 독립 검토의
두 권고(must_resolve 직접 단언, 복구 topology 검산)를 반영한 뒤 최종 ELF를 빌드했다.
중간 ELF는 시험 실행에 사용하지 않았다.

정적 독립 D1 검토: ACCEPT, 필수 0·잔여 권고 0.
첫 실행 blob: `129ac60ae4d9971fffdee4a168e5d8dd50bdbebd`.
실행 결과와 로그 검토는 아래에 별도 기록한다.

첫 실행에서 LeafUnlink 두 사례는 각각 125.36초·125.11초에 FAILED였다. 오류·fence·note·gate
검산 뒤 새 시험이 `reachable`/`anchor_of` helper로 차단된 세션을 읽었는데, helper 내부의
`pool.target_request`가 Fenced를 반환하여 unwrap이 실패했다. 제품 결함이 아니라 시험의
건강 상태 전제 오류다. 실패 세션에서는 API의 Fenced 거절을 단언하고, 예상 successor는
원래 트리에서 직접 계산하도록 수정했다. 복구 후의 topology·행·note 검산은 유지한다.
첫 실행 기록과 수정 뒤 결과는 별도 디렉터리에 보존한다.

RootCollapse 첫 실행 두 사례도 같은 helper 전제로 53.69초·53.39초에 FAILED였다.
그 뒤 공통 helper를 수정한 네 사례만 다시 선택했다. 최종 결과는 **4 passed, 0 failed**이며
각 프로세스는 1 passed, 2426 filtered out이다. 제품 수정은 없다.

| 사례 | 최종 시간(초) | WAL 추가 bytes | 최종 로그 SHA-256 |
| --- | ---: | ---: | --- |
| LeafUnlink write | 90.25 | 31 | `48642bde7d3f15423e485378a963a051b5568a885070b80cc8fc8dc0b9869bf9` |
| LeafUnlink sync | 90.30 | 35248 | `ccc7b673fd3c59999a0cb7843fb0e7772a16c4af77ea9752c23bf5a59ec3a248` |
| RootCollapse write | 54.46 | 31 | `24dfe433983381b7d7cff9a24250d34f2c4543bf0392e35d15bf9e43bfb13c5f` |
| RootCollapse sync | 54.10 | 19032 | `3c91bccdb81f81a39cda19b9e01135eeb7b6276e782517114d4d3e6af1b8c91f` |

최종 실행 identity (`fixed/identity.json`):

- source_sha256: `ea3220660cdc920a43f38677bb72d59efaa6eb75475e9672b13bdd434fccfba3`
- elf_sha256: `fd451474caa5f4323b871b41aee37a064e11c5334ce6397d315130f4f9fbae48`
- input_sha256: `f6d099316c3ae6b40bf2bbe8d40c180ed8f6372be175928fd0cba47735ff825b`

최종 Rust blob은 `21e5fd0306e2936671906fd7d04de57cc48b1646`이다. 이후 변경은 이 실행 기록과
gate JSON뿐이다. 소스 manifest의 hash는 실행 시점 전체 파일 목록을 식별하며, 나중에 작성한
보고서까지 같은 bytes였다고 주장하지 않는다.

원 실패 로그 (`fixed/` 밖 `case-1.log`..`case-4.log`, 첫 실행 ELF
`f47ba1ea39c4fb1886ad85073471b884f0c104d86d27acc064f9bc8b95bd9c1e`):

- case-1.log: `a94e1192e71389eb9a55164aa27a565e7737ddc95c3468f91e495029511cf565`
- case-2.log: `0f70be3d2319701086036f629fd119f69d5274e64c89938320aea68e8fd5e0c4`
- case-3.log: `35368092ddfec7c948a4f5c2eac9415c307b7a5c623496e695c4d70a20c96148`
- case-4.log: `b41f4d3650e4618549e2bbb4119db1577362f5e4c655a3c49f0a1420c477849c`

별도 검토자는 최종 Rust diff, 네 exact 로그의 PASS·fault 도달·WAL 추가량, 소스 manifest·실제
ELF·입력 계획·로그 SHA-256의 결속, gate의 신규 blob과 네 selector를 직접 대조했다.
실행 증거 포함 최종 독립 D1 ACCEPT, 필수 0·권고 0이다. 검토자는 시험을 반복 실행하지 않았다.
원 실패와 기존 리뷰는 보존한다. 전송 commit의 fmt·Clippy·문서 검사는 pre-push hook에서 수행하고
로그를 `/workspace/scratch/pr58-wal-review/push.log`에 보존한다.
