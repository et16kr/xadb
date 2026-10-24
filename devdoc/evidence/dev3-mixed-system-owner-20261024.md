# DEV3 혼합 Status·Free Undo owner — 선택 검증

2026-10-24 · D3-MIXED-SYSTEM-OWNER-001 · **혼합 owner의 제한 수명·현재 제품 이어가기 D1 수용 / 일반 pool·live undo·DEV3/S5 미완료**.
기준 commit은 `b807eb4fae0dabb4c9a18a3c85fddb63245664bc`다. [소유 계약](../contracts/dev3-mixed-system-owner.md),
[source별 색인](dev3-mixed-system-owner-20261024.json), [실행 inventory](../../crates/engine/tests/fixtures/dev3-mixed-system-owner/runtime-manifest.json)를 따른다.
DEV3/S5와 live undo·첫 행 거래는 미완료다. AI 방법론의 완료 소비자는1/3을 유지한다. 이번 제한 단위를 일반 page/live undo 소비자2/3으로 승격하지 않는다.

## 실제 공급과 검토

실제 plan3의 원 3-frame SharedPool에 allocator·Status A·Free Undo B/C를 연결했다. reserve/INIT,
typed read, N+1 pressure, WAL 선행 flush/retirement, exact INIT history load와 원 owner 계속을
한 경로로 사용한다. 별도 acquire_mixed는 canonical system-only 이력과 typed target census로
닫힌 현재-build 복구를 제공한다. 기존 plan2 Status API·기존 read-only 복구 scope는 유지한다.
Free root만 지원하며 live undo record/chunk, 일반 page 종류, Commit ACK/행·index/catalog·READY는
이 단위의 증거에 포함하지 않는다.

[독립 D2](../../artifacts/dev3/mixed-system-owner-20261024/review/design-d2.md)는 실제 역할/원 owner,
sealed 권한, exact history와 기존 F05 상한을 조건으로 수용했다.
[중간 코드 검토](../../artifacts/dev3/mixed-system-owner-20261024/review/common-draft-01.md)의 Prepared escape·
class/control retained 비용·wrapper/return 수명 지적을 수정했다.
[r4 source/F05/input 검토](../../artifacts/dev3/mixed-system-owner-20261024/review/pre-effect-r4.md)는
storage38경로와 raw286개를 대조하고 메모리138개만 최초 실행하도록 수용했다.
이후 [load6](../../artifacts/dev3/mixed-system-owner-20261024/review/pre-effect-r5-load.md)와
[수정 flush2](../../artifacts/dev3/mixed-system-owner-20261024/review/pre-effect-r6-flush.md)를 별도 수용했다.
[최종 독립 D1](../../artifacts/dev3/mixed-system-owner-20261024/review/final-d1.md)은
`ACCEPTED_LIMITED_D1_CURRENT_PRODUCT_CONTINUATION`으로 수용했다. code/fixture/script56와
문서/metadata12의68경로를 전부 검토하고 실제 r7 source 동등성·원 receipt·native 전체 파일을 대조했다.
미해결 차단은 없으며 원 동일 ELF3 미충족·일반 page/live undo·AI 소비자2/3 불승격을 유지한다.
검토자가 제품 시험을 실행한 것으로 표시하지 않는다.

## 원 source와 실패 보존

전체 source/ELF SHA·명령·시간·원 로그는 JSON 색인과 각 immutable export에 있다. no-run 명령은
`cargo test -p xadb-engine --lib --no-run --locked --offline --message-format=json`이다.

| export | source SHA 앞부분 | build/선택 결과 |
| --- | --- | --- |
| r0 |57db7424e470 |compile FAIL: 구문·경로·새 인자와 fixture lifetime 통합 오류 |
| r1 |09a94ef3bb58 |동시 DEV4 lock 변경 중 locked build FAIL; 새 compiler checker6 PASS |
| r2 |f679fdcdcbcf |compile PASS, compiler9+baseline1 PASS, F05 FAIL |
| r3 |d8837776d9d6 |compile PASS, 진단을 보강한 F05 FAIL: plans4256>4096 |
| r4 |ce1af3511dbf |F05 수정 후 static3 PASS, memory136 PASS·flush 입력1 FAIL; Clippy 상수 assertion FAIL |
| r5 |b24b1171bb85 |새 load6 PASS, 새 native checker10 PASS, const block 수정 Clippy PASS |
| r6 |7f4f6d5dde70 |수정 flush2 PASS, native checker 변경3+신규3 PASS, Clippy PASS; native producer PASS·recover FAIL |
| r7 |882118bbec8b |이어가기 harness와 terminal hold 수정 compile/Clippy PASS; 신규 continuation checker8+변경 ledger checker1 PASS; recover/reentry2 PASS |

