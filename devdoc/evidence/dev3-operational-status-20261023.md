# DEV3 반복 Status INIT의 consuming owner 선택 검증

2026-10-23 · **P2 live owner 구현·선택 검증·최종 독립 D1 수용 / 반복 sequence 복구·P3·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-task-05-buffer-flush.md#operational-status-p2) · [source·receipt 색인](dev3-operational-status-20261023.json)

`3f7b6f5` 뒤 원 `BoundStatusWithFlush`를 통째로 소비하는 `OperationalStatus`를
추가했다. 원 WAL·descriptor·workspace·Drop을 그대로 보존하며 Before 거절은 exact
owner를 돌려준다. 독점 `prepare_next_status`는 clean/빈 DPT·pin0·claim0·flush Ready에서
현재 Status를 퇴출하고 다음 generation을 준비한다. split·선형 reservation을 유지한 동안
독점 준비는 허용되지 않는다. 다음 reserve/cancel/INIT는 allocator의 기존 Allocated Status
survivor와 work high-water를 보존한다. 고정 allocator0·payload1의 두 물리 frame만 쓴다.

새 trusted population/terminal은 descriptor bytes로 선택되지 않는다. 기존 closed mode는
page1/gen1·한 INIT 제한을 유지한다. 새 mode는 첫 extent의 Status 역할만 허용하며 각각의
allocator/Status LSN을 검산한다. INIT claim에 두 generation을 보존하고 admission,
preflight, install, release, finish에서 오래된 권한이 새 공개·cleanup을 승인하지 못하게 한다.

이 커밋 단위는 **live owner 공급**이다. 반복 sequence를 성공 복구하는 current-build
경로는 아직 없다. 기존 closed recovery PASS를 새 sequence에 전용하지 않는다. 해당 복구의
fresh producer→새 process reopen·partial-prefix/fault cut를 공급하기 전 P2/P3 durable
integration은 미완료다. 현재 allocator membership 기반 P3 load·3주소/2frame eviction/reload,
반복 live Status 변경·batch8/64·pressure·일반 recovery/READY와 DEV3/S5도 필수 잔여다.

## source와 실제 실행

보존 root는 `artifacts/dev3/operational-status-20261023`이다. source 전체에는 별도
COMM/DEV2 작업도 포함될 수 있으며 아래 실행은 변경된 engine 경로에 한정한다.

| source·실행 | 실제 결과 |
| --- | --- |
| r0 `9510eaab…` | 제품 컴파일 PASS, runtime 미실행 |
| r1 `7e65024c…` | 새 테스트의 retained Result borrow로 compile FAIL, runtime 미실행·원 로그 보존 |
| r2 `120cd4f5…` | compile PASS; static6·actual owner runtime2 PASS |
| r3 `e8630984…` | guard 삽입 위치와 native test owner Drop 순서의 compile FAIL, runtime 미실행·원 로그 보존 |
| r4 `cb7c64c8…` | compile PASS; 신규 오류·reader 의미 runtime9, fresh Linux native1 PASS |
| r5 `486dbc0d…` | compile PASS; 실제 stage 뒤 stale install/completion runtime2와 직접 영향받는 closed CF15-01/02 selector1 PASS; Clippy FAIL 원본 보존 |
| r6 `4894c028…` | 무동작 unit식·no-Drop 테스트 view 정리, compile·engine Clippy/fmt PASS. runtime 재실행 없음 |
| r7 `e6ad54e7…` | 새 producer budget/wrong-kind 사례1 PASS, compile·engine Clippy/fmt PASS. 이전 runtime 미반복 |

r2 이력은 A INIT→두 home flush→A 퇴출→B reserve/cancel→B reserve/INIT이다.
A membership·home bytes와 B 재예약 incarnation3, payload generation2, allocator counter6,
전체 descriptor·WAL6 frames·home3 pages를 독립 literal 입력과 비교한다. dirty conversion의
Before 반환 뒤 동일 owner를 flush하고 변환하는 경로도 확인한다.

