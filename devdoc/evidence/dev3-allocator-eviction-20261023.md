# DEV3 allocator 실제 퇴출·재적재 선택 검증

2026-10-23 · **제한 구현·선택 검증·최종 독립 D1 수용 / 일반 pool·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-allocator-residency.md#eviction-lifecycle) · [실행 색인](dev3-allocator-eviction-20261023.json) · [현재 구현](../implementation/dev3-implementation.md#allocator-eviction)

## 구현과 채택 범위

기준 HEAD `8184857` 뒤 실제 비상주 구간을 추가했다. `evict_allocator`가 clean 등록을
제거하고 `EvictedAllocator`가 원 owner를 독점 대여한다. `load_allocator(slot)`은 실제
selected home·원 allocator 전체 image·descriptor·durable prefix를 검산한 뒤 caller
frame에 다시 등록한다. 같은 슬롯과 다른 슬롯을 지원하며 기존 writer/flush는 정체성으로
이동한 frame을 찾는다. 활성 예약과 원 Bound/Drop·WAL·flush 소유는 유지한다.

inactive directory entry는 세대 tombstone일 뿐 address authority가 아니다. 실패한 읽기의
발급 세대를 보존하고, prior image는 재시도 scratch로 덮지 않는다. source MAX도 다른
슬롯으로 퇴출·재적재할 수 있다. guard 미완료 Drop은 I/O 없이 fence하고, guard 유실 뒤
writer/reload는 MissingTarget으로 거절한다. slot63·stale epoch·foreign origin을 검산한다.

설계 담당 `/root`와 별도 `/root/pool_design_review`가 D2를 수용했다. 독립 정적 리뷰는
EVICT-001(최초 observation의 이전 scratch 사용)과 EVICT-002(load claim의 origin 누락)를
지적했다. prior→before slot 복사와 core/binding 결합을 추가했고 원 지적을 보존한다.
단순 cache relocation은 실제 Empty를 검증하지 못하므로 채택하지 않았다.

## Source별 선택 실행

| source·실행 | 실제 결과·범위 |
| --- | --- |
| r0 | test helper의 invariant lifetime 컴파일 오류. 제품 시험 미실행. 로그와 source 보존 |
| r1 `43a374ce…` | 신규 runtime16·직접 영향받는 기존 runtime7·static5 PASS |
| r1 native | fresh Linux DB1, reserve/flush와 cancel/flush 사이 실제 eviction/load2·read12. 두 frame의 최종 RAM/home/WAL을 기존 독립 literal과 대조. namespace mutation0·파일 inventory 불변 PASS |
| compiler `result.json`의 보존 source | 원본 control1·해제 후 positive1·live guard/split의 정확한 E0499 negative2 PASS, compiler 누적7.32초 |
| r2 `1df975cb…` | DPT 불일치·pending writer 신규2건 PASS. 제품은 r1과 동일하며 완료한 시험을 반복하지 않음 |
| r2 정적 도구 | engine all-targets/all-features Clippy PASS. fmt는 reexport 순서1건 FAIL을 보존하고 포맷만 수정 |
| 후속 포맷 | final fmt PASS. 이후 Rustdoc 설명만 최신 capability에 맞췄으며 runtime을 반복하지 않음 |

총 신규 runtime18·관련 기존7·static5를 각각 원 source의 결과로 보존한다. 기존7개는
이동 가능한 index/eviction epoch를 소비하는 reload5와 Fixed/Status predicate2다.
static5는 guard/claim, Bound 변환, directory, shared dirty, Status readback의 실제 layout을
검산한다. r2는 r1 전체 시험을 다시 실행한 결과가 아니며 final binary 전수 PASS로 표시하지
않는다. native 이력은 process kill·power loss·일반 recovery·commit ACK 시험이 아니다.

기존 source fingerprint·FAIL·완료 시험은 수정하지 않았다. 전체 workspace·하위 wrapper·
무변경 모델·S5를 실행하지 않았다. 문서 자동 검사는 push gate에 남긴다.

## 최종 독립 검토

`/root/pool_design_review`의 최종 판정은 `ACCEPTED_LIMITED_UNIT`이며 남은 차단 지적은
없다. `artifacts/dev3/allocator-eviction-20261023/review/final-d1.json`에 원 판정과
파일별 `(path,status)` coverage를 보존한다. `open-code-review-delegate`의 preview·rule을
사용했으며 OCR은 파일 선택·규칙 해석을, 별도 reviewer는 실제 코드 검토를 맡았다.
preview23개 중 reviewable11개를 모두 검토했다(total11/reviewed11/skipped0/coverage100%).
기본 제외된 Markdown·fixture10개도 직접 검토하고 별도 transport·DEV2 파일2개는 범위 밖으로
명시했다. 공유 색인은 staged DEV3 hunk만 이번 commit에 포함한다.

검토 보존 source는 `e5a1deb82c3dac600636049dfa15b386b55acdd243eac48c0c6126aec39759cd`다.
독립 검토가 source 지문·receipt22개·compiler 로그와 metadata hash를 확인했다. r2 뒤 제품
차이는 Rustdoc header3개와 reexport 순서뿐이다. 최종 판정 뒤 변경은 이 판정·receipt를
기록하는 문서 동기화이며 제품·시험은 바뀌지 않았다. 원 EVICT-001/002·r0 compile FAIL·
r2 fmt FAIL과 S5 NOT_RUN을 유지한다. 상세 artifact hash는 [실행 색인](dev3-allocator-eviction-20261023.json)에 기록한다.

## 자원과 남은 공급

guard296·snapshot200·claim104 bytes, SharedPool delta376≤512다. directory 증가16bytes,
Bound19960/Result20032·변환 peak62000≤65536, generic source-value outer≤4096이다.
shared dirty hot165496/cold155088≤196608을 확인했다. 기존 Status와 flush 예약 한도도
static gate 안에 유지한다. 새 heap/map/page scratch·DB write는 없다. caller frame bytes는
기존 buffer 몫이며 native는2frame, slot63 시험은64frame을 사용한다. 이 값은 source/type
회계이며 OS stack/RSS·전체 engine F05를 인증하지 않는다.

S5의 `eviction_s5_three_slot_history`는32round·64회 퇴출/재적재로 작성·NOT_RUN이다.
서로 다른 주소·일반 allocator membership load 권한·N+1 pressure·반복 Status·clock/
concurrent loading·일반 recovery/READY와 전체 DEV3/S5는 다음 공급·인수 의무다.


병행 COMM 작업이 이 실행 사이 `6fcbc47`로 commit됐다. 후속 transport 변경과 기존 DEV2 리뷰 문서/
등록은 이번 DEV3 commit에 포함하지 않는다. 이전 source snapshot에는 당시 작업 트리의
별도 변경이 함께 보존되어 있으며 실행 선택은 위 DEV3 경로로 한정했다.
