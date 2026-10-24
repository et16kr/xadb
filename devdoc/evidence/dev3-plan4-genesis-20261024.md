# DEV3 plan4 정리 큐·빈 Catalog 최초 설치 — 선택 검증

2026-10-24 · **독립 D2 수용 / 필수304·compiler9 PASS / 최종 독립 D1 제한 수용**.
기준 `39fe1cb0546166ea664ac4dd192c552105282126`, 통합·실행 `/root`,
제품·독립 시험 준비 `/root/mixed_history_implementation`·`/root/mixed_owner_test_preparation`,
독립 검토 `/root/dev3_independent_review`.
[소유 계약](../contracts/dev3-plan4-genesis.md),
[중앙 시험 목록](../plans/dev3-test-plan.md#plan4-genesis-followup),
[source별 원본 색인](dev3-plan4-genesis-20261024.json)을 따른다.

같은 FreshStorageSession·maintenance·DWB로 첫 extent에 allocator, 정리 큐6page,
빈 Catalog anchor/leaf의9page를 설치한다. plan4/high-water5이며 Status2/Undo3은
payload 없는 예약 identity다. private Plan4GenesisSelection은 canonical genesis의
writer exclusion과 borrowed observation만 제공한다. WAL frame, Pending, non-Free job,
nonempty Catalog, CURRENT/base/rebase와 부분 설치는 수리하지 않고 거절한다.
제품 selector의 unselected DWB는 geometry 검산이며 독립 native whole-file 판정과 구별한다.

독립 raw는 새15개161,920bytes와 기존 참조27개112,800bytes다. 원 생성은 한 번 수행했다.
[manifest](../../tests/fixtures/dev3-plan4-genesis/manifest.json)는
`82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22`다.
raw/참조42개274,720bytes의 기대값은 제품 encoder 출력에서 가져오지 않았다.
G와 Q/S/native recipe, compiler fixture는 별도 source-bound 입력으로 보존한다.
Q/S/native sidecar의 `g_recipe_sha256=ae2cc6aa…`는 tests-r2 준비 당시 G를 가리키는 역사
필드다. 현재 결합 hash로 해석하지 않는다. JSON의 `recipe_successor_binding`에 원 r2 경로·해시와
r4 `4091326c…`, 현재 r5 `ed36e00a…`를 연결한다. 현재 Q/S source hash와 G source hash·정확 ID의
결합은 별도로 확인하며 이 설명 때문에 원 sidecar·완료 source·시험을 변경하거나 반복하지 않았다.

| frozen source | 실제 준비·실행 |
| --- | --- |
| product-r0 `178a179e…` | 첫 제품 compile FAIL. guard borrow와 fence 접근 문제의 원 진단 보존 |
| product-r1 `dade8181…` | 해당 제품 compile PASS. 이후 namespace 검산 보완 전 source |
| tests-r0 `1b5473ac…` | 첫 test compile FAIL. private import/const 및 closure index type 오류 보존, 제품 실행0 |
| tests-r1 `8cb5c8d5…` | no-run compile PASS, fmt·Clippy FAIL. 제품 실행0 |
| tests-r2 `d8b26587…` | compile/fmt/engine Clippy PASS. ST01/ST02 PASS. compiler CP01–06 PASS, CP07 진단 기대 불일치 FAIL |
| tests-r3 `65b19719…` | test adapter WAL open·정확 오류 효과 보완 source. build/제품 실행0 |
| tests-r4 `66efc9c9…` | CP07 fixture lifetime bound 보완. compile/fmt/Clippy PASS. compiler CP07–09 PASS. memory135 PASS, K30 FAIL1, 나머지163 NOT_RUN |
| tests-r5 `570fbe8a…` | K30/K31의 Pin 기대값·recipe만 수정. compile11.083163초/fmt0.954777초/Clippy10.306838초 PASS. 실패1+미실행163만 선택하여164 PASS |

r5 ELF는 `177a53ae1ff47c0d82f4309cd34d4695aa644c02d22203637c5efa1c70a08f5b`,
41,478,288bytes다. 최초 세 source는 전체 export, 이후는 immutable base inventory와 정확
overlay·전체 fingerprint로 복원 가능한 source view다. 공유 inode의 backing은 한 번 계상하며
각 view의 논리 길이는 별도로 기록한다. 동시 DEV4 변경을 이 단위 시험 source에 섞지 않았다.

필수304는 static2+memory299+native3이며19 exact selector로 선택한다. S5 추가1,417은
memory1,411+native6의7 selector로 작성·ignored/NOT_RUN이다. 둘을 합친 inventory는1,721이다.
compiler9와 수집기 checker15는 이 제품 case 수에 더하지 않는다. 변경 kind12/13 입력을 기존
page/allocation/DWB/WAL 시험에서 분리했으며 unchanged 하위 wrapper를 실행하지 않았다.

memory의 원 r4 결과는 C3과 다음8함수132를 합친135 PASS다. K30은 기존 test-support
adapter의 Pin이 Some(page)를 반환하는데 시험이 None을 기대하여 실패했다. r5는 Pin/Read의
같은 projection과 독립 full-image SHA·전체 identity/kind/LSN/cells를 대조한다.
이는 raw 배열의 직접 byte 비교라는 주장이 아니다. 별도 K28/29가 raw byte 비교를 수행한다.
원 실패와 모든 완료 ID를 보존하고, 이후164에서 완료135를 제외했다. 앞선 작업 중140/159로
전달한 수치는 합산 오류였으며 원 receipt의135/164로 정정했다. r4 누적4.913995초와 r5의
6.936207초를 합한11.850201초는180초 한도 안이다. 생략된 사례의 PASS 승격은 없다.

static2의 제품 local peak는 memory23,556/native24,852bytes, selection은30,748/32,532bytes로
65,536 한도 안이다. actual owner Result는560/696bytes, File은32/160bytes다.
old nested prepare12,208/readback14,788은16,384 한도 안이며 공통 BootstrapPlan/Kind와
DescriptorSpec/SystemProgress/UndoRecord 크기는 변하지 않았다. 이는 source/type 생존 값
계산으로 stack/RSS·일반 F05 상한이 아니다. producer loan196,608bytes는 원 maintenance에서
빌리고 반환 owner에 남지 않는다. 기존 DWB4MiB backing도 다시 만든 별도 pool이 아니다.

compiler CP01/02는 실제 private sibling 문맥의 positive borrow, CP03–09는 private field,
literal, old owner 전환, plan literal, borrow escape/drop 및 Clone 경계를 확인한다.
원 CP07은 E0309+E0515가 함께 발생해 정확1진단 기준을 실패했다. `D::Writer: 'a`의
fixture bound만 추가하고 CP07/08/09를 선택하여 E0515/E0505/E0277 각1개를 확인했다.
원 실패와 미실행8/9를 보존했고 완료1–6을 반복하지 않았다. 누적 보수 시간24.161797초다.
수집기 checker는 root7+native8의 실제15 PASS이며 원 checker import 실패도 보존한다.

native는 같은 r5 ELF의 profile1 PRODUCE/SELECT/REENTRY를 모두 통과했다. 실제 PID는
219159/219162/219164이며 stable15file/22,069,248bytes를 독립 whole-byte로 검산했다.
원 producer snapshot과 working DB 두 copy44,138,496bytes를 보존하고 읽기 전용 단계는
동일 inode·길이·해시 및 mutation0·entropy0을 확인했다. File peak3/2/2·DB FD peak5·terminal0이다.
phase 시간은0.766028/0.054929/0.057789초, 수집·복사·검산을 포함한 campaign2.327258초다.
최종 결과를 쓴 뒤 관측2.338127초에 accounting allowance1초를 더한3.338127초로30초 누적
한도를 검산했다. 결과까지의 retained44,414,509/log276,013bytes, 마지막 accounting387bytes를 더한
최종44,414,896/log276,400bytes와 두 receipt의64KiB 예약·실제 길이를 확인해 phase10초·DB24MiB·history128MiB·log8MiB를 지켰다.

원 준비 예산992MiB는 보존한다. 네 번째 ELF를 보존하기 위해 실행 전 독립 D2로
ELF cap128→192MiB, 총 준비 cap **1056MiB(1GiB보다32MiB 큼)**를 채택했다.
제품·semantic·F05·native/S5 예산은 바꾸지 않았다. r5 build 뒤 source backing332,660,202,
ELF165,897,992, target+positive compiler metadata118,966,736,
그 밖의 receipt/tool/review4,794,158bytes를 확인했다. 해당 cap은 각각320/192/384/32MiB다.
source/cache 위치로 DB를 옮기거나 실패 증거를 삭제하지 않았다.

이 단위의 live 큐 실행·Catalog CREATE/CLR·첫 행 거래·일반 복구/READY·DEV3 최종 S5는
미완료이며 AI 완료 소비자는1/3이다. 최종 독립 D1은 소유95경로 전수를 검토하여 제한 수용했다. 문서 verifier·push는 수행하지 않았다.
원 raw/source/ELF/log/리뷰는 `artifacts/dev3/plan4-genesis-20261024`에 보존한다.

최종 검토는 OCR preview101 중 소유95를 검토하고 동시 DEV4의6경로를 명시 제외했다.
OCR 선택28과 수동 검토67을 모두 계상하며 frozen source80은 실제 r5와 byte 일치한다.
소유 문서14와 역사 corpus 주석1을 별도로 검토했다. 원221개 artifact index 및1721개 recipe
결합도 확인했다. 최종 준비 회계의65,536byte 리뷰 예상치는98,676byte 전파일 회계로 초과했으나,
후속 실제량을 원 preparation32MiB 한도 안에서 계상했다. 원 snapshot·예상치를 덮어쓰거나
제품/실행 한도를 높이지 않았다. 최종 D1과 후속 회계 원본은 JSON에 hash로 연결한다.
