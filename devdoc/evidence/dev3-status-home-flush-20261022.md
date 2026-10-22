# DEV3 Status home flush 선택 인수

2026-10-22 · **필수 선택·native 실행 완료 · 최종 독립 D1 수용 · 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-status-home-flush.md) · [source별 실행 색인](dev3-status-home-flush-20261022.json) · [현재 상태](../implementation/dev3-implementation.md#status-home-flush)

실제 INIT가 공개한 Status와 allocator를 각각 whole batch1로 flush한다. 기존 bound를 소비하는 owner가 하나의 workspace·capture·sealed authority·retired proof를 보유하고, WAL force→DWB→home→whole retirement→조건부 RAM finalize를 연결한다. Status home은 zero 또는 정확한 현재 image만 허용한다. 이 제한 단위는 일반 거래·recovery/reopen/READY 인수가 아니다.

## 변경과 실제 선택 범위

기준 commit은 `d5c26281d239a40b414184360698d06594ee9487`이다. consuming adapter, published Status authority, Status image/home/FILEMAP 검산, shared prepared/retired association 및 두 target의 순차 flush가 새 코드다. 실제 writer/flusher 경합·redirty·retirement 뒤 RAM retry·실패 원인과 bytes 보존·반환된 원 owner 재사용을 새 시험으로 검산했다. 기존 .a·allocator-only·DEV0/1/2·workspace wrapper를 반복하지 않았다.

| 범위 | 실행·증거 |
| --- | --- |
| 필수43 | Rust42와 Python RF03 witness1. 엄격 성공 그룹42행, 원 실패 그룹 안에서 먼저 완료된 BA01 한 행의 독립 파생 감사 |
| 자원 | RF01의 실제77개 layout/cap 및 authority return guard, RF02의 실제 generic admission과 반환 owner 재사용 |
| compiler | BC00–05 semantic6 PASS. 원 BC00 입력 실패와 별도 infrastructure baseline2회 보존; 실제 compiler 호출9회 |
| native | profile1/2/3 새 Linux DB3 PASS. AS·SA·cancel→allocator preclean→INIT→SA, whole batch7의 flush217효과와 WAL16효과 |
| checker | compiler4/literal4/runtime4/native4 unique16 PASS. native 최종 owner 관측을 추가한 변경 method1만 후속 실행 |
| 정적 검사 | 최종 native assertion source의 fmt와 engine all-targets/all-features Clippy PASS. 테스트 실행·문서 verifier와 구분 |
| S5 확대 | fault564+history24 추가588 작성·NOT_RUN. mandatory와 exact동일6 alias를 제외한 unique runtime/witness631 |

RF01 source는 `abeb2525…`, BN6와 BA01 완료 prefix는 `8ce135f8…`, 나머지35는 `77abf3ae…`, native3는 `381499a7…`다. compiler 성공6은 `abeb2525…`이며 각 source/ELF/입력/명령/원시 로그 hash는 JSON에 보존했다. 이후 source 변경을 근거로 이전 완료 시험을 반복하거나 현재 source의 전체 PASS로 표현하지 않는다.

실패 원본을 포함한 필수 실행은43.527702초/120초, compiler는14.306275초/30초다. native 합계8.555175초이며 각 DB30초/24MiB와 총72MiB 이내다. build/setup과 checker 실행은 별도 기록한다. native 실제 DB 합계66,427,744 bytes, wrapped File peak2/final0, source 경계 DB FD baseline3/peak4/final0이다. `/proc` sampler와 exact 경계 관측을 구별하며 연속 physical peak나 성능 인수로 주장하지 않는다.

## 실패 원본과 좁은 수정

| 원본 | 원인·후속 처리 |
| --- | --- |
| compile r0 | 새 fixture의 closure/lifetime/associated-type 및 test ACK의 구체 port 타입 오류12. 수정 후 compile-only이며 제품 PASS로 세지 않음 |
| compiler r0 | BC00 external-core positive가 실제2-frame array 대신 slice를 전달해 E0308. matching BC00/BC04 입력의 배열 타입만 수정; BC00 재선택과 미실행 BC01–05 실행. 제품 visibility 불변 |
| mandatory r0 | BN6 완료 후 BA01의 두 variant와 CASE가 완료됐고 BA02 foreground 다음 Maintenance admission에서 Capacity. initializer의 단일 Maintenance 슬롯이 살아 있던 시험 오류 |
| mandatory r1 | 같은 core의 Maintenance rejection 입력에서만 initializer owner를 반환하도록 수정. BA02와 미도달34행만 선택, BA01·BN6·RF01 반복 없음 |
| static r0 | byte carrier8개 식의 manual_bits 오인, test mut/borrow 및 helper 인수 수 경고. 동일 byte 크기 표현·test binding·참조 문맥 묶음으로 수정 |
| native 입력 r0 | 실행 전 독립 검토에서 최종 retained owner의 직접 관측 누락 발견. 실제9개 Option/phase/fence를 assert→TSV→독립 exact oracle에 연결; 새 native 실행은 수정 입력에서만 수행 |

원 mandatory r0의 BA libtest 그룹은 계속 FAIL이다. BA01은 두 호출의 실제 assertions와 최종 handle/writer 반환 뒤 나온 marker, 다음 BA02 BEGIN과 실패 위치를 독립 감사한 **case prefix의 파생 수용**이다. BA02의 중단된 remainder나 미도달 case를 그 원 실행의 PASS로 표시하지 않았다. RF03 valid/known-bad12는 독립 witness checker이며 제품 mutant12회가 아니다.

## 독립 입력·소유·자원 증거

독립 literal은 명시 offset과 stdlib struct/hashlib로 생성했다. 실제 파일 전부, WAL prefix/frames, 두 home, DWB image/control/selector, RAM page/descriptor, original inode와 namespace census를 비교했다. native에서는 각 batch31효과와 전체 FD 경계 순서, 마지막 Status/base obligation·batch/terminal 및 flush capture/retired/authority/Ready 상태를 직접 읽어 검산했다.

실제 old/new bound18976/20376, flush control1400, authority144 bytes다. adapter phase27168/32768/47608/48944, metadata hot64064/cold48680, normal generic deep25088은 모두65536 이하다. workspace164000/4194304, Status codec3272≤3840, image/home phase3720/3776, encode6673/execute5329≤8192다. upper authority phase5888 및 outer capture/finalize join8080의 source 수명/return carrier와 실제 typed guard를 함께 검토했다. 기존 dirty196608/flush65536 예약을 증액하지 않았으며 physical stack/RSS 측정이 아니다.

RF02의 individual File8184/8192도 owner 합산식이 먼저 초과하므로 정상 Capacity 거절이다. 실행 전 DWARF 관측으로 도달 불가능한 positive 기대를 고쳤고, 별도 실제 File aggregate 및 owner aggregate의 fitting/넘침 경계를 검산했다. 거절 뒤 exact original bound로 reserve/cancel이 가능하고, Busy 뒤 workspace unlock과 conversion 재시도도 검산했다.

## 남은 인수

일반 pool·batch8/64·다중 file·거래 Status 변경·tree/index/catalog·undo/commit·recovery/reopen/READY와 DEV3 최종 S5 전수는 남아 있다. S5 추가588은 실행하지 않았다. 문서 verifier는 push gate 전용이므로 이번 편집·commit 중 실행하지 않았다.
