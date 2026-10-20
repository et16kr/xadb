# DEV3 FILEMAP·DWB selector port

2026-10-21 · **D3-SELECTOR-PORT 계약 독립 검토 수용 / 제품 초안 동결·사용자 요청 중지 / 실행 없음**

[control 형식과 A 선택](dev3-control-format.md) · [구현 소유](../implementation/dev3-implementation.md#wave-2) · [버전 정책](../policies/version-compatibility-policy.json)

control 계약의 A안을 foundation/Linux namespace와 시험용 fault port에서 수신한다. 새 selector의 body·generation·closure 검증, DB 생성/open, FILEMAP growth 장벽, DWB repair/retirement, WAL/복구 예약은 이 작업의 효과가 아니다. 기존 `publish_selector`의 file sync → rename → directory sync 분리는 유지한다.

## 1. Port 이름과 보호

- `Selector::FileMap.as_str()`은 `FILEMAP`, `Selector::Doublewrite.as_str()`은 `DWB`다. 기존 Writer/Current의 bytes와 동작은 유지한다.
- Linux adapter의 selector 예약 이름은 네 이름 모두다. `create_new`, `remove_retired`, `publish_selector`의 temporary 인수로 사용하면 첫 효과 전에 InvalidName/Unchanged다. selector의 `open_existing`·`metadata`는 기존 읽기 의미를 유지한다.
- 선택 destination이나 temporary가 symlink/hardlink/비정규 파일이면 기존 검산과 같은 방식으로 거절한다. selector 이름을 일반 파일 rename 문자열로 우회하는 새 API는 추가하지 않는다.
- `publish_selector`는 prepared 이름을 소비한다. caller가 준비 file을 먼저 sync하고 rename 성공 뒤 directory sync를 수행한다. rename 이후 오류는 CallerMustResolve이며 새 성공/깨끗한 abort를 만들어 주지 않는다.
- 이 enum 확장은 개발 API이며 새 fixture를 사용한다. 기존 내부 포맷·DEV2 frame를 변환하거나 삭제하지 않는다.

## 2. Fault target와 범위

시험 protocol의 `Target::FileMap`/`Doublewrite`는 JSON `file_map`/`doublewrite`다. 둘 다 WriteBefore/WritePartial/WriteAfter/SyncBefore/SyncAfter/RenameBefore/RenameAfter/DirectorySyncBefore/DirectorySyncAfter의9개 boundary만 허용한다. CreateBefore/After·UnlinkBefore/After는 거절한다. 기존 target/관측 판정과 protocol version1을 유지하며, 새 문자열을 모르는 이전 개발 binary의 거절은 호환 보장으로 해석하지 않는다.

| 준비/선택 파일 이름 | write/sync target |
| --- | --- |
| `d3-filemap-select-stage`, `FILEMAP` | FileMap |
| `d3-dwb-select-stage`, `DWB` | Doublewrite |

이 네 exact 이름만 사용한다. 비슷한 prefix, `d3-filemap-0`/`d3-dwb-0`, data page는 여기서 selector로 분류하지 않는다. bytes의 checksum/포맷을 oracle처럼 해석하지 않으며 다른 payload bytes라도 같은 이름의 준비 write cut에 도달한다. 기존 DEV2 AXF1 dispatch가 이 이름의 명시 역할을 덮어쓰지 않는다.

새로 열거나 만든 위 네 파일의 write/sync는 명시 역할을 유지한다. 기존 파일의 Open phase sync를 `RecoveryOldFile`로 분류하는 기존 규칙은 **이 네 이름 이외의 파일**에 유지한다. 이 선택은 FileMap/Doublewrite target에 지정한 복구 중 selector sync도 식별하기 위한 것이다. Target::Any는 기존처럼 모든 효과를 받을 수 있다.

rename의 target은 enum에서 정한다. rename 성공 뒤 같은 request/context의 directory sync는 그 selector의 lineage를 소비한다. 다른 request/context에는 lineage를 넘기지 않는다. rename 후 오류여도 이미 수행한 효과를 숨기지 않는다.

`WritePartial`은 실제1-byte write 뒤의 cut이다. 쓰기 전 단순 오류를 partial 성공으로 기록하지 않는다. 이 adapter의 오류·관측 시험은 process kill, cache loss, DEV3 recoverability 시험을 대신하지 않는다.

WritePartial에서 Continue는 단일 `FileIo::write_at`이 실제1을 반환하는 것과, caller의 `write_all_at` 재시도로 전체 bytes가 완성되는 것을 각각 검사한다. ReturnError는 실제1-byte prefix가 남는다. FileIo의 write/sync 오류는 `io::Error`이며 namespace의 RenameBefore/Unchanged 판정을 그대로 적용하지 않는다. lineage는 마지막 namespace 효과 하나를 연결하므로 각 사례는 rename→directory sync 순서로 실행한다. 여러 rename 뒤 한 번의 sync가 모든 target에 개별 귀속된다고 주장하지 않는다.

## 3. 구현 전 고정 검증

| 시험 | 고정 범위 |
| --- | --- |
| `typed_selectors_replace_reopen_and_preserve_reserved_names` (platform) | 네 enum의 exact 이름, 각2회 prepared write/file sync/rename/directory sync/reopen. 각 selector의 direct create·retire·temporary 거절, 다른 selector 내용 보존. 실제 Linux 임시 directory |
| `typed_selector_preflight_rejects_aliases_without_effects` (platform) | 새2 selector×temporary symlink/hardlink·destination symlink/hardlink·missing temporary의5사례=10. 원본 file/선택 내용 보존 |
| `dev3_selector_cut_matrix_and_wire_names` (protocol) | 새2 target×13 boundary=26: 허용18/거절8. 두 target JSON literal·다른 target witness 거절 |
| `dev3_selector_faults_preserve_target_lineage_and_certainty` (worker) | 새2 selector×rename/dirsync 전후4 boundary×Continue/ReturnError=16. 실제 Linux rename·sync와 UnixStream controller 관측, 요청/파일/target 결합. RenameBefore 오류만Unchanged; 나머지오류CallerMustResolve. 성공 시 정확 새 bytes, rename 전 오류는 old bytes |
| `dev3_selector_write_sync_faults_use_exact_names` (worker) | 새2 stage×write 전/partial/후·sync 전/후5 boundary×Continue/ReturnError=20. actual partial prefix1 확인, 준비 file의 bytes/관측 일치. 별도 exact4이름과 비슷한 이름4개, Open phase 역할 우선순위 확인 |
| `dev3_selector_repeated_unknown_reopens_do_not_cross_targets` (worker) | 새2 selector 각각 rename-after 오류→handle 해제→reopen→다른 새 generation의dirsync-before 오류→reopen. 각 단계 selected bytes 확인, 다른 selector bytes 보존. 서로 다른 request의 lineage 미계승도 검사 |

비슷한 이름4개는 `d3-filemap-0`, `d3-dwb-0`, `d3-filemap-select-stage-extra`, `DWB-extra`로 고정한다.

전체 corpus와 원본 로그·동결 source/hash를 보존한다. 실제 해당 test 이름을 가진 binary가 실행되었는지 확인하며 wrapper의 exit0만으로 테스트 분모를 인정하지 않는다. 별도 담당이 source/contract 및 실제 시험을 독립 검토한다. 기존 DEV0/1·DEV2 current-build 회귀는 후속 통합 source에서 수행한다.

`engine-worker/tests/native_fault.rs`의 기존 target exhaustive match도 새 variant를 수신해야 한다. 기존 DEV2 31-case corpus에는 FileMap/Doublewrite가 없으므로 명시 unsupported arm으로 처리한다. 새 target을 기존 Checkpoint 동작에 매핑하거나 기존 분모를 DEV3 시험으로 바꾸지 않는다. 이 연결은 독립 계약 검토에서 추가로 확인했다.

## 4. 책임과 실패 경계

root가 이 계약을 소유하고 `/root/dev3_review`가 foundation/platform/protocol/worker 변경과 고정 시험을 독점 구현한다. `/root/dev3_model`은 계약 지문 `336a5454f8c5186f6283c381c5f025e72b3e549e588a66376d0dc2931dedc1da`를 독립 검토하여 수용했다. 이 소유 이관은 의미·시험 분모를 바꾸지 않으며 root가 완성 소스를 독립 검토한다. 새 dependency나 OS primitive를 추가하지 않는다. 시험에서 native 호출이 성공하더라도 전체 파일시스템의 power-loss durability나 DEV3 S2/F04/F05 완료로 표시하지 않는다.

## 5. 중지 시점

2026-10-21 사용자 요청으로7개 source/test 초안 작성 뒤 멈췄다. compile·시험·Clippy·fmt·독립 소스 검토는 미실행이며 `artifacts/dev3/selector-port-paused-r1/status.json`에 실행 전 원본을 보존했다. 이 계약의 채택을 제품 실행 PASS로 간주하지 않는다.