r2 export의 offline lock 해석은 기존 package identity를 보존했고 당시 원/해석 후 lock SHA가 같았다.
원 workspace lock을 바꾼 복구가 아니다. r2/r3 F05는 cap을 올리지 않고 private prepare와 wrapper
mapping의 수명을 나누어 해결했다. r4 Clippy 실패는 제품 실행 실패가 아니라 새 시험의 상수 assertion
형식이며 const block으로 수정했다. 동시 DEV4 구현은 별도 작업이고 이번 선택 결과를 그 검증으로 쓰지 않는다.
최종 changed Rust39의 format 검사에서38개는 통과하고 그 const block의 줄바꿈만 실패했다.
해당 파일만 rustfmt 후 format-r1을 통과했으며 원 format FAIL도 보존한다. r7의 제품·빌드 관련220경로는
r6와 같고 native 시험의 승인된 차이와 이 test-only 공백 차이를 별도로 대조한다.

## 완료된 메모리 선택과 자원

[r4 memory-a](../../artifacts/dev3/mixed-system-owner-20261024/r4/memory-a/result.json)는7 PASS 후
MO-FLUSH-003에서 실패했다. 첫 DWB 효과의 기대 파일명이 images였으나 실제 protocol은 stage/offset0/
PAGE_BYTES다. State MutexGuard 안의 잘못된 assertion 뒤 Drop panic으로 exit −6가 되었고 원 stdout/
stderr를 보존했다. fault 위치를 옮기지 않고 기대값을 고쳤으며 단언 전에 guard를 해제했다.

[memory-b](../../artifacts/dev3/mixed-system-owner-20261024/r4/memory-b/result.json)는 완료7과 동일 입력
오류의003/004를 제외한129 PASS다. [r5 load](../../artifacts/dev3/mixed-system-owner-20261024/r5/memory-load/result.json)는
home EOF2·SHA-valid 잘못된 WAL/descriptor4의 새6 PASS다.
[r6 flush](../../artifacts/dev3/mixed-system-owner-20261024/r6/memory-flush/result.json)는 실패003 재검증과
미실행004의2 PASS다. 합계 고유144이며 완료 ID의 중복 실행은0이다. 실패 job의 BEGIN만으로 PASS를
등록하지 않고 각 원 stdout의 유일한 BEGIN→PASS와 exact libtest 결과를 연결한다.

source-value3는 r4의 mixed owner, mixed recovery, 직접 변경된 기존 Empty containing layout이다.
native binding60488, activation63592, INIT33144, load61504, pressure62576 및 mixed recovery64690은
각65536 이하다. plans4096, INIT caller696≤768, reader3588≤3648, dirty192312≤196608,
whole257848≤262144를 확인했다. 실제 class/control delta8을 기존 Empty 식에도 반영하여
conversion65424/load64840/pressure65152가 각각65536 안에 든다. 원 source 수치를 새 export의
새 실행으로 표시하지 않으며 실제 ABI stack/RSS 한도로 해석하지 않는다.

## Compiler와 실행 도구

[compiler 결과](../../artifacts/dev3/mixed-system-owner-20261024/compiler-result-r2.json)는 baseline1과
MOC00–08의9 semantic 사례다. 실제 mixed interface의 positive와 core/role/Prepared fabrication,
Prepared/actor 혼동, output/guard/reservation 재대여 거절의 정확 diagnostic/path/line을 확인했다.
기대된 negative exit101과 최초 build FAIL을 구별한다. 이후 동일 권한 경계를 반복하지 않았다.

compiler checker6, memory runner checker의 원6 PASS·후속8 PASS/1 ERROR·실패 입력 수정1+신규1·
원 stderr 보존 신규1을 각각 보존한다. parser에서 selected SKIP을 거절하게 바꾼 뒤 기존 checker
입력을 맞춰 수정했으며 앞서 완료한 무변경 사례는 제외했다. runner 진단은 별도 JSON 필드에 쓰고
subprocess 원 stdout/stderr는 바꾸지 않는다.

native checker는 r5의 최초10과 r6의 변경3+신규3을 구별한다(고유13). 검토에서 home 효과의 mask9를
write count2로 잘못 비교한 오류, live DB를 immutable retention에 넣은 오류, 균형만 확인한 ledger,
fixture 수285 오기를 수정했다. actual raw286, exact ledger24344416+16384=24360800·protected23377760,
원 ancestor failure 로그와 snapshot 보존을 검산했다. 이 checker PASS는 native 제품 실행 PASS가 아니다.

