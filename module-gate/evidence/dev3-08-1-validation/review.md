# DEV3 08-1 완료 후 독립 검토

2026-11-07 KST · 검토 `/root/independent_review` · 구현·통합 `/root`

기준은 PR #59까지 병합된 `be52fc679deca576ed455dcb698a7f33472a76a8`이다. 제품 검토는
`c52e555c..be52fc67`의 08-1 변경 가운데 현재 leaf 탐색, split·unlink의 준비와 live 효과,
복구 replay와 Base4 소유 검산을 중심으로 수행했다. 저장소의 `AGENTS.md` 및
`devdoc/contracts/dev3-tree-smo.md`를 기준으로 읽기 전용 검토했고, 변경 없는 제품 시험은 재실행하지 않았다.

## 제품 설계·구현 검토

확인한 경로에서는 새 필수 정확성 결함을 발견하지 않았다. 이는 모든 실행 조합의 무결함 인증이 아니다.

- 구조 변경의 준비는 같은 canonical cut에서 후보를 다시 만들고 frame을 비교한다. live unlink의
  `prepare_unlink`와 replay의 `apply_unlink`가 같은 의미 검산을 사용하며, after를 공개하기 전에 바뀐 target을 확인한다.
- Reserve와 Forward 사이에는 RootReserved가 SMO를 막는다. Retained 이후 CLR·Prune은 Job의 옛 leaf hint가
  아니라 현재 anchor에서 RowId를 route한다. split 이후 거래 rollback이 구조 변경을 되돌리지 않는 계약과 일치한다.
- B의 읽기 연산은 read-operation gate 안에서 현재 root에서 경로를 읽고, 복사한 separator 범위와 anchor revision을
  확인한다. leaf 끝의 cursor는 상한 separator로 다음 leaf를 route하며 진행하지 않는 상한을 거절한다.
- LeafUnlink와 RootCollapse는 퇴역 page를 즉시 Free로 만들지 않는다. XTR1 목록의 정확한 incarnation·kind·level,
  빈 retired leaf와 도달/퇴역의 배타성을 검사하고 `reached ⊎ retired == owned`를 유지한다. replay의
  `RetentionFrameObservation.retired`를 비워 기존 CREATE/split provenance를 보존하는 처리는 그 칸이 Free를 뜻한다는
  계약과 맞는다.
- RAM note는 권한이 아닌 hint다. unlink가 현재 경로·leaf의 빈 상태·부채를 재검사하고, FlushRequired/Busy는 note를
  보존한다. root leaf·non-root unary parent를 유지하는 NotNeeded와 최대 8개 note는 명시된 제한이다.

방향성은 디스크 DBMS의 안정적인 RowId, in-place page, WAL 기반 redo-only SMO, 영속 소유 검산과 맞는다.
다음 제한을 일반 tree/제품 인수 완료로 확대하면 안 된다.

- 첫 extent 64 page와 최대 55개 동적 page 한정이다. 퇴역 page의 Free/reuse는 후속 3.03.4이며, 반복 split→rollback→collapse는
  용량을 더 빨리 소진할 수 있다. root 축소는 level 1→0만 지원한다.
- 제품 CREATE의 Catalog Job drain은 일정 10-1에 남아 있어, 그 부채가 남은 table의 자동 split/unlink는 아직 허용되지 않는다.
- UPDATE 값 검사가 증가분 대신 새 cell 전체 공간을 요구하는 07의 기존 편차는 계약에 기록되어 있다. 이번 변경의 신규 결함으로
  분류하거나 해결했다고 표시하지 않는다.
- live canonical WAL 재검증과 그룹별 durable 대기 비용, 전역 gate, page 재사용, SMO를 건너는 cursor/locator 및 overflow는
  각 후속 작업의 의무다. 08-1의 기능 검증으로 확장성·성능·전원 단절 내구성을 입증하지 않는다.

## 후속 판정기 수정 검토

두 Python 파일의 수정과 선택 native 증거는 **D2 ACCEPT, 필수 미해결 0건**이다.

기존 N05 판정은 first-byte cut과 뒤의 DWB 복구 trace를 확인했지만, 복구 전 실제 home page가 checksum-invalid인지 직접
확인하지 않았다. 이미 유효한 XDP1 page의 첫 byte는 새 page와 같을 수 있다. 다만 보존된 기존 N05 실행에서는 data-file hash가
N04 등과 다르므로, 그 실행이 실제 손상을 만들지 않았다고 단정하지 않는다. 수정은 제품 복구 알고리즘이 아니라 판정 근거의 보강이다.

