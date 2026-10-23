# DEV3 반복 Status 이력의 현재-build 복구 선택 검증

2026-10-23 · **제한 복구 구현·선택 검증 / 최종 독립 D1 수용 / P3·DEV3/S5 미완료**

[소유 계약 F09](../contracts/dev3-recovery-effects.md#operational-status-recovery) · [source·receipt 색인](dev3-operational-recovery-20261023.json) · [실행 inventory](../../tests/fixtures/dev3-operational-recovery/manifest.json)

`1699dd7`의 반복 INIT live owner가 생산한 A INIT→B cancel→B INIT을 기존
`ClosedRecovery`에서 복구한다. 첫 extent의 immutable Status1..63을 허용하며 public
readonly Prefix의 첫 INIT 제한은 유지한다. private scope와 trusted terminal/population
validator를 공유하고 매 frame에서 직전 allocator·descriptor·high-water를 검산한다.

canonical census는 주소별 INIT LSN·incarnation·전체 image digest를 저장한다. 두 분석
pass는 caller arena의 기존 bytes를 초기화하고 같은 census/allocator/descriptor를 검산한다.
물리 REDO와 Pending 수리는 실제 주소별로 진행하고 dirty target을 주소순으로 flush한다.
A@2의 미래 LSN을 전체 adopted end@6로 허용하지 않으며 checksum-valid identity/내용
충돌을 torn page로 덮지 않는다. 기존 `status_image()`는 마지막 INIT 주소를, 새
`status_image_at(page_no)`는 검산된 해당 주소의 불변 image만 빌려준다.

이 단위는 P2 반복 immutable Status의 현재-build 복구 통합이다. P3 membership 기반
A reload·3주소/2frame 교체, live Status 변경, 일반 pool/pressure·batch8/64·다중 file,
REBASE/새 epoch/undo/READY·전체 DEV3/S5는 미완료다.

## 실제 source별 결과

보존 root는 `artifacts/dev3/operational-recovery-20261023`이다. source export에 별도
COMM/DEV2 변경이 포함될 수 있지만 실행 범위와 이 commit은 아래 engine 관련 변경뿐이다.

| source | 실행·결과 |
| --- | --- |
| r0 `29481eb1…` | compile FAIL: 기존 CanonicalImages 시험 fixture의 새 targets 누락. runtime 없음, 원 로그 보존 |
| r1 `9f94c986…` | compile PASS, source/type static1 및 신규 runtime57 PASS |
| r2 `6f95015e…` | native harness 작성 후 compile PASS, runtime 없음 |
| r3 `e238f7c1…` | compile PASS, native static1 PASS. 신규 checker8 중7 PASS/1 ERROR, 변경 compiler checker2 PASS |
| r4 `e8a80979…` | compile PASS, 실패 checker1 수정+새 checker3의4 PASS. fmt PASS, Clippy FAIL5 보존 |
| r5 `87c0c796…` | compile·fmt/Clippy PASS. 새 codec-valid 의미8·직접 소비자3·native4이력/14worker PASS |
| compiler `8aee6bc7…` | r5와 Python cache 한 파일 차이인 실제 보존 source에서 EC09–11+baseline PASS(7.47초); 아래 identity 기록 참조 |
| r6 `e0146aab…` | ignored S5 fault2232 작성 후 compile·fmt/Clippy PASS, runtime 없음 |
| r7 `d41ff6dc…` | S5 typed cause/last-call 검산 보강. compile·fmt/Clippy PASS, runtime 없음 |

신규 runtime65는 r1의57과 r5의8이며11개 Rust selector 안의 경계 단언 수다. r1은
mixed home9·complete/partial prefix20·historical Pending6·intact conflict7·완전 WAL
변조8·readonly scope/주소63/세대 overflow/두pass census drift 각1·retained terminal3이다.
WAL 변조8은 outer checksum을 reseal한 입력이며 inner codec까지 유효한 의미 변조로
주장하지 않는다. r5 ORS8은 codec-valid survivor owner/kind/generation/incarnation,
terminal accounting, 중복 INIT 주소, partial required-prefix, 잘못된 budget을 구별한다.

직접 소비자 ES003·ES005는 달라진 population materializer/analysis의 plan1
Exclusive cancel·Dirty63 cancel 경로다. PPL004는 targets가 추가된 CanonicalImages
fixture의 mode 거절이다. 전체 ES24/PPL12·하위 wrapper·이미 완료한 무변경 시험은
실행하지 않았다. r1 제품 코어7파일은 이후 revision에서도 byte가 같으며 새 test/harness만
달라졌다. 새로운 source 지문을 이전 시험의 재실행 근거로 쓰지 않았다.

## native·compiler·checker

native는 fresh 실제 producer가 A INIT 후 A/allocator를 flush하고 B cancel·INIT을
진행한 뒤 종료한다. 별도 PID가 복구하고 다시 별도 PID가 재진입한다. 정상·최종 INIT
38000bytes 중19000bytes partial tail·B home8192bytes partial write·B retirement
publish 후 오류의4이력/14worker가 모두 PASS했다. 합7.14초(이력당1.57–1.86초), 한도는
각30초/합120초다. 이는 주입 I/O 오류와 프로세스 경계 검증이며 SIGKILL/전원 장애 시험으로
표시하지 않는다. fresh producer의 기존 동작을 새 하부 PASS 분모로 중복 집계하지 않는다.

독립 wire table과 literal로 전체 파일 bytes·source inode·allocator/descriptor·Status A/B,
64주소의 initial/final LSN·apply/skip·unaddressed 영역, 실패 Pending 경계와 재진입
home write0을 검산했다. entry FD peak5/final0, handles final0이며 DB logical 크기 한도는
각24MiB/합96MiB다. 원 source/ELF·caller arena·trace·실패 snapshot은 별도이며 RSS 한도로
오해하지 않는다. 실패 snapshot은 home/선택 control/두 bank/slot prefix만 보존한다.

compiler EC09는 두 Status image와 owner Drop 뒤 scratch 반환의 양성, EC10/11은
live image를 둔 owner 이동과 불변 image 쓰기의 음성이다. baseline은 별도 실제 무수정
control이다. E0505/E0594의 정확 probe line7 및 단일 primary error를 검산했고 기존 EC00–08은
재실행하지 않았다. compiler 입력 검산/새 선택 checker2도 r3에서 PASS했다.

r3 checker의 원1 ERROR는 기존 독립 DWB oracle이 Status page1만 허용하기 때문이었다.
기존 oracle을 변경하지 않고 새 시험용 주소1..63 wire table을 작성했다. r4에서 실패1과
새 wire/high-bit 관측 검산3만 실행해4 PASS했다. 신규 checker 고유11건의 완료 결과는
r3의7과 r4의4에 분산되어 있으며 r3 job 자체는 FAIL로 보존한다.

native 완료 뒤 직접 소비자 선택 runner의 Python import가 r5 export에 `.pyc` 한 파일을
생성했다. native 원 source/hash 종료 기록은 `87c0…`, 이후 compiler의 실제 보존 source는
`8aee…`다. code/fixture 차이는 없으며 원 cache bytes/mtime/hash를 별도 보존한 뒤 r5를
원 지문으로 복원했다. compiler baseline과 원 receipt는 변경하지 않았다. 정확 경로와
시각은 `source-deviations/r5-python-cache.json`에 보존한다.

## 자원·리뷰·남은 인수

source/type 회계는 memory outer17920+max(prefix33384,work31992,inventory3320)=51304,
native outer18056+max(prefix33456,work31992,inventory3320)=51512로65536 안이다.
PhysicalObservation은3104bytes다. 64page scratch와3072byte census는 기존 maintenance
12MiB 내부이며 flush4MiB를 포함한16MiB 전체 loan과 FD5 한도는 유지한다. 이 수치는
OS stack/RSS 측정이나 전체 engine F05 인증이 아니다. F09의 확대 page topup·전 target
DWB generation overflow를 첫 mutation 전에 검산한다.

`/root/pool_design_review`가 OCR delegate의 선정/rule을 이용해 D2, r1/r4/r5의
정확 실행 inventory·source/census gate와 실제 결과를 별도로 검토했다. D2 원 보고서의
source_context 해시 기록 시점은 acceptance 뒤 구현을 포함하므로 source-addendum에서
실제로 검토한 HEAD7파일을 별도로 고정했다. 원 보고서를 덮어쓰지 않았다. 최종 D1은 반복 immutable Status의 현재-build 복구 한정 수용이며 열린 지적0건이다.
OCR 선정22개 전수검토와 제외된 관련 문서·fixture 수동검토 및 무관 변경의 제외를 구분했다.
원 판정과 후속 coverage addendum은 JSON 색인에 연결한다.

S5 확대2379는 작성·NOT_RUN이다: profile2/3 전체63주소2건,4096cancel 긴 이력1건,
3profile×8home×6Pending144건, home subset별 dirty target의31call×Before/After2232건.
장애 vector는 성공 trace에서 역산하지 않았고 원 cause·last-call tuple·실패 owner의 lease와
clean/unaddressed home·source 보존을 판정한다. S5 의무를 중간 필수 시험으로 축소하거나
미실행을 PASS로 표시하지 않는다. 문서 자동 검사는 push 전용으로 실행하지 않았고 push도
요청하지 않았다. 별도 COMM/DEV2 변경은 이 commit에 포함하지 않는다.