## Native 원 실패와 이어가기

[첫 native 실행](../../artifacts/dev3/mixed-system-owner-20261024/r6/native-a/result.json)의 producer PID147421은
PASS다. 전체 파일·원 A/B load·allocator/C dirty·독립 raw WAL을 확인했다. recover PID147423은 실제
복구 뒤 시험의 terminal hold 기대값0에서14를 받아 실패했다. 원 파일 효과는 보존된 snapshot에서
allocator+C 쓰기2, WAL 쓰기0, canonical home4장과 DWB generation13/batch4를 보이지만, 필수 dump와
완료 marker가 없으므로 **원 recover FAIL을 PASS로 재분류하지 않는다**. reentry는 미실행이다.

terminal hold는 Finished inventory/remaining0과 별개인 완료 주소 union14다. native 입력 검토에서
이를 놓친 사실과 [후속 독립 D2](../../artifacts/dev3/mixed-system-owner-20261024/review/native-continuation-d2.md)를
보존한다. 원 producer를 반복하지 않고 검증된 producer snapshot의 새 inode 복제본에서 실패 recovery와
미실행 reentry만 이어간다. 제품·실제 wrapper·빌드 설정 동일성을 확인하고 승인된 시험/harness 차이를
기록해야 한다. 원 execution.tsv나 producer PASS의 source를 고치지 않는다.

원 동일 ELF 세 phase PASS 요건은 미충족으로 남긴다. 후속의 실제 증거는 “r6 실제 producer 입력 →
제품 소스가 같은 후속 build의 복구·재진입”으로 구별한다. 원6.206148767초와 immutable evidence
71,393,368bytes를 후속의30초/128MiB 합계에 포함한다. [r7 동결 실행 전 검토](../../artifacts/dev3/mixed-system-owner-20261024/review/pre-effect-r7-continuation.md)를
수용한 뒤 [이어가기 결과](../../artifacts/dev3/mixed-system-owner-20261024/r7/native-continuation-a/result.json)는
recover PID159102와 reentry PID159104가 모두 PASS다. 원 producer PID147421·실패 PID147423와도
서로 다름을 [별도 읽기 검산](../../artifacts/dev3/mixed-system-owner-20261024/r7/native-continuation-pid-audit.json)으로
확인했다. 새 두 phase의 source882118bbec8b/ELF3856f83d7706은 같으며 원 producer identity는 별도 보존한다.

원 producer의15파일22,247,264bytes를 fsync한 새 inode로 복사했고, 원 실패 DB·immutable79파일은
변경하지 않았다. 복구는 canonical home4장·typed Status/Undo census·terminal hold14를 검산하고
allocator/C home 쓰기2·WAL 쓰기0을 확인했다. 재진입은 home 쓰기0이며 최종 전체17파일은 독립
whole-file oracle와 일치한다. 원 실패 시간을 포함한 누적13.747716977초, immutable 합120,619,878bytes,
log 합155,538bytes로 각각30초/128MiB/8MiB 안이다. 최종 snapshot은 동일한 reentry/disk를 참조하며
원 실패 기록을 덮어쓰거나 새 producer PASS를 만들지 않았다. 최종 독립 D1은 이 기능적 이어가기 범위만 수용했다.

## 개발 방법론과 남은 인수

[AI 효율 계획](../plans/dev3-ai-throughput-improvement.md)에 따라 root가 공유 상태·통합·실행을 맡고,
별도 agent가 recovery·독립 입력·native 검산기와 독립 D2/D1을 병렬 준비했다. 최초 효과 전에
source/ELF/F05/selector를 동결하고 원 실패와 변경한 입력만 이어 검증했다. compiler 통합 오류,
F05 초과, fixture/runner 기대값 오류, 독립 검토 누락을 각각 재작업으로 남긴다. commit 수나 agent 수를
속도 이득으로 세지 않으며 효과 비교는 NOT_RUN이다. 시간은 원 receipt·timing 기록에서만 가져온다.

S5의 profile2/3 실제 owner 확대2case는 작성·ignored/NOT_RUN이다. 별도 진단 출력 MO-LAYOUT-001은
인수에 필요한 세 F05 함수로 대체해 인수 수에 넣지 않으며 실행하지 않았다. 최종 DEV3 전수 인수,
일반 page·live undo·첫 행 거래·일반 recovery/READY의 의무는 유지한다. 문서 검증은 pre-push gate에
남겼고 이 작업에서 실행하지 않았으며 push도 하지 않았다.