- `check_produce`는 `check_cut`으로 anchor page의 전체 write 요청·정확한 offset·첫 byte 완료를 확인한 뒤, trace가 지정한
  home 파일의 그 offset에서 정확히 16,384 B를 읽는다. 이 호출은 recovery process를 시작하기 전이다.
- 마지막 32 B와 나머지 page의 SHA-256을 비교하는 규칙은 `storage/page.rs`의 `DIGEST_OFFSET = PAGE_BYTES - 32` 및
  `decode_page`와 일치한다. `foundation::codec::content_digest`도 envelope 없는 SHA-256이다. 제품 encoder를 기대값 생성에 쓰지 않는다.
- 새 두 시험은 checksum-valid page와 16,383 B/1 B의 truncated page를 거절하고, pageLSN byte를 바꿔 checksum을 깨뜨린 page를
  수락한다. 전체 `check_produce` 경로를 쓰고 WAL·trace의 나머지 조건을 만족시키므로 새 predicate의 검출력을 직접 검증한다.
- `/workspace/scratch/dev3-08-1-review/red.log`에서 수정 전 위 거절 3개 subtest가 실패했고,
  `green.log`에서 수정 후 두 시험이 모두 통과한 기록을 코드와 대조했다. 검토자가 시험을 재실행하지 않았다.

검토한 Git blob:

| 파일 | blob |
| --- | --- |
| `scripts/check_dev3_unlink_native.py` | `c16d67a925547d622378a2ecaed2daa41f01b35d` |
| `scripts/test_check_dev3_unlink_native.py` | `18787f468c9687debe1ca23792071525bb9ab4e9` |

## 선택 native 증거

`/workspace/scratch/dev3-08-1-review/native/`의 `execution-plan.json`, `results.json`, `complete.json`과
두 사례의 phase별 원 `receipt.tsv`·`effects.tsv`를 직접 읽었다. 시험을 다시 실행하거나 복구된 DB를 produce 시점의
page인 것처럼 다시 검사하지 않았다.

- N05-L/R 두 이력, produce/recover/reenter 각 3개로 총 6 process다. produce의 종료는 각각 SIGABRT(-6),
  recover/reenter는 모두 0이다. controller가 새 checksum 검사를 통과한 뒤 각 recovery를 실행했고 두 사례 모두 PASS로 끝났다.
- produce trace는 anchor page 12(offset 196,608)의 전체 16,384 B write 요청 뒤 첫 1 B 성공과 cut을 기록한다.
  recover trace는 DWB image read 뒤 같은 home offset의 16,384 B 성공 쓰기를 첫 selector publish보다 앞에 기록한다.
- N05-L은 route `3,5`, level 1, 퇴역 page `{13}`, 8행, Structural high-water 168이다.
  N05-R은 route `5`, level 0, 퇴역 page `{13,34}`, 5행, high-water 146이다. 두 경우 모두 recover Q=5,
  빈 Commit ACK Q=6, reenter Q=6 및 handle 0이며 receipt의 source/ELF/input identity가 실행계획과 일치한다.
- 실행계획 controller SHA-256 `2ac0ef99bc5bbdb041e00ef9328c863959e0be67b4549c8e1306ba3e3afaa568`과
  inventory hash를 실제 파일에서 다시 계산해 대조했다. source manifest의 SHA-256
  `6cccc5041f6df7ecefa950b156f51e85b4a3f590db29f6d3d8db11784cb121bf`도 실행계획과 일치한다.
  이 manifest는 실행 당시 작업 트리의 정체성이다. 뒤에 추가된 검토·기록 문서까지 포함한 최종 commit 전체의 실행을 뜻하지 않는다.
- 이 폴더의 전체 보고서와 기존 U4 README의 후속 링크는 검토 범위와 기존/신규 실행을 구분하고 제품 제한을 유지한다.
  실행 증거를 보존한 뒤 임시 build/DB를 삭제해도 새 검증 범위를 확대하지 않는다.

기존 전체 12 이력·38 process의 PASS는 원 source의 증거로만 보존하며, 새 checksum 판정까지 통과한 것으로 소급 표시하지 않는다.
