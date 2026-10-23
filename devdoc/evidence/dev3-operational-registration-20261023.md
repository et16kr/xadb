# DEV3 일반 page 등록 코어 P1 선택 검증

2026-10-23 · **P1 구현·선택 검증·최종 독립 D1 수용 / P2·P3·일반 pool·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-registration) · [source·receipt 색인](dev3-operational-registration-20261023.json)

`22e7724` 뒤 기존 SharedPool의 유일한 directory를 슬롯별 generation과
OnceLock request로 바꿨다. Fixed provenance와 allocator resident bit/tombstone은 유지하고,
closed Status INIT도 그 슬롯의 publication을 사용한다. 기존 별도 publication cell은 제거했다.
generation/clear/replace는 독점 단계만 수행하며 shared publication은 준비된 generation을
바꿀 수 없다. 잘못된 generation은 cell을 초기화하지 않고 거절한다. 새 heap·mutex·page
scratch·DB write와 병행 우회 reader는 추가하지 않았다.

이는 일반 page 수명의 P1 공급이다. P2 consuming owner·실제 반복 INIT, P3 현재 allocator
membership 기반 load와 allocator/Status A/B의3주소·2frame 이력은 아직 공급하지 않았다.
반복 live Status 변경·batch8/64·pressure·recovery/READY와 전체 DEV3/S5도 미완료다.

## source와 변경 범위 선택

| source·실행 | 실제 결과 |
| --- | --- |
| r0 `e09be2e6…` | 제거된 전역 publication을 참조하던 test hook의 컴파일 오류. runtime 미실행·원 로그 보존 |
| r1 `ac56a8e5…` static | directory·allocator 변환·shared dirty·Status INIT CF17-01·Status flush RF01의5개 source/type gate PASS |
| r1 runtime | 신규 슬롯3·변경 기존2·직접 영향받는 기존15의 총20 selector PASS. CF06-08·CF12-01/02·CF17-02/03·RF02의 실제 도달 marker 확인 |
| r1 Linux | allocator 실제 eviction/load2와 Status INIT/두 target flush의2개 native selector PASS. 각 fresh DB·기존 독립 literal oracle 사용 |
| r1 정적 도구 | engine all-targets/all-features Clippy·workspace fmt PASS. 시험 case 실행과 구별 |

변경 기존2는 invalid fixed request 관측과 CF06-08이다. shared publication에 generation0을
주입하는 이전 내부 표현이 없어져 CF06-08은 준비된 generation1에 wrong-kind request를
주입한다. 새 슬롯 시험은 generation0/다른 generation이 publication 전에 거절되는지 별도로
검증한다. 동시 publication의 단일 승자, 슬롯별 독립성, 원 pin/LSN/세대/retirement 소비도
검산한다. C1–C5의 미공개 reader와 실제 publication 뒤 reader 일정을 원 선택 범위로 실행했다.

실행 전에 exact selector·환경·source·ELF를 보존했고 독립 F05 수용 뒤 runtime을 실행했다.
선택 초안의 CF17 ID 오기는 실행 전에 CF17-02/03으로 정정했으며 미실행 초안도 보존한다.
완료된 이전 source 결과를 수정하지 않았고 이번 변경과 직접 관계없는 하위 wrapper·전체
workspace·S5·compiler fixture는 실행하지 않았다. native 이력은 crash/power-loss나 일반
recovery/commit ACK 증거가 아니다. 문서 검사는 push 전용이다.

## 자원과 독립 검토

Slot80bytes는 종전 Option<Registration>72보다8bytes 크다. 전역 cell 제거를 포함한
directory 증가는432bytes, SharedPool delta808≤512+64×8=1024다. 새 전체 한도를 만들지
않고 allocator 변환63296≤65536, shared dirty hot165928/cold157680≤196608, Status
hot194056/cold159232≤196608과 Status+flush259592≤262144를 유지한다. flush metadata
hot64064/cold48680·adapter 최대49856≤65536다. source/type 회계이며 OS stack/RSS나
전체 engine F05를 인증하지 않는다.

`/root/pool_design_review`가 D2·실행 전 자원 gate와 최종 D1을 P1 범위로 수용했다.
미해결 지적은 없다. 사용자 요청에 따라 OCR delegate의 preview/rule을 적용했으며,
최종 preview21개 중 검토 대상11개 전수 검토·skip0·coverage100%다. 제외10개 중 관련
Markdown6개는 직접 검토했고 병행 작업4개는 범위 밖으로 명시했다. 중간 preview에서
잘못 노출된 COMM script2개도 범위 밖으로 기록하고 원 출력을 보존했다.

독립 검토는 제품·시험8개 파일과 r1 source, ELF, receipt40개의 hash 및 runtime22개
원 로그·필수 marker를 확인했다. stage17개와 공유 색인의 부분 stage도 확인했다.
제품·시험 재실행은 없었으며 마지막 수정은 수용 상태와 review artifact hash 기록이다.
`review/final-d1.json`·`review/final-d1.md`와 원 OCR 출력의 경로·hash는
[source·receipt 색인](dev3-operational-registration-20261023.json)에 보존했다.

병행 COMM/transport와 DEV2 리뷰 문서·색인 변경은 이번 단위의 검토·commit 범위 밖이다.
보존 source에는 당시 작업 트리의 별도 변경도 포함되며 실행은 위 engine selector에 한정했다.
