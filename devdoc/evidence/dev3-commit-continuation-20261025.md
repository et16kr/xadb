# DEV3 원 Commit continuation의 공통 소유 공급

2026-10-25 · **공통 계약 D2·독립 D1 한정 수용 / actual Commit·nonempty 복구·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-commit-continuation.md) · [source별 기록](dev3-commit-continuation-20261025.json) · [S5 inventory](../plans/dev3-test-plan.md#commit-continuation-inventory)

`D3-COMMIT-CONTINUATION-001`은 기존 `PageConcurrency`, `WriteTargets`의 원 PinSet, WAL single slot을 사용한다. stage 전에 counter·내용·identity를 확인하고 Intent를 취득하며, page latch→version/admission→pin 순서로 취소한다. Pending은 같은 물리 pin을 보유하고 durable 이후 Drain이 새 content 진입을 막는다. 기존 reader의 counter 반환은 같은 mutex predicate/Condvar로 기다리며 외부 fence는10ms 간격으로 재검사한다. lower-image flusher의 clean 전이 뒤에는 actual DPT stamp를 다시 사용한다. 상위 flusher의 정상 설치 대기는 정확히 `Unchanged + Concurrency(InstallationPending)`만 RAM retry한다.

원 WAL의 첫 File write marker는 stage 완료와 구별된다. original single-slot drainer의 range·file lock·health 확인 뒤 실제 write helper 직전에 기록한다. post-I/O fence 뒤에도 같은 origin과 exact requirement를 확인하여 관측한다. `StagedCommit`, `DurableCommit`, `InstalledCommit`의 의미 생성자는 닫혀 있다. 이 공통 코드로 실제 Commit을 성공시킨 시험은 없으며 실제 terminal producer가 준비·stage-call certainty·Root C/Job/credit/ATT·publication·ACK를 함께 공급해야 한다.

| 검증 | 원 source에 귀속한 결과 |
| --- | --- |
| 정적2 | r1 PASS. Pool N4=11880B/N9=20440B, core common3060B. native recovery peak65464B≤65536B, metadata93768B, whole159304B |
| pre-stage 소유 | r1의 pool3/core1 PASS. r2에서 새 N64 capacity/fence 우선순위1 PASS |
| WAL 물리14사례 | r1의 단일 함수는 case3에서 FAIL. 앞 case0–2는 assertion·cleanup을 완료한 부분 증거다. r2에서는 수정 case3과 미실행4–13만 별도11함수로 실행하여 PASS |
| 컴파일러7 | r2 baseline1·positive1·negative5 PASS. 이중 consume·Owner 재대여·Pending 재사용·raw LSN grant·physical durable 대입을 exact code/span으로 검산 |
| 도구6 | r2 신규6함수 PASS. 불완전 진단·잘못된 span/완료·이전 실행 누락·완료 사례 재선택을 독립 literal로 거절 |
| 품질 | r2 fmt와 engine lib/tests all-features Clippy PASS. 제품 하위 회귀·workspace·push 전용 문서 verifier 실행0 |

r0 source `d7ff04504acfdfe7bf04c1814d0cdd009856507a33724353a7153c9893bec120`는 module 경로와 시험 Result 대여 수명 때문에 build FAIL이었다. r1 source `210ad7d975b4dd625ba30096df09b471353d7408fb3863762a38f24284404ea6`에서는 WAL 부분 쓰기 주입의 기대 호출 수를 잘못 셌다. 원 fixture는192B 요청에서96B를 쓴 다음 남은96B 요청에서 BeforeError를 반환하므로 호출/쓰기2회, offset4096/4192가 맞다. 제품 WAL은 바꾸지 않았고 독립 리뷰 뒤 기대값을 수정했다. r1 fmt의 module 순서 FAIL도 보존했다.

r2 source `a16902ac8e48abbe49263ac86278b44fc32976f3d75a6ef4a6c679bd20052d80`, ELF `6d8d66f942be5d9b9b6085318bd745c13f6c23c0bd9d5354125dfa06f7f35fb0`에서 변경·미실행 memory12만 실행했다. r1 완료 사례를 반복하거나 r2 전체 PASS로 승격하지 않았다. compiler7은24.029초(compiler18.062초), 출력110643B, 당시 artifact226973581B로 동결 한도를 지켰다. 최종 S5에서는 분리된21개 함수(정적2·pool4·core1·WAL14)와 compiler7/tool6을 확정 inventory에 포함한다.

독립 `/root/page_lifetime`은 원 source·diff·receipt에서 pin 이동, cleanup 순서, Abandoned, Drain predicate, recLSN, retry 분류, first-write 경계 및 receipt 생성0을 확인하고 차단 finding 없이 이 한정 단위의 D1을 수용했다. `/root`가 채택했으며 reviewer의 제품 편집·시험 재실행은0이다. actual stage→durable→Drain/resume/install/ACK, lower-flush interleaving과 nonempty native recovery는 **NOT_RUN**이다. B2/B3 producer의 factory·unwind guard·snapshot horizon/Undo pin·F05를 함께 검증하기 전 실제 소비를 활성화하지 않는다.

원 증거는 `artifacts/dev3/commit-common-20261025/`에 보존한다. r0–r2 전체 source와 compiler7 overlay의 모든 member/bytes/hash를 압축본과 대조한 뒤 펼친 사본을 정리했다. 원 실패·진단·metadata·두 ELF는 남겼고, 사용 완료한 전용 target114779020B와 compiler target을 제거했다. 다른 작업 cache는 수정하지 않았다. 정리 뒤 이 단위 보존량은256513114B이며 정리 기록 자체가 추가되면 소량 증가한다. 이 결과는 실제 CREATE·Row INSERT·Commit·재시작 인수나 DEV3 완료를 대신하지 않는다.
