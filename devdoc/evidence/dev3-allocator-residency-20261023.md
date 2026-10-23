# DEV3 allocator 동일 주소 reload 선택 검증

2026-10-23 · **제한 단위 검증 완료 / 최종 독립 D1 수용 / 일반 pool·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-allocator-residency.md) · [실제 구현](../implementation/dev3-implementation.md#allocator-residency) · [source별 실행 색인](dev3-allocator-residency-20261023.json)

기준 `d31cfb9`에서 원 Bound를 보존하는 consuming allocator-only adapter와 실제 selected home reload를 구현했다. 원 descriptor·active reservation·WAL/flush owner를 유지하며 같은 주소의 resident generation을 checked 증가시킨다. 계약·제품·통합 `/root`, native/compiler/checker `/root/allocator_reload_acceptance`, 별도 읽기 검토 `/root/pool_design_review`다.

## source와 실제 선택

원 source·ELF·receipt·inventory·실패 로그와 각 export는 `artifacts/dev3/allocator-residency-20261023/`에 보존했다. r9 실행 guard가 다른 작업의 source 변경을 발견해 product runtime 전에 중단했다. 그 변경은 수정하지 않았고 보존 export에서 r10을 새 빌드했다. 진행 중 다른 작업 commit `f4fca8f`가 main에 추가됐으며 그 변경의 인수 결과로 이 보고서를 사용하지 않는다.

| phase | source SHA-256 | ELF SHA-256 | 실제 증거 |
| --- | --- | --- | --- |
| r9 | `6471b880686ab374ec86e29ce5c4e9fa5770c9d2c3514b6884f75a01d3306f40` | `e7867658ce68c5d6683495549b0b843a05b61f1b0da774564921426a23e29e59` | static3 PASS. 제품 I/O0 |
| r10 | r9와 동일 | `3c9f307a48413b84777a9126c2fa86449ef5c115fb93d58a06031de656bfed07` | runtime15 PASS·1 FAIL·뒤6 NOT_RUN, native1·compiler3·checker10 PASS |
| r11 | `3b0985674860935fefea73798f85051fbdedb2ce198087b28d052d7b79d341ab` | `15b58855ebecd4e2f8e123e55efeeab4de8fd5c6bdd4361b6332da1bcd3581b7` | 수정2+미실행 일부:4 PASS·1 FAIL·뒤3 NOT_RUN |
| r12 | `f53b5e0ec5ef6ed501fd036e4913a3ff57eafea1aa012f0019d38efd382d6e1e` | `8c167a31493f6e9d5a6ea033efb912b56019c215f6b7f4172245f3975612072d` | 실패1+미실행3 PASS, engine Clippy·fmt PASS |

r10→r11→r12의 source 차이는 새 allocator_residency_tests.rs의 fault 좌표·기대값·불필요한 drop 제거뿐이다. 제품 로직은 r9 이후 변경하지 않았다. 같은 source의 재빌드도 다른 ELF로 구별한다. 문서 변경은 source fingerprint 밖이다.

최종 선택은 신규 runtime19+Fixed mode 직접 소비2의 고유21개, static4개다. 마지막 성공 source를 각 exact selector에 연결했으며21개 전체를 마지막 ELF에서 다시 실행한 결과가 아니다. native/compiler/checker도 r10 원 source 범위다. 고정 등록 direct2는 closed publication·invalid fixed publication, static supplement1은 수정된 registration_layout_receipt다. 이미 통과한 static3와 무변경 하위 wrapper·전체 leaf·DEV0/1/2는 반복하지 않았다.

## 검산 결과와 자원

새 runtime은 reserve→flush→reload→cancel→flush→reload의 generation1→2→3과 active descriptor·예약 PageRef·high-water를 보존했다. 이어 gen3의 새 reserve/cancel→flush가 work high-water2·LSN4로 진행한다. pristine0, dirty/pin/uncaptured claim/DPT·requirement 불일치/retained installation/terminal/retired pending/MAX의 사전 gate, 실제 read6·open4 각 fault의 원 cause와 명시 재시도, malformed/wrong identity/future LSN/valid same-LSN wrong bytes·actual older home, poison3·old seal·원 Bound 반환·namespace8분류·Unknown read·실제 HOME 중 공통 fence·open/HOME unwind2를 확인했다.

native1은 Linux fresh DB 하나에서 두 flush batch·두 reload를 실행했다. independent literal full RAM/home/WAL/descriptor와 inode·길이·hash를 비교했고 reload마다 actual HOME16384 read1+control read5, mutation0을 확인했다. flush logical62효과와 active reservation `PageRef(4294967297,1,2)`를 보존했다. compiler는 실제 baseline·release positive·live split E0499 정확8행 negative다. checker10은 이 native/compiler 판정의 known-bad다.

| 항목 | 실제 값·상한 |
| --- | --- |
| metadata conversion / outer | 61952≤65536 / 2400≤4096 |
| reload source 상한 | conservative60416≤65536; actual Linux26816, padded File4096은51104 |
| File·Reader 경계 | native48/400, mock32/384, File4096/Reader4448 수용, File4104 거절. File8200/Reader8552는 두 상한 동시 초과이며 독립 Reader 분기 검출 아님 |
| shared dirty / Status | pool delta360; shared hot165480/cold154992; Status reader3608≤3648·hot193608/cold157440; hot+flush=259144≤262144 |
| native 시간·DB | 3.019초≤30초, DB22111648·replacement22115744≤24MiB, WAL46496·고정 소유22073344bytes |
| native file·FD | tracked File peak3/final0, DB FD baseline3·peak5·final0. sampler와 경계 관측이며 연속 OS peak 보장 아님 |
| compiler·checker | compiler3 합계5.704초≤30초; checker10 subprocess0.064초 |

source 수명/sizeof 회계와 caller heap payload5373952bytes를 구별한다. OS stack/RSS/OOM·전역 disk 선확보·power-loss·PF09 성능 인수 결과가 아니다. public DBMS와 동일 구현·SQL 의미를 주장하지 않는다.

## 원 실패·보정·남은 인수

초기 compile-r0/build-r1–4의 import·private FileId·fixture macro/borrow·오류 getter·scoped closure 문제, build-r8의 확대 테스트 count 타입 오류를 보존했다. build-r5/6/7/9와 export-r10/11/12는 no-run 빌드다. 원 compile 실패의 미보존 source 지문을 사후 추정해 source 결합 PASS로 만들지 않는다.

r10 read-fault 실패는 mock이 open과 read를 함께 계수하는데 read만 센다고 가정한 테스트 오류다. 실제10site(4open+6read)와 typed namespace/read를 독립 고정했고 HOME 위치9를 검산한다. r10 common-fence PASS는 FILEMAP root 경계의 원 관측으로만 유지하고 실제 HOME 경계는 r11에서 확인했다. r11 source-mismatch 실패는 slot directory가 header128에 있는데 footer 쪽에서 찾은 fixture 오류다. r12가 offset128로 고쳐 full valid same-LSN mismatch를 확인했다. r11 Clippy의 drop_non_drop2건은 불필요한 drop을 제거했고 r12 정적 검사로 해결했다. 실패와 뒤 NOT_RUN을 새 PASS로 덮지 않았다.

D2 설계·F05 숫자 gate는 별도 검토 수용 상태다. 최종 제품 D1은 본문·정확 source별 결과·소유19개 파일과 export 일치 검토 후 수용했다. 미해결 필수 수정은 없다. 최종 S5의32round/64reload 혼합-count 이력은 작성·NOT_RUN이다. 일반 directory·Empty/다른 주소의 load/eviction·N+1 pressure·반복 Status·tree/undo/행 거래·일반 recovery/READY·전체 DEV3/S5는 미완료다. 이번 문서 verifier·push는 실행하지 않았다.