r4는 dirty/빈 슬롯/MAX, 실제 `mem::forget` pin과 INIT claim, 실제 retired-pending-finalize,
stale admission/release, preflight generation, trusted budget/genesis/terminal/survivor,
독립 allocator LSN6·A Status LSN2를 검사한다. 마지막 reader 사례는 literal 입력의 의미
검증이며 실제 A reload 증거는 아니다. Linux worker는 새 DB에서 같은 두 INIT와 B cancel을
실행하고 실제 home/WAL 전체 bytes, namespace inventory, FD·Drop을 기록한다. crash/reopen
시험으로 세지 않는다. 제품 비교는 profile1만 실행했으며 profile2/3 literal은 입력만 생성했다. r5는 실제 B stage authority를 만든 뒤 install key 또는 completion의
세대만 변조하며 현재 pin/claim·image·descriptor·DPT·publication 불변과 fence를 검사한다.

완료된 r2/r4 runtime은 반복하지 않았다. static6은 r2의 원 source/type 결과다. r4의
scope/budget·preflight 검산은 구조체·scratch·codec·반환 carrier를 바꾸지 않으며, r5는
시험 seam/body와 주석만 추가했다. 별도 검토자가 diff로 자원 gate를 수용했다. 현재 source
지문 변경을 이유로 이전 static 또는 하위 runtime을 다시 실행하지 않았다.

## 자원과 검토

r2 source/type 측정: consuming conversion64768≤65536, 원/새 bound20864,
SharedPool delta824≤1024, InitClaimRecord208, claim slot216, INIT pin set464≤512,
key240≤256, write544≤640다. dirty hot165944≤196608, Status hot194088≤196608,
Status+flush259624≤262144, flush metadata hot64064≤65536을 유지한다. 이는 보수적인
source/type 회계이며 OS stack/RSS 측정이나 전체 engine F05 인증이 아니다.

`/root/pool_design_review`의 D2·r2 실행 전 gate와 r4/r5 delta gate 원문을 보존한다.
리뷰로 드러난 preflight의 local identity/key 재검산과 Status population/budget·genesis
terminal 모순 거절을 보완했다. 최종 D1은 **P2 live owner 한정 수용**이며 차단 지적0이다. OCR delegate 선정23개 전수
검토·skip0·coverage100%이며, 전체 preview112 중 제외89개도 관련85개 직접/입력 무결성
검토와 무관4개 제외로 구분했다. staged108개와 r7 제품·시험 byte 일치를 확인했다.
원 receipt100개 hash·source·ELF·선택 로그를 대조했으며 검토자가 시험을 재실행하지 않았다.
원 D1·coverage·CF15 로그 marker 해석 보완과 gate 기록은 JSON 색인에 연결한다.
r7의 새 사례는 producer가 잘못된 trusted budget에서 frame/after 출력에 손대기 전에
거절하는지와 codec-valid RowLeaf survivor를 거절하는지 확인했다.

새 compiler input은 원본 control1, 양성1, 음성4다. 양성은 released split·Before exact
bound·최종 Drop 뒤 scratch 반환을, 음성은 old bound 이동·live split/reservation·원 scratch
재접근 거절을 다룬다. checker 입력/선택/변조 자체 시험4 PASS 원 source를 따로 보존한다.
compiler semantic5+control1은10.61초에 PASS이며 현재 소유 API와 원 scratch lifetime을 확인했다.
Clippy 최초 FAIL은 보존하고 무동작 정리 뒤 r6 engine all-targets/all-features Clippy와
engine fmt가 PASS했다. 이 정리는 완료 runtime 재실행 근거로 사용하지 않았다.

문서 자동 검사는 push 전용이므로 실행하지 않는다. 별도 COMM/transport·DEV2 문서와 공유
색인의 해당 변경은 이 단위 commit 범위 밖이다. skipped·중단·미실행은 PASS가 아니다.
