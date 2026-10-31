# XADB 모듈 독립 개발 구조 전환 작업 계획

2026-10-31 [S5 Row 읽기 중간 구현](../evidence/module-independence-survey-20261026.md#s5-row-read-checkpoint): main `2ed254ac`의 별도 branch에서 원 source capture·Status/Missing·copy와 B live 연결을 구현하고 변경 관련 선택 검증을 진행했다. 전체 census·native·확대 검증·최종 리뷰·PR은 미완료이며 S5 완료로 세지 않는다.

2026-10-31 [S4 live Row rollback](../evidence/module-independence-survey-20261026.md#s4-row-rollback): main `139a05f4`의 별도 branch `codex/s4-row-rollback`에서 statement/full rollback·원 CLR/receipt·flusher/pin 인계를 구현했다. source별 선택20개·변경 compiler1개·native2process를 검증했으며 최종 독립 D1/D2 ACCEPT·필수 수정0을 확인했다. S5/C4/S6·전체 M4/M5는 미완료다.

2026-10-31 [S3 Row Commit](../contracts/dev3-module-boundaries.md#row-commit-s3): 병합 기준 `6ec2d343`에서 클라우드 별도 branch `codex/s3-row-commit`으로 착수했다. S3 한정 구현·source별 선택 검증과 최종 독립 D1/D2 ACCEPT·필수 수정0을 확인했다. 실제 Row Commit·native/current-build gate의 [source별 실행 증거](../evidence/module-independence-survey-20261026.md#s3-row-commit)에 원 실패·미실행과 후속 PASS를 보존했다. S4–S6·전체 M4/M5는 미완료다.

2026-10-30 [S2-f V09 대체 oracle과 S2 마감](../evidence/module-independence-survey-20261026.md#s2f-closeout): RowWrite writer가 resident reader 둘의 반환을 기다린 뒤 admission 경계를 지키는 MBC10-V09와 MBC40-V09 census(부분 공급)를 더했고, S2-e와 합친 통합 시험 186건이 통과했다. 실제 census와 계약 읽기는 [S2 공급 기록](../contracts/dev3-module-boundaries.md#row-write-s2-supply)에 있다. 사용자 지시로 S2에서 멈추며 다음은 S3다.

2026-10-30 [S2-e A INSERT driver live 경로와 frame-static D15](../evidence/module-independence-survey-20261026.md#s2e-live-driver): A driver가 실제 C wrapper로 행을 넣고 거절·Cancel·FlushRequired 재시도를 처리하는 live 시험 11건으로 G2를 닫았다. A driver를 단계별 frame으로 나눠 모든 C 호출 지점에서 A frame 전체가 상한 안이다(가장 작은 여유 preflight 813B).

2026-10-30 [S2-d Row Cancel 본문과 S2-c 위 통합](../evidence/module-independence-survey-20261026.md#s2d-row-cancel): Reserve한 행을 replay 없이 취소하고, Forward와 entry·재계산·반환을 공유하도록 합쳤다. 통합 시험 170건이 통과했고 cancel의 C 쪽 여유는 1579B다. finish를 빼고 쓰기 단계가 모두 실제 본문이다. 다음은 S2-e(A driver live 경로)다.

2026-10-30 [S2-c Row Forward 경로](../evidence/module-independence-survey-20261026.md#s2c-row-forward): producer_lane·prepare·apply가 replay 없이 draft commitment를 다시 계산하며 Forward를 설치한다. 실제 census는 C 쪽 여유 producer_lane 6023·prepare 1946·apply 1842B이고, 시험 156건이 통과했다. 다음은 같은 base에서 병렬로 만든 S2-d(cancel) 통합이다.

2026-10-30 [S2-b2 validation_lane·RowReserve 본문](../evidence/module-independence-survey-20261026.md#s2b2-row-reserve): reserve가 replay 없이 preflight commitment를 다시 계산하고 RowReserve를 설치한다. 실제 census는 C 쪽 여유 +795B(가장 작은 단계)로 상한 안이고, 시험 144건이 통과했다. 다음은 S2-c(producer_lane·prepare·apply)와 S2-d(cancel)다.

2026-10-30 [S2-b1 Row 쓰기 preflight 본문](../evidence/module-independence-survey-20261026.md#s2b1-row-preflight): 행당 한 번의 `validate_live`·arena 검산과 commitment를 가진 실제 preflight가 들어갔다. 실제 census는 C 쪽 여유 +1112B로 상한 안이고, 새 시험 17건과 통합 시험 127건이 통과했다. 다음은 S2-b2(reserve)다.

2026-10-30 [A-W1 A driver 축소와 호출 지점별 몫](../evidence/module-independence-survey-20261026.md#a-w1-driver): M1·M2로 A producer가 50790 → 19260B로 줄었고, MCA00-D15에서 모든 C 호출 지점의 A 몫이 상한 안이다(preflight 3264/6144, 나머지 최대 6552/11520). M7은 필요 없다. `FlushRequired` 뒤 flush와 재시도가 들어갔다.

2026-10-30 [S2-a′ RowWrite census 재계수](../evidence/module-independence-survey-20261026.md#s2a-prime-census): S2-0b대로 C 단계 상계를 다시 세고 A 몫 상한을 preflight 6144B·나머지 11520B로 나눴다. 모든 단계의 C 쪽 여유가 양수다(최소 prepare 1147B). 다음은 S2-b1과 A-W1이다.

2026-10-30 [S2-0b Row 쓰기 replay 검산 재배치 반영](../evidence/module-independence-survey-20261026.md#s2-0b-validate-once): Q06 2차 개정을 독립 D2 r3 ACCEPT 뒤 계약에 넣었다. 개정 내용은 다음과 같다. `validate_live`는 행당 preflight 한 번만 하고 digest 연쇄로 잇는다. group carrier 계수를 정정한다. A 몫 상한을 6144·11520B 둘로 나눈다. 다음은 S2-a′(census 구조와 두 상수)이고, A-W1과 S2-b1~S2-d가 그 뒤를 잇는다.

2026-10-30 [S2-a Row 쓰기 최종 표면](../evidence/module-independence-survey-20261026.md#s2a-row-write-surface): RowWrite class, preflight step·gate·lease·cut·8번째 lane part와 A 적응을 넣었다(본문은 Unsupported). 단계 census로 `ROW_WRITE_CALLER_BYTES`가 58B로 정해졌고, A-W1 전에 C 본문을 줄이는 Q06 2차 개정이 필요하다.

2026-10-30 [CR1 Catalog native 로컬 실행](../evidence/module-independence-survey-20261026.md#cr1-catalog-native-r2): 사용자 PC에서 `2bc18d1`로 사전 census 4건 PASS, 독립 admission ACCEPTED 뒤 Catalog native N01·N02·N03이 모두 PASS했다. U2-C4 전환의 로컬 요구가 끝났다.

2026-10-30 [branch Clippy 정리](../evidence/module-independence-survey-20261026.md#branch-clippy-20261030): pre-push gate의 Clippy 31건을 사유를 단 allow와 동작이 같은 작은 수정으로 정리했다(`dd81d85`). 이후 단위는 round마다 Clippy도 돌린다.

2026-10-30 [native 시험 adapter 정정과 CR1 범위 변경](../evidence/module-independence-survey-20261026.md#native-adapter-file-cr1-scope): 시험 전용 native adapter의 `File` 크기 차이가 commit window native 초과의 원인이었다(PR #3과 같은 정정 `94ede44`, native pending_full 66612 → 65460). native census가 memory + 8B로 내려왔다. 사용자 결정으로 F05를 CR1에서 뺐고 CR1은 Catalog native N01–N03만 남는다.

2026-10-30 [S2-0 Row 쓰기 계약 개정 반영](../evidence/module-independence-survey-20261026.md#s2-0-q06-amendment): Q06 개정(gate·flush lease를 보유하는 preflight와 `FlushRequired` step, lane 8번째 part, phase별 호출 지점 계수, 128B cut, S2-e 진입 조건, V09·MCI03-V02 oracle 대체)을 독립 D2 r3 ACCEPT 뒤 계약에 넣었다. 사용자 지시에 따라 이 branch는 S2(S2-a·A-W1·S2-b1~S2-f)까지만 진행하고 S5는 진행하지 않는다.

2026-10-29 [S2/S5 설계와 Row 쓰기 예산 판정](../evidence/module-independence-survey-20261026.md#s2-design-q06): S2의 Forward producer 단계가 60625B로 진행 charge 44304B를 넘어 Q06 재검토가 필요하다. lane 8부분·preflight flusher를 담은 개정 초안(D2 r1 필수 5 반영)을 만들었으나, 사용자 결정(S1까지만 마무리)에 따라 계약에는 넣지 않았고 S2 구현은 시작하지 않았다.

2026-10-29 [S1-e Row statement mark와 두 번째 문장](../evidence/module-independence-survey-20261026.md#s1e-statement-mark): Row 문장이 ATT `undo_head`를 slot 88..128에 mark로 쓰고, 매 경계에서 sequence·mark를 하나의 비교로 대조하며, 깨끗한 종료 뒤 두 번째 문장을 연다. cap 불변(handle 408 ≤ 512). 신규 11 PASS, 통합 round 33 PASS. 독립 검토 필수0. 이로써 S1(S1-a~S1-g)이 모두 공급됐고 다음은 S2-0 Q06 개정이다.

2026-10-29 [S1-g RowReserve replay 값 commitment 채택](../evidence/module-independence-survey-20261026.md#s1g-reserve-replay): recovery가 RowReserve frame에서 C와 projection만 채택하고 나머지를 다시 계산해 replay하며, Row root의 U allocator 항목을 Reserve arm과 결합 closure에서 검산한다. 새 counted 항 없음(history_row 25707/25743). 신규 7 selector PASS, 통합 round 2회. 독립 검토 필수0. S2가 live RowReserve를 쓰기 위한 선행 조건이 갖춰졌다.

2026-10-29 [S1-d Row Begin과 빈 Row 철회](../evidence/module-independence-survey-20261026.md#s1d-row-begin): Row Begin과 빈 full abort를 Catalog lifecycle과 같은 gate/progress shell과 설치 본문으로 공급하고, Row control byte는 replay와 같은 `history::row::record`만 쓴다. slot kind 2, RowProgress class, Catalog 진입점의 Row capsule 거절, S1-b 검토의 A1·A2·A3·A9·A13을 닫았다. cap 불변(Catalog lifecycle native 44253/44304). 신규 14 PASS, 실패 16은 base와 같음. 독립 검토 필수0.

2026-10-29 [S1-b/S1-c Row replay arm과 혼합 bank](../evidence/module-independence-survey-20261026.md#s1b-row-replay): `apply_frame` scope3 arm을 `history/row.rs`의 단일 Row 본문으로 공급하고 결합 closure와 Row 상태를 거절하는 임시 분석 gate를 더했다. Catalog 선택 loop는 Row cell을 건너뛰고 Job3 오류는 보존한다. recovered peak 불변(65368/65504). 신규 17 selector PASS, 기존 실패 6은 base와 같음. 독립 검토 필수0. RowReserve replay(Q1)는 S2 전에 정한다.

2026-10-29 [S1-a Row observer replay tail 대여](../evidence/module-independence-survey-20261026.md#s1a-borrowed-observer): N1 observer의 큰 배열을 WORK_END tail 9076B로 빌리는 `observe_borrowed`를 더했다. 모든 출구에서 tail을 0으로 되돌린다. borrowed bound 21323B로 S1-b Row arm에 2996B 여유를 남긴다. 11 selector PASS, 독립 검토 3회. 아직 호출되지 않으며 연결은 S1-b다.

2026-10-29 [S1-f CheckedTable 발급자](../evidence/module-independence-survey-20261026.md#s1f-finish-table): `CatalogStatementCall::finish_table`을 `CheckedTable`의 유일 발급자로 공급했다. 같은 호출의 lookup Found만 근거로 하고, publication과 isolation lock을 겹치지 않게 잡으며 read bit·catalog flag를 반환 전에 등록한다. 전환된 형식에서 8 selector PASS, 독립 검토 2회(필수2 반영). 오류 형식 정정과 미공급 문구를 계약에 반영했다.

2026-10-29 [U2-C4 단일 current 전환](../evidence/module-independence-survey-20261026.md#u2-single-current): Profile3 → Root3/Job3 → base4를 유일한 current 형식으로 바꿨다. 이전 Profile2-current 지점은 모두 정확한 `{3, 1, 명시 거절}` 분기이고 `>=`·dual-current·fallback은 없으며 순수 Profile2 codec은 남겼다. recovered peak native 65504·memory 65368(cap 65536). r1 304·r3 30 selector가 동결 기대와 일치하고, base 실패 34개는 main `85ceef3`에서도 같은 위치로 실패한다. genesis native PASS. 독립 검토 2회 수용. Catalog native와 F05 recovery native는 사용자 로컬의 남은 요구(CR1)다.

2026-10-29 [C3 scope3 Row 전이 producer·observer](../evidence/module-independence-survey-20261026.md#c3-row-transition): `semantic_row::transition`에 Row 거래의 C 소유 group producer(AgingStart·Begin·Reserve·Cancel·RollbackStart·StatementFinish·Commit·AbortEnd·Prune·ReleaseRole·RootFree·TerminalFinish)와 A Forward·CLR까지 받는 borrowed whole-group observer를 미호출 공급으로 더했다. Reserve는 첫 효과 전 RowLeaf proof를 한다. 독립 literal 바이트 비교 7 selector PASS(r1 FAIL2 보존), 독립 검토 2회 필수0, source bound 35256B. 연결은 U2-C4 전환 뒤 S1이다.

2026-10-29 [Row Begin·Row 쓰기 capsule 계약 등록](../contracts/dev3-module-boundaries.md#row-begin-capsule): U2-C4 전환 뒤 S1이 공급할 명시 `MutationPort::begin_row_transaction`(registry16 Terminal recipe로 scope를 Begin에 고정), slot actor kind 2(Row, RAM 전용), Catalog/Row capsule 혼합의 효과 전 거절, Row 문장 mark(ATT undo_head), `CatalogStatementCall::finish_table` 한정 공급, live·replay 공용 `history/row.rs` 본문, C4 전까지의 Row scope 분석 gate와 WORK_END tail 차용 자원 규칙을 XADB-MODULE-ROW-BEGIN-001로 등록했다. 독립 D2 검토 2회(필수 7, 재검토 필수 2)를 모두 반영해 수용됐다. 계약 등록만이며 코드·시험은 없다.

2026-10-29 [A INSERT statement driver](../evidence/module-independence-survey-20261026.md#a-insert-driver): A 전용 경로에 C0 Row 경계를 소비하는 문장 driver(행별 preflight→Reserve→produce→prepare/apply, Cancel·caller 구동 문장 rollback·Commit·full abort)를 더하고 C0_ONLY `MCA00-D01..14`를 등록했다. r1 D12 FAIL을 보존하고 r4에서 14개 PASS, 독립 검토 필수2 해결·마지막 필수0이다. live 연결은 C3의 Row Begin/CheckedTable, write·rollback 본문과 flush-through(G1)·rollback drop 의미(G5) 공급 뒤에 한다.

2026-10-29 [U2 base4 Catalog 잔여 연결](../evidence/module-independence-survey-20261026.md#base4-catalog-wiring): base4 Row graph의 Catalog hook을 U2-C3 Profile3 ledger로 연결하고 XOI Row3 모드에 Catalog Reserved item을 투영해 `CatalogUnwired`를 없앴다. Catalog-scope actor가 있는 base4가 이제 끝까지 검증되며, selector 11개 PASS·독립 검토 필수0이다. B4는 59675B다. U2-C4 전환의 선행 P1–P5가 모두 공급되었다.

2026-10-29 [U2-C2 base4 순수 closure](../evidence/module-independence-survey-20261026.md#base4-closure): Catalog base의 relations·ledger·allocation·XOI 본문을 공유 seam 하나로 바꾸고, 그 위에 Profile3 recovery base(base4)의 Row graph·actor·table·Row ledger·Row Reserved 관찰자를 순수 공급했다. 독립 검토 네 번(필수 1·1·1 해결, 마지막 필수0)으로 미 Prune CREATE 행 수용과 creator 뒤 q 순서를 더했고, r5 10개와 통합 tree 재실행 10개가 PASS다. base3 기존 selector의 FAIL11은 main의 기존 실패와 같다. Catalog-scope 의미 잔여는 `CatalogUnwired`로 닫혀 있고 다음 wiring commit이 U2-C3 ledger로 연결한다. B4는 59707B다.

2026-10-29 [U2-C4a/C4b 전환 준비 공급과 Profile3 입력](../evidence/module-independence-survey-20261026.md#u2-switch-prep): 단일 current 전환이 쓸 순수 공급으로 정확한 한 decoder만 고르는 설치 Profile 문법(`decode_installed_profile`, `PageFileReader::open_profile`, flush_io helper)과 전환 뒤 recovery census 투영을 더하고, Profile3 genesis 전체 namespace와 native3·memory-gap3·memory-retire3 입력 corpus를 stdlib generator로 동결했다. 신규7 PASS, 검토 필수1(낡은 U2-C2 입력 인용)·권고8을 반영해 5개 재실행 PASS, 재검토 새 필수0이다. production 호출과 product 실행은 없고, native 투영 여유는 8B(보수 입력)다. 전환(C4c)은 U2-C2 통합 뒤에 한다.

2026-10-29 [U2-C3 Catalog scope2 Profile3 adapter](../evidence/module-independence-survey-20261026.md#catalog3-adapter): `semantic_catalog::format`의 `Format`/`CatalogJob` seam으로 Catalog 관찰자 본문 하나가 정확한 Profile 2/3 분기로 version2와 Root3/Job3를 함께 처리하고, runtime alias 교체용 `retention_page::catalog3` view·guarded Root3 encode·Row Aging 상계를 더했다. r1 FAIL2(author의 sender digest 결함)를 보존하고 r2–r4로 신규10·Profile2 재실행14가 PASS, 독립 검토 필수2 해결·재검토 새 필수0이다. current 전환(U2-C4)은 base4 closure(U2-C2) 뒤에 한다.

2026-10-29 [B Row 읽기 driver](../evidence/module-independence-survey-20261026.md#b-read-driver): B 독점 경로에서 C0 read operation·view·cursor credit을 소비하는 point read와 `RowCursor`를 더했다. 독립 검토 필수2(cursor open 오류 순서, operation·table과 view의 결속)를 고쳐 B가 판정하는 같은 view에서 operation을 직접 열게 했고, r2 MCB00-D01..12 각1회 PASS·재검토 해결(새 필수0)이다. 통합 때 `Owner` 형식을 disk API로 재노출했다. 실제 C2 Row read source와 MCB01–05·MCI는 미완료다.

2026-10-29 [A Row INSERT scope3 Forward/CLR 후보 생산](../evidence/module-independence-survey-20261026.md#consumer-a-scope3-forward-clr): A 독점 경로에서 C0 관측값으로 Forward·CLR 후보 group(observe→prepare→frame)을 만드는 순수 본문과 MCA00-V06 독립 literal을 더했다. r1 신규7 PASS 뒤 독립 검토 필수1(출력 뒤 거부)을 고쳐 모든 입력 거부를 첫 출력 byte 전으로 옮겼고, r2 16 selector 각1회 PASS·재검토 해결(새 필수0)이다. test build는 30초 case 한도를 넘는 cold build였다. 실제 RowReserve·lane·WAL·install·replay와 MCA01–05·MCI는 C3/C4 공급 뒤 남는다.

2026-10-29 [U2-C1 Profile3 genesis 순수 공급](../evidence/module-independence-survey-20261026.md#row-genesis3-supply): `31e4614` 위에서 current 전환 전에는 호출되지 않는 `retention_genesis::{encode,validate}_row_current`와 Row Root3 decode·Row codec·Profile3 codec·genesis source 상계를 더했다. stdlib 독립 author의 genesis page12·거절7과 신규6 선택이 각1회 PASS, 독립 검토 필수1(증거 hash 표기)·선택 반영 뒤 바뀐3 선택 r2 PASS. current는 Profile2 그대로이며 base4 closure(U2-C2)·Catalog Profile3 adapter(U2-C3)·단일 전환(U2-C4)은 미완료다.

2026-10-29 [C0 Row 소비 경계](../evidence/module-independence-survey-20261026.md#c0-row-boundary): `036d4be` 위에서 A/B 본문이 호출할 Row write(preflight→RowReserve→prepare/apply/cancel)·read operation·live rollback wrapper와 Commit/abort 종료, cursor credit을 실제 타입으로 공급했다. 본문은 효과 전 Unsupported이고 cursor credit만 원 slot에512B를 실제 예약한다. 독립 검토의 필수 지적(credit이 view보다 오래 남으면 거래 종료가 fence)을 credit이 view를 빌리도록 고쳐 [계약](../contracts/dev3-module-boundaries.md#c0-row-boundary)에 기록했다. compiler probe r1 14개·r2 바뀐4개, runtime r1 2개·r2 5개가 각 1회 PASS다. recovery 타입·MBC00-P03/N11은 C4로 남겼고 실제 C2/C3 adapter·A/B 연결·전체 M4/M5는 미완료다.

2026-10-29 [R1–R3 논리 경계 판정](#r123-logical-boundaries): `76742f3`에서 L5가 남긴 논리 경계 재설계 입력 셋을 rustc 소비자 census로 닫았다. R1 `AR::operation`이 `wal_io` 경계를 넘는 것은 권한 없는 검산 함수·크기 상한·오류 enum 6개뿐이므로 현재 경로를 유지한다. R2 mixed owner 표면 15개 이름은 crate 전체로 재노출돼 있었지만 이름 경로 소비자가 0이었으므로, 초기화 파일 4곳의 중계와 `status_init`의 `pub(crate)` 재노출을 걷어냈다. 재노출 줄만 6개 파일에서 바꿨고 frozen compiler probe가 쓰는 3개 이름은 `pub(super)`로 남겼다. R3 초기화 사슬은 원 상태의 privacy seal이므로 유지한다. R2 입력과 L3 소비자 표의 grep 오판도 정정했다. [증거](../evidence/module-independence-survey-20261026.md#r123-logical-boundaries)의 `cargo check` warning 0, 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이고, 원 checker로 경로 소비 probe 5개를 commit tree에서 PASS했다. 독립 검토는 ACCEPT·필수 수정0이다. 전체 M4/M5는 미완료다.

2026-10-29 [L6 `storage::wal_io` 시험 자손 배치 적용](../evidence/module-independence-survey-20261026.md#l6-wal-io-move): `85ceef3`에서 L5 분류 밖으로 남긴 `storage/` 직속 `wal_io` 시험·지원 자손 5개를 진입 폴더 `storage/wal_io/`로 옮겼다. `#[path]` 5줄과 위치 기준 `include_bytes!` 3줄만 바꿨으며, 이동만 한 4개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이다. 논리 경계 재설계 R1–R3과 전체 M4/M5는 미완료다.

2026-10-29 [L5-D 복구 배치 적용](../evidence/module-independence-survey-20261026.md#l5d-recovery-move): main `47e3994`에서 `recovery_prefix`의 평면 자손 3개와 `plan4_pages`의 `recovered_bind`를 각 진입 폴더로 옮겨 L5 물리 이동 40개를 마쳤다. `#[path]` 4줄만 바꿨으며, 이동만 한 4개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이다. `wal_io/` 직속 Rust 파일은 23개이고 이름에 `test`가 있는 직속 파일은 없다. 논리 경계 재설계 R1–R3과 전체 M4/M5는 미완료다.

2026-10-29 [L5-C Status 배치 적용](../evidence/module-independence-survey-20261026.md#l5c-status-move): main `ff77992`에서 `status_init`·`status_init_operation`·`status_flush::tests::native`·`operational`·`resident`의 평면 자손 16개를 각 선언 파일의 폴더로 옮겼다. L3에서 남긴 `mixed_system_init.rs`도 여기에 포함된다. `#[path]` 16줄만 바꿨으며, 이동만 한 14개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이다. L5-D와 전체 M4/M5는 미완료다.

2026-10-29 [L5-B 설치·세션 조립 시험 배치 적용](../evidence/module-independence-survey-20261026.md#l5b-install-move): main `510d5f6`에서 `bootstrap`·`doublewrite_install`·`plan4_genesis`·`mutable_storage`의 평면 시험·지원 자손 11개를 각 진입 폴더로 옮겼다. `#[path]` 11줄만 바꿨으며, 이동만 한 8개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이다. L5-C·D와 전체 M4/M5는 미완료다.

2026-10-29 [L5-A 할당 배치 적용](../evidence/module-independence-survey-20261026.md#l5a-allocation-move): main `b8099bf`에서 `allocator_residency`·`allocator_flush`·`dirty_allocation`의 평면 자손 9개를 각 진입 폴더로 옮겼다. `#[path]` 9줄과 위치 기준 `include_str!` 7개만 바꿨다. 이동만 한 6개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했고 시험 본문은 NOT_RUN이다. L5-B–D와 전체 M4/M5는 미완료다.

2026-10-29 [L5 책임별 분류](#l5-classification): main `af50b46`에서 `wal_io/` 직속 63개를 책임별로 분류했다. 진입은 유지하고, 평면 자손 40개를 진입 폴더로 옮기는 물리 이동 4단위(L5-A–D)를 정했다. 원 source를 고정한 입력·checker 상수는 바꾸지 않으며, 논리 경계 재설계 R1–R3은 입력만 기록했다. 이 분류 작성 때 제품 변경과 시험 실행은 없다. L5-A–D 이동과 전체 M4/M5는 미완료다.

2026-10-29 [L4 배치 판정](#l4-record): main `08d854d`에서 `plan4_apply/`(10)·`plan4_retention/`(48)이 모두 한 모듈 `plan4_apply.rs`(production `apply`, 시험 `retention`)의 자손이고 소유가 같음을 확인했다. 이행 중 helper의 최종 소유가 미정이고 공유 시험 helper 2개가 외부 `#[path]`로도 쓰이므로, 권장안대로 현재 배치를 유지하고 재검토 조건을 기록했다. 제품·JSON 경로 변경과 시험 실행은 없다. L5와 전체 M4/M5는 미완료다.

2026-10-29 [L3 배치 적용](../evidence/module-independence-survey-20261026.md#l3-mixed-system-move): main `d251b8f`에서 [L3 이동표](#l3-move-table)의 권장안대로 `mixed_system_owner`의 자손6개를 `mixed_system_owner/`로, recovery 시험을 선언 폴더 `recovery_effect/`로 옮겼다. init 두 파일은 형제를 가진 평면 부모 아래에 남기고 L5에서 판단한다. `#[path]` 7줄과 fixture inventory의 `path` 151개만 바꿨고, 이동만 한 6개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했다. 시험 본문은 NOT_RUN이다. L4·L5와 전체 M4/M5는 미완료다.

2026-10-29 [L2 배치 적용](../evidence/module-independence-survey-20261026.md#l2-concurrency-move): main `b691fc7`에서 `transaction::concurrency`의 자손7개를 `transaction/concurrency/`로 옮기고 `#[path]` 7줄과 fixture `include_str!` 경로2개만 바꿨다. 이동만 한 5개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했고 시험 본문은 NOT_RUN이다. L3–L5와 전체 M4/M5는 미완료다.

2026-10-29 [L1 배치 적용](../evidence/module-independence-survey-20261026.md#l1-allocation-move): main `feb2b14`에서 확정한 배치 B로 파일7개를 옮기고 `#[path]` 7줄만 바꿨다. 이동만 한 5개의 byte 동일, `cargo check` PASS, 이동 전후 시험 목록 동일을 확인했고 시험 본문은 NOT_RUN이다. L2–L5와 전체 M4/M5는 미완료다.

2026-10-29 [L1 파일별 이동표](#l1-move-table): main `082d329`에서 `allocation_reservation`의 진입1개와 자손7개, 8개 파일의 선언·소유·소비·visibility·경로 참조를 대조한 초안을 작성했다. 사용자가 권장 배치 B(진입 파일 유지)를 확정했다. 제품 이동·빌드·시험은 NOT_RUN이며 전체 M4/M5는 미완료다.

2026-10-29 [폴더 재정리 후속 계획](#folder-reorganization): Job 검산 작업의 `517208d` 종료와 clean 상태를 확인하고 임시 조사안을 기존 계획에 반영했다. 참조·소유·API 경계를 기준으로 배치 정리와 논리 의존 변경을 구분한다. 문서 범위의 독립 D2 수용·필수 수정0이며 제품 이동·시험은 NOT_RUN, 전체 M4/M5는 미완료다.

2026-10-29 [실제 Job page·원 bank 전이](../evidence/module-independence-survey-20261026.md#row-job-images): main `21592e9`에서 Job11개 전이·원64 bank·실제 page/native/redo를 결합했다. raw26쌍·source별 신규10개 선택 검증과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 전체 M4/M5는 미완료다.

2026-10-29 [실제 RowLeaf·같은 frame Undo/Status](../evidence/module-independence-survey-20261026.md#row-leaf-images): main `7baef05`에서 원 schema/anchor·Job·sealed payload와 Row 전이/이웃 보존을 연결한다. 독립 입력15쌍·source별 신규8개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-29 [Row Undo·원 Job·Status 관계](../evidence/module-independence-survey-20261026.md#row-undo-images): main `09173aa`에서 Undo6개 전이·실제 H/nearest Job·같은 frame Status 연결을 구현하고 raw17쌍·신규9개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-29 사용자 후속 지시: [분리 완료 후 추가 개발의 독립성 평가와 push·merge 강제 계획](#post-separation-gates)을 저장했다. 적용 시점은 M4/M5 분리 인수 뒤 추가 개발 착수이며, 현재 자동 검사·원격 보호 설정은 변경하지 않는다.

2026-10-29 [Row Status의 실제 페이지/frame 결합](../evidence/module-independence-survey-20261026.md#row-status-images): main `b50e734`에서 Status9개 전이와 native/redo 관계를 구현하고 독립 literal24쌍·source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row scope3 frame 검산](../evidence/module-independence-survey-20261026.md#row-scope3-wire): main `2fdff66`에서 별도14opcode·다중 statement core·native/redo 입구를 구현하고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [Row actor의 실제 페이지 관계 검산](../evidence/module-independence-survey-20261026.md#row-actor-graph): 사용자의 전체 계획 완료 요청에 따라 `main` `d27bfa4`에서 재개했다. 한 actor의 Base4/Status/Undo/Row graph를 구현했고 source별 신규10개 선택 검증을 통과했다. 최종 독립 D1/D2 한정 수용·필수 수정0이며 전체 M4/M5는 미완료다.

2026-10-28 [CREATE 원 flush 대여](../evidence/module-independence-survey-20261026.md#c3-create-lease): main `3fa5f6f`에서 예약의 disjoint 원 자원·연속 flush lease를 구현했다. r1 compile FAIL을 보존하고 r2 compiler3/runtime10/nativeN01을 통과했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 admission·M4/M5는 미완료다.

2026-10-28 [CREATE 원 자원의 borrow 본문](../evidence/module-independence-survey-20261026.md#c3-create-parts): main `8613c53`의 원6자원을 빌리는 private view로 실제 본문을 분리한다. r1 fixture FAIL을 보존하고 r1 compiler2/runtime5 및 r2 수정·미실행 runtime2/nativeN01을 검증했다. 최종 독립 D1/D2 한정 수용이며 shared 제품 CREATE와 M4/M5는 미완료다.

2026-10-28 [공유 pool CREATE claim](../evidence/module-independence-survey-20261026.md#c3-pair-shared): main `c845ea7`에서 exact 후보·공유 종료·ordinary gate를 구현한다. r1 compiler4/runtime12/nativeN01 3PID PASS, 최종 독립 D1/D2 한정 수용이며 actual shared reservation과 M4/M5는 미완료다.

2026-10-28 [CREATE 기존 대상 key 접근](../evidence/module-independence-survey-20261026.md#c3-pair-existing): main `4e68a4d`에서 typed subset·실제 Reserve/Forward 준비/Cancel 연결을 진행한다. r2 compiler5/runtime8/nativeN01 3PID PASS·r1 주입 FAIL 보존, 최종 독립 D1/D2 한정 수용이며 shared admission·M4/M5는 미완료다.

2026-10-28 main `ef95a8a`의 [exclusive CREATE pair 원자 leaf](../evidence/module-independence-survey-20261026.md#c3-pair-atomic)를 진행한다. r3 runtime8/nativeN01 3PID PASS·원2개 FAIL 보존, 최종 독립 D1/D2 한정 수용이며 shared admission·C2/C3–C5·M4/M5는 미완료다.

2026-10-28 main `9c334fe`의 [Commit 준비·적용 단일 예약](../evidence/module-independence-survey-20261026.md#c2-commit-call)은 source별 runtime11/compiler4·원 N03 2PID를 통과했다. 원3개 FAIL을 보존하며 최종 독립 D1/D2 한정 수용다. CREATE/일반 floor/Row·M4/M5는 미완료다.

2026-10-29 · **D2 ACCEPTED / M0–M3 완료 / M4 C0·C1·C2·C3 부분공급(C0 Row 소비 경계의 컴파일 공급, C2 compact source·statement 읽기·Catalog Commit writer 창, C3 InstallGrant·TerminalWal·Prepared) / M5 Row 순수 codec·ledger 부분구현 / 전체 M4·M5 제품 인수 미완료 / Catalog Commit writer 창을 main에 통합**

[개발 문서 진입점](../README.md) · [기존 병렬 정책](../policies/parallel-development-policy.md) · [저장소 배치](../policies/repository-build-plan.md) · [DEV3 작업 분해](dev3-implementation-plan.md#remaining-work) · [변경 코드 시험 지시](../../AGENTS.md)

2026-10-28 main `af78960`의 후속 [Begin·AgingStart progress](../evidence/module-independence-survey-20261026.md#c2-start-progress)는 두 시작 경로를 같은 원 gate/source에 연결한다. 선택 비용과 issued-local gap을 유지하며 r2 compiler3/runtime5·같은 ELF native N02(3PID) 각1회 PASS와 r1 compile FAIL을 보존했다. CREATE/일반 floor/Row·M4/M5는 미완료다.

2026-10-28 main `bf62459`의 후속 [Catalog lifecycle progress](../evidence/module-independence-survey-20261026.md#c2-lifecycle-progress)는 RollbackStart/AbortEnd/CancelReserved/ReleaseRole의 공유 동기 호출과 원 ordinary application view를 구현한다. r3 compiler4/runtime12·같은 ELF native N02(3PID) 각1회 PASS와 r1/r2 compile FAIL을 보존한다. 나머지 C2/Row/C3–C5와 M4/M5는 미완료다.

2026-10-28 main `c217fbf`의 후속 [Catalog 회수 progress 공급](../evidence/module-independence-survey-20261026.md#c2-retirement-progress)은 CLR/Prune/RootFree/TerminalFinish를 원 gate·고정 source ledger에 연결했다. Maintenance 역할과 장기 snapshot horizon을 보존하며 source별 선택7+6 PASS와 원 FAIL을 구분한다. 이어 main `fa0f033`의 [공유 회수 준비 body](../evidence/module-independence-survey-20261026.md#c2-retirement-preparation)를 같은 gate/source에 연결하고 선택12를 검증했다. 후속 main `582ccbf`의 [동기 회수 호출](../evidence/module-independence-survey-20261026.md#c2-retirement-call)은 준비·적용을 한 원 gate/source 안에 묶어 회수 Prepared의 외부 보유 gap을 닫는다. compiler4/runtime13·같은 ELF native N02(3PID)를 각1회 PASS했다. 남은 공급은 일반 진입/반환 floor·Commit Prepared lifetime·나머지 writer/Row의 ledger 참여와 C3–C5이며 M4/M5 전체 인수는 계속 미완료다.

<a id="purpose"></a>
## 1. 목적과 이번 인계 범위

**기능 담당자가 확정된 하위 계약을 사용해 다른 기능의 내부 구현을 수정하지 않고 개발·검증할 수 있는 구조로 전환한다.** 필요한 하위 의존성과 최종 통합 순서는 유지하면서, 파일·공유 상태·미확정 API 때문에 생기는 불필요한 공동 수정과 대기를 줄인다.

사용자는 초기부터 “모듈별로 구성하고, 병렬로 개발해도 충돌을 최소화”하기를 요구했다. 현재 평가를 확인한 뒤 현재 구조를 변경하고 이 목적대로 개발되도록 에이전트가 문서를 수정할 작업 계획서를 요청했다. 따라서 후속 문서 담당은 현 구조를 설명하거나 병렬 운영 안내만 추가하는 데 그치지 않고, **목표 구조·내부 API·소유 계약·개발 순서를 수정안으로 구체화**한다.

이번 산출물은 그 문서 수정과 후속 구조 전환의 계획이다. 작성·리뷰만으로 진행 중인 다른 채팅·프로세스가 중지됐거나 소유 계약·제품 코드가 바뀐 것으로 기록하지 않는다. 최초 계획 작성은 계획서와 탐색 색인의 작성·리뷰에 한정했다. 후속 전체 실행 지시에 따른 현재 상태는 [실행 추적](#execution)에 둔다. M0–M3를 완료했고 M4 C0의 borrowed codec·생산자 타입과 C1의 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급을 진행했으며 C2의 [동기 Catalog 조회 본문 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)도 부분공급했고 M4 전체·M5는 미완료다.

2026-10-28 사용자는 `codex/module-independence-finish`를 `main`에 병합하고 이후 main에서 진행하도록 지시했다. [앞선 통합 기록](../evidence/module-independence-survey-20261026.md#merge-main-20261028) 뒤 별도 잔여 작업 branch의 `af54a4e`도 리뷰·사용자 지시에 따라 main으로 통합했다. 현재 재개 기준은 [Catalog Commit writer 창 병합 기록](../evidence/module-independence-survey-20261026.md#writer-progress-merge-main-20261028)이며, 위 초기 공급 설명과 아래 source별 기록은 당시 범위로 보존한다. C2 전체 동시 admission·C3 나머지 공급·Row actual 연결과 M4/M5 제품 인수는 미완료다.

우선 조사 범위는 DEV3의 WAL·페이지/버퍼·할당·거래/undo·복구·Catalog 연결이다. 실제 첫 전환 범위는 M2에서 선정하는 두 소비 기능과 그 기능에 필요한 공통 경계로 닫는다. foundation/platform·SQL·COMM은 이미 확보한 경계를 보존하면서 수신 계약에 영향이 있는 부분만 대조한다. 이후 단계도 영향을 받는 선행·인계 문구를 갱신하되, 전체 DBMS를 다시 설계하는 작업으로 확대하지 않는다.

<a id="baseline"></a>
## 2. 출발 근거와 상태 재확인

| 관측 | 근거와 해석 |
| --- | --- |
| 초기 요구가 이미 정책에 존재 | [병렬 정책](../policies/parallel-development-policy.md#section-1)의 책임별 파일 구성·공통 계약 최소화·작은 조립 지점. 새 목표를 발명하는 작업이 아님 |
| 큰 모듈의 분리는 실제 작동 | [SQL](../../crates/sql/Cargo.toml)·[transport](../../crates/transport/Cargo.toml)의 제품 의존은 foundation. [DEV4 앞단](../evidence/dev4-frontend-20261024.md)은 DEV3 변경을 제외한 소스와 별도 소유 경로로 개발 |
| DEV3 공통 경로에 변경 집중 | 평가 기준 `9275154`까지 2026-10-20 이후 저장 소스를 바꾼71개 커밋 중33개가 `wal_io.rs`, `bootstrap.rs`, `doublewrite_install.rs`, `mutable_storage.rs` 가운데 하나 이상을 수정. 등록·재노출·명칭 변경도 포함한 Git 경로 집계이며 실제 merge 충돌 수나 생산성 수치가 아님 |
| 기능 연결과 하위 기반 보완이 결합 | `e443115`의 Catalog 연결은 시험을 포함한 Rust101파일 변경. [원 구현 범위](../evidence/dev3-catalog-live-milestone-20261025.md)는 첫 연결의 정상적인 필요와 미완료 공급을 구분. 파일 수만으로 잘못된 설계라고 판정하지 않음 |
| 타입 재노출 사슬의 실제 비용 | 같은 Catalog 권한 타입을 여러 중간 모듈에서 재노출. 위 증거의 일반 빌드 오류는 시험 전용 조건 속성의 잘못된 연결이었고 이미 수정됨. 현재 미수정 결함으로 재보고하지 않음 |
| 최초 평가 이후 상태 변화 | 계획 작성 시작 HEAD는 `107667d3cde769bfd20232da1c1dcc288c60c863`. DEV3 후속 수정과 DEV2 변경이 추가됐고 DEV4 병합 중 공용 README·문서 색인에 충돌을 관측. 최초 평가의 미커밋5파일·미완료 목록을 현재 사실로 복사하지 않음 |

후속 담당은 M0에서 최신 HEAD·dirty/index·진행 작업을 다시 확인한다. 위 관측은 출발 근거이며 현재 소스 전체의 인수 결과가 아니다. 기존 정상·FAIL·UNKNOWN·중단·NOT_RUN과 source별 증거는 그대로 유지한다.

계획 작성 중 병합은 다른 작업에서 완료됐고, 탐색 색인 등록 시점의 기준은 `27c2288b6ed7b48e556c179067348c686650b3f9`이며 충돌은 없었다. 이 계획 작업은 병합·기존 제품 수정을 수행하지 않았다.

<a id="principles"></a>
## 3. 구조 전환의 설계 기준

1. **변경 책임을 경계로 삼는다.** 같은 기능의 값·오류·설정·계측·시험은 해당 책임에 둔다. 파일 수·줄 수·crate 수를 목표 지표로 삼지 않는다.
2. **공통 의미와 조립을 분리한다.** 실제 공유 ID·참조·권한·오류 의미는 단일 소유 계약을 유지한다. WAL·buffer·undo·catalog를 함께 사용하는 순서는 명시된 거래/복구 조립 담당에 둔다. 서로 다른 초기화 단계를 중첩한 모듈 경로가 모든 기능의 접근 경로가 되지 않게 한다.
3. **독립 개발은 계약과 좁은 접근 권한으로 확보한다.** 상태 전체나 manager를 넘기는 대신 필요한 값·읽기 view·소유권 이전·검증된 capability를 제공한다. 내부 구조체 필드의 공개 확대, 전역 singleton, raw 포인터, 공통 가변 context로 결합을 숨기지 않는다.
4. **모듈의 컴파일 의존과 실행 조립 관계를 따로 그린다.** 실행 시 서로 협력한다고 제품 모듈이 상대의 내부 구현을 import해야 하는 것은 아니다. trait는 실제 대체 구현·효과 경계가 필요한 곳에 사용하고 모든 함수를 추상화하지 않는다.
5. **개발 선행과 통합 선행을 분리한다.** 하위 전체 단계 완료 대신 사용할 API·불변식·최소 기능 공급을 특정한다. 확정 계약과 필요한 컴파일 경계를 사용해 구현할 수 있는 소비 기능의 실제 로직은 하위 전체 구현을 기다리지 않는다. M2에서 각 소비자의 착수 가능 구현, 허용할 계약 대역, 실제 공급을 기다릴 통합 인수를 나눈다. 대역의 허용 효과·한계·독립 기대값과 실제 공급으로의 연결 의무를 정하며, 실제 저장 효과·완료 판정은 실제 공급과 연결한 뒤에만 인정한다.
6. **공통 기반 수정은 유한한 선행 작업으로 닫는다.** 소비 A/B가 각각 자신의 요구에 맞춰 공통 타입·WAL 의미·lock 순서를 바꾸지 않는다. 필요한 변경을 먼저 합의·통합한 뒤 같은 기준을 받는다. 새 요구가 드러나면 영향받는 작업만 다시 조정하고 무관한 작업은 계속한다.
7. **독립 단위의 인수도 설계한다.** 서버 없는 실제 core 호출, 필요한 최소 fixture·독립 기대값·명시된 오류/자원 경계를 제공한다. 실제 I/O·동시성·복구가 계약에 포함되면 해당 통합 인수를 별도로 유지한다.

보존할 제품 불변식은 [구현 계약의 DEV3 적용 경계](../contracts/implementation-contracts.md), [disk 저장 정본](../contracts/disk-storage-design.md), [버전 정책](../policies/version-compatibility-policy.json)을 따른다. WAL-before-page, Commit/ACK와 UNKNOWN, 원 pin/lease/fence·authority의 유일성, lock 순서, 원자 공개, undo/CLR·재장애, 자원 상한과 회수 진행을 분할 편의 때문에 약화하지 않는다. 런타임 단일 writer 보장과 개발 담당자의 작업 직렬화는 별개로 평가한다.

<a id="target-boundaries"></a>
## 4. 문서 담당이 확정할 목표 책임 경계

아래는 **설계 검토를 시작할 책임 분류**다. 실제 Rust 모듈명·파일 경로·crate 분리·함수 signature는 M1/M2에서 현 코드와 소비자를 대조해 확정한다. 이 표만으로 새 경로를 build inventory에 등록하지 않는다.

| 책임 | 소유할 상태·동작 | 다른 책임에 제공할 최소 경계 | 피해야 할 결합 |
| --- | --- | --- | --- |
| 파일/저장 수명 | namespace·파일 identity·공통 fence·설치/복구 후 수명 | 제한된 파일 효과·검증된 저장 수명 인계 | 설치 전용 wrapper를 모든 정상 연산의 접근 사슬로 사용 |
| WAL | append slot·prefix·물리 write/sync·durable 관측 | stage·force·정확 requirement 확인·실패 effect | Catalog/행 업무 규칙을 WAL 구현에 등록 |
| 페이지/버퍼 | frame·pin·dirty/recLSN·load/evict·DWB/home flush | 실제 page 획득·변경 admission·flush 결과·수명 토큰 | 상위 거래/Catalog의 상태 전체를 보유하거나 직접 실행 |
| 할당/객체 수명 | reserve/cancel·generation·초기화·재사용 권한 | typed 예약·설치/반환 인계·할당 검산 | 임의 caller가 헤더/번호만으로 유효 권한 생성 |
| tree/행/index/Catalog | 각 데이터의 표현·검색·변경 후보·도메인 불변식 | 명시된 페이지/undo/거래 계약을 소비하는 기능 API | 각자 buffer/WAL/commit 의미를 복제하거나 다른 기능 본문 수정 |
| 거래/undo/가시성 | transaction/statement 상태·undo mark·snapshot·권한·공개 | 변경 묶음 준비·적용·commit/abort의 닫힌 수명 | caller에게 부분 commit 순서나 실패 정리를 떠넘김 |
| 복구/checkpoint/회수 | restart 상태·redo/undo 순서·horizon·정리 의무 | 검증된 복원 상태와 원 소유권 인계·회수 요청 | live와 recovery가 비슷한 필드를 쓴다는 이유로 상태 의미 통합 |
| 조립 지점 | 위 모듈의 인스턴스/수명 연결·원자적 실행 순서 | 기능 담당이 사용하는 제한된 실행 경계 | 기능별 거대 dispatch·오류·설정·계측을 중앙에 누적 |

M2 산출물에는 다음 두 그림을 각각 둔다: (a) 허용 Rust import/타입 의존 그래프, (b) 생성→정상 실행→오류/종료→재시작의 소유권·호출 흐름. source 경로와 코드 symbol을 연결하고 순환·예외의 이유를 기록한다. 초기화/복구 수명을 분리하면서 기존 원 owner를 복제하거나 capability 생성자를 넓히지 않는지 별도로 검토한다.

각 경계는 **소유 상태, provider/consumer, 입력/출력 타입, 소유/borrow 수명, mutation 권한, 정상·실패 효과, 취소/정리, 동시성·lock, 자원 회계, 영속/복구 의미, 실제 최소 공급, 선택 시험, 미공급 의무**를 갖는다. 해당하지 않는 항목은 이유를 적는다. 선언만 있는 API와 현재 호출 가능한 API를 구분한다.

<a id="m2-handoff"></a>
### M2 설계 인계의 구체 형식

M2에서는 위 경계를 선정한 A/B의 실제 호출에 대입한다. 아래 항목은 기존 소유 계약·작업 기록의 해당 절에 작성하고 이 계획에서 연결한다. 같은 내용을 별도 문서마다 복제하거나 새 관리 도구를 만들지 않는다. 이 표는 산출물의 작성 기준이다. A/B 선정과 API 설계의 실제 채택 상태·근거는 [실행 추적](#execution)과 [소유 문서 채택 결정](#m3-adoption)에 둔다.

| 산출물 | M2에서 확정할 내용 | 검토자가 확인할 연결 |
| --- | --- | --- |
| A/B 기능별 작업 명세 | 기존 backlog ID, 입력과 관측 가능한 완료 동작, 지원/제외 범위, 담당·전용 경로, 각 담당이 구현할 제품 본문 | 두 기능이 실제로 소비할 선정 경계와 독립 구현 범위. 상대 기능의 미완료 본문·fixture를 선행으로 요구하지 않는지 확인 |
| API 사용 설계 | 현재 symbol→목표 모듈/함수, Rust signature·visibility, 입력/반환 값과 원 owner·borrow·권한의 이동. A/B 각각의 정상 호출 및 관련 오류·취소·종료/복구 흐름을 짧은 호출 예나 순서표로 설명 | 필요한 모든 호출이 허용 경계로 표현되는지, 호출자가 내부 필드를 열거나 원자성·실패 정리를 다시 구현해야 하는 지점이 없는지 확인. 원자 공개·WAL·pin/fence 불변식과 대조 |
| 공통 공급별 이행 명세 | 각 M4 단위의 현재→목표 경로, 제공 API·완료 동작, 직접 선행, 담당, 제외 범위·되돌림 단위. 각 공급이 해소할 A 또는 B의 구체적인 호출 제약을 연결 | A/B의 호출에서 필요한 공급으로 거슬러 올라가고, 각 공급에서 실제 소비자로 되짚어 범위 누락·불필요한 확대를 확인. 한쪽만 필요한 경계 공급도 이유를 남기며 A/B 기능 본문과 구별 |
| 착수·검증 인계 | C0에서 사용할 계약·최소 컴파일 경계·허용 대역, C에서 받을 실제 공급, 단독/합류 시나리오와 기대값 출처, 기존 selector 또는 구현과 함께 추가할 case, 변경에 필요한 자원 검산·실제 I/O/복구 의무 | 공급 수신으로 해소할 미실행 의무와 실제 diff에 따른 선택 범위를 연결. A/B가 선정 경계를 함께 소비하는 합류 시나리오의 관측값을 정하며, 런타임 동시 호출은 제품 계약이 지원·요구하는 범위에서만 배정 |

목표 경로·signature와 아직 작성하지 않은 case는 계획 상태로 표시한다. 호출 예는 문서 설계이며 M2에서 prototype 구현이나 컴파일·제품 시험을 수행하라는 뜻이 아니다. 실제 경계 구현과 관련 검증은 M4/M5의 해당 작업에서 한다.

M2 종료 시 위 연결에서 남은 **선정 범위의 설계·배정 공백**을 열거한다. A/B에 필요한 API·권한 의미나 필수 공급·인수의 담당/완료 조건이 미정이면 M2를 미완료로 두고 해소할 작업을 특정한다. 계약과 배정이 확정된 M4/M5의 미구현 공급·미실행 시험은 후속 의무로 남기며 M2의 실행 선행으로 올리지 않는다. 범위 밖 기능의 미정사항은 제외 근거와 후속 의무로 남긴다. M4 도중 새 공통 공급이 필요하면 이 표의 영향 행과 기존 시도·결과를 보존해 갱신하고 해당 변경을 독립 검토한다. A/B의 기능 본문을 공통 담당에게 옮기거나 검토 없이 공급 완료 조건을 늘리는 방식으로 해결하지 않는다.

<a id="phases"></a>
## 5. 작업 순서와 단계별 완료 조건

각 단계 종료는 정해진 산출물과 독립 검토로 판단하며, 사용자에게 일상 작업마다 재승인을 요청하는 절차를 추가하지 않는다. 이번 계획 작성은 아래 실행을 수행했다는 증거가 아니다.

| 단계 | 작업·담당 | 산출물 | 종료 조건 |
| --- | --- | --- | --- |
| M0 기준선·작업 보존 | 통합 담당 | 현재 revision/dirty/index·관련 untracked·동시 작업·실행 중 job·원 증거·미완료 목록; 영향 범위와 소유자 | 다른 작업을 덮어쓰지 않는 편집 범위, merge 진행/미해결 상태와 재개 지점을 확인. 새 기능 보류의 대상과 실제 확인 상태를 구분 |
| M1 현재 경계 조사 | 구조 문서 담당 | 상태 소유표·실제 의존 그래프·공통 파일 변경 이유·consumer별 차단 표 | 조사 대상의 각 의존을 필수 의미/공통 계약 미완료/불필요한 내부 접근/단순 등록 중 하나로 설명. 판단 보류는 근거·해결 질문과 연결 |
| M2 목표 구조·이행 설계 | 계약 담당+독립 검토자 | [설계 인계표](#m2-handoff)의 현재→목표 매핑, 경계 계약, 두 제품 작업과 공통 공급의 사전 선정, 유한한 이행 단위·제외 범위·되돌림 단위 | 두 작업의 정상/오류 호출 순서를 같은 하위 계약으로 대조. 소비자별 구현 착수/통합 선행, 각 M4 단위의 공급 동작·선택 인수와 M5에 남길 기능 본문 확정. 필수 미정사항 해소, 원자성·수명·복구 반례 검토, 독립 D2 수용 |
| M3 소유 문서 정합화 | 문서 담당+통합 담당 | 선정 경계에 필요한 6절의 실제 Markdown/JSON 수정, 대체 결정·선행 관계·작업 계약·변경 영향 인수 | 현재 계약과 목표 계약의 적용 이행 단위를 구별. 선정 범위의 오래된 금지/직렬 선행과 충돌 없음. 실제 공급과 미구현을 구별. 독립 D2 수용 |
| M4 구조 변경 | 경계별 구현 담당 | M2의 이행 단위별 코드·관련 선택 검증·현재 소비자 연결·D1/D2 기록 | 선정한 공통 공급과 소비 경계를 실제 코드로 제공. 제품 의미/자원 보존, 변경 경로의 실제 복구 의무 충족. A/B 기능 본문은 M5의 담당 범위로 두며 해당 실행 지시가 있을 때 착수 |
| M5 두 제품 기능 병렬 인수 | 제품 담당 A/B+통합+별도 검토 | 같은 기준에서 진행한 서로 다른 두 제품 변경, 각 국소 인수, 실제 합류 결과·충돌/대기 기록 | 8절의 사전 고정 인수 기준 충족. 계획/코드 분할/검증 도구 병행만으로 완료하지 않음. 후속 일반 기능 확대 재개 |

**문서 정비 중 보류 범위:** M0에서 지정한 DEV3 공유 경계에 새 상태 전이·소비 기능을 추가하는 작업을 보류 대상으로 기록한다. 기존 실행 중 작업은 담당의 안전한 경계에서 결과·FAIL·중단·미완료를 보존하고 인계한다. 다른 세션을 무단 종료하거나 서비스/호스트를 재시작하지 않는다. 무관한 독립 조사·문서·검토까지 일괄 중단하지 않는다.

보류 대상의 지정과 실제 보류 확인은 다르다. 각 대상의 담당·기준 source·인계 위치·확인 상태를 기록하고, 확인하지 못한 작업은 진행 가능성이 있는 것으로 취급한다. M0의 목록만으로 다른 채팅에 연락하거나 작업을 중지할 권한을 추정하지 않는다. 충돌 범위의 변경은 인계 확인 또는 편집 범위 분리 뒤 진행한다. M2/M3 채택 전과 M4/M5 착수 전에는 기준 이후의 실제 diff를 대조하여 영향을 받는 설계·계약·시험 선택만 갱신한다. 무관한 HEAD 변경으로 완료한 검토·시험을 처음부터 반복하지 않는다.

**첫 전환의 한도:** M2는 공통 공급의 각 이행 단위에 제공 API·완료 동작·필수 인수·제외 범위를 적는다. 해당 공급에 필요하지 않은 DEV3 잔여 기능이나 4절의 모든 책임 재배치를 M3/M4의 선행으로 추가하지 않는다. 새로운 공통 의미가 필요해 범위가 늘면 원 계획과 이유를 보존하고 영향을 받는 M2 설계를 다시 검토한다. 전체 모듈화가 끝날 때까지 두 소비 기능의 착수를 미루는 포괄적 선행으로 바꾸지 않는다.

**재개는 필요한 공급별로 판단한다.** M3 수용 뒤에는 구조 변경인 M4를 진행할 준비가 된다. 사전 선정한 A/B의 제품 구현은 M2에 정한 착수 계약과 최소 컴파일 경계가 제공된 부분부터 M5 작업으로 시작할 수 있다. 경로·의미가 겹치지 않는 M4의 잔여 공급과 병행하되, 실제 공급이 필요한 인수는 해당 공급의 수신 뒤 수행한다. M4 전체 완료를 모든 소비 구현의 일괄 선행으로 삼지 않는다. 일반 기능 확대는 M5 인수 뒤 해당 경계부터 연다. 전체 DEV0–DEV9의 모듈화 완성이나 DEV3 S5 완료를 M5의 선행으로 요구하지 않는다. M4/M5에는 후속 해당 작업의 실행 지시가 필요하며, 이 계획서 작성만으로 제품 구현·시험을 시작하지 않는다.

<a id="document-map"></a>
## 6. 후속 에이전트의 문서 수정 대상과 방법

현재 소유 문서의 본문과 관련 JSON을 함께 고친다. 종합 계획에 새 원칙을 덧붙이고 실제 owner 계약은 그대로 두는 방식으로 완료하지 않는다. 아래는 우선 읽기/수정 표이며 모든 파일을 기계적으로 수정하라는 목록은 아니다. 각 행에 실제 수정 절 또는 미수정 근거를 남긴다.

| 소유 문서·연결 자료 | 필요한 수정 |
| --- | --- |
| [병렬 정책](../policies/parallel-development-policy.md)·관리 JSON | 독립 개발의 정의, 공통 계약 동결/변경 절차, `owned_paths`와 의미 중복 검토, 공유 등록 지점, PD01–03의 실제 인수 연결. 측정 전 충돌 감소를 선언하지 않음 |
| [코딩 컨벤션](../policies/coding-convention.md#section-4)·관리 JSON | 기능 소유 경계, 최소 visibility, 조립과 공통 의미, 부모 재노출 사슬의 판단 기준. 일률적 crate/trait/파일 분할 규칙 도입 금지 |
| [저장소 배치](../policies/repository-build-plan.md)·관리 JSON | 목표 역할·허용 의존·테스트 의존·공유 조립 책임. 실제 경로·package·source fingerprint·export 목록은 M4 구현과 함께 등록 |
| [구현 계약](../contracts/implementation-contracts.md)·[disk 저장 설계](../contracts/disk-storage-design.md#ownership) | transaction/storage/WAL/recovery의 실제 소유·호출 방향. 불변식·원자성은 유지하면서 원 owner의 생성/이전과 모듈 접근 경계를 재설계 |
| [저장 세션](../contracts/dev3-storage-session.md#consumer-driven-refinement)·[WAL I/O](../contracts/dev3-wal-io.md)·[Commit continuation](../contracts/dev3-commit-continuation.md) | 설치→정상→복구 owner 관계, 원 fence/lease/slot/pin 권한, 상태 토큰·정확 durability 관측. 중첩 모듈 구조를 유지해야 한다는 가정을 검토·대체 |
| [DEV3 작업별 설계](../contracts/dev3-task-designs.md#catalogue)의 해당 3.02–3.14 | 각 기능의 provider/consumer, 허용 API, 오류/자원/수명, 독립 구현 가능 범위와 실제 통합 인수. 소유 계약과 다르게 이중 정의하지 않음 |
| [Catalog CREATE](../contracts/dev3-catalog-create.md)·[plan4 복구 owner](../contracts/dev3-plan4-recovery-owner.md) 등 선택 경계의 직접 계약 | 현 코드의 symbol·권한 생성자·공유 버퍼·복구 인계와 목표 구조 대응. 옮기는 책임과 유지하는 제품 의미를 별도 표시 |
| [DEV3 구현 계획](dev3-implementation-plan.md#parallel-development)·[공급 설계도](../contracts/dev3-task-designs.md#dependency-map) | 전체 기능 번호순 선행을 최소 공급/독립 준비/실제 합류로 구분. M0–M5와 일치하는 착수·재개, A/B 실제 담당·파일 범위·통합 지점 배정 |
| [하위 계층 개발](bottom-up-development.md)·[개발 단계](development-stages.md)·단계/완료 JSON | bottom-up 실제 통합·인수 순서는 보존. 개발 착수의 독립 가능 범위를 명시하고 새 구조 인수를 제품 단계 완료나 S5 면제로 바꾸지 않음 |
| [DEV3 시험 계획](dev3-test-plan.md)·[모듈 시험 목록](../contracts/module-test-matrix.json) | 새 경계의 좁은 인수와 실제 연결 의무, diff→case 선택, 원 시험 ID 이동 매핑. 기존 전수 inventory·원 FAIL·원 source 증거 유지 |
| [DEV4 독립 앞단](../contracts/dev4-independent-frontend.md)·[준비 계획](dev4-preparation-plan.md)·[구현 계획](dev4-implementation-plan.md) | SQL에서 받을 실제 disk API와 최소 공급을 새 경계에 연결. 독립 frontend/model 진행과 엔진 인수를 구분. 최신 DEV4 병합 상태를 다시 수신 |
| [AI 작업 절차](../policies/ai-development-method.md#section-8)·[작업 계약 예시](../examples/task-contract.example.json)·[AI 효율 계획](dev3-ai-throughput-improvement.md) | “같은 상태이므로 한 담당이 계속 구현”을 상시 해법으로 삼지 않음. 공통 공급 동결→제품 A/B→합류를 실제 작업 입력에 반영. 독립 검토와 구현 병렬을 구별 |
| [AGENTS.md](../../AGENTS.md)·[문서 진입점](../README.md)·폴더 README·[문서 색인](../documents.json) | 유효한 구조 전환 방향·보류/재개 범위와 읽기 경로를 짧게 연결. 시험 제한·예외의 적용 범위·push gate를 그대로 유지 |
| [구현 상태](../implementation/README.md)·해당 구현 문서 | 실제 코드가 바뀐 범위만 갱신. 문서 설계만 고친 단계는 “목표 구조 미구현”으로 표시. 과거 실행 증거는 재작성하지 않음 |

후속 문서 담당은 `문서/절 → 기존 규칙 → 변경 문구/계약 → 이유 → 영향 API·소비자 → 독립 검토 → 구현/인수 상태` 표를 이 계획의 실행 기록 또는 기존 결정 절에 남긴다. 장문 규범은 소유 문서 한 곳에 두고 이 계획은 작업 순서·진행 참조를 유지한다.

### 기존 결정과의 대체 관계

- [D3-DIRECTION-002](dev3-implementation-plan.md#direction-refinement)와 [저장 세션의 국소 개선](../contracts/dev3-storage-session.md#consumer-driven-refinement)은 당시 첫 행 연결을 우선하여 전체 재배치를 선행하지 않도록 했다. **이번 사용자 방향은 모듈 독립성을 위한 구조 변경을 먼저 설계하는 것**이다. M3는 해당 “국소 변경만 허용/구조 정리를 선행하지 않음” 범위에 한정해 명시적인 대체 관계를 기록한다. 기존 결정을 지우거나 제품 의미·품질·첫 행 최종 목표까지 폐기하지 않는다.
- 단일 작성자는 공통 계약 변경과 원자적 합류의 담당이다. 모든 기능 본문을 한 사람이 순차 개발해야 한다는 규범으로 확대하지 않는다. 기존 병렬 표의 준비 역할과 실제 제품 A/B 역할을 구분한다.
- 계획 채택 시점에는 기존 owner API를 임의 변경할 수 없다. M2/M3의 구체 D2 결정이 채택되면 해당 이행 단위부터 새 계약을 적용한다. 과거 source의 증거와 호환 정책은 당시 의미를 유지한다.

<a id="handoff"></a>
## 7. 에이전트 작업 입력과 변경 관리

문서 정비는 공통 의미·선행 관계를 바꾸는 D2다. [DP01](../policies/development-policy.md#section-2)에 따라 작성자와 다른 검토자가 실제 원문·diff·반례를 읽고 확인한다. 사용자가 확정한 방향 안의 통상 결정에 반복적인 사람 승인을 추가하지 않는다.

후속 에이전트의 첫 작업 입력은 다음과 같이 고정한다.

> 이 계획의 M0–M3를 수행한다. 최신 소스와 진행 작업을 확인해 영향을 받는 기능 확장의 보류·인계 범위와 실제 확인 상태를 기록한다. DEV3 내부의 현재 상태 소유·의존·접근 경로를 조사하고, 서로 다른 두 제품 기능이 같은 하위 계약을 사용해 독립 개발할 목표 구조와 API를 설계한다. [M2 설계 인계표](#m2-handoff)에 따라 M4의 유한한 공통 공급과 M5에 남길 A/B 기능 본문, C+A·C+B 단독 검증과 실제 합류 인수를 먼저 정한다. 선정 경계에 필요한 6절의 실제 소유 문서·JSON·개발 순서·작업 계약을 수정하고 독립 D2 검토를 받는다. 현 구조의 설명·파일명 변경안만으로 종료하지 않는다. 제품 구현·시험 실행은 이 문서 정비 작업에 포함하지 않는다. 기존 코드·병합·시험 증거는 보존한다.

각 실제 작업에는 다음을 기록한다.

| 구분 | 필수 내용 |
| --- | --- |
| 기준·소유 | HEAD와 dirty/index·관련 untracked digest, 작업/계약/통합 담당, 좁은 독점 경로, 공용 문서·등록 담당, 실제 동시 작업과 보류 확인 상태 |
| 제공·소비 | 공급할 계약/실제 API, 소비자 둘과 최초 호출 경로, 구현 착수의 최소 선행, 허용 대역·한계, 실제 공급을 기다릴 통합 인수 |
| 변경 영향 | 공유 상태·공통 API·형식·lock/자원·오류 의미의 변경 여부, 미공급 기능, 다른 담당이 수정해야 하는 파일과 이유 |
| 인수·인계 | 새/변경 코드의 case 선택 이유, 원 결과·미실행 의무, 별도 검토자, 병합 순서, 다음 담당이 받을 source/계약 identity |

공유 README·문서 색인·manifest·모듈 조립은 한 담당이 최신 원문에 좁게 반영한다. merge 충돌이 진행 중이면 해당 owner에게 인계하거나 분리된 문서 작업 범위로 진행하고, 충돌 표식을 임의 삭제하거나 상대 변경을 옛 사본으로 덮지 않는다. 별도 worktree도 같은 파일/의미를 바꿀 수 있으므로 독립성의 증거로 삼지 않는다.

M4의 각 이행 단위는 변경 전 소비 경로, 이동/추출한 책임, 변경한 API, 영향 case, 인계 revision을 갖는다. 한 단위의 실패는 해당 patch/commit을 기준으로 복구하며 다른 작업을 일괄 reset하지 않는다. 필요하면 수명이 정해진 내부 adapter를 두되 소유자·제거 조건을 기록하고 이중 권한/이중 정답 구현을 만들지 않는다. 이전 사내 DB 호환을 위해 영구 adapter를 추가하지 않는다.

M5 이후에도 같은 작업 입력을 사용한다. 새 소비 기능 때문에 다른 기능의 내부나 공통 의미를 바꿔야 하면 원인과 최소 공급을 먼저 기록하고, 해당 경계의 계약 변경을 별도 검토한다. 공통 파일 수정이 반복되면 기능 구현을 무조건 직렬화하는 대신 경계를 다시 평가한다. 이번에 인수한 경계를 다른 미검토 영역의 독립성 증거로 확대하지 않는다. 이 지속 적용 기준은 M3에서 병렬 정책·AI 작업 절차·작업 계약 예시에 반영한다.

<a id="acceptance"></a>
## 8. 모듈 독립성의 실제 인수

### M2에서 먼저 선정할 두 제품 작업

각 작업은 사용자 기능 또는 저장 기능의 관측 가능한 동작을 실제로 추가·변경해야 한다. 구현+oracle, 구현+리뷰, 문서+시험 도구를 두 제품 작업으로 세지 않는다. 코드 이동만 두 갈래로 수행하는 것도 해당하지 않는다.

후보는 현재 미완료 backlog의 tree/행 연산과 Catalog 조회·변경 등에서 고른다. **행과 index가 반드시 독립이라고 미리 가정하지 않는다.** 공통 tree/undo를 동시에 바꿔야 하면 그 최소 공급을 M4 선행으로 추출하거나, 같은 새 경계를 소비하는 다른 실제 기능을 선정한다. 각 후보의 의미 의존·소유 경로·완료 동작·필수 시험을 비교해 독립 검토자가 적합성을 확인한다. 쉬운 rename이나 관계없는 SQL lexer 작업으로 DEV3 내부 경계의 인수를 대신하지 않는다.

M2 종료 전에 A/B의 실제 기능·기준 계약·하위 공급·허용 변경 경로·공유 수정 목록·합류 소비자를 확정한다. 작업 분해에서 A와 B 각각에 실제 제품 동작을 구현하는 본문과 인수를 배정한다. 기능별 본문을 M4의 공통 담당에게 몰아 구현하고 M5에서 이름·연결·시험만 나누는 방식은 독립 개발 인수가 아니다. 선정할 수 없으면 막는 공통 공급과 한정된 해결 단위를 기록하고 M2를 미완료로 둔다. 후보를 실행 뒤 유리한 결과에 맞춰 교체하지 않는다.

### 단독 개발·검증 기준

실제 구현 착수의 공통 source·동결 계약·최소 컴파일 경계를 **C0**로 기록한다. C0에 대역이나 미공급 효과가 있으면 허용할 구현·시험 범위와 미실행 의무를 표시한다. M4에서 선정 경계의 실제 공급을 수신한 인수 기준을 **C**로 고정하고 C0→C의 공급 변경을 A/B 기능 변경과 구분한다. 기존 최소 공급으로 충분하면 C0와 C는 같을 수 있다.

A 단독 산출물은 **C+A**, B 단독 산출물은 **C+B**로 기록한다. 각 산출물에는 사전 지정한 해당 기능의 최소 연결·시험 코드만 포함하며 상대 기능의 미완료 코드·fixture·빌드 산출물에 의존하지 않는다. C0에서 시작하는 최초 기능 검증부터 상대 기능이 섞이지 않은 source를 사용한다. 대역으로 확인한 범위는 실제 공급의 인수와 구분한다. 별도 worktree 또는 고정 export 등으로 입력을 보존하되 수단 자체를 독립성의 증거로 삼지 않는다.

source·binary·fixture·출력과 필요한 build 경로를 구분하여 상대 작업의 변경이 실행 중 섞이지 않게 한다. 자원 간섭 때문에 시험을 순차 실행할 수 있으며, 제품 구현의 실제 겹침과 시험 동시 실행을 혼동하지 않는다. 합류 source는 **C+A+B+사전 지정 연결 변경**으로 추적한다. 최신 통합 기준을 수신할 때는 C 이후 차이를 분리해 영향만 검토한다. 공통 의미가 바뀌면 원 시도·결과를 보존하고 MI02에 따라 새 기준을 정하며, 조용히 rebase한 결과를 원래 기준의 독립 인수로 표시하지 않는다.

### M5 필수 기준

| ID | 판정할 사실 | 인수 조건 |
| --- | --- | --- |
| MI01 계약으로 독립 착수 | 동일 C0의 동결 계약과 최소 경계를 받은 A/B가 각자의 명세·API·시험 입력으로 구현 | 별도 구현 담당과 겹치는 실제 제품 구현 구간을 기록. 서로의 미완료 제품 본문이나 비공개 필드 수정을 기다리지 않음. C0→C 공급 대기와 실제 통합 의무를 별도 표시 |
| MI02 기능 변경의 국소성 | A/B의 전용 경로와 공용 변경 diff | 상대 기능 내부의 계획 밖 변경0. 공통 변경은 사전 지정한 연결/등록에 한정. 공통 의미 수정이 새로 필요하면 원 시도를 보존하고 M2/M4로 환류 |
| MI03 독립 인수 | C+A, C+B 각각의 실제 core·오류/수명·자원 경계 | 상대 기능이 포함되지 않은 고정 source와 실제 최소 공급에서 독립 기대값으로 선택 검증. 제품 코드와 oracle 신뢰 경계 보존 |
| MI04 실제 합류 | C를 수신한 A/B와 연결 변경을 합쳐 실제 소비 경로 연결 | 최신 통합 기준과의 차이를 기록하고 변경된 연결/신규 인수 코드에 필요한 상호작용·오류·복구를 확인. text merge 성공만으로 합격하지 않음 |
| MI05 보호와 추적 | 공유 파일·증거·반영 순서 | 다른 담당 변경/원 결과 덮어쓰기0. 텍스트 충돌과 의미 충돌, 계약 재개방·대기·재작업 원인을 따로 기록 |
| MI06 별도 판정 | 계약·실제 diff·원 실패·source별 결과 | 독립 D1/D2 검토의 미해결 필수 지적0. SKIP/중단/미공급/미실행을 PASS로 바꾸지 않음 |

MI02의 사전 등록 연결 수정은 통합 담당의 작업으로 남기며 기능 본문 공동 수정을 그 이름으로 숨기지 않는다. 연결 파일의 단순 text conflict는 해결 내용과 비용을 기록해 판정할 수 있지만, 미예정 공통 의미 변경으로 원 계약이 깨진 경우는 MI02를 충족하지 못한다. 판정 기준은 실행 전에 동결한다.

PD01은 이 A/B 사례에 연결한다. PD02/PD03은 작업 중 실제 공통 계약·문서/생성물 동시 변경이 있었다면 해당 사례로 판정하고, 없었다면 NOT_RUN으로 남긴다. M5 완료를 PD01–03 전부의 PASS로 확대하지 않는다. 새 인수는 원 PD 기록·과거 실패를 소급 수정하지 않고 source별 증거를 추가한다.

기록할 비용은 실제 시작/종료·계약 대기·공유 파일 조정·merge 충돌 해결·리뷰 재작업이다. 중첩한 작업자의 시간을 경과 시간으로 더하지 않는다. 대조군 없는 두 작업으로 병렬 속도 배율을 주장하지 않는다. 본 인수의 주장은 **선정한 두 기능의 독립 구현과 실제 합류 가능성**이며 전체 DBMS의 무간섭 보장이 아니다.

<a id="verification"></a>
## 9. 시험·검사와 증거 보존

- M0–M3 문서 작업에서는 제품 코드·시험·측정 실행을 시작하지 않는다. 독립 검토는 소스·계약·diff를 읽어 수행한다. 새 문서와 JSON을 추가했다는 이유로 문서 verifier를 실행하지 않는다.
- M4/M5는 마지막 검증 이후 **실제 코드 diff→영향 계약→선택 case**를 실행 전에 연결한다. 새 경계·이동/변경한 코드와 실제로 영향받은 소비 경로만 검증한다. 명칭·이동·구조 변경이라도 필요한 의미/컴파일 경계 검증은 하되 무관한 기존 시험을 포함하지 않는다.
- 단독 source는 최초 기능 검증부터 사용하며 병렬 인수용 재실행 단계를 추가하지 않는다. C0→C 수신은 실제 바뀐 공급·연결 경로의 인수를 선택하고, 완료한 동일 A/B 시험이나 무변경 C의 시험을 반복하지 않는다. 병합·source fingerprint·상위 단계 변경만으로 반복하지 않으며, 병합에서 코드가 바뀌면 그 diff와 관련 실패/미실행 사례를 선택한다. 새 합류 인수가 필요하면 M2에서 실제 연결을 확인할 신규 시험을 함께 설계한다.
- owner·pin·fence·WAL/flush·recovery의 의미/수명이 바뀌면 해당 변경에 필요한 현재-build 실제 I/O·동시성·재시작/재장애 인수를 배정한다. 무변경 DEV0/1/2 전체 회귀·workspace·하위 wrapper를 구조 전환 인수로 실행하지 않는다.
- 기존 DEV3 최종 S5의 확정 inventory와 전수 예외는 그대로 유지한다. M5는 S5를 대체하지 않으며 새 전수 실행 예외도 아니다. COMM·PF04의 별도 진단/시험 지시를 구조 변경의 무변경 회귀 허가로 확대하지 않는다.
- 문서 검사·전체 fmt/Clippy push gate는 [DOC-002](../policies/documentation-policy.md#push-gate)에 따라 실제 push할 tip commit에서 수행한다. 편집/commit 때 실행하지 않으며, Clippy 정적 검사와 기능 시험 결과를 구별한다.
- old/new source·시험 selector·binary·fixture·원 FAIL/UNKNOWN/NOT_RUN의 연결을 보존한다. 경로를 옮길 때 현재 링크와 역사적 원 경로의 매핑을 기록하고 과거 증거를 현재 결과로 재표기하지 않는다.

<a id="decision"></a>
## 10. 결정·실행 기록

| 항목 | 기록 |
| --- | --- |
| 결정 ID | XADB-MODULE-INDEPENDENCE-001 |
| 분류·상태 | D2 · ACCEPTED / 작업 계획과 문서 인계 범위에 한정 |
| 권한 근거 | 2026-10-26 사용자의 현재 구조 변경 방향 및 후속 에이전트 문서 정비를 위한 작업 계획서 작성 요청 |
| 작성·계약·통합 | `/root`; 구체 경계의 후속 담당과 실제 실행 A/B는 M0/M2에서 배정 |
| 선택 | 필요한 하위 의존·제품 불변식을 유지하면서 실제 소유 계약과 최소 공급을 정비. 준비된 계약 경계부터 두 제품 기능을 독립 개발하고 실제 공급을 수신해 합류를 확인 |
| 비교 대안 | 현 구조에서 인원만 늘리면 공통 수정 대기를 해결하지 못함. 파일/이름 이동만으로는 API와 소유 경계가 유지됨. 모든 역할의 별도 crate·전체 선설계는 검증/이행 범위를 불필요하게 확대. 실제 소비자 두 개를 기준으로 경계를 확정하는 안 선택 |
| 대체 범위 | 기존 초기 독립 개발 요구를 재확인. 현재 계약은 M3의 명시적 후속 결정으로 수정. 국소 정리만 허용하던 작업 순서의 관련 제한을 구조 전환 범위에서 대체할 대상으로 지정; 품질·제품 범위·S5/호환 의무는 유지 |
| 최초 별도 검토 | `/root/module_plan_review` · 2026-10-26 · 최초 계획 전체와 탐색 색인3파일의 변경, DP01/병렬 정책·관련 DEV3 소유/시험/국소 개선 계약 및 실제 WalRuntime·InstalledStorageCore·Plan4Retention 경계를 읽기 대조해 수용. 후속3개 문구 보완도 재확인, 당시 필수 수정0·차단0. `/root` 반영 확인. Git 집계 재산출·제품/문서 검사 실행 없음. 아래 재검토 수정본이나 M2/M3의 구체 API/계약 검토, M4/M5 실행 인수를 대신하지 않음 |
| 실행 | 계획서·색인 작성만. 소유 문서 일괄 개정·제품 구조 변경·시험·병렬 pilot·다른 채팅 중지 모두 미실행 |
| 재검토 조건 | 새 구조가 권한/원자성/복구·자원을 약화, 두 실제 소비자를 선정할 수 없음, 공통 의미 수정이 반복, adapter가 영구화, 무관한 선행/자동화가 확대되거나 후속 사용자 방향이 변경됨 |

후속 실행 기록은 M0–M5별 담당·기준 identity·산출물 링크·판정·잔여를 추가한다. 작업 시작 전은 NOT_RUN, 문서 채택 뒤 제품 미구현, 구조 변경 뒤 pilot 미실행을 각각 구별한다. 이 계획서의 존재를 개발 중지·구조 전환 완료·충돌 감소의 실행 증거로 사용하지 않는다.

<a id="review-002"></a>
### 2026-10-26 사용자 요청 재검토

기준 HEAD는 `27c2288b6ed7b48e556c179067348c686650b3f9`이며 계획서와 기존 색인3파일이 미커밋 상태였다. 사용자의 계획서 리뷰·즉시 수정 지시에 따라 `/root`가 원문과 관련 정책을 다시 대조하고, 최초 검토와 다른 `/root/module_plan_second_review`가 독립 D2 검토를 수행했다. 수정 대상은 이 계획서다.

| 발견한 문제 | 반영한 수정 |
| --- | --- |
| 개발 선행과 통합 선행을 나눈 원칙과 달리, 실행표는 M4 전체 실제 공급 뒤에만 소비 구현을 시작하는 것으로 읽힘 | 소비자별 구현 착수·허용 대역·실제 공급 인수를 M2에서 구분. 준비된 경계는 M4 잔여 공급과 병행하며 착수 C0와 실제 인수 C를 구별 |
| 공통 공급의 종료 범위가 추상적이어서 DEV3 잔여 작업이나 A/B 기능 본문까지 M4에 흡수할 수 있음 | 첫 전환의 유한한 공급 단위·완료 동작·제외 범위를 고정. A/B 각각의 실제 제품 로직은 해당 기능 담당에게 배정 |
| 각각 시험했다는 기록만으로는 상대 기능이 섞인 소스의 숨은 의존을 배제할 수 없음 | 최초 검증부터 단독 source를 사용하고 C+A·C+B 및 합류 source를 구분. 공급 변경·빌드/fixture 입력을 추적하되 완료한 무변경 시험은 반복하지 않음 |
| 보류 대상 지정만으로 실제 작업이 정지했거나 M0 기준이 계속 유효하다고 오해할 여지 | 실제 보류 확인·인계 상태를 별도 기록. 채택/착수 시 관련 변경만 다시 대조하고 미확인 작업과의 충돌 범위를 분리 |

**수정본 D2 수용 · 미해결 필수 수정0 · 차단0.** `/root/module_plan_second_review`가 수정 전후 diff와 수정본 전체, DP01/DP02·병렬 정책·문서 push gate·DEV3 공급/완료 규칙을 읽기 대조했다. C0/C 구분, 경계별 M4/M5 병행, A/B 실제 본문 배정, 단독 source와 실제 공급 인수, 변경 코드 시험 제한의 일관성을 확인했고 `/root`가 반영을 확정했다. 기존 수용 기록을 이번 수정본의 검토 근거로 재사용하지 않았다.

제품·시험·빌드·자동 문서 검사·커밋·다른 채팅 중지는 실행하지 않았으며, 이 재검토로 M0–M5 실행 상태를 승격하지 않는다. 구체 API·구조 구현·실제 병렬 개발의 인수는 후속 작업으로 남는다.

<a id="review-003"></a>
### 2026-10-26 M2 설계 인계 보완

| 항목 | 기록 |
| --- | --- |
| 기준·권한·담당 | HEAD `fc122f5b17ae6b88cd421f3384b00b40287f85eb`, 착수 시 작업 트리/index clean. 사용자의 “계획문서의 보완이 필요하면 해주세요” 요청. 작성·계약·통합 `/root` |
| 분류·상태·관계 | D2 · 보완안 ACCEPTED. XADB-MODULE-INDEPENDENCE-001의 M2 산출물 구체화이며 기존 목표·M0–M5 순서·MI01–06·제품/시험 의무를 유지 |
| 이유·선택 | 기존 원칙의 차단 결함은 발견하지 않았으나 A/B·API·공통 공급·인수 항목이 각 절에 분산되어 실제 설계 인계에서 연결을 누락할 위험이 있음. [설계 인계표](#m2-handoff)로 필요한 호출과 공급을 양방향 대조하고 선정 범위의 미정사항을 종료 조건에 연결 |
| 대안·범위 | 현재 조사 없이 A/B나 API를 미리 확정하는 대안은 배제. 별도 보고 체계 대신 기존 계약·작업 기록을 연결. 계획 본문·단계표·후속 작업 입력과 탐색 README2개만 보완. 문서 ID·경로·제목·역할은 같아 `documents.json`의 기존 등록 유지 |
| 검토·반영 | `/root/module_handoff_review`가 최신3파일 diff·계획 전체·AGENTS·DP01/DP02·코딩/병렬/문서 정책·하위 계층 순서·DEV3 시험 선정 규칙을 읽기 전용 대조하여 D2 수용, 미해결 필수 수정0·차단0. M2의 설계/배정과 M4/M5의 미구현/미실행 의무 구분, C0/C·병행·A/B 본문 배정·범위 한도·불변식 보존을 확인했고 `/root`가 반영을 확정. 과거 review-002의 수용을 재사용하지 않았으며 구체 API 설계·실제 공급·병렬 인수의 검증은 범위 밖 |
| 실행·재검토 | 계획·탐색 색인 보완에 한정. M0–M5 수행·제품 변경·테스트·빌드·자동 문서 검사·commit/push 없음. 표가 무관한 기능 선설계·범위 확대·중복 문서 작성의 선행으로 사용되면 재검토 |

2026-10-26 후속 사용자 지시에 따라 의미 있는 변경 단위마다 필요한 선택 검증·독립 검토를 마치고 커밋한다. 커밋 완료는 해당 부분공급의 저장이며 M4/M5 전체 완료를 뜻하지 않는다.

<a id="execution"></a>
## 11. 2026-10-26 계획 실행

2026-10-28 사용자 지시로 `codex/module-independence-finish`의 기존 작업을 `main`에 병합한다. 이후 이 계획의 개발·재개 기준은 **`/home/et16/work/xadb`의 `main`**이다. 채팅을 새로 열어도 실제 마지막 작업 위치·미커밋 상태·검증 source를 먼저 확인하며 기존 작업을 다른 기준선에서 다시 구현하지 않는다. [병합 기록](../evidence/module-independence-survey-20261026.md#merge-main-20261028)은 두 parent의 고유 변경·원 증거·선택 검증을 구별한다. 아래 과거 브랜치별 실행 기록과 수치는 당시 source에 귀속하며 병합 source의 실행 결과가 아니다. M4/M5 전체는 미완료다.

같은 날 후속 사용자 지시로 남은 작업은 main `854bd8d`에서 만든 별도 branch `claude/module-independence-remaining`에서 단계별 commit으로 진행한다([작업 지침](../../AGENTS.md)). 병합 전 stash 초안은 적용하지 않는다. 각 실행의 build target·임시 export·ELF는 결과 기록 뒤 삭제하고 commit/tree·파일별 hash·로그·결과로 source identity를 보존한다.

사용자의 후속 “이 계획서를 실행해 주세요” 요청으로 M0–M5 실행을 시작한다. 이 요청은 계획 작성만 허용했던 앞선 작업과 구별하며 M4/M5의 실행 지시를 포함한다. 기존 제품 불변식·독립 검토·변경 코드 시험·최종 S5 의무를 유지한다.

| 단계 | 현재 상태·근거 | 남은 종료 조건 |
| --- | --- | --- |
| M0 | [기준선 확인](../evidence/module-independence-survey-20261026.md#baseline) 완료, 별도 worktree 격리. `66ddf61` 재개와 main `b7ce51d` 확인. 다른 세션 중지 없음 | 후속 합류 때 실제 관련 diff 재확인 |
| M1 | [소유·의존 분류와 소비 후보](../evidence/module-independence-survey-20261026.md#survey)·[원 source 독립 검토](../evidence/module-independence-survey-20261026.md#review) 완료 | 선정 경계의 후속 관련 diff가 생기면 영향만 갱신 |
| M2 | 완료·ACCEPTED. [최종 독립 D2](../evidence/module-independence-survey-20261026.md#final-design-review) 수용·필수 수정0을 수신해 API·소비자·유한 공급·인수 설계 동결 | M4/M5에서 새 반례 또는 계약 변경이 생기면 영향 행만 재검토 |
| M3 | 완료·ACCEPTED. 소유 문서·운영/시험 JSON 채택 반영, MBC31 보완의 한정 재확인까지 독립 D2 수용·필수 수정0 | 구현에 따른 실제 공급 상태와 등록을 해당 diff에 맞춰 갱신 |
| M4 | IN_PROGRESS. C0 borrowed codec 선택6 PASS·[독립 수용](../evidence/module-independence-survey-20261026.md#c0-borrowed-codec), 생산자 타입 compiler4 자동 PASS·수명 거절1 독립 수동 수용·원 판정기 FAIL 보존([증거](../evidence/module-independence-survey-20261026.md#c0-producer-types)). [C1 실제 물리 WAL 분리](../evidence/module-independence-survey-20261026.md#c1-physical-wal)·컴파일3 PASS와 [원 owner/port 이전](../evidence/module-independence-survey-20261026.md#c1-owner-ports)·컴파일4/신규 메모리2 PASS에 이어 [control/isolation backing](../evidence/module-independence-survey-20261026.md#c1-isolation-backing)·컴파일3/변경·신규 runtime4 PASS·독립 한정 수용. [epoch backing 소유](../evidence/module-independence-survey-20261026.md#c1-epoch-backing)·compiler3/변경 runtime3 PASS·독립 D1/D2 한정 수용. [Catalog epoch 활성화](../evidence/module-independence-survey-20261026.md#c1-epoch-activation)는 fresh/최종 복구 seeding·CREATE/Commit을 구현하고 source별 선택 실행·원 FAIL·빈 Commit 범위 정정을 보존했다. [읽기 전용 거래/statement 수명](../evidence/module-independence-survey-20261026.md#c1-read-only-capsules)과 장기 credit을 추가하고 source별 compiler·선택 runtime7 결과를 보존했다. [C2 동기 Catalog 분리](../evidence/module-independence-survey-20261026.md#c2-catalog-read)는 기존 actual lookup 수신·source별 runtime7을 확인했으며 statement factory·short aggregate는 후속이다. [조회 수명/Commit Drain 연결](../evidence/module-independence-survey-20261026.md#c2-read-operation-lifetime)은 source별 선택13을 검증했으며 동시 byte 합은 아직 미공급이다. [Catalog 후보 scratch 재사용](../evidence/module-independence-survey-20261026.md#c2-catalog-scratch)은 중복 local buffer를 제거했으나 전체 peak는 그대로이며 aggregate 여유 확보로 승격하지 않는다. [거래·Aging 생성 수명 분리](../evidence/module-independence-survey-20261026.md#c2-transaction-source-phases)는 source별12+7회 선택 PASS와 lookup42850→41552B 감소·C09 설명 정정을 보존하고 최종 독립 D1/D2를 한정 수용했다. aggregate admission은 여전히 미공급이다. [Pending 보유/stage 분리](../evidence/module-independence-survey-20261026.md#c2-pending-carrier)는 원 pin 이동·Drain 후 guard 재구성과 선택11개 PASS·실행 설명 정정을 보존하고 최종 독립 D1/D2를 한정 수용했다. [CatalogFull source 예약](../evidence/module-independence-survey-20261026.md#c2-short-source)은 원 isolation header의 고정 charge와 선택17개를 검증했고 독립 D1/D2를 한정 수용했다. [상주 Catalog class](../evidence/module-independence-survey-20261026.md#c2-resident-read)는 무 I/O 경로·두 reader의 합55976B와 선택9개 PASS·원 FAIL을 보존하며 독립 D1/D2를 한정 수용했다. [최종 source 반환 수명](../evidence/module-independence-survey-20261026.md#c2-source-return)의 선택7개를 검증하고 독립 D1/D2를 한정 수용했다. [Commit phase 관측](../evidence/module-independence-survey-20261026.md#c2-commit-phases)은 cfg(test)1 PASS·원 compiler FAIL을 보존하며 당시 source의 Full/Pending layout 충돌을 확인했다. [Pending image 수명 분리](../evidence/module-independence-survey-20261026.md#c2-pending-image)는 준비 자료568B와 관련12 PASS·원 compile FAIL을 보존한다. [source 진입 수명](../evidence/module-independence-survey-20261026.md#c2-source-entry)은 progress→source→refs와 source 실패 정리·선택5 PASS를 보존한다. [Aging/Root source 계산](../evidence/module-independence-survey-20261026.md#c2-aging-source)은 관련3 PASS·import-only compile과 Full37724B를 보존하며 Full/Pending 합69100B는 여전히 한도 초과다. [WAL wait phase 관측](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source)은 cfg(test) 신규1과 반환 경계 보완1의 source별 PASS, Pending5320B/Full 합65300B를 보존하며 제품 charge는 유지한다. [Drained 실제 소비 경계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff)는 원 Drain/guard 재구성을 분리하고 normal Drop 순서·memory 복구 및 관련10 PASS를 보존한다. [Catalog CallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope)는 원 예약을 결과 소비까지 유지하고 selected query를 이동했으며 source별14+1+2·compiler4 PASS와 원 I/O fixture FAIL을 보존한다. 당시 Full38076B로 Pending 관측 합65652B였으며 [budget 행 borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-budget-rows)로 Full36972B·합64548B가 됐다. 같은 source의5+5 선택 PASS와 harness identity 원 FAIL을 보존하며 entry/tail·writer/progress의 전체 aggregate는 미공급이다. [progress Owner borrow 후보](../evidence/module-independence-survey-20261026.md#c2-borrowed-progress-rejected)는 원 identity를 바꿀 수 있는 반례로 기각·원복하고 후보14/compiler3 PASS를 별도 보존했다. [readonly replay source](../evidence/module-independence-survey-20261026.md#c2-readonly-replay-source)는 생산자 배열을 분리해 Full35895B와 source별6+계수 보완1 PASS를 확인했으나 전체 floor/phase 예약은 미완료다. [작은 Catalog 진입/정리](../evidence/module-independence-survey-20261026.md#c2-compact-call-boundary)는 실제 오류/consuming close를 공급하고 runtime25·compiler4 PASS와 Full35619B를 보존한다. 전체 bound floor·writer/progress phase 예약은 미완료다. [replay 작업 공간 대여](../evidence/module-independence-survey-20261026.md#c2-borrowed-replay-workspace)는 원 zero tail3328B를 actual observer에 빌려 Full33980B·선택9 PASS를 확인했고 원 compile FAIL/경고와 import-only build를 구별한다. 전체 entry/tail·writer phase는 미완료다. [split 고정 source 계산](../evidence/module-independence-survey-20261026.md#c2-split-source-charges)은 actual entry의 계산을 원 exclusive split으로 옮기고 실제3port·복구 caller와 ticket16B를 계수했다. 선택16 PASS·Full33972B/shared22400B이며 전체 entry/tail·writer phase는 미완료다. [예약 밖 호출 phase 관측](../evidence/module-independence-survey-20261026.md#c2-uncharged-call-phases)은 작은 source 실패 helper·선택4 PASS·원 test compile FAIL을 보존한다. 같은 원 core34입구의 compact floor15057B와 기존 wide tail은 cfg(test) 관측이며 제품 admission 미연결이다. [query 종료 경계](../evidence/module-independence-survey-20261026.md#c2-query-tail-boundaries)는 정상 소비 block·작은 오류 정리와 선택5 PASS, 정상710B/오류1394B 한 호출 관측을 보존하며 전체 floor는 미연결이다. [compact DPT](../evidence/module-independence-survey-20261026.md#c2-compact-dpt)는 원 private 배열3072B를 줄이고 helper660B를 actual source에 연결했다. r3 선택12·r2 원 표현/진단2 PASS와 기존 FAIL을 보존하며 shared19328B/Full33972B/Resident17588B·전체 floor 미공급이다. [LOAD claim borrow](../evidence/module-independence-survey-20261026.md#c2-borrowed-load-claims)는 원 conflict predicate·선택4 PASS와 compact floor14776B 관측을 보존하며 admission은 미연결이다. [예약 뒤 원 검산](../evidence/module-independence-survey-20261026.md#c2-admitted-validation)은 내부 입구 우선순위 D2를 한정 재개방하여 pool/snapshot 검산6958B를 원 Full/Resident 예약 안으로 옮겼다. 선택12 PASS·원 test-build FAIL을 보존하며 전체 floor·writer phase는 미공급이다. [statement Catalog 읽기](../evidence/module-independence-survey-20261026.md#c2-statement-reads)는 기존 Full/Resident Scope에 실제 statement를 연결하고 Found/Missing 자동 catalog bit를 공급한다. runtime10/compiler3 PASS·원 test-build FAIL을 보존하며 whole floor/writer는 미완료다. [apply 설치 권한](../evidence/module-independence-survey-20261026.md#c3-install-grant)은 Catalog3 actual producer와 legacy4 소비 bridge를 원 generic installer에 연결했다. source별 runtime3+7/compiler5 PASS·원 legacy fixture/test-build FAIL을 보존하며 독립 D1/D2 한정 수용이다. [Catalog terminal 소유](../evidence/module-independence-survey-20261026.md#c3-terminal)를 apply로 이전하고 실제 Busy와 관련 r3 runtime9/compiler4 PASS·r1/r2 FAIL을 보존했으며 독립 D1/D2 한정 수용이다. [Prepared 경계](../evidence/module-independence-survey-20261026.md#c3-prepared)는 실제 정의를 apply root로 이동하고 build2/compiler4 PASS·독립 D1/D2 한정 수용이다. [Root3 순수 codec](../evidence/module-independence-survey-20261026.md#row-root3-codec)은 신규4의 source별 PASS·원 FAIL을 보존했으며 독립 D1/D2 한정 수용이다. current Profile2를 유지한다. [Job3 cell·census](../evidence/module-independence-survey-20261026.md#row-job3-census)의 신규11 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Profile3·Row page 입구](../evidence/module-independence-survey-20261026.md#row-envelope-codecs)의 신규8 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Base4 raw layout](../evidence/module-independence-survey-20261026.md#row-base4-layout)는 별도 exact 입구의 r1 build2/runtime8 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Root3 Aging](../evidence/module-independence-survey-20261026.md#row-aging)은 별도 descriptor/후보의 r1 build2/runtime5 PASS를 보존하며 독립 D1/D2를 한정 수용했다. [Row 거래 ledger](../evidence/module-independence-survey-20261026.md#row-transaction-ledger)는 다중 root/statement의 pure 현재 잔여 검산과 source별9 selector PASS·원 fixture LSN 오류 FAIL을 보존하며 독립 D1/D2를 한정 수용했다. C3 전체는 미완료다. [Catalog Commit writer 창](../evidence/module-independence-survey-20261026.md#c2-writer-progress)은 terminal read gate와 stage/Pending/install/publish 고정 몫을 원 isolation ledger에 연결했고 r1 compiler3·runtime59 PASS·독립 D1/D2 한정 수용이다. 다른 writer·진입/반환 floor·Row progress는 미공급이다. [C0 Row 소비 경계](../evidence/module-independence-survey-20261026.md#c0-row-boundary)는 wrapper·수명·cursor credit을 컴파일 공급하고 compiler r1 14·r2 4, runtime r1 2·r2 5 PASS와 독립 검토 필수1 반영을 기록했다. 본문은 Unsupported다. 전체 C0·C1–C5 공급과 제품 인수 미완료 | 동결한 공급 단위의 구조 변경·관련 선택 검증·검토 |
| M5 | IN_PROGRESS. [A/B C0 제품 본문 착수](../evidence/module-independence-survey-20261026.md#consumer-c0-start): INSERT 검산/leaf 후보와 point/cursor 순수 본문을 같은 기준에서 병행하고 상대 source/fixture가 없는 첫 export에서 A10/B3 선택 PASS. actual 공급·단독/합류 제품 인수 NOT_RUN | C+A/C+B 실제 저장·조회·복구 수신, 각 단독/합류 인수·MI01–06 |

최초 실행 위치는 `/home/et16/.codex/worktrees/module-independence/xadb`, 출발 commit은 `b7ce51d2b3ee62b5b8dbc0d9c852a17827d76bd9`다. 2026-10-27 재개는 깨끗한 main `0852f06`을 확인하고, 기존 작업 공간이 다른 채팅 소유이므로 같은 commit의 `/home/et16/.codex/worktrees/module-independence-finish/xadb`로 격리했다. 이전 작업 공간과 원 artifacts는 읽기만 하며 다른 세션을 중지하지 않았다. 이번 단계의 권한·소유·동시 작업·미확인 세션은 [조사 기록](../evidence/module-independence-survey-20261026.md)에 보존한다. 계획의 전체 목표는 M5 인수까지 유지하며 문서 정비만으로 완료하지 않는다.

채택한 M2 계약은 [원 소유·소비 경계](../contracts/dev3-module-boundaries.md), [두 제품 소비자](../contracts/dev3-module-consumers.md), [Missing Row 참조 이동](../contracts/dev3-visibility-retirement.md#row-missing-traversal), [statement/cursor F02](../contracts/dev3-task-06-tree-rows.md#module-statement-cursor), [두 격리 수준의 공급](../contracts/dev3-task-10-concurrency.md#module-row-isolation), [자원 배정](../contracts/dev3-resource-bounds.md#row-missing-module-budget)에 둔다. 최종 독립 검토와 M2/M3 채택을 수신했으며 실제 공급·인수와 구별한다. 새 경로는 실제 구현 시에만 build inventory에 등록한다.

2026-10-27–28 이번 한 단계는 clean `main`·`4ed0cf9`에서 Aging/Root source 계산을 적응했다. 참고 `c4b9b6f`의 제품5파일과 main 전용 관측/선택 입력을 수신하며 다른 브랜치 전체·과거 실행 결과는 병합하지 않는다. [이번 증거](../evidence/module-independence-survey-20261026.md#c2-aging-source-main)에 원 실패·선택4개와 독립 검토를 구별한다. 다음 공급 의무는 실제 WAL 대기/Drain의 호출·반환 수명 구체화와 writer/progress·전체 동시 source admission이다.

2026-10-28 이번 한 단계는 clean `main`·`e76e8c4`에서 실제 WAL wait 호출·반환 source 관측을 적응했다. 원 core 포함 main 값에서 Full+Pending 합의65536B 초과를 확인했다. 제품 charge/admission은 변경하지 않으며 [선택1개 PASS와 합 기준 FAIL](../evidence/module-independence-survey-20261026.md#c2-wal-wait-source-main)을 구별한다. 다음은 Drain 완료와 content guard 인계 수명 분리, 이후 writer/progress·entry/return 전체 합의 공급이다.

2026-10-28 이번 한 단계는 clean `main`·`d2baeb9`에서 [Drain 완료/content guard 인계](../evidence/module-independence-survey-20261026.md#c2-drained-handoff-main)를 분리했다. 참고 `00feccd`의 필요한 코드만 main4phase/core 회계에 적응했으며 원 pins/Installing을 private Drained로 이동한다. 정상 미완료 Drop의 fence-before-pins와 선택11개1회 PASS를 확인했고 OCR 전체18파일의 독립 D1/D2 한정 수용·지적1건 수정·필수 수정0을 반영했다. 기존 Full+Pending 합 기준 FAIL은 보존한다. 다음은 writer/progress source 예약·entry/return 전체 합과 초과 해소이며 M4/M5는 미완료다.

2026-10-28 이번 한 단계는 clean `main`·`ae042ae`에서 [CatalogCallScope](../evidence/module-independence-survey-20261026.md#c2-call-scope-main)를 적응해 조회 결과 소비까지 원 예약/Drain 표시를 유지했다. 참고 `847def2`의 필요한 코드만 main의 core·반환 Result 회계에 맞췄다. 선택 query 수신·runtime18/compiler4 각1회 PASS와 현재 Full+Pending 합 기준 FAIL을 구별한다. OCR 전체23파일 독립 D1/D2 한정 수용·시험선택 지적1건 보완·필수 수정0을 반영했다. 다음은 Full 중첩 source 비용·진입/반환 전체 합 정리와 writer/progress 예약이며 M4/M5는 미완료다.

2026-10-28 잔여 작업 branch의 첫 단위는 `c855f1b`에서 [Catalog Commit의 terminal read gate와 writer source 창](../evidence/module-independence-survey-20261026.md#c2-writer-progress)을 공급했다. Commit은 새 read를 닫고 admitted read 반환을 기다린 뒤 stage 몫을 예약하며, Pending·publication에서는 read와 합산하고 Drain 뒤에만 install로 증가한다. 사전 독립 검토의 REQUIRED3을 실행 전에 해소했고 r1 compiler3·runtime59 각1회 PASS다. 다음 공급 의무는 나머지 MutationPort writer와 Row progress class의 ledger 참여, 진입 전·반환 뒤 floor이며 M4/M5는 미완료다.

2026-10-28 `b35150c` 이후 [실제 빈 쓰기 capsule](../contracts/dev3-module-boundaries.md#writable-empty-capsule)을 공급한다. 원 Begin actor·빈 statement·원 RollbackStart/AbortEnd를 연결했고 source별 선택 검증과 독립 D1/D2 한정 수용·필수 수정0을 확인해 이 단위를 commit한다. 자동 Status 선택과 own-actor Catalog 읽기·shared CREATE admission·일반 Row mark/rollback/commit·전체 entry/return floor·C3–C5 및 M4/M5 인수는 계속 미완료다. [이번 증거](../evidence/module-independence-survey-20261026.md#c3-write-capsule)를 재개 기준으로 사용한다.

<a id="m3-adoption"></a>
### D3-MODULE-ADOPTION-001 — 선정 경계의 소유 문서 채택

2026-10-26 · **D2 ACCEPTED / M2·M3 완료 / M4 C0 타입·C1 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급 / 공통 공급·제품 인수 미완료**. 검토 기준 `ae6fb6d`, 검토 결과 저장 commit `66ddf61`, 담당·계약·통합 `/root`, 독립 검토 `/root/module_handoff_review`. 사용자의 계획 실행 지시와 DP01의 기존 위임에 따라 [최종 독립 검토](../evidence/module-independence-survey-20261026.md#final-design-review)를 수신하고 소유 계약·JSON의 채택 상태를 반영했다. 당시 PROPOSED 중간 저장과 미해결 기록은 원 증거에 보존한다. 이번 채택은 실제 구현·compiler·runtime·native 인수 결과가 아니다.

선택은 기존 실제 owner를 보존한 C0–C5 경계를 각 정본에 수신하고 두 제품 본문을 별도 담당에게 맡기는 것이다. 기존 구조를 그대로 둔 등록/rename만으로 끝내는 대안과 전체 저장 모듈을 한 번에 재작성하는 대안은 채택하지 않는다. D3-DIRECTION-002의 **국소 개선만 허용한 작업 순서 제한**과 AI 효율 계획의 **두 번째 역할을 준비 작업으로만 제한한 부분**만 선정 범위에서 대체한다. 첫 행 목표·원자성·격리·복구/자원·DEV2 미완료·S5·호환·과거 source 증거는 유지한다.

Root3/Job3/Profile3·Row scope3·base4는 이름 변경의 부수 효과가 아니라 [Row 의미/dispatch](../contracts/dev3-module-consumers.md#format-dispatch)의 별도 D2 구체화다. 기존 [호환 정책](../policies/version-compatibility-policy.json)의 development fresh fixture와 현재-build 복구 의무를 유지하고, 과거 format의 자동 fallback/converter를 만들지 않는다. 기존 pure decoder·원 실행 결과를 보존하며 새 current dispatch/fixture/복구는 M4에서 함께 구현·검증한다.

| §6 소유 행 | 실제 반영 절 또는 미수정 근거 | 영향·검토·실행 상태 |
| --- | --- | --- |
| 병렬 정책·JSON | [계약 인계](../policies/parallel-development-policy.md#module-contract-handoff)와 관리 JSON의 module_contract_handoff | C0/C·제품 구현 중첩·첫 단독 source·MI01–06 수신; ACCEPTED·PD01–03 NOT_RUN |
| 코딩 컨벤션·JSON | 미수정. 기존 §4–5가 공유 의미와 상위 조립, 최소 visibility, API 수명/오류 완결성을 이미 요구 | 새 파일 크기/trait/crate 규칙 없음; 실제 M4 diff에 적용 |
| 저장소 배치·JSON | 미수정. 기존 engine 내부 역할·허용/실제/시험 의존 구별을 유지 | 신규 source 경로·fingerprint·export는 실제 M4 구현과 함께 등록 |
| 구현 계약·disk 저장 | [구현 계약의 모듈 전환](../contracts/implementation-contracts.md#module-transition), [disk 책임/API](../contracts/disk-storage-design.md#ownership) | C의 actual 권한/효과와 A/B 제품 소유 구분; ACCEPTED·actual 공급 미완료 |
| 세션·WAL·Commit | [세션](../contracts/dev3-storage-session.md#consumer-driven-refinement)의 한정 대체/원 소유, [WAL I/O](../contracts/dev3-wal-io.md)의 물리/의미 분리, [continuation](../contracts/dev3-commit-continuation.md)의 exact receipt/whole PinSet | 원 W/File/slot/fence·lease·Drain·epoch/Q 순서 보존; ACCEPTED |
| 해당 3.02–3.14 | [공급 설계도](../contracts/dev3-task-designs.md#dependency-map), 3.06 statement/cursor·3.09 CLR·3.10 격리·3.11 terminal·3.12 복구 및 참조/자원 owner | 기존 일반 allocation/index/checkpoint/회수 의무는 그대로. 직접 owner가 수신한 의미를 모든 task에 중복 정의하지 않음 |
| CREATE·plan4 복구 | [CREATE](../contracts/dev3-catalog-create.md)의 기존 claim/flush lease, [복구 owner](../contracts/dev3-plan4-recovery-owner.md)의 최초 census·typed Row action | 실제 Catalog/Row 상태·오류 공급; 기존 source 결과는 새 API 검증 아님 |
| DEV3 실행·공급 순서 | [한정 대체](dev3-implementation-plan.md#direction-refinement)·[A/B 배치](dev3-implementation-plan.md#parallel-execution-plan), 공급 설계도 | 두 제품 담당·공통 단일 작성·동시 구현2개 한도 보존; ACCEPTED |
| bottom-up·단계/완료 JSON | 미수정. 실제 하위 공급을 받은 통합·기능 인수, DEV2 partial·DEV3/S5 미완료와 최종 전수 예외가 이미 명확 | 내부 모듈 착수·M5를 새 단계 gate나 완료로 표시하지 않음 |
| 시험 계획·모듈 목록 | [MBC/MCA/MCB/MCI 배정](dev3-test-plan.md#module-independence-tests)과 module-test-matrix.json | 실제 diff→case, 첫 C+A/C+B, 새 합류와 원 native/S5 의무; 실행 NOT_RUN |
| DEV4 문서군 | 미수정. 기존 독립 frontend/model 착수와 실제 engine 뒤 Q1의 구분을 유지 | 아직 C의 내부 목표 API만 존재. DEV4 공개 공급/연결이 구현됐다는 변경 없음; 재개 시 실제 DEV3 공급 수신 |
| AI 절차·예시·효율 계획 | [작업 절차](../policies/ai-development-method.md#section-8), [예시](../examples/task-contract.example.json)의 C0/C/단독/합류·validation, [효율 계획](dev3-ai-throughput-improvement.md)의 제품 병행 | 예시는 PROPOSAL_EXAMPLE_NOT_EXECUTED 유지. 원 소비자 수·실측 시간·효율 결과 변경0 |
| AGENTS·진입점·색인 | 미수정. 현재 README에 구조 전환·M2/owner/인수 경로가 있고 새 두 계약은 폴더 README와 documents.json에 등록됨 | 신규 문서 ID/파일 없음. 기존 테스트·검토·push gate 유지 |
| 구현 상태 | [구현 색인](../implementation/README.md)에 C0 codec·생산자 타입과 C1 물리 WAL·원 owner/port·isolation/epoch backing·Catalog epoch 활성화 부분공급·전체 C0/C1–C5/M5 미완료를 표시 | `66ddf61`까지 제품 diff0. 후속 C0 실제 diff·선택 인수는 별도 기록하며 과거 실행 증거 승격0 |

별도 검토자는 최종 M2를 수용했고, M3의 module-test-matrix.json에서 누락된 MBC31을 추가한 뒤 해당 행을 한정 재확인하여 M3도 수용했다. 두 판정의 미해결 필수 수정은0이며 [검토 기록](../evidence/module-independence-survey-20261026.md#final-design-review)에 입력 identity와 범위를 보존한다. 구체 API가 actual source로 구현 불가능하거나 원 권한/격리/복구·자원을 약화하는 경우, 공통 의미 재개방이 A/B의 독립 착수를 막는 경우에 해당 행을 다시 검토한다. M4/M5의 compiler·runtime·native·source census와 MI01–06는 실제 구현 뒤의 미완료 의무다.

2026-10-28 `4ac878c` 이후 [첫 Catalog statement의 거래 완료 연결](../contracts/dev3-module-boundaries.md#catalog-capsule-completion)을 한정 수용했다. actual Forward를 등록한 원 capsule의 Commit/full abort와 변경 범위 검증·최종 독립 D1/D2를 마쳤으며, shared CREATE 제품 admission·Row/C4/C5·M4/M5 완료 목표는 축소하지 않는다. [source별 진행 증거](../evidence/module-independence-survey-20261026.md#c3-catalog-capsule)를 따른다.


2026-10-28 `9f36840` 이후 [원 statement의 shared CREATE 제품 예약](../contracts/dev3-module-boundaries.md#catalog-shared-create)을 구현·선택 검증한다. 원 reservation의 source/gate/flush·claim 보존과 private 반환을 공급하며 [source별 결과](../evidence/module-independence-survey-20261026.md#c3-create-admission)를 기록한다. source별 compiler4·신규 runtime9/source1과 최종 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 일반 Row/C4/C5·전체 floor·M4/M5는 미완료다.


2026-10-28 `830d053` 이후 [원 writable view의 Catalog own 조회](../contracts/dev3-module-boundaries.md#catalog-own-read)를 연결한다. 실제 Status head와 CREATE Undo를 검산하는 첫 CREATE 한정 공급이며 [source별 검증](../evidence/module-independence-survey-20261026.md#c3-own-catalog-read)과 독립 D1/D2 한정 수용·필수 수정0을 확인했다. 사용자 후속 지시로 이 문제 해결 뒤 새로운 개발은 개시하지 않는다. 일반 Row/CheckedTable/C4/C5·전체 short floor·M4/M5는 미완료다.

<a id="post-separation-gates"></a>
## 12. 분리 완료 후 추가 개발의 독립성 평가와 push·merge 강제 계획

2026-10-29 · **사용자 지시에 따른 후속 계획 저장 / 자동 검사·원격 강제 구현 NOT_RUN**

사용자는 모듈 독립성 평가를 push·merge 기준으로 강제하는 방안을 저장하고, 분리가 끝난 뒤 추가 개발을 시작할 때 적용하도록 지시했다. 여기서 분리 완료는 기존 M4 공급과 M5의 [MI01–06 인수](#acceptance)를 충족한 상태다. 다음 추가 개발의 작업 입력을 정할 때 이 절을 읽고 적용 범위·검사·증거를 배정한다. 이 후속 자동화를 현재 M4/M5 완료의 새 선행 조건으로 추가하거나, 이번 저장 요청으로 제품 개발·검사기 구현·원격 설정 변경을 시작하지 않는다.

목적은 기능 담당이 확정된 API로 개발하고, 경계 위반이나 필수 평가 누락이 있는 변경의 통합을 막는 것이다. 두 기능의 기존 인수는 선정 경계의 증거로 보존하며 다른 모듈의 독립성이나 개발 속도 개선을 자동 보증하지 않는다. 허용 의존·원자성·수명은 [공통 경계 계약](../contracts/dev3-module-boundaries.md), 실제 작업과 조정 기록은 [병렬 정책](../policies/parallel-development-policy.md#module-contract-handoff)을 정본으로 사용한다.

### 적용할 평가와 차단 조건

| 평가 | 측정·검증 입력 | 통합 차단 조건 |
| --- | --- | --- |
| 허용 의존과 접근 | 채택된 모듈 의존 방향, 실제 Rust 접근·재노출·feature별 의존, 관련 컴파일 성공/거절 사례 | 금지 의존·순환·내부 접근 또는 필수 분석 범위 누락. Cargo package 관계나 `use` 문자열 수만으로 같은 crate 내부 경계의 검증을 대신하지 않음 |
| 독립 착수 MI01 | 병렬 작업이면 같은 출발 계약, 별도 구현 담당, 실제 구현 구간·대기 사유와 source 이력 | 병렬 인수를 주장하면서 근거가 없거나 상대의 미완료 본문에 의존. 단일 기능 변경마다 두 담당의 병렬 실험을 새로 요구하지 않음 |
| 변경의 국소성 MI02 | 실제 diff와 사전 지정 전용 경로·공용 연결 범위; rename의 이전/이후 경로와 공통 의미 변경 검토 | 상대 기능 내부의 계획 밖 변경 또는 미검토 공통 계약 변경. 필요한 계약 변경은 원 시도를 보존하고 해당 설계·공급으로 환류 |
| 독립 인수 MI03 | 상대 코드·fixture·빌드 산출물을 제외한 단독 source와 해당 기능의 선택 검증 | 상대 구현 없이는 빌드·관련 인수 불가, 단독 source 목록 또는 필수 결과 누락 |
| 실제 합류 MI04 | 연결 diff와 실제 저장·조회·오류·복구의 영향 사례 및 source별 결과 | 필요한 상호작용의 FAIL·UNKNOWN·NOT_RUN·미도달. 텍스트 merge 성공만으로 통과하지 않음 |
| 증거 유효성 MI05 | 코드·관련 의존·시험·oracle·입력·설정 identity, 원 로그·결과와 현재 diff의 대응 | 다른 source의 결과 오용, 필수 증거 누락·덮어쓰기, SKIP·중단을 PASS로 표시 |
| 독립 판정 MI06 | 실제 변경 범위를 확인한 별도 D1/D2 검토와 지적 처리 기록 | 리뷰 누락·적용 범위 불일치·필수 지적 미해결. 작성자가 채운 reviewer/PASS 문자열만으로 독립 판정을 인정하지 않음 |

자동 판정이 어려운 공통 API의 의미·소유권 설계는 독립 리뷰가 판정하고, 자동 검사는 해당 리뷰의 대상·출처·결과가 유효한지 확인한다. 필수 평가 누락·판정기 오류는 차단하며, 적용 대상이 아닌 항목은 근거 있는 N/A로 구분한다. 미구현·시간 부족을 N/A로 바꾸지 않는다.

기능별로 계획 밖 상대 내부 변경 건수, 공통 계약 재개방 건수·이유, 계약 대기·조정·재작업 시간, 실제 텍스트/의미 충돌을 기록한다. 반복되는 공통 의미 수정은 경계 재평가 입력으로 사용한다. 파일 수·LOC·주관적 완료율 하나를 합격 점수로 쓰거나 근거 없는 개발 속도 개선율·시간 상한을 만들지 않는다.

### 강제 위치와 검사 정책 보호

- **로컬 push:** 기존 [commit export 검사](../policies/documentation-policy.md#push-gate)에 동일한 경계·증거 판정을 연결하는 안을 설계한다. 미커밋 수정으로 전송 commit의 실패를 가리지 않는다. 로컬 훅은 해제·우회할 수 있으므로 원격 강제의 증거가 아니다.
- **원격 main 반영:** 필수 검사와 독립 리뷰를 요구하는 보호 규칙을 적용하고 direct push·관리자·자동화 계정의 우회 범위를 통제한다. 작업 branch의 push 뒤 수행되는 CI를 이미 수락한 push의 사전 차단으로 표현하지 않는다. 지원 기능·권한·현재 설정을 시행 때 확인하고 실제 거절 결과로 적용을 입증한다. 로컬 merge의 성공도 원격 main 반영 허가와 구별한다.
- **규칙 자체의 변경:** 허용 의존·변경 범위·시험 선택·판정기·CI workflow·보호 규칙의 완화는 별도 D2 검토 대상으로 둔다. 변경자가 같은 patch에서 규칙을 넓히거나 자체 PASS를 작성해 우회하지 못하도록 신뢰할 검사 실행 주체와 승인 경로를 정한다. 필수 최종 판정은 누락·실패·미실행을 성공으로 취급하지 않는다.

검사 이름·허용 그래프 schema·증거 형식·검사 실행 주체·원격 보호 방식은 시행 전 D2 설계에서 확정할 항목이다. 현재 [push 검사기](../../scripts/check_push.py)·[workflow](../../.github/workflows/dev0.yml)와 원격 설정이 이 계획을 이미 집행한다고 기록하지 않는다. 현재 main 개발 위치를 포함한 운영 방식의 변경은 그 시행 범위에서 명시한다.

### 기존 시험 정책을 유지하는 도입 순서

1. M4/M5 완료 근거를 확인하고 추가 개발의 대상 모듈·담당·허용 API·전용 경로·사전 허용 연결을 작업 시작 전에 정한다. 기존 [저장소 정책](../policies/repository-build-plan.md)·[개발 운영 방침](../policies/development-policy.md#section-2)에 시행안을 반영하고 독립 D2 검토를 받는다. 실제 의존을 그대로 허용 목록으로 복사해 기존 위반을 승인하지 않는다.
2. 좁은 경계·증거 검사기를 구현한다. 정상 변경과 금지 의존, 미등록 내부 수정, 상대 fixture 혼입, 오래된 결과, 필수 증거 누락, 검사기 오류가 각각 올바르게 통과·차단되는지 신규 검사기 관련 사례로 검증한다.
3. 로컬 훅과 원격 main 반영 조건에 연결하고 보호된 시험 ref 등으로 거절·수락을 확인한다. 실제 설정·검사 source·결과를 남긴 뒤 자동 강제 활성 상태를 기록한다. 원격 강제가 미적용이면 그 미완료를 드러내며 로컬 검사만으로 대체 완료하지 않는다.
4. 추가 기능의 일반 통합에는 해당 diff의 경계·증거·리뷰만 요구한다. 단계 완료 판정에는 전체 필수 의무의 유효한 증거가 있는지 확인하되, 완료한 무변경 시험을 일괄 재실행하지 않는다. 이전 source의 증거는 그대로 보존하고 관련 코드·의존·시험 조건이 유지되는 경우 재사용 근거를 연결한다. 변경·실패·신규 상호작용만 기존 사용자 지시의 범위에서 선택하며 DEV3 최종 S5 예외는 그대로다.

이 계획의 저장·검토는 검사기 검증이나 원격 보호 적용 증거가 아니다. 신규 문서를 만들지 않았으므로 `documents.json`의 기존 문서 ID·경로·역할을 유지하고 진입점에서 이 절로 연결한다. 작성·통합 담당은 `/root`이며 후속 시행의 실제 담당·기준 source·결과는 착수 때 배정한다.

2026-10-29 `/root/module_gate_plan_review`가 이 절·상단 안내와 `AGENTS.md`·두 README의 실제4파일 diff를 MI01–06·DP01/DP02·DOC-002·병렬 인계 정책과 읽기 대조하여 **후속 계획 저장에 한정한 독립 D2 수용·필수 수정0**으로 판정했다. 검토 기준 HEAD는 `09173aae2da1a8d43000881d9cebc5ab442cd2cc`, 이 판정 문구 반영 전 계획 파일 SHA-256은 `b70566e67bda3511b23112bde9bb770c9b80f8daa3a90e44048c8517863d1857`이다. `/root`가 판정을 수신해 기록했으며 후속 검사 설계·구현·원격 적용과 M4/M5 인수는 이 검토의 범위가 아니다. 제품·시험·빌드·자동 문서 검사·push는 실행하지 않았다.

<a id="folder-reorganization"></a>
## 13. 분리 모듈의 폴더 재정리 후속 계획

2026-10-29 · `XADB-MODULE-LAYOUT-001` · **D2 ACCEPTED / 후속 계획 문서 반영 완료 / 제품 이동은 [L1](../evidence/module-independence-survey-20261026.md#l1-allocation-move)·[L2](../evidence/module-independence-survey-20261026.md#l2-concurrency-move)·[L3](../evidence/module-independence-survey-20261026.md#l3-mixed-system-move) 적용, [L4](#l4-record)는 현재 배치 유지, [L5](#l5-classification)는 분류 완료·[L5-A](../evidence/module-independence-survey-20261026.md#l5a-allocation-move)·[L5-B](../evidence/module-independence-survey-20261026.md#l5b-install-move)·[L5-C](../evidence/module-independence-survey-20261026.md#l5c-status-move)·[L5-D](../evidence/module-independence-survey-20261026.md#l5d-recovery-move) 이동 완료, [L6](#l6-wal-io-tests)의 `storage::wal_io` 시험·지원 자손5개 [이동 완료](../evidence/module-independence-survey-20261026.md#l6-wal-io-move), [R1–R3 논리 경계](#r123-logical-boundaries)는 R1 유지·R2 미사용 재노출 제거·R3 유지로 판정(독립 검토 ACCEPT·필수 수정0), 시험 본문 NOT_RUN**

사용자는 파일 분리에 비해 폴더 정리가 부족한 부분을 먼저 조사하도록 요청했고, 참조 관계·상태 소유·API가 독립성의 핵심인지 의견을 확인했다. 이어 임시 계획을 저장하고 진행 중인 작업 종료 뒤 기존 문서에 반영하도록 지시했으며, 후속 “진행해 주세요” 요청에 따라 이 절을 작성한다. **이번 실행 범위는 후속 계획과 관련 문서의 정비다.** 제품 파일 이동·참조 변경의 구현은 아직 시작하지 않는다.

### 기준 source와 적용 범위

임시 조사 기준은 `21592e9285ff5116116bdbc414d82a5dce74031c`였다. 반영 기준은 main `517208d4cddd721db5415fd2e32b759a1ad114ba`, tree `daa9542e8b24968dc289615b918caa4d15a85a18`이다. 진행 중이던 Job 검산 작업은 local 채팅 `01a182cb-7080-72b3-963a-e54cbdfeb554`의 turn `01a18355-afc1-7651-b9de-63dd6b1a3e5f`에서 commit·검토·정리를 마쳤다는 최종 보고를 수신했고, 현재 checkout의 clean 상태와 해당 채팅의 종료를 읽기 확인했다. 별도 활성 관련 편집은 관측하지 않았다. 종료된 것은 그 작업 단위이며 전체 M4/M5 인수는 미완료다.

두 기준 사이의 제품 변경은 Row Job 검산과 그 연결·시험에 해당하며 아래 후보 배치는 유지된다. 현재 `storage/` 직속 Rust 파일86개 중 파일명에 `test`가 포함된 시험·지원은44개, `storage/wal_io/` 직속77개 중 같은 분류는41개다. 하위 폴더를 제외한 탐색 밀도 관측이며 파일 수로 의존성·결함·생산성 개선을 판정하지 않는다. 이미 나뉜 `row/`, `semantic_row/`, `semantic_catalog/`, `dirty_frame/`는 유지 후보로 둔다.

이 계획은 기존 M4/M5의 전체 범위나 종료 조건을 늘리지 않는다. 제품 배치 작업을 시작할 후속 지시가 있을 때 최신 source와 동시 작업을 확인해 후보별 범위를 확정한다. 현재 공급과 겹치는 후보는 해당 작업의 인계 뒤 진행하며, 전체 폴더 재정리를 A/B 착수의 새 선행으로 요구하지 않는다. [12절의 자동 강제](#post-separation-gates)는 계속 M4/M5 인수 뒤 추가 개발의 별도 계획이다.

### 판단 기준과 목표

독립성은 **필요한 하위 계약만 참조하고, 기능 변경이 소유 경계 안에 머무는지**로 판단한다. 폴더는 그 책임을 탐색 가능하게 표현한다. 실제 기준은 [코딩 컨벤션](../policies/coding-convention.md#section-4), [배치 정책](../policies/repository-build-plan.md#module-folder-layout), [채택된 허용 의존](../contracts/dev3-module-boundaries.md#target-imports)을 따른다.

- **배치 정리:** 같은 책임의 구현·지원·시험을 모으고 반복 접두어를 폴더로 표현한다. 논리 모듈 경로·production/test cfg·visibility·시험 selector와 제품 의미를 유지한다.
- **논리 의존 변경:** 다른 모듈의 내부 필드·구체 타입 참조, 초기화 조상 의존, 역방향 호출·재노출 사슬을 검토한다. 필요한 최소 API·원 소유·생성 권한·수명·오류 효과를 확정한 별도 D2 단위로 다룬다.

소유가 다른 파일을 접두어만으로 합치거나, 파일 이동을 위해 권한을 넓히거나, 공통 가변 manager·상태 복제로 결합을 숨기지 않는다. 폴더 진입점은 선언·좁은 재노출·명시적 조립을 맡으며, 줄 수나 타입당 파일 수를 목표로 삼지 않는다. 물리 WAL이 Catalog/Row 의미를 알지 않아도 되는 의존 방향과 실제 기능 변경의 국소성이 평가 대상이다. `use` 문자열 수 감소·새 facade·별도 crate·텍스트 merge 성공만으로 독립성 향상을 주장하지 않는다.

### 후보별 목표 배치와 선행 확인

아래 경로는 `crates/engine/src/` 기준이며 **목표 후보는 미구현**이다. 실행 직전 전체 자손·지원·fixture/tool 참조를 포함한 파일별 이동표를 확정한다. 우선순위는 후속 정리 내부의 순서이며 제품 기능 개발 순서를 바꾸지 않는다.

| 순서·단위 | 현재 파일군 | 목표 후보 | 논리 경계와 실행 전 확인 |
| --- | --- | --- | --- |
| 1 / L1 | `storage/wal_io/allocation_reservation.rs`, `allocation_operation.rs`, `allocation_budget.rs`, 관련 시험·지원 | `storage/wal_io/allocation_reservation/{mod,operation,budget,tests,native_tests,resource_tests}.rs`를 중심으로 배치 | 실제 `bootstrap::doublewrite_install::allocation_reservation`의 operation/budget/test 자손이다. 논리 부모·`pub(super)`를 유지하고 operation 안의 추가 자손과 경로 참조까지 매핑한다. |
| 1 / L2 | `transaction/concurrency.rs`, `concurrency_commit.rs`, `concurrency_read.rs`, 관련 시험·지원 | `transaction/concurrency/`의 진입점·`commit.rs`·`read.rs`·지원·시험 | 실제 `transaction::concurrency` 자손을 모은다. commit/read 각각의 시험과 support 접근, 다른 모듈의 호출·compiler fixture 경로를 함께 확인한다. |
| 2 / L3 | `storage/wal_io/mixed_system_*` | `storage/wal_io/mixed_system/`의 초기화·owner·history·load·resources·시험 | init은 `status_init_operation::mixed`, owner는 `status_flush::mixed`(`status_home_flush.rs`가 선언)이고 history/load/resources는 owner의 자손이다. 물리 분류를 묶더라도 논리 부모는 보존한다. 같은 부모로 옮길 필요가 있으면 그 부분을 별도 의존 변경으로 분리한다. |
| 2 / L4 | `storage/wal_io/plan4_apply.rs`, `plan4_apply/`, `plan4_retention/`의 실제 하위 구현 | apply의 실제 소유와 이행 중 legacy에 맞춘 폴더 대응 | apply가 retention 경로의 create_pair·visibility·control·history·read 등을 연결한다. 각 소유와 이행 의무를 확인하고 중복 배치·잔여 이유를 기록한다. `plan4_pages`의 production `apply`와 test `retention` 선언/alias 및 원 selector를 보존하며 retention 전체를 일괄 재부모화하지 않는다. |
| 3 / L5 | `storage/wal_io/`의 설치·할당·Status·복구·세션 조립 책임 | 책임별 폴더와 필요 시 상위 배치의 별도 설계 | 원 초기화 조상을 유지하는 채택 범위와 대조한다. 우선 조사 결과를 `현재 배치 유지 / 물리 이동 / 논리 경계 재설계`로 닫고, 후속 D2는 실제 소비자와 해결할 내부 참조를 지정한 유한 단위만 연다. 전체 DEV3 재설계를 이번 계획의 선행으로 추가하지 않는다. |

### 이행 순서와 산출물

1. **기준·소유 확인:** HEAD·dirty/index·관련 작업·source별 검증을 확인한다. 이미 정리된 후보는 재실행하지 않고 결과를 수신한다. 각 단위에 담당·공유 조립 파일·허용 경로·되돌림 범위를 지정한다.
2. **참조와 이동표 확정:** `현재 물리 경로 → 목표 물리 경로 → 논리 mod 경로 → 상태 소유자 → provider/consumer → visibility → 영향 case`를 작성한다. 실제 Rust import/타입 의존 그래프와 생성→정상→오류/정리→복구의 소유·호출 흐름을 구별한다. cfg별 `#[path]`, 부모 private 접근, raw factory/seal, tool·fixture·source census 참조를 확인한다.
3. **배치 단위 실행:** L1/L2부터 영향이 닫힌 단위를 선택하고 L3/L4는 소속 차이를 먼저 해소한다. 배치·선언 변경을 기능 본문 수정과 섞지 않는다. 이동을 위해 의미 변경이 필요하면 해당 부분을 별도 계약 변경으로 돌린다. 모듈 조립·현재 링크·실제 source inventory는 실제 이동과 함께 갱신한다.
4. **논리 의존 단위 설계:** L5 및 앞 단계에서 분리한 항목은 실제 소비자를 기준으로 최소 API와 권한 인계를 정한다. 원 pool·WAL·fence·pin/lease·복구 책임·자원 한도를 보존하고 별도 D2 검토 뒤 구현 단위를 확정한다. 구체적인 재설계를 이 절의 후보 표만으로 채택하지 않는다.
5. **변경 범위 검증·인계:** 실행 전 실제 diff→영향 계약→선택 case를 연결한다. 배치만 바뀌면 영향받은 production/test cfg의 컴파일·모듈 연결·접근 제한을 확인하고 의미·수명·오류 경로가 바뀐 부분에는 필요한 시험을 배정한다. 기존 통과 시험·무변경 하위 회귀·workspace 전체를 반복하지 않는다. 각 단위의 source·선택 결과·독립 리뷰·현재/원 경로 대응을 보존한다.

새 경로는 실제 구현 때만 관리 JSON의 source inventory에 등록한다. 과거 증거의 원 경로·revision·hash·selector와 FAIL/UNKNOWN/NOT_RUN은 소급 수정하지 않는다. 필요한 현재-build 복구 검증은 실제 소유·수명·효과 변경과 연결하며 배치 정리 명목으로 의무를 면제하지 않는다. 문서 verifier와 전체 fmt/Clippy gate는 기존 push 시점 정책을 유지한다.

<a id="l1-move-table"></a>
### L1 파일별 이동표

2026-10-29 · **이동표 확정(B) / L1 이동 적용·정적 검증 PASS / 시험 본문 NOT_RUN**

사용자 지시 “L1 이동표 작성”에 따라 위 이행 순서의 1–2단계를 L1에 적용했다. 사용자는 이어서 “권장으로 진행해 주세요”라고 지시해 배치 B를 확정했다. 이 표는 실행 전 계획이며, 실제 이동과 검증 결과는 [L1 배치 적용 증거](../evidence/module-independence-survey-20261026.md#l1-allocation-move)에 있다.

**기준·소유.** 기준은 main `082d329a28a46fbf201f0276b319a840af1b4929`, tree `dd41b0a5bbd610877bf4e1a9a4767feab509a70c`이며 checkout은 clean이고 `origin/main`과 같다. 다른 worktree `/home/et16/work/xadb-dev4`는 main의 조상 `7327e7a`에 detached·clean 상태라 L1 파일의 동시 편집은 없다. 적용하지 않기로 한 C2 초안 `stash@{0}`은 `allocation_budget.rs`·`allocation_operation.rs`·`plan4_allocation_operation.rs`를 수정하므로, 이동 뒤에는 rename을 반영해야만 다시 적용할 수 있다. 작성 담당은 이 Claude 세션이다. 아래 경로는 `crates/engine/src/storage/wal_io/` 기준이며 `AR`은 논리 경로 `crate::storage::wal_io::bootstrap::doublewrite_install::allocation_reservation`이다.

**배치 형식 — B 확정(이동표 작성 당시 비교).** 13절 후보는 `allocation_reservation/{mod,operation,budget,tests,native_tests,resource_tests}.rs`이고 당시 [코딩 컨벤션 §4](../policies/coding-convention.md#section-4)는 `name/mod.rs` 확장을 썼다. 반면 당시 `crates/engine/src`의 실제 폴더 모듈은 진입 파일을 폴더 옆에 두는 `name.rs`+`name/` 형식이 38개, `mod.rs` 형식이 4개였다. `wal_io`에서는 `plan4_pages`·`plan4_recovery`(`tests.rs`+`tests/` 포함)·`plan4_apply`·`empty_transactions`와 하위의 `plan4_retention/control.rs`+`control/`이 앞 형식이고 [XADB-NAME-001](../evidence/naming-cleanup-20261025.md)도 이 형식으로 옮겼다. `mod.rs` 형식은 `recovery_effect/` 하나였다.

- **B(권장):** 진입 `allocation_reservation.rs`는 제자리에 두고 자손7개만 `allocation_reservation/`로 옮긴다. 공유 조립 파일 `doublewrite_install.rs`를 수정하지 않고, 한 단위에 `mod.rs`가 셋 생기지 않는다. 채택하면 단위별 예외로 남기지 않고, §4 문구를 실제 배치에 맞출지를 별도 D2로 정한다.
- **A(§4를 자손까지 적용):** 진입도 `allocation_reservation/mod.rs`로 옮긴다. 13절 후보는 `operation.rs`·`tests.rs`를 파일로 두지만, §4를 자손에도 적용하면 자손을 가진 둘은 `operation/mod.rs`·`tests/mod.rs`가 된다. `doublewrite_install.rs:22`의 `#[path]` 1줄, 진입 파일을 가리키는 Markdown 링크2개와 JSON 경로4개를 추가로 고친다.

두 안 모두 논리 모듈 경로·visibility·cfg·시험 selector가 같다. 아래 표는 B 기준이다.

2026-10-29 후속 사용자 지시에 따른 §4 문구 정리는 [XADB-CODE-LAYOUT-001](../policies/coding-convention.md#module-entry-layout)에서 독립 D2 수용·미해결 필수 수정0으로 채택했다. 기본 배치 B와 기존 `mod.rs` 유지 범위를 코딩 컨벤션·저장소 정책에 명시하는 문서 작업이며, L1–L3·L5의 실제 이동이나 검증을 반복하지 않는다. R1–R3·L4 재검토·분류 밖 5개 파일·M4/M5는 후속 범위로 유지한다.

#### 이동과 선언

| 현재 파일(줄) | 목표 파일(B) | 논리 경로 | 선언 위치와 새 `#[path]` | cfg |
| --- | --- | --- | --- | --- |
| `allocation_reservation.rs`(541) | 유지 | `AR` | `doublewrite_install.rs:22`, 변경 없음 | production |
| `allocation_operation.rs`(1757) | `allocation_reservation/operation.rs` | `AR::operation` | `allocation_reservation.rs:15` → `"allocation_reservation/operation.rs"` | production |
| `plan4_allocation_operation.rs`(432) | `allocation_reservation/operation/plan4_retention.rs` | `AR::operation::retention` | 이동한 `operation.rs:1756` → `"operation/plan4_retention.rs"` | production |
| `allocation_budget.rs`(103) | `allocation_reservation/budget.rs` | `AR::budget` | `allocation_reservation.rs:19` → `"allocation_reservation/budget.rs"` | production |
| `allocation_reservation_tests.rs`(1124) | `allocation_reservation/tests.rs` | `AR::tests` | `allocation_reservation.rs:23` → `"allocation_reservation/tests.rs"` | `test` |
| `allocation_budget_profile_tests.rs`(295) | `allocation_reservation/tests/budget_profiles.rs` | `AR::tests::budget_profiles` | 이동한 `tests.rs:13` → `"tests/budget_profiles.rs"` | 부모의 `test` |
| `allocation_native_tests.rs`(510) | `allocation_reservation/native_tests.rs` | `AR::native_tests` | `allocation_reservation.rs:27` → `"allocation_reservation/native_tests.rs"` | `test`·linux |
| `allocation_resource_tests.rs`(150) | `allocation_reservation/resource_tests.rs` | `AR::resource_tests` | `allocation_reservation.rs:31` → `"allocation_reservation/resource_tests.rs"` | `test`·linux |

`#[path]`로 읽힌 파일 안의 `#[path]`는 그 파일의 폴더를 기준으로 한다(`plan4_recovery/tests.rs`의 `"tests/cases.rs"`와 같은 형식). `plan4_allocation_operation.rs`는 이름에 plan4가 있지만 논리 부모가 `operation`이므로 L4가 아니라 이 단위에서 옮긴다. `plan4`는 XADB-NAME-001이 실제 bootstrap 종류로 유지한 이름이므로 새 파일 이름에도 남긴다. `status_init_operation::retention`(`plan4_status_init_operation.rs`)은 이름만 같은 별도 모듈이며 이동하지 않는다. 기능 본문·`super` 사슬·`use`는 바꾸지 않는다. 대상8개 파일에 `include!`·`file!`·`module_path!`·`cfg_attr(path)`는 없다.

#### 소유·제공·소비

소비자는 `allocation_reservation` 경로, `doublewrite_install`·`bootstrap`·`wal_io`의 재노출 이름, 조상 import 별칭을 거친 항목 이름을 grep으로 모은 파일 목록이다. L1 내부 파일은 뺐다. `operation`의 `Batch`·`SemanticError`·`TerminalOutcome`처럼 `use super::*`와 부모 import를 거쳐 쓰이는 이름은 독립 검토에서 추가로 찾은 파일까지 넣었으므로, 그 목록은 확인된 하한이다. rustc 이름 해석 결과가 아니므로 실행 때 컴파일로 최종 확인한다. 경로는 `wal_io/` 기준이며 `storage/`로 시작하면 `crates/engine/src/` 기준이다.

| 논리 모듈 | 상태 소유 | 제공 | 실제 소비자 | visibility(유지) |
| --- | --- | --- | --- | --- |
| `AR` | static 없음. `AllocationSession`이 이동받은 설치 `DoublewriteStorageSession`과 예약 `AllocationState`를 소유하고, 그 core의 fence·`reader_active` flag를 읽고 바꾼다 | `AllocationSession`, `AllocationObservation`, `AllocationSessionError`, `Phase`, `Cause`, `into_allocation_session` | 운영7: `storage/wal_io.rs`, `bootstrap`, `allocator_eviction`, `dirty_allocation`, `mixed_system_owner`, `mixed_system_resources`, `status_init`. 시험6: `allocator_residency_tests`, `recovery_effect/native_tests`, `recovery_prefix_native_tests`, `status_bootstrap_tests`, `status_init_tests`, `undo_bootstrap_tests`. borrow probe6: `crates/engine/tests/fixtures/dev3-allocation-reservation/compiler/*.rs` | private `mod`, 항목 `pub(crate)`, `doublewrite_install` 재노출 |
| `AR::operation` | 보관 상태 없음. `PreparedAllocation` 생성 봉인과 allocator before/after 검산 | `SemanticError`, `AllocationPopulation`, `OperationBudget`, `MixedInitKind`, `Batch`, `Plan`, `validate_*`, plan4/Catalog base bridge | 운영26 이상: `storage/plan4_recovery_base.rs`, `storage/plan4_recovery_base/catalog/graph/ledger.rs`, `storage/wal_io.rs`, `bootstrap`, `dirty_allocation`, `empty_transactions/{history,load,pressure}`, `fixed_system_operation`, `flush_io`, `mixed_system_history`, `mixed_system_init`, `mixed_system_load`, `operational_status`, `plan4_apply`, `plan4_retention/{history,population}`, `plan4_status_init_operation`, `recovery_effect/{physical,types}`, `recovery_empty_transactions`, `recovery_prefix`, `status_home_flush`, `status_init`, `status_init_operation`, `status_load`. 시험13 이상: `storage/plan4_recovery_base_bridge_tests.rs`, `dirty_allocation_test_support`, `mixed_system_init_tests`, `operational_status_tests`, `plan4_apply/owner_tests`, `plan4_genesis_tests`, `recovery_effect/operational_tests`, `recovery_prefix_tests`, `resident_status_tests`, `status_home_flush/home_tests`, `status_init_operation_tests`, `status_init_resource_tests`, `status_init_tests`. compiler fixture `dev3-mixed-system-init/compiler/owner_drop.rs` | `pub(super) mod`, 항목 `pub(in super::super)`·`pub(crate)`, `doublewrite_install`→`bootstrap`→`wal_io` 재노출 |
| `AR::operation::retention` | 보관 상태 없음. plan4 reserve/cancel 준비 값과 검산 | `RetentionAllocationPlan`, `RetentionReservation`, `RetentionAllocationState`, `PreparedRetentionAllocation`, prepare/validate/materialize, source-bytes 상한 | 운영2: `plan4_retention/population`, `plan4_status_init_operation`. 시험1: `plan4_apply/owner_tests` | `pub(crate) mod`(실효 범위는 부모의 `pub(super)`) |
| `AR::budget` | 상태 없음. 고정 live-value 상한 | `COMMON`…`CONSTRUCTOR` 상수, `fits` | 운영10: `allocator_residency`, `empty_transactions`, `empty_transactions/{live,pressure}`, `mixed_system_history`, `mixed_system_init`, `mutable_storage`, `recovery_prefix`, `status_init`, `status_init_operation`. 시험4: `allocator_residency_native_tests`, `dirty_allocation_tests`, `recovery_prefix_tests`, `status_init_resource_tests`. L1 내부의 `AR`·`operation`·`retention`·`resource_tests`도 쓴다 | `pub(super) mod`, 항목 `pub(in super::super)` |
| `AR::tests` | 시험 지원. 보관 상태 없음 | `#[test]` 9(AR01–AR08, constructor 경계)와 `cfg(test)` hook `Point`·`TerminalCapture`·`Hook`·`stage_cut`·`cut` | `AR` 본문의 `cfg(test)` 경로가 hook을 호출한다. 시험은 `bootstrap::test_support`와 fixture `dev3-allocation-reservation`을 쓴다 | private `mod`, hook `pub(super)` |
| `AR::tests::budget_profiles` | 시험 | `#[test]` 1 | fixture `dev3-dirty-allocation`을 쓴다 | private |
| `AR::native_tests` | 시험 | `#[test]` 2(`allocation_native_profiles`·`allocation_native_faults`) | `scripts/dev3_allocation_native.py`의 `MODULE` selector | private |
| `AR::resource_tests` | 시험 | `#[test]` 2 | `scripts/check_dev3_allocation_stack.py`의 `TEST` selector, `bootstrap::test_support` | private |

#### 영향 case와 실행 때의 처리

| case | 연결 | 처리 |
| --- | --- | --- |
| `AR::tests`의 AR01–AR08·constructor 경계, `budget_profiles` | 이동 파일의 시험 | 시험 발견 목록의 동일성만 확인한다. 본문은 그대로이므로 NOT_RUN |
| `native_tests` 2개 | `scripts/dev3_allocation_native.py`의 논리 selector | 목록에서 selector를 확인한다. native 실행 NOT_RUN |
| `resource_tests` 2개 | `scripts/check_dev3_allocation_stack.py`의 논리 selector 1개 | 목록에서 selector를 확인한다. 실행 NOT_RUN |
| borrow probe baseline과 6사례 | `scripts/check_dev3_allocation_borrows.py`. 기대 오류 위치는 probe 파일의 줄이다 | 논리 경로와 probe 파일 위치가 그대로이므로 NOT_RUN |
| mixed-init `owner_drop` | `scripts/check_dev3_mixed_init_borrows.py`. primary span은 probe 파일이다 | 같은 이유로 NOT_RUN |
| 위 표의 다른 소비자 모듈 | 논리 경로·재노출 | production·test cfg 컴파일로 확인 |

#### 경로 참조의 갱신과 보존

| 대상 | 처리(B) |
| --- | --- |
| 현재 Markdown 링크 | `contracts/dev3-allocation-reservation.md:159`(budget)와 `contracts/dev3-task-03-allocation.md:78`(operation)를 새 경로로 바꾼다. 진입 파일 링크2개(task-03·task-14)는 그대로 유효하다 |
| 현재 서술 | `contracts/dev3-recovery-control-base.md:41`의 `allocation_operation.rs` 표기를 현재 경로로 바꾼다 |
| 증거 Markdown 링크 | `evidence/dev3-allocation-resource-review-20261022.md:13`의 budget 링크는 문서 검사의 깨진 링크 거절 대상이다. XADB-NAME-001 선례대로 링크만 현재 위치로 바꾸고 원 경로·기준 commit·변경 전 SHA-256을 덧붙이며 원 byte는 로컬 `artifacts/maintenance/`에 보존한다 |
| 관리 JSON | `repository-build-plan.json`의 L1 경로 문자열29개 중 진입 파일4개를 뺀25개를 새 경로로 바꾼다. XADB-NAME-001도 `implemented_paths`·`modified_paths`를 현재 경로로 갱신했다 |
| 보존 | 증거 파일14개의 원 경로 표기(위 링크1개만 예외), `implementation/dev3-implementation.md:593`의 역사 서술, 이 절의 후보 표 |
| 변경 없음 확인 | `scripts/`는 논리 selector만, compiler fixture와 borrow probe는 논리 경로만 쓴다. `.github`·Cargo manifest의 참조는 없다 |

#### 실행 때의 검증 선택

1. **정적 대응:** `crates/` 안의 `git diff -M`에서 이동7개와 `#[path]` 7줄(진입5·operation1·tests1)만 바뀌어야 한다. 이동만 하는 5개(`plan4_retention`·`budget`·`budget_profiles`·`native_tests`·`resource_tests`)는 아래 기준 SHA-256과 byte가 같아야 하고, 진입·`operation`·`tests`는 지정한 줄만 달라야 한다.
2. **컴파일:** `cargo check -p xadb-engine --all-targets --all-features --locked`로 production·test cfg(linux 포함)의 모듈 연결과 접근 제한을 확인한다.
3. **시험 발견:** 이동 전·후 `cargo test -p xadb-engine --lib --all-features --locked -- --list`의 전체 출력이 같아야 한다. 그중 `allocation_reservation::` 행의 정적 예상은14개이며 실제 수는 이동 전 목록으로 확정한다. script selector 3개(native2·resource1)가 목록에 있는지도 확인한다.
4. **시험 본문:** 본문은 그대로이고 위치 attribute만 바뀌므로 NOT_RUN으로 기록한다. 기존 결과는 원 source의 증거로 유지한다.
5. **fmt·Clippy·문서 검사:** 기존 push gate에서 수행한다.
6. **독립 리뷰와 기록:** 실제 diff와 이 표를 대조하고, 결과는 [조사 증거](../evidence/module-independence-survey-20261026.md)에 새 anchor로 남긴다. build target·임시 export는 결과 기록 뒤 삭제한다.

되돌림은 하나의 commit 단위로 한다. commit 전이면 이동7개와 수정 파일을 기준 HEAD로 복원하고, commit 뒤면 그 commit을 revert한다. 허용 경로는 위 8개 파일과 새 폴더, 갱신 표의 문서·JSON·증거 Markdown 각 해당 줄, 이 계획과 조사 증거다.

기준 SHA-256(`082d329`): `allocation_reservation.rs` `6d8d1ba52723a452838112fe9f948b7fe44fa41f2015e6855de3273c5f551cbb`, `allocation_operation.rs` `aa3a49b8e29b6c272f6f432f5c47ae8e3fccc82afc8394f3155d118bf24868b7`, `plan4_allocation_operation.rs` `4e3a05306a28f9eeebb0e6f966826beaaa2c5be6fc719ac2a582cda3d98ed08d`, `allocation_budget.rs` `5eb34f0804bbe3ac69aa4964aade3de17ee3414861ad88085d70d8b6ab7c8426`, `allocation_reservation_tests.rs` `42984184eacbe1e8e8143e52be5497dbe463bf9067dd6b4cd311f7ef235e9f2b`, `allocation_budget_profile_tests.rs` `fd6c140f4c8ff34470caf144b29f0d21ace70a56d96571999d0b3aff7a089dfd`, `allocation_native_tests.rs` `ec501278cf1d632d6e95cfb097c657c4782a0bd6460b7c307d4125cc74d6db03`, `allocation_resource_tests.rs` `ed3187e5f595ffa4100e10f7843f254fe9a9d386d7ed93b4b296ba8efaa4615f`, `doublewrite_install.rs` `a26734af6f3a2c97844b178ba96687df8342c57a8064c5535eef3a53d74b01a5`.

#### L1 범위 밖 관찰

`AR::operation`은 L1 밖의 운영 파일 26개 이상, `AR::budget`은 운영 파일10개가 `super` 사슬·crate 절대 경로·조상 import 별칭·재노출로 참조한다(위 소비자 표). `operation`의 일부 항목은 `doublewrite_install`→`bootstrap`→`wal_io` 재노출을 거쳐 `wal_io` 밖의 `plan4_recovery_base`까지 이어진다. 배치 정리는 이 참조를 바꾸지 않는다. 최소 API로 좁힐지는 위 이행 순서 4단계에서 실제 소비자를 기준으로 판단할 입력이다.

**2026-10-29 정정([R1 판정](#r123-logical-boundaries)).** 위 “운영 파일 26개 이상”은 grep 하한이었다. rustc census로 확인한 L1 밖 운영 파일은 28개다. 내역은 `doublewrite_install` 자손 23개, 재노출 중계 3개(`wal_io.rs`·`bootstrap.rs`·`doublewrite_install.rs`), `wal_io` 밖 2개(`storage/plan4_recovery_base.rs`·`plan4_recovery_base/catalog/graph/ledger.rs`)다. `wal_io` 경계를 넘는 항목은 6개뿐이다. 이 문단 위의 기록은 당시 판단으로 보존한다.

**독립 검토.** 이 세션이 띄운 별도 검토 subagent가 초안을 실제 source·정책·선례와 읽기 전용으로 대조해 **ACCEPT WITH REQUIRED FIXES**로 판정했다. 이동 대상8개, `#[path]` 해석, 줄 수, SHA-256, JSON 29/25개, 링크, 증거14개, 명령 형식은 일치했다. 필수 지적2건은 소비자 목록 누락과 `AR::tests`의 hook 제공 누락이다. 선택 지적7건은 상태 소유 표현, 영향 case, 검증 단계 정의, A의 명칭과 §4 처리, 파일 이름의 plan4, C2 stash, 파일 수 표현이다. 작성자가 각 지적을 source로 다시 확인한 뒤 반영했다. 같은 검토자의 반영본 재검토는 다른 수정을 모두 확인했지만, `operation` 소비자 목록에서 glob import를 거친 운영6·시험4개 파일의 누락을 필수 지적으로 남겼다. 작성자가 해당 줄을 확인해 10개 파일을 추가하고 목록을 하한으로 표기했다. 이 마지막 반영은 이동 실행 뒤의 [최종 검토](../evidence/module-independence-survey-20261026.md#l1-allocation-move)에서 확인됐다. 이 절의 검토 시점에는 제품 이동·빌드·시험을 실행하지 않았다.

<a id="l2-move-table"></a>
### L2 파일별 이동표

2026-10-29 · **이동표 / L1에서 확정한 배치 B 적용 / 실행 결과는 증거 문서**

사용자는 L1 완료 뒤 “작업을 이어서 해주세요”라고 지시했고, push는 모든 작업과 추가 리뷰를 마친 뒤로 미뤘다. L1에서 확정한 배치 B를 그대로 쓴다. 기준은 main `b691fc74689dd1275d1e92b946f56984208cb591`, tree `ad13f6644c4caa60835c372c75346a4d5a998eae`이며 checkout은 clean이다. 아래 경로는 `crates/engine/src/transaction/` 기준이고 `CC`는 논리 경로 `crate::transaction::concurrency`다.

진입 `concurrency.rs`는 `transaction.rs:6`의 `pub(crate) mod concurrency;`가 `#[path]` 없이 읽으며 제자리에 둔다. 진입 안의 `#[path]`는 inline module 밖에 있으므로 진입 파일의 폴더 `transaction/`이 기준이다. 이동한 `commit.rs`·`tests.rs`는 `#[path]`로 읽히므로 그 안의 `#[path]`는 새 폴더 `concurrency/` 기준이다.

#### 이동과 선언

| 현재 파일(줄) | 목표 파일 | 논리 경로 | 선언 위치와 새 `#[path]` | cfg |
| --- | --- | --- | --- | --- |
| `concurrency.rs`(743) | 유지 | `CC` | `transaction.rs:6`, 변경 없음 | production |
| `concurrency_commit.rs`(492) | `concurrency/commit.rs` | `CC::commit` | `concurrency.rs:14` → `"concurrency/commit.rs"` | production |
| `concurrency_commit_tests.rs`(122) | `concurrency/commit/tests.rs` | `CC::commit::tests` | 이동한 `commit.rs:491` → `"commit/tests.rs"` | `test` |
| `concurrency_read.rs`(703) | `concurrency/read.rs` | `CC::read`(inline `tests` 포함) | `concurrency.rs:17` → `"concurrency/read.rs"` | production |
| `concurrency_test_support.rs`(477) | `concurrency/test_support.rs` | `CC::test_support` | `concurrency.rs:35` → `"concurrency/test_support.rs"` | `test` |
| `concurrency_catalog_test_support.rs`(98) | `concurrency/catalog_test_support.rs` | `CC::catalog_test_support` | `concurrency.rs:41` → `"concurrency/catalog_test_support.rs"` | `test` |
| `concurrency_tests.rs`(1962) | `concurrency/tests.rs` | `CC::tests` | `concurrency.rs:742` → `"concurrency/tests.rs"` | `test` |
| `concurrency_resource_tests.rs`(134) | `concurrency/tests/resource.rs` | `CC::tests::resource` | 이동한 `tests.rs:1797`(`NORMAL` 줄바꿈 뒤) → `"tests/resource.rs"` | 부모의 `test` |

`concurrency_tests.rs:12,14`의 `include_str!("../../../../tests/fixtures/dev3-concurrency/…")`는 파일 위치 기준 상대 경로다. 한 단계 깊어지므로 `../../../../../`로 바꾼다. 같은 이름의 `crates/engine/tests/fixtures/dev3-concurrency/`가 있으므로, 새 문자열이 원래와 같은 저장소 루트의 `tests/fixtures/dev3-concurrency/*.tsv`를 가리키는지 실행 때 실제 경로로 대조한다. 다른 L2 파일에는 `include!`·`file!`·`module_path!`·`cfg_attr(path)`가 없다. 기능 본문·`super` 사슬·`use`는 바꾸지 않는다.

#### 소유·제공·소비

소비자 수는 `crates/engine/src`에서 `concurrency::`·`transaction::concurrency` 경로를 쓰는 파일을 grep으로 센 근사값이며 L2 파일은 뺐다. `crates/engine/tests/fixtures`의 compiler fixture 약80개도 같은 논리 경로를 쓰며 위치에 의존하지 않는다. rustc 이름 해석 결과가 아니므로 실행 때 컴파일로 최종 확인한다.

| 논리 모듈 | 상태 소유 | 제공 | 실제 소비자 | visibility(유지) |
| --- | --- | --- | --- | --- |
| `CC` | DEV3 operation admission과 version-edit gate의 원 core·slot 상태 | admission·gate 타입과 `commit`·`read` 항목의 `pub(crate) use` 재노출 | 113개 파일(이름상 운영44·시험69). `storage/dirty_frame`, `dirty_flush`, `wal_io`의 `plan4_apply`·`plan4_retention`·`plan4_pages`·`plan4_recovery`·`status_*`·`empty_transactions` 등 | `pub(crate) mod` |
| `CC::commit` | 원 core를 이어 받는 private common continuation. 의미 Commit receipt는 만들지 않음 | `CommitInstalling`, `CommitStage`, `PendingInstallation`, `PrepareCommitError`, source-bytes 상한 등 | `CC`의 재노출을 거친 위 소비자, `commit::tests` | private `mod` |
| `CC::read` | read-operation 수명 동안 원 admission slot 표시와 checked ticket | `ReadOperationAdmission`, `TerminalReadGate`, source-bytes 상한 등 | `CC`의 재노출을 거친 Catalog adapter 등 | private `mod` |
| `CC::test_support` | `cfg(test)` 관측·fault와 모의 Commit 소유 전이. WAL은 다루지 않음 | `Installation`, `PausePoint`, `Resume`, `poison_state` 등 | `CC`, `read`, `CC::tests` | `pub(super) mod` |
| `CC::catalog_test_support` | 실제 소비자 관측만. 권한·상태를 만들거나 바꾸지 않음 | `hit`, `Point` 등 | 34개 파일(운영 파일의 `cfg(test)` hook 10개 포함, `plan4_apply`·`plan4_retention`·`plan4_recovery/tests/catalog_memory`) | `pub(crate) mod` |
| `CC::tests` | 시험 | `#[test]` 11, fixture `dev3-concurrency/*.tsv` | `scripts/check_dev3_concurrency_runtime.py`의 `transaction::concurrency::tests::` selector | private |
| `CC::tests::resource` | 시험 helper. `#[test]` 없음 | `type_ceilings`, `all_live_control` 등 | `CC::tests`(`tests.rs:1803,1805`) | private |
| `CC::read`의 inline `tests` | 시험 | `#[test]` 10 | — | private |
| `CC::commit::tests` | 시험 | `#[test]` 1 | — | private |

시험은 `CC::tests` 11, `read`의 inline 10, `commit::tests` 1로 모두 22개다.

#### 경로 참조와 검증

| 대상 | 처리 |
| --- | --- |
| 현재 Markdown 링크 | `contracts/dev3-concurrency-admission.md:174`(resource 시험), `contracts/dev3-task-10-concurrency.md:302`(commit) |
| 현재 서술 | `contracts/dev3-concurrency-admission.md:20–22`의 파일 역할 표 3행, `policies/repository-build-plan.md:629`의 `transaction/concurrency_read.rs` |
| 관리 JSON | L2 경로 문자열35개 중 진입 파일9개를 뺀26개 |
| 보존 | 증거 파일4개(JSON3·Markdown1)의 원 경로 표기. 링크가 아닌 본문 표기라 문서 검사 대상이 아니다 |
| 변경 없음 확인 | `scripts/`와 compiler fixture는 논리 selector·probe 파일 위치만 쓴다. `tests/fixtures/dev3-concurrency/manifest.json`에는 `.rs` 경로가 없다 |

검증은 L1과 같다. `crates/` diff는 rename 7개, `#[path]` 7줄, `include_str!` 2줄뿐이어야 하고, 이동만 하는 5개(`commit/tests`·`read`·`test_support`·`catalog_test_support`·`tests/resource`)는 기준 SHA-256과 같아야 한다. `cargo check -p xadb-engine --all-targets --all-features --locked`와 이동 전후 `cargo test -p xadb-engine --lib --all-features --locked -- --list` 전체 출력의 동일성을 확인한다. 시험 본문, runtime script, borrow probe는 본문·논리 경로·probe 위치가 같으므로 NOT_RUN이다. `include_str!`이 읽는 fixture byte가 같다는 것은 실제 경로 대조로 확인한다.

기준 SHA-256(`b691fc7`): `concurrency.rs` `1cb7c28602a6b66addf605fa75cf75513931aad0990d659184664ec6a55315cf`, `concurrency_commit.rs` `10c3245ca5d79a43a2a2cefe414c007288c68a5b4e2ae46373e72b75b69aaa47`, `concurrency_commit_tests.rs` `ebe20fc93f0c333ca4e8ffa564322e7f47cd18b5a4a6b7a97c8c4c2959d4c1ee`, `concurrency_read.rs` `b4d77422aa142f57b18b0fa8f8b2e909c4c2374efdeecb850ece9e31c92e0f22`, `concurrency_test_support.rs` `8f6aa2a939cb8c865077fb366a7f661a2485636a5e1c9e16e9bc05914debb5d0`, `concurrency_catalog_test_support.rs` `11ef50fdf68f1f78d42916ba9a2006aa9553796137b0bfedff06ed5bff7c20c5`, `concurrency_tests.rs` `4d0f0176ae71ea4da39991edc82ad4e933afd45f7e1e33cf73021ae35c30c8ff`, `concurrency_resource_tests.rs` `1629d2889b637c4bdf1f106ff8d260ddf4cf14a3c159b4b0f43f8bd44956efe5`, `transaction.rs` `fcbd03dbd368d21483c800820e36350edc5f41aee49ef52151b16eda52b20b4b`.

<a id="l3-move-table"></a>
### L3 파일별 이동표

2026-10-29 · **이동표 / 논리 부모별 배치(권장안) / 실행 결과는 증거 문서**

L2 뒤 L3의 두 안을 제시했고 사용자는 “권장으로 진행”을 선택했다. 이어 “권장으로 쭉 진행해 주세요, 권장이 없고 결정이 꼭 필요한 경우에만 멈추고 질문해 주세요”라고 지시했다. 기준은 main `d251b8f62b77e16ca92cffefc691bbe13511ffe8`, tree `8f3143a67cedebb7cb48479a432dab91079972fe`이며 checkout은 clean이다. 아래 경로는 `crates/engine/src/storage/wal_io/` 기준이고 `MS`는 논리 경로 `crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage`다.

**논리 부모.** 위 후보 표는 `mixed_system_*` 10개를 `mixed_system/` 한 폴더에 모으는 안이었다. 실제 선언을 대조하면 10개는 서로 다른 하위 트리 4개(owner·init·init 시험·recovery 시험)에 속하고, 직접 논리 부모는 6곳이다.

| 파일(줄) | 논리 경로 | 선언 위치 | cfg |
| --- | --- | --- | --- |
| `mixed_system_owner.rs`(378) | `MS::status_init::status_flush::mixed` | `status_home_flush.rs:36` | production |
| `mixed_system_history.rs`(392) | `…::mixed::history` | `mixed_system_owner.rs:11` | production |
| `mixed_system_load.rs`(365) | `…::mixed::load` | `mixed_system_owner.rs:13` | production |
| `mixed_system_resources.rs`(170) | `…::mixed::resources` | `mixed_system_owner.rs:15` | production(`#[test]` 1) |
| `mixed_system_owner_tests.rs`(1767) | `…::mixed::tests` | `mixed_system_owner.rs:19` | `test` |
| `mixed_system_owner_native_tests.rs`(392) | `…::mixed::tests::native` | `mixed_system_owner_tests.rs:1132` | 부모의 `test`·linux |
| `mixed_system_owner_load_tests.rs`(234) | `…::mixed::tests::load_addendum` | `mixed_system_owner_tests.rs:1766` | 부모의 `test` |
| `mixed_system_init.rs`(199) | `MS::status_init_operation::mixed` | `status_init_operation.rs:43` | production |
| `mixed_system_init_tests.rs`(897) | `MS::status_bootstrap_tests::undo::mixed_init` | `undo_bootstrap_tests.rs:7` | `test` |
| `mixed_system_recovery_tests.rs`(753) | `MS::recovery_prefix::effect::tests::mixed` | `recovery_effect/tests.rs:1199`의 `"../mixed_system_recovery_tests.rs"` | `test` |

**채택: 논리 부모를 따르는 배치.** owner 자손 6개는 owner 진입 파일 옆 `mixed_system_owner/`로 옮긴다(배치 B). recovery 시험은 선언한 `recovery_effect/tests.rs`의 폴더로 옮긴다. 그 부모의 다른 시험 자손 `operational_tests.rs`·`empty_transaction_tests.rs`가 `recovery_effect/`에 `<이름>_tests.rs`로 있으므로 같은 형식의 `mixed_system_tests.rs`로 하고, `recovery_effect/tests.rs`에서 상위 폴더를 가리키던 `../` 경로가 없어진다. `status_home_flush/native_tests.rs:589–598` 등 다른 `../` 선언은 L3 범위가 아니다. init 두 파일은 그대로 둔다. 부모 `status_init_operation.rs`에는 `fixed`(`fixed_system_operation.rs`)·`retention`(`plan4_status_init_operation.rs`) 형제가 있고, `undo_bootstrap_tests.rs`에는 `native_undo` 형제가 있다. `mixed`만 옮기면 형제가 갈라지므로 이 부모들의 폴더화는 설치·Status 책임을 다루는 L5에서 판단한다.

비교한 대안은 후보 표대로 10개를 모두 `mixed_system/`에 모으고 논리 부모를 `#[path]`로 유지하는 것이다. 그러면 init·init 시험·recovery 시험 3곳에서 물리 폴더와 논리 부모가 어긋나고, 선언 파일 `status_init_operation.rs`·`undo_bootstrap_tests.rs`도 고쳐야 한다. 두 안 모두 논리 경로·visibility·cfg·시험 selector는 같다.

#### 이동과 선언

| 현재 파일 | 목표 파일 | 선언 위치와 새 `#[path]` |
| --- | --- | --- |
| `mixed_system_owner.rs` | 유지 | `status_home_flush.rs:36`, 변경 없음 |
| `mixed_system_history.rs` | `mixed_system_owner/history.rs` | `mixed_system_owner.rs:11` → `"mixed_system_owner/history.rs"` |
| `mixed_system_load.rs` | `mixed_system_owner/load.rs` | `mixed_system_owner.rs:13` → `"mixed_system_owner/load.rs"` |
| `mixed_system_resources.rs` | `mixed_system_owner/resources.rs` | `mixed_system_owner.rs:15` → `"mixed_system_owner/resources.rs"` |
| `mixed_system_owner_tests.rs` | `mixed_system_owner/tests.rs` | `mixed_system_owner.rs:19` → `"mixed_system_owner/tests.rs"` |
| `mixed_system_owner_native_tests.rs` | `mixed_system_owner/tests/native.rs` | 이동한 `tests.rs:1132` → `"tests/native.rs"` |
| `mixed_system_owner_load_tests.rs` | `mixed_system_owner/tests/load_addendum.rs` | 이동한 `tests.rs:1766` → `"tests/load_addendum.rs"` |
| `mixed_system_recovery_tests.rs` | `recovery_effect/mixed_system_tests.rs` | `recovery_effect/tests.rs:1199` → `"mixed_system_tests.rs"` |
| `mixed_system_init.rs`, `mixed_system_init_tests.rs` | 유지(L5에서 판단) | 변경 없음 |

`mixed_system_owner.rs`는 `#[path]`로 읽히므로 그 안의 `#[path]`는 `wal_io/` 기준이고, 이동한 `tests.rs`의 `#[path]`는 새 폴더 `mixed_system_owner/` 기준이다. `recovery_effect/tests.rs`는 `recovery_effect/mod.rs`의 `mod tests;`로 읽히며 inline module 밖의 `#[path]`는 그 파일의 폴더 `recovery_effect/` 기준이다. 세 `#[path]`는 모두 inline module 밖에 있다. 대상 파일에 `include!`·`include_str!`·`file!`·`module_path!`·`cfg_attr(path)`는 없고, fixture는 `env!("CARGO_MANIFEST_DIR")` 기준으로 읽으므로 위치와 무관하다.

#### 소유·제공·소비

소비자는 owner 표면의 타입 이름과 `status_flush::mixed` 경로를 grep으로 모은 파일 목록이며 L3 파일은 뺐다. rustc 이름 해석 결과가 아니므로 확인된 하한이고 실행 때 컴파일로 최종 확인한다.

| 논리 모듈 | 상태 소유 | 제공 | 실제 소비자 | visibility(유지) |
| --- | --- | --- | --- | --- |
| `…::mixed`(owner) | static 없음. 원 binding·pool·flush continuation 위의 plan3 actor이며 이전 Status actor나 가변 frame을 밖으로 내지 않는다 | `MixedSystem`·`MixedSystemBinding`·`MixedSystemFlusher`·`MixedSystemWriter`·`MixedSystemReader`·`MixedSystemReadGuard`·`ReservedSystemPage`·`SystemPage`·`SystemPageKind`·`SystemSlotStep`·`SystemResidencyCursor`·`InitializedSystemObservation`, load의 `SystemLoadError`·`SystemLoadObservation`·`SystemResidencyStep`·`MixedLoadInstall`, resources의 `fits` | `status_init.rs:58`의 `pub(crate) use` 재노출을 거쳐 운영 15개 이상: `storage/wal_io.rs`, `bootstrap`, `doublewrite_install`, `mutable_storage`, `status_init`, `status_init_operation`, `status_load`, `recovery_prefix`, `dirty_allocation`, `fixed_system_operation`, `allocation_reservation/operation`, `plan4_retention/population`, `storage/plan4_recovery_base/catalog/graph.rs`, `graph/{ledger,relations}.rs`. L3 안에서는 유지하는 `mixed_system_init`·`mixed_system_init_tests`가 쓴다 | `pub(super) mod`, 항목 `pub(crate)`·`pub(in super::super)` |
| `…::mixed::history` | 보관 상태 없음. WAL에서 history를 재구성 | `HistoryError`, `History`, `reconstruct`, `call_bytes` | owner | private `mod` |
| `…::mixed::load` | 보관 상태 없음. 원 load snapshot에서 residency 단계를 진행 | `LoadCause`, `SystemLoadError`, `SystemLoadObservation`, `SystemResidencyStep`, `MixedLoadInstall`, `fits` | owner와 그 재노출 소비자 | private `mod` |
| `…::mixed::resources` | 상태 없음. activation 크기 census | `ActivationLayout`, `Census`, `census`, `fits` | owner(`fits` 재노출 → `status_init.rs:360`) | private `mod` |
| `…::mixed::tests`와 `native`·`load_addendum` | 시험 | 시험 78·3·6(scenario macro 생성 포함) | `scripts/dev3_mixed_owner_native.py`의 `PREFIX`(`…::mixed::tests::native::`)와 `XADB_MIXED_OWNER_INCLUDE` case ID | private |
| `…::effect::tests::mixed` | 시험 | 시험 9 | `XADB_MIXED_RECOVERY_INCLUDE` case ID | private |
| `MS::status_init_operation::mixed`(유지) | 보관 상태 없음. mixed init plan 준비·검산·materialize | `Kind`, `Plan`, `Expected`, `Prepared`, `prepare`, `validate`, `materialize` | `operation::mixed` 경로로 운영 12개 이상: `status_home_flush`, `status_init`, `status_init_operation`, `status_load`, `recovery_prefix`, `recovery_effect/physical`, `fixed_system_operation`, `plan4_status_init_operation`, `empty_transactions/{load,pressure}`, owner와 `history`. 시험 3개: `mixed_system_init_tests`, `status_init_tests`, `status_home_flush/authority_tests` | `pub(super) mod`, 항목 `pub(in super::super)` |
| `MS::status_bootstrap_tests::undo::mixed_init`(유지) | 시험 | 시험 9 | — | private |

**2026-10-29 정정([R2 판정](#r123-logical-boundaries)).** 위 owner 행의 “실제 소비자”는 이름 충돌에 따른 grep 오판을 포함한다. 확인 결과는 다음과 같다.

- `storage/plan4_recovery_base/catalog/graph.rs`·`graph/{ledger,relations}.rs`의 일치는 같은 이름의 다른 enum variant `ReservedInventory::SystemPage`(`graph.rs:33` 선언)다. `plan4_retention/population.rs`의 일치도 이 variant다.
- `dirty_allocation`·`recovery_prefix`·`status_init_operation`과 그 `fixed`·`mixed`, `allocation_reservation/operation`의 일치는 `AR::operation`의 `AllocationPopulation::MixedSystem` variant다.
- `status_load`(현재 `operational_status/load.rs`)는 `MixedLoadInstall`을 owner의 `pub(super)` 재노출로 쓰며 `status_init.rs:58`을 거치지 않는다.
- `storage/wal_io.rs`·`bootstrap`·`doublewrite_install`·`mutable_storage`·`status_init`의 일치는 재노출 줄 자체다.
- 같은 행의 `mixed_system_init_tests`(현재 `mutable_storage/status_bootstrap_tests/undo/mixed_init.rs`)도 `AllocationPopulation::MixedSystem` variant만 쓴다(19행).

rustc로 확인한 이름 경로 소비자는 owner 자신과 그 자손뿐이다. `status_init.rs:58` 재노출을 이름으로 쓰는 cargo target 코드는 없고, `mutable_storage.rs`에 주입하는 frozen compiler probe 5개만 그 경로를 쓴다. 표 자체는 당시 이동 계획의 기록으로 보존한다.

#### 영향 case와 경로 참조

| 대상 | 처리 |
| --- | --- |
| 이동 파일의 시험 87개(owner 78·native 3·load_addendum 6), recovery 9, resources 1 | 시험 발견 목록의 동일성만 확인한다. 본문이 그대로이므로 NOT_RUN |
| native 실행 `scripts/dev3_mixed_owner_native.py` | 논리 `PREFIX`와 case ID만 쓴다. 목록에서 selector를 확인하고 native 실행 NOT_RUN |
| borrow probe `check_dev3_mixed_owner_borrows.py`·`check_dev3_mixed_init_borrows.py` | probe는 `wal_io/`에 두고 `mutable_storage.rs`에 주입하며 primary span은 probe 파일이어야 한다. probe 위치·논리 경로가 같으므로 NOT_RUN |
| 현재 fixture inventory `crates/engine/tests/fixtures/dev3-mixed-system-owner/runtime-manifest.json` | case별 `path` 151개(owner 시험 78·native 3·load_addendum 6·recovery 64)를 새 위치로 바꾼다. 읽는 script·시험은 없지만 [DEV3 시험 계획](dev3-test-plan.md)이 선택 inventory로 연결하는 현재 문서다. 각 case의 `function`이 이동 전 경로와 새 경로의 파일에 있는지, 다른 필드가 같은지를 대조한다. 이 파일의 원 SHA-256을 적은 증거 JSON은 고치지 않는다 |
| 이력 continuation `scripts/dev3_mixed_owner_continue.py` | `WORKER`·`FORMAT_ONLY_TEST` 상수는 보존된 원/continuation source 두 root 안의 경로를 비교하므로 바꾸지 않는다. 현재 source로 새 continuation을 돌리면 `WORKER`가 없어 `FAIL_NATIVE_WORKER_LINEAGE` 또는 승인 identity 검사에서 멈춘다 |
| 관리 JSON | `repository-build-plan.json`의 이동 대상 경로 문자열 8개(`dev3_mixed_system_owner` 7개와 다른 단위의 `mixed_system_load.rs` 1개) |
| Markdown | 이동 파일을 가리키는 현재 링크·본문 표기가 없다. 증거 JSON 5개의 원 경로 표기는 보존 |

검증은 L1·L2와 같다. `crates/` diff는 rename 7개, `#[path]` 7줄, fixture inventory의 `path` 151개뿐이어야 하고, 이동만 하는 6개(`history`·`load`·`resources`·`tests/native`·`tests/load_addendum`·recovery 시험)는 아래 기준 SHA-256과 같아야 한다. 유지하는 init 두 파일과 선언 파일 `status_home_flush.rs`·`status_init_operation.rs`·`undo_bootstrap_tests.rs`는 바뀌지 않는다. `cargo check -p xadb-engine --all-targets --all-features --locked`와 이동 전후 시험 목록 전체 출력의 동일성을 확인한다.

기준 SHA-256(`d251b8f`): `mixed_system_owner.rs` `3063ac137d4fca24a6bc265100e2de4ba8f2724faf3c4cdb1fd03275e8a34d28`, `mixed_system_history.rs` `f7ce83d76b05f882c762215d42173bbe6d28b5646c4784dd7549a076cba2200d`, `mixed_system_load.rs` `2d7fbacc27b1072c7552f7d29626c6dd22ad00d2422ab1ee01a4daeb9fa867de`, `mixed_system_resources.rs` `130fe67d1c4e1a5783a4039a3034a800331e6bfa694355dc12aa9228edffb903`, `mixed_system_owner_tests.rs` `b7992a420ad0aea72b1fa18d5f10585dc7e01715ebf92f142f36fefcbda465d7`, `mixed_system_owner_native_tests.rs` `06f114a632a6e52db99ff67dd78e712d0afa7151930503769da018df8bf43147`, `mixed_system_owner_load_tests.rs` `4ad20e4ef52a49858c22c5dfab28d9d51334cd7da213283a296aa17a25a37b04`, `mixed_system_recovery_tests.rs` `2b2b531eef4eeae61713501ec2f2316405130dd5c18a14d0e486e3928d601536`, `recovery_effect/tests.rs` `bb4dbdac462e7f713a0ee08297e86bc9fae4baf63e68eaa3849c3cf14fcfcc6d`, `runtime-manifest.json` `bac34bd2dacd49410f21419ab53dc59e504c625edfd135ba74577729272ce040`.

#### L3 범위 밖 관찰

owner 표면은 `status_init.rs`의 `pub(crate) use` 재노출을 거쳐 `wal_io` 밖의 `plan4_recovery_base` Catalog graph까지 쓰인다. init 두 파일은 형제를 가진 평면 부모 아래에 남는다. 배치 정리는 이 참조와 부모를 바꾸지 않으며, 둘 다 L5와 위 이행 순서 4단계의 입력이다.

**2026-10-29 정정.** 첫 문장의 `plan4_recovery_base` Catalog graph 소비는 grep 오판이다(위 소유·제공·소비 표의 정정). 재노출 자체는 crate 전체에 열려 있었지만 실제 이름 경로 소비자는 없었고, [R2 판정](#r123-logical-boundaries)에서 이 재노출을 정리했다. init 두 파일은 L5 이동에서 각 선언 위치의 폴더(`status_init_operation/mixed.rs`, `mutable_storage/status_bootstrap_tests/undo/mixed_init.rs`)로 옮겼다.

<a id="l4-record"></a>
### L4 배치 판정 기록

2026-10-29 · **판정: 현재 배치 유지 / 소유·중복·잔여 이유 기록 / 제품 변경 없음**

L2 뒤 L4의 두 안을 제시했고 사용자는 L3와 함께 권장안, 즉 이동하지 않고 기록하는 안을 선택했다. 이 판정의 기준은 L3 적용 commit `08d854da53c8ac9e8398946c4532a9161800af20`(tree `780fb1c17990c6849c55ab52ca0af10ce4de7267`)이며 checkout은 clean이다. 판정은 제품·시험·JSON 경로는 바꾸지 않는다. 아래 경로는 `crates/engine/src/storage/wal_io/` 기준이고 `MS`는 L3와 같다.

**한 모듈의 두 폴더.** `plan4_apply.rs`(1032줄)는 `plan4_pages.rs:23–32`에서 같은 파일을 두 번 선언한다. production은 `cfg(not(test))`의 `MS::plan4_pages::apply`이고, 시험은 `cfg(test)`의 `MS::plan4_pages::retention`에 `use retention as apply`를 더한다. 주석은 frozen selector의 source identity를 지키려는 것이며 동작 차이는 없다고 적는다. 이 한 모듈의 자손이 두 폴더에 나뉘어 있다. 두 폴더 모두 두 cfg 이름으로 함께 컴파일되며, cfg에 따라 나뉜 것이 아니다. `plan4_retention/`은 원래 별도 모듈 `plan4_retention.rs`의 폴더였고, commit `ad0c45f`가 그 모듈을 `plan4_apply.rs`로 합치면서 `retention`을 시험 이름으로만 남겼다. 소유자가 같으므로 L3와 같은 논리 부모 불일치는 없고, 남은 차이는 폴더 이름이다.

| 폴더 | 파일 | `plan4_apply.rs`의 직접 선언 | 묶음과 역할(각 파일 첫 주석 요약) |
| --- | --- | --- | --- |
| `plan4_apply/` | 10 | 7줄: `terminal`, `install`, `legacy`, `isolation`, `source`, `transaction`, `owner_tests`. 하위 3개(`source_tests`, `transaction_catalog`, `transaction_write`)는 각 부모 파일이 선언 | 실제 apply producer 쪽이다. Catalog stage/durable/install의 terminal receipt, 읽기 전용 installer 관측, 원 allocation/empty/scope1 producer의 임시 bridge `legacy`, 원 session이 소유하는 isolation ledger와 짧은 source 예약, transaction·statement 소유 |
| `plan4_retention/` | 48 | 17줄. 하위 31개는 각 부모 파일이 선언 | 원 plan4 owner의 helper 39개와 시험 9개다. CREATE/pool bridge(`create_pair`와 `catalog_*` 8), visibility bookkeeping(+8), byte-backed live ledger `control`(+1), 물리 claim `flush`, durable 구간의 canonical replay `history`(+3), N=0 group `live`, latest-home `load`, reader `lookup_load`, count-one 예약 `population`, read actor `read`(+6), recovery 관측 `recovery`(+1), source-value `resources`. 시험은 `tests`·`lifecycle_tests`·`literal_tests`·`s5_tests`·`free_undo_literal_tests`·`free_undo_tests`·`catalog_source_tests`·`recovery_observation_tests`·`read/catalog_tests` |

**시험 selector.** `MS::plan4_pages::retention::` 아래 시험은 62개다. `plan4_apply/` 쪽이 `source` 16·`owner_tests` 10·`transaction` 2·`install` 1, `plan4_retention/` 쪽이 `read` 14·`tests` 11·`free_undo_tests` 4·`catalog_source_tests` 2·`create_pair` 2다. 두 폴더 모두 시험에서는 `retention` 이름으로 선택된다.

**중복 배치.** `plan4_retention/`의 시험 입력 helper 두 개는 다른 모듈도 `#[path]`로 함께 컴파일한다. 둘 다 `clippy::duplicate_mod` 허용과 이유를 적었다.

| 파일 | `plan4_apply` 쪽 논리 경로 | 다른 선언 |
| --- | --- | --- |
| `plan4_retention/free_undo_literal_tests.rs` | `…::retention::free_undo_literals` | `storage/plan4_free_undo_base_tests.rs:7`의 `storage::plan4_recovery_base::free_undo_tests::inputs` |
| `plan4_retention/literal_tests.rs` | `…::retention::tests::lifecycle::literals` | `plan4_recovery/tests/literals.rs:11`의 `MS::plan4_recovery::tests::literals::population` |

**판정과 이유.** 현재 배치를 유지한다.

1. 두 폴더는 한 모듈의 자손이고 소유가 같다. 옮겨도 논리 경로·visibility·selector는 그대로이며, 바뀌는 것은 폴더 이름뿐이다.
2. `plan4_apply.rs` 첫 주석은 기존 control/history/flush helper가 이행 동안 private 자손으로 남고, Row producer의 실제 source/reservation/install adapter는 아직 없다고 적는다. `legacy`도 원 producer의 임시 bridge다. 최종 소유가 정해지면 helper별로 다시 옮겨야 하므로, 지금 일괄로 합치면 이동이 두 번 생긴다.
3. 공유 helper 두 개는 이 모듈 밖 두 곳의 `#[path]`도 함께 바꿔야 한다.
4. `plan4_retention/`을 `plan4_apply/`로 합치는 대안의 비용은 `#[path]` 19줄(내부 17·외부 2), 관리 JSON 경로 184개, 현재 계약 문서 3개의 Markdown 링크 11개, 증거 링크 4개의 날짜 주석이다. 그에 비해 얻는 것은 이름 정리뿐이다.

**재검토 조건.** 다음 중 하나가 생기면 그 단위에서 소유별로 옮긴다. (1) 이행이 끝나 helper의 최종 소유가 정해짐(`legacy` bridge 제거, Row producer adapter 도입 등). 이때 목표는 [소유 계약](../contracts/dev3-module-boundaries.md)의 계획 배치 `plan4_apply/{state,read,catalog,row,terminal,legacy}*.rs`다. (2) frozen `retention` selector 이름을 바꾸는 별도 결정. (3) 공유 literal helper의 소비자 변경. 그 전에는 13절 후보 표의 “retention 전체를 일괄 재부모화하지 않는다”를 따른다.

<a id="l5-classification"></a>
### L5 책임별 분류

2026-10-29 · **분류 완료 / 물리 이동 4단위(L5-A–D) 계획 / 논리 경계 재설계는 입력만 기록 / 이 절 작성 때 제품 변경 없음**

사용자는 권장안이 있으면 묻지 않고 진행하도록 지시했다. 기준은 main `af50b46230f8ee4cf050b3aa61e7e940aeef3dfd`, tree `beca31399b29b1d390e8c7c650c5d7d8f978c97a`이며 checkout은 clean이다. L1·L3 뒤 `wal_io/` 직속 Rust 파일은 63개이고 그중 이름에 `test`가 있는 파일은 33개다. 13절 L5 행대로 각 책임을 `현재 배치 유지 / 물리 이동 / 논리 경계 재설계`로 닫는다. 아래 경로는 `crates/engine/src/storage/wal_io/` 기준이다.

**분류 기준.** 제자리에 둘 진입과, 그 진입과 함께 옮길 하위 모듈을 아래처럼 나눈다.

1. **진입으로 유지:** 다음은 진입으로 제자리에 둔다(배치 B). 진입을 부모 폴더로 옮기면 그 아래 모든 `#[path]` 기준과 compiler probe anchor가 함께 바뀐다. 이는 소유 변화 없는 상위 배치이므로 이번 정리에 넣지 않는다.
   - 상위 조립 사슬 `bootstrap` → `doublewrite_install` → `mutable_storage`와 `physical`·`disk_api`.
   - 이 사슬이 직접 선언한 production 자식, 곧 각 책임의 진입.
   - `status_flush` 아래에서 각자 actor를 소유하는 `mixed`·`operational`·`resident`. L3에서 owner 진입을 유지한 선례다.
   - 이미 자기 폴더가 있는 모듈: `allocation_reservation`, `empty_transactions`, `mixed_system_owner`, `plan4_pages`, `plan4_recovery`, `recovery_effect`, `status_home_flush`.
2. **진입과 함께 물리 이동:** 위 진입이 선언한 평면 자손은 진입 옆 `진입이름/`으로 옮긴다.
   - 시험·지원만이 아니라, 진입 하나의 구현 일부인 production 하위 모듈도 자기 자손과 함께 옮긴다(L1 `operation` 선례). 해당하는 것은 `eviction`, `operational::load`(`status_load.rs`), `status_init_operation`의 `fixed`·`mixed`·`retention`, `recovery_prefix::transactions`, `plan4_pages::recovered_bind`다.
   - 새 파일 이름은 논리 모듈 이름으로 한다. 실제 bootstrap 종류 `plan4`는 파일이나 폴더 이름 중 하나로 경로에 남긴다.
3. **현재 배치 유지:** 자손이 없는 leaf(`undo_bootstrap`, `flush_io`)와 L4의 `plan4_apply` 자손.
4. **경로 기록의 처리:**
   - case 위치를 가리키는 현재 authored inventory는 새 위치로 갱신한다. 상태 표시나 SHA 고정이 없는 입력이며, L3의 `runtime-manifest.json`이 예다.
   - 다음은 원 source의 기록이므로 바꾸지 않고, 증거에 현재 위치 대응만 남긴다(L3의 `dev3_mixed_owner_continue.py` 선례): frozen·prepared·input-only 상태이거나 source·파일 SHA를 고정한 입력, 그리고 보존된 이력 source root를 검증하는 checker 상수.
5. **논리 경계 재설계:** 논리 부모나 소비 범위를 바꿔야 하는 항목이다. 입력만 기록하고 위 이행 순서 4단계의 별도 D2로 둔다.

#### 책임별 분류와 이동 목표

| 책임 | 진입(논리 모듈) | 분류 | 평면 자손 → 목표 |
| --- | --- | --- | --- |
| 세션 조립 상위 사슬 | `bootstrap.rs` → `doublewrite_install.rs` → `mutable_storage.rs`, 그리고 `physical.rs`·`disk_api.rs` | 진입 유지 | 채택 계약이 유지하는 원 초기화 조상이다. production 자식 진입(`plan4_genesis`, `undo_bootstrap`, `allocation_reservation`, `dirty_allocation`, `plan4_recovery`, `recovery_prefix`, `status_init`, `status_init_operation`, `allocator_flush`, `flush_io`, `allocator_residency`, `plan4_pages`)도 각자 진입으로 둔다 |
| 설치 | `bootstrap` | 물리 이동 L5-B | `bootstrap_test_support.rs` → `bootstrap/test_support.rs`, `bootstrap_tests.rs` → `bootstrap/tests.rs` |
| 설치 | `doublewrite_install` | 물리 이동 L5-B | `doublewrite_install_tests.rs` → `doublewrite_install/tests.rs` |
| 설치 | `plan4_genesis` | 물리 이동 L5-B | `plan4_genesis_tests.rs` → `plan4_genesis/tests.rs`, `plan4_genesis_test_support.rs` → `plan4_genesis/tests/support.rs` |
| 세션 조립 시험 | `mutable_storage` | 물리 이동 L5-B | `runtime_core_tests.rs` → `mutable_storage/runtime_core_tests.rs`; `status_bootstrap_tests.rs` → `mutable_storage/status_bootstrap_tests.rs`, 그 자손 `status_bootstrap_native_tests.rs` → `…/status_bootstrap_tests/native.rs`, `undo_bootstrap_tests.rs` → `…/status_bootstrap_tests/undo.rs`, `undo_bootstrap_native_tests.rs` → `…/status_bootstrap_tests/undo/native_undo.rs`, `mixed_system_init_tests.rs` → `…/status_bootstrap_tests/undo/mixed_init.rs` |
| 설치·세션 조립 leaf | `undo_bootstrap`, `flush_io` | 현재 배치 유지 | 자손 없음 |
| 할당 | `allocator_residency` | 물리 이동 L5-A | `allocator_eviction.rs` → `allocator_residency/eviction.rs`, `allocator_eviction_tests.rs` → `allocator_residency/eviction/tests.rs`, `allocator_residency_native_tests.rs` → `allocator_residency/native_tests.rs`, `allocator_residency_tests.rs` → `allocator_residency/tests.rs` |
| 할당 | `allocator_flush` | 물리 이동 L5-A | `allocator_flush_native_tests.rs` → `allocator_flush/native_tests.rs`, `allocator_flush_tests.rs` → `allocator_flush/tests.rs`. 시험의 위치 기준 `include_str!` 5개에 `../`를 더한다 |
| 할당 | `dirty_allocation` | 물리 이동 L5-A | `dirty_allocation_test_support.rs` → `dirty_allocation/test_support.rs`, `dirty_allocation_native_tests.rs` → `dirty_allocation/native_tests.rs`, `dirty_allocation_tests.rs` → `dirty_allocation/tests.rs`. 시험의 위치 기준 `include_str!` 2개에 `../`를 더한다. `scripts/check_dev3_dirty_runtime.py:83–84`의 경로 상수와 `tests/fixtures/dev3-dirty-allocation/runtime-manifest.json`의 `implementation_files`는 `--root`로 받은 보존 source를 검증하고, 기록된 SHA-256은 이미 현재 파일과 다르다. 기준 4에 따라 바꾸지 않는다 |
| Status | `status_init` | 물리 이동 L5-C | `status_init_tests.rs` → `status_init/tests.rs`, 그 자손 `status_init_native_tests.rs` → `status_init/tests/native.rs`, `status_init_protocol_tests.rs` → `status_init/tests/protocol.rs`, `status_init_resource_tests.rs` → `status_init/tests/resources.rs` |
| Status | `status_init_operation` | 물리 이동 L5-C | `fixed_system_operation.rs` → `status_init_operation/fixed.rs`, `mixed_system_init.rs` → `status_init_operation/mixed.rs`, `plan4_status_init_operation.rs` → `status_init_operation/plan4_retention.rs`, `status_init_operation_tests.rs` → `status_init_operation/tests.rs`. L3에서 남긴 init과 그 형제를 함께 옮긴다 |
| Status | `status_flush`(`status_home_flush.rs`) | 물리 이동 L5-C | `status_home_flush/native_tests.rs`가 `../`로 선언하는 `operational_status_native_tests.rs` → `status_home_flush/native_tests/operational.rs`, `status_load_native_tests.rs` → `…/native_tests/status_load.rs`, `empty_history_native_tests.rs` → `…/native_tests/empty_history.rs`, `empty_load_native_tests.rs` → `…/native_tests/empty_load.rs`. production 자식 `mixed`·`operational`·`resident`는 진입 유지 |
| Status | `operational`(`operational_status.rs`) | 물리 이동 L5-C | `operational_status_tests.rs` → `operational_status/tests.rs`, `status_load.rs` → `operational_status/load.rs`, `status_load_tests.rs` → `operational_status/load/tests.rs` |
| Status | `resident`(`resident_status.rs`) | 물리 이동 L5-C | `resident_status_tests.rs` → `resident_status/tests.rs`. 자식 `empty_transactions`는 자기 폴더가 있어 유지 |
| 복구 | `recovery_prefix` | 물리 이동 L5-D | `recovery_empty_transactions.rs` → `recovery_prefix/transactions.rs`, `recovery_prefix_native_tests.rs` → `recovery_prefix/native.rs`, `recovery_prefix_tests.rs` → `recovery_prefix/tests.rs`. 자식 `effect`(`recovery_effect/mod.rs`)는 유지 |
| 복구·apply | `plan4_pages` | 물리 이동 L5-D | `plan4_recovered_bind.rs` → `plan4_pages/recovered_bind.rs`. `apply`/`retention`은 L4대로 유지 |

물리 이동은 4단위 40개(A 9·B 11·C 16·D 4)다. 실행 뒤 `wal_io/` 직속은 23개가 되고, 이름에 `test`가 있는 직속 파일은 남지 않는다. 대상 `#[path]` 선언은 모두 inline module 밖에 있다. 위치 기준 macro는 `allocator_flush_tests.rs`(5개)와 `dirty_allocation_tests.rs`(2개)에만 있고, 다른 fixture는 `env!("CARGO_MANIFEST_DIR")` 기준이다.

#### 단위 실행 방식

각 단위는 L1–L3와 같이 한 commit으로 진행한다. 순서는 A → B → C → D다.

1. 기준 SHA-256과 이동 전 시험 목록을 남긴다.
2. rename과 `#[path]`·`include_str!` 줄만 바꾼다.
3. 현재 Markdown 링크·본문 표기, 관리 JSON 경로, 상태 문구를 갱신한다.
4. 다음을 확인한다: `cargo check -p xadb-engine --all-targets --all-features --locked`, 이동 전후 시험 목록 전체의 동일성, 이동만 한 파일의 byte 동일, 변경 파일의 `rustfmt --check`.
5. 독립 리뷰를 받는다.
6. 시험 본문은 본문이 같으므로 NOT_RUN이다.

현재 참조 중 미리 확인한 것(저장소 전체 파일 이름 검색):

- 현재 Markdown 링크: `bootstrap_tests.rs`, `doublewrite_install_tests.rs`, `operational_status_tests.rs` 각 1개.
- 본문 표기: `dev3-test-plan.md`, `dev3-task-05-buffer-flush.md`, 저장소 정책.
- 증거 Markdown 링크 2개: `xadb-direction-review-20261025.md:144,165`의 `recovery_empty_transactions.rs`. L1 선례대로 날짜 주석과 원 byte 보존으로 갱신한다.
- 관리 JSON 경로 문자열 69개: A 13, B 22, C 29, D 5.
- 기준 4에 따라 바꾸지 않는 입력. 이 파일들의 현재 위치 대응은 각 단위 증거에 남긴다.
  - L5-A: dirty runtime checker의 상수, 같은 옛 경로로 합성 root를 만드는 `scripts/test_check_dev3_dirty_runtime.py:80–81`, runtime manifest(위 표).
  - L5-B: `tests/fixtures/dev3-plan4-genesis/recipes-g.json`. 상태는 `SOURCE_PREPARED_NOT_EXECUTED`다. `source_files`의 SHA-256 고정은 이미 현재 파일과 다르며, 원 경로는 `source_files` 2개와 `cases[].test_source` 227개에 있다.
  - L5-C: `crates/engine/tests/fixtures/dev3-resident-status/manifest.json`. 상태는 `FROZEN_INPUTS_NOT_EXECUTION_EVIDENCE`이고, `changed_path`는 각 case를 추가할 때 바뀐 원 파일을 적는다.
  - L5-D: `crates/engine/tests/fixtures/recovery_prefix/runtime/manifest.json`. 상태는 `INPUT_ONLY_NOT_RUN`이고, `scripts/dev3_recovery_prefix.py:22`가 이 파일의 SHA-256을 고정한다. 원 경로는 `source` 1개에 있다.

#### 논리 경계 재설계 입력

실행하지 않으며, 각 항목은 실제 소비자를 지정한 별도 D2 단위로만 연다.

- **R1:** `AR::operation`은 L1 밖 운영 파일 26개 이상이 쓰고, 재노출 사슬로 `plan4_recovery_base`까지 이어진다([L1 관찰](#l1-move-table)).
- **R2:** mixed owner 표면은 `status_init.rs`의 `pub(crate) use`로 `plan4_recovery_base` Catalog graph까지 쓰인다([L3 관찰](#l3-move-table)).
- **R3:** 설치·할당·Status·복구의 진입이 모두 `bootstrap` → `doublewrite_install` → `mutable_storage` 한 초기화 사슬 아래에 있다. 이를 책임별 상위 폴더로 나누려면 논리 부모를 바꿔야 하므로, 채택 계약의 초기화 조상 유지와 함께 다시 설계해야 한다.

**2026-10-29 판정.** 세 입력은 아래 [R1–R3 논리 경계 판정](#r123-logical-boundaries)에서 닫았다. R1은 현재 경로 유지, R2는 미사용 재노출 제거, R3는 초기화 사슬 유지다. 위 R1의 “26개 이상”과 R2의 Catalog graph 소비는 grep 기준 서술이며 rustc census로 정정했다. 입력 문구는 당시 기록으로 보존한다.

**범위 밖 관찰.** `storage/` 직속의 `wal_durability_test_support.rs`·`wal_durability_tests.rs`·`wal_first_write_tests.rs`·`wal_io_test_support.rs`·`wal_io_tests.rs`는 `storage::wal_io`의 자손이다. 그러나 13절 L5 행의 파일군(`storage/wal_io/`) 밖에 있으므로 이번 분류에 넣지 않는다. 이 5개는 아래 [L6](#l6-wal-io-tests)에서 판정했다.

<a id="l6-wal-io-tests"></a>
### L6 `storage::wal_io` 진입의 storage 직속 시험 자손

2026-10-29 · **판정: 진입 폴더로 물리 이동 / 실행 결과는 [증거](../evidence/module-independence-survey-20261026.md#l6-wal-io-move) / 시험 본문 NOT_RUN**

사용자는 이 계획을 “이어서 완료까지” 진행하도록 지시했고, 앞선 단위에서 권장안이 있으면 묻지 않고 진행하도록 했다. 기준은 `85ceef3b35c5c2d0ef1a43056dcea0ba6a1443ea`(tree `504d3e9084e7888e06c7cc78c637c06716c221c9`)이며 작업 branch는 `claude/project-thread-h2j8oc`, checkout은 clean이다. 위 범위 밖 관찰의 5개만 다룬다. `storage/` 직속의 다른 시험 파일은 각자 다른 진입(`wal_frame`, `page_io` 등)의 자손이므로 이 단위에 넣지 않는다.

**분류.** 진입 `storage/wal_io.rs`는 `storage/mod.rs`가 `#[path]` 없이 선언하며 이미 자손 폴더 `storage/wal_io/`를 가진다(배치 B). 5개는 모두 이 진입이 `cfg(test)`로 선언한 평면 자손이므로 L5 분류 기준 2를 그대로 적용해 진입 폴더로 옮긴다. 새 파일 이름은 논리 모듈 이름으로 한다. 진입·`#[path]` 해석은 `storage/wal_io.rs`의 폴더 `storage/` 기준이므로 새 문자열은 `wal_io/…`다.

| 현재 파일(줄) | 목표 파일 | 논리 경로 | 시험 수 |
| --- | --- | --- | --- |
| `storage/wal_durability_test_support.rs`(99) | `storage/wal_io/durability_test_support.rs` | `storage::wal_io::durability_test_support` | 0 |
| `storage/wal_durability_tests.rs`(404) | `storage/wal_io/durability_tests.rs` | `storage::wal_io::durability_tests` | 8 |
| `storage/wal_first_write_tests.rs`(251) | `storage/wal_io/first_write_tests.rs` | `storage::wal_io::first_write_tests` | 14 |
| `storage/wal_io_test_support.rs`(443) | `storage/wal_io/test_support.rs` | `storage::wal_io::test_support` | 0 |
| `storage/wal_io_tests.rs`(1162) | `storage/wal_io/tests.rs` | `storage::wal_io::tests` | 10 |

`wal_io_test_support.rs`의 위치 기준 `include_bytes!` 3개는 한 단계 깊어지므로 `../`를 하나 더한다. 같은 `crates/engine/tests/fixtures/dev3-genesis-{profile,epoch,writer}.bin`을 가리키며, 줄이 100자를 넘는 2개는 rustfmt의 줄바꿈을 따른다. 다른 4개 파일에는 `#[path]`·위치 기준 macro가 없다. 논리 경로·visibility·cfg·시험 selector(`scripts/test_check_dev3_leaf.py`의 `storage::wal_io::tests::`)는 그대로다.

**경로 기록.** 현재 Markdown 링크 1개(`dev3-test-plan.md`), 현재 본문 표기 1개(`dev3-wal-io.md`), 관리 JSON 경로 문자열 7개를 갱신한다. 증거 JSON 3개(`dev4-supply-handoff-20261024.json`, `dev3-commit-continuation-20261025.json`, `dev3-runtime-core-20261024.json`)의 원 경로·SHA-256 기록과 leaf 보고서의 `wal_io_tests` 키 이름은 원 source의 증거이므로 바꾸지 않는다. 링크가 아니라 문서 검사 대상이 아니다.

**검증 선택.** L5와 같다. `crates/` diff는 rename 5개, `#[path]` 5줄, `include_bytes!` 3줄뿐이어야 하고 이동만 한 4개는 기준 SHA-256과 같아야 한다. `cargo check -p xadb-engine --all-targets --all-features --locked`, 이동 전후 시험 목록 전체의 동일성, 변경 파일의 `rustfmt --check`를 확인하고 독립 리뷰를 받는다. 시험 본문은 본문이 같으므로 NOT_RUN이다.

이동 뒤 `storage/` 직속 Rust 파일은 86→81개, `storage/wal_io/` 직속은 23→28개다. L5-D 때 0개였던 `wal_io/` 직속 `test` 이름 파일은 이제 5개이며 모두 진입 `wal_io` 자신의 시험·지원이다. 이 수는 탐색 밀도 관측일 뿐 독립성 판정이 아니다.

<a id="r123-logical-boundaries"></a>
### R1–R3 논리 경계 판정

2026-10-29 · **판정: R1 현재 경로 유지 / R2 미사용 재노출 제거(재노출 줄만, 6개 파일) / R3 초기화 사슬 유지 / 실행 결과는 [증거](../evidence/module-independence-survey-20261026.md#r123-logical-boundaries) / 시험 본문 NOT_RUN / 독립 검토 ACCEPT·필수 수정0**

L5가 남긴 [논리 경계 재설계 입력](#l5-classification) 셋을 각각 실제 소비자를 지정한 D2 단위로 조사했다. R1·R2를 한 작업, R3를 별도 작업으로 나누었고 통합 담당이 두 결과를 수용했다. 조사 기준은 `85ceef3b35c5c2d0ef1a43056dcea0ba6a1443ea`이고, 적용 기준은 작업 branch `claude/project-thread-h2j8oc`의 `76742f3c29f6a4f840a27c106440b3672a26c082`(L6 적용)다. 두 기준 사이의 제품 변경은 L6의 `wal_io.rs` `#[path]` 5줄과 이동한 시험 파일뿐이다. 판단 기준은 위 [판단 기준과 목표](#folder-reorganization)의 논리 의존 변경 기준이다. 필요한 최소 API와 원 소유·생성 권한을 확인하고, `use` 문자열 이동이나 새 facade만으로 독립성 향상을 주장하지 않는다.

**소비자 census는 grep이 아니라 rustc로 확정했다.** 재노출을 한 층씩 임시로 지우고 `cargo check -p xadb-engine --all-targets --all-features --locked --keep-going`로 다시 검사해, 오류 위치와 새로 미사용이 된 재노출을 기록했다. 정의 파일의 최상위 항목에 임시 `#[deprecated]`를 붙여 all-targets와 `--lib`를 각각 검사하고, 그 차이로 production과 시험을 나누었다. R3는 진입 선언을 `wal_io`의 형제로 옮겨 보는 실험을 더했다. 임시 편집은 모두 되돌렸다. 방법·표·로그는 증거 절과 artifact에 있다.

#### R1 — `AR::operation`: 현재 경로 유지

**실제 소비.** L1 밖 운영 파일은 28개다. 이 수는 deprecation census가 표시한 top-level `pub` 항목의 이름·variant·field 사용을 센 것이며, method 호출만 하는 파일은 세지 않았다.

- `doublewrite_install` 자손 23개가 내부 경로로 쓴다. `operation`은 `pub(super) mod`이고 대부분 항목이 `pub(in super::super)`이므로, rustc가 이 사용을 `doublewrite_install` 안으로 제한한다. 같은 소유 안의 사용이다.
- 재노출 중계 3개(`wal_io.rs`·`bootstrap.rs`·`doublewrite_install.rs`)가 있다.
- `wal_io` 밖 2개는 `storage/plan4_recovery_base.rs`와 `plan4_recovery_base/catalog/graph/ledger.rs`다.

시험 전용은 `doublewrite_install` 자손 15개, `wal_io/plan4_genesis/tests.rs`(루트 `wal_io::` 경로로 시험 별칭 사용), `wal_io` 밖의 `storage/plan4_recovery_base_bridge_tests.rs`다.

**경계를 넘는 항목.** 6개와 같은 타입의 시험 별칭 하나뿐이다.

- `SemanticError`(별칭 `CatalogAllocationError`, 시험 별칭 `Plan4TestSemanticError`)
- `validate_plan4_base_allocation`, `validate_plan4_system_base_allocation`, `validate_catalog_base_allocation`
- `plan4_base_allocation_source_bytes`, `plan4_system_base_allocation_source_bytes`

재노출 경로는 `doublewrite_install.rs:31–32·694–698` → `bootstrap.rs:42–43·1208–1211·1234–1236` → `wal_io.rs:99–102·744–748·772`이며 모두 `pub(in crate::storage)`다. 줄 번호는 `76742f3` 기준이다.

**판정과 이유.**

1. 넘는 항목에는 권한이 없다. 검산 함수는 호출자가 준 page byte·`Profile`·raw descriptor·scalar만 받아 `Result<(), SemanticError>`를 돌려주고, 나머지는 `usize` 상한과 `Copy` 오류 enum이다. 문서 주석도 INIT·WAL·예약 권한을 만들지 않는다고 적는다. 생성자, `PreparedAllocation`, reservation producer 등 다른 항목은 rustc가 `doublewrite_install` 밖으로 내보내지 않는다.
2. 소비 관계는 이미 채택된 것이다. [Free Undo 계약](../contracts/dev3-plan4-free-undo.md)은 base가 `validate_plan4_system_base_allocation`을 소비하며 기존 algebra를 공유한다고 정했다. `plan4_recovery_base`는 storage의 private 모듈이고 `wal_io`의 복구 코드(`plan4_recovery`, `plan4_retention`, `plan4_apply`)만 쓴다. 제품 소비자가 아니라 `wal_io` 복구가 AR algebra를 되부르는 관계다.
3. [채택 계약 §4.1](../contracts/dev3-module-boundaries.md#target-imports)의 방식, 곧 `disk_api`가 정의 모듈을 직접 재노출하는 방식은 공개 확대를 요구한다. `allocation_reservation`(private)과 `operation`(`pub(super)`)을 `pub(in crate::storage::wal_io)`로 열어야 하기 때문이다. 그러면 `pub(crate) mod retention`의 producer, 즉 `prepare_retention_reserve/cancel`, `PreparedRetentionAllocation::stage_required`, `materialize_retention_*`, `RetentionAllocationPlan.fence`가 `wal_io` 전체에 열린다.

**비교 대안.**

- **A′:** `disk_api`가 `doublewrite_install`의 재노출을 받아 6개를 옮기는 안이다. 공개 확대 없이 컴파일되고 warning도 0이었지만 채택하지 않았다.
  - 경로 이름만 바뀌고 의존 방향과 변경 국소성은 같다.
  - 같은 소비자가 쓰는 `plan4_genesis` 항목(`encode_plan4_catalog_genesis`, `plan4_catalog_genesis_source_bytes`, 시험 `Plan4GenesisCause`)은 루트에 남는다. `wal_io` 통과 지점이 둘로 나뉜다.
  - 복구 내부 algebra를 C0 제품 선택 지점인 `disk_api`에 섞는다. `storage::disk`가 그 지점을 그대로 비춘다.
  - `plan4_recovery_base.rs` 함수 본문 안의 경로 한정자를 바꾸고 rustfmt 줄바꿈도 생긴다.
  - 참고용 patch는 증거 artifact에 보존했다.
- **A0:** 루트 재노출이 `doublewrite_install`을 직접 가리키게 해 `bootstrap` 중계만 빼는 안이다. 표기 정리일 뿐이다.

**남긴 항목.**

- `wal_io` ⇄ `plan4_recovery_base` 순환
- 시험 별칭 `Plan4TestSemanticError`의 3단 중계
- 루트에 남는 `plan4_genesis` 항목

**재검토 조건.**

1. `wal_io` 밖, 또는 `doublewrite_install` 밖의 새 소비자가 `AR::operation` 항목을 요구한다.
2. 넘는 항목이 fence·WAL·예약 같은 소유 상태나 권한을 갖거나 owner borrow를 받는다.
3. `plan4_recovery_base`에 `wal_io` 밖 소비자가 생기거나 부모가 바뀐다.
4. 공유 pure algebra(`validate_plan4_population`·`validate_reserved`·`SemanticError`·크기 상한)를 하위 모듈로 추출하는 D2를 연다. 함수 본문 이동이 필요하며, 이 D2가 순환을 없앤다.
5. storage 형제가 쓰는 `wal_io` 루트 재노출을 `disk_api`로 일괄 이전하기로 결정한다. 이때 A′을 `plan4_genesis` 항목과 함께 적용한다.
6. `operation`이나 `allocation_reservation`의 module visibility 변경이 제안된다. 이때는 `operation::retention` producer를 먼저 좁히거나 봉인한다.

#### R2 — mixed owner 표면: 미사용 재노출 제거

**입력 정정.** R2 입력과 [L3 소비자 표](#l3-move-table)가 든 `plan4_recovery_base` Catalog graph 소비는 grep 오판이다(L3 절의 정정 참고).

**실제 소비.** 대상은 다음 15개 이름이다.

`InitializedSystemObservation`, `MixedSystem`, `MixedSystemBinding`, `MixedSystemFlusher`, `MixedSystemReadGuard`, `MixedSystemReader`, `MixedSystemWriter`, `ReservedSystemPage`, `SystemLoadError`, `SystemLoadObservation`, `SystemPage`, `SystemPageKind`, `SystemResidencyCursor`, `SystemResidencyStep`, `SystemSlotStep`

이 이름을 `wal_io.rs` → `bootstrap.rs` → `doublewrite_install.rs` → `mutable_storage.rs` 순서로 하나씩 빼며 모든 target을 다시 검사했다.

- 매 단계 오류는 0이었다.
- 마지막 단계에서 `status_init.rs:58`의 재노출이 production과 test 두 unit 모두에서 미사용으로 보고됐다.
- 따라서 crate 안에 이 이름을 경로로 쓰는 코드는 없고, `wal_io` 밖 소비자도 없다.

이름이나 field로 쓰는 곳은 owner 자신과 그 자손, 그리고 `MixedLoadInstall`을 owner의 `pub(super)` 재노출로 쓰는 `operational_status/load.rs`뿐이다. 경로를 쓰는 것은 frozen compiler probe 5개뿐이다. `dev3-mixed-system-owner/compiler`의 MOC00 positive·MOC01 core_escape·MOC05 old_actor·MOC07 guard_owner_reuse·MOC08 reservation_owner_reuse가 `mutable_storage.rs`에 주입된 자식 모듈에서 `super::status_init::{MixedSystem, SystemPage, SystemPageKind}`를 쓴다.

**변경.** 재노출 줄만 바꿨다. 줄 번호는 `76742f3` 기준이다.

| 파일:줄 | 변경 |
| --- | --- |
| `wal_io.rs:56–68`, `bootstrap.rs:68–80`, `doublewrite_install.rs:41–53`, `mutable_storage.rs:48–59` | 각 중계 블록에서 15개 이름을 삭제했다. 나머지 Status 26개는 그대로다 |
| `status_init.rs:58–63` | `pub(crate) use status_flush::mixed::{15개}`를 probe가 쓰는 3개만 남긴 `pub(super) use status_flush::mixed::{MixedSystem, SystemPage, SystemPageKind}`로 좁혔다. `#[allow(unused_imports, reason = "frozen mixed-owner compiler probes name these from mutable_storage")]`를 붙였다 |
| `mixed_system_owner.rs:22` | owner 재노출 `pub(crate) use load::{…}`에서 `SystemLoadObservation`을 뺐다. 유일한 소비자가 삭제한 `status_init` 중계였다. 남겨 두면 unused import warning이 새로 생긴다 |

타입 정의·필드·method·생성자·seal·cfg·논리 모듈 경로·시험은 바꾸지 않았다. 변경 전에는 writer·예약·flusher actor(`MixedSystemWriter`·`ReservedSystemPage`·`MixedSystemFlusher`·`MixedSystemBinding`)를 포함한 owner 표면을 crate 어디서나 `crate::storage::wal_io::…`로 이름 붙일 수 있었다. 이제 이름으로 쓸 수 있는 범위는 정의 경로가 닿는 `status_init` 하위와, probe용 3개 이름에 한해 `mutable_storage` 하위다. 이것은 독립성 향상의 주장이 아니라 소비자 없는 통로의 제거다.

**비교 대안.**

- **유지:** 소비자 없는 crate 전체 통로는 [채택 계약 §4.1](../contracts/dev3-module-boundaries.md#target-imports)이 초기화 파일의 `pub use`를 신규 소비자 통로로 쓰지 않는다고 한 규칙과 맞지 않는다. [코딩 컨벤션 §4](../policies/coding-convention.md#section-4)의 최소 공개와도 맞지 않는다.
- **`disk_api` 경유:** 옮길 소비자가 없으므로 공개만 넓어진다.
- **`status_init` 재노출 전체 삭제:** frozen probe 5개의 import 해석이 실패한다.
- **15개 모두 `pub(super)`+allow:** 12개 이름에 대해서는 allow 이유가 사실이 아니다.
- **owner 22행에 allow:** 미사용 재노출을 가릴 뿐이다.

**남긴 항목.**

- probe 전용 3개 이름 재노출
- 같은 5파일 중계를 지나는 나머지 Status·resident·empty-transaction 이름 26개. 이 단위의 범위 밖이며 같은 census로 판정할 수 있다.

**재검토 조건.**

1. owner 밖 실제 소비자가 owner 타입 이름을 요구한다. 새 D2에서 owner의 실제 경로로 가장 좁게 노출하고, 소비자가 `wal_io` 밖이면 `disk_api`로 노출한다.
2. mixed owner probe fixture를 다시 동결한다.
3. `status_init`이나 `status_flush`의 부모가 바뀐다.

#### R3 — 초기화 사슬: 유지

L4와 같이 단위를 열지 않고 재검토 조건을 기록한다. 조사 보고는 증거 artifact의 `r3/`에 있다.

**판정과 이유.**

1. **사슬은 폴더 모양이 아니라 원 상태의 privacy seal이다.** 책임 진입은 사슬 owner의 private 상태에 대한 inherent impl과 생성자다.
   - `MutableStorageOwner`의 `binding` 필드와 `AVAILABLE/CLAIMED/BOUND/ABANDONED`(`mutable_storage.rs:79–82`)를 공개 bind 진입 6개와 CAS 4곳이 함께 쓴다.
   - Drop 경로(`BindingClaim`·`BoundAllocation`·`Plan4Pages`·Status)가 fence와 ABANDONED를 직접 쓴다.
   - `plan4_recovery`는 `BootstrapPlan::prepare_for_kind`, `InstallPlan` literal, private `EpochTransferSeal`을 가진 `SelectedEpochParts`, private `MutableStorageOwner::from_plan4_recovery`를 쓴다.
2. **Rust로는 형제에서 같은 제한을 표현할 수 없다.** `pub(in …)`은 조상만 가리킬 수 있다(계약 §4.1). 진입 하나를 형제로 옮기면 공통 조상 `wal_io`까지 넓혀야 한다. 그러면 `wal()`, `binding`, `BootstrapPlan`·`InstallPlan` 생성자, `from_plan4_recovery`가 `physical`·`disk_api`를 포함한 모든 진입에 열린다. 계약이 금지하는 raw 생성자와 필드의 확대다.
3. **실제 소비자가 없다.** 사슬 안의 경로를 이름으로 쓰는 `wal_io` 밖 파일은 0개다. 밖 23개 파일은 `wal_io::` 재노출을 쓰고, 새 소비자는 `disk_api`/`storage::disk`로 간다. 위 이행 순서 4단계가 단위를 여는 조건으로 둔 실제 소비자 지정이 없다.
4. **frozen 입력의 비용이 크다.**
   - fixture 88개 파일의 논리 경로 문자열 약 6,014개. 그중 54개 이상의 파일이 FROZEN 또는 NOT_RUN 계열 상태다.
   - script 24개의 26개 문자열
   - `mutable_storage.rs`를 anchor로 쓰는 probe script 4개

**rustc 실험.** 진입 12개 각각의 `mod` 선언을 `wal_io` 아래 형제로 옮기고, 원 부모에는 `use`를 두었다. 하위 트리를 벗어나는 `super` 사슬을 `crate::storage::…`로 기계적으로 바꾼 뒤 오류 위치를 셌다.

- 고유 오류 위치는 최소 1,427곳이다. struct private 오류 뒤에 숨는 field literal 오류가 있으므로 하한이다.
- 순수 배치 변경으로 옮겨지는 진입은 하나도 없다.
- 진입별 수치는 증거 절에 있다.

**비교 대안.**

- **책임별 형제 전체 또는 진입 하나의 부분 재부모화:** 소비자가 없는데도 위 공개 확대와 selector·probe 비용이 따른다. 복구 진입은 원 seal 계약과도 충돌한다.
- **논리 부모는 `#[path]`로 두고 물리적으로만 책임 폴더를 만드는 안:** 코드에 없는 경계를 폴더로 암시한다. L3의 “논리 부모를 따르는 배치”, L5 기준 1의 “소유 변화 없는 상위 배치”와도 어긋난다.
- **공개 확대·facade·공유 context:** 계약 §10이 거절한 방식이다.
- **MS claim API를 제자리에서 추출하는 안:** 유한한 변경이지만 지금 필요한 소비자가 없다.

**재검토 조건.**

1. 같은 실험을 진입별로 다시 돌려 privacy·visibility 오류가 0(경로 문자열만 남음)인 진입이 나오면, 그 진입을 배치 전용 단위로 연다. C2/C3가 계약 §3의 “불필요한 내부 접근”을 없앤 뒤에야 기대할 수 있다.
2. 실제 소비자가 7번째 bind 진입, 5번째 CAS, 새 `BootstrapPlanKind`를 요구한다. 이때는 재부모화가 아니라 MS 소유 claim API의 제자리 D2를 연다.
3. M4/M5 뒤 등에 계약의 원 초기화 조상 유지(§4·§4.2와 §7의 C1 행)를 다시 연다.
4. `wal_io` 밖 코드가 `disk_api`나 `wal_io::` 재노출 대신 사슬 안 경로를 이름으로 써야 한다(현재 0).
5. frozen selector 입력을 은퇴시키거나 별도 결정으로 재기준한다(L4 재검토 조건 2와 같다).

**인계.**

- 바꿀 가치가 있는 조상 접근(`storage.core.plan`, `storage.binding`, `wal().completion`)은 C2/C3의 범위다.
- R3 조사 시점에 네 단계 모두에서 재노출되던 이름 126개(R2의 15개 포함)는 계약 §3의 “단순 등록”이다. 소비자를 옮길 때 `disk_api`로 이전한다.

**검증 선택.** R2만 제품 diff가 있다. 영향은 재노출 visibility와 이름 해석에 한정되므로 다음을 확인한다.

- `cargo check -p xadb-engine --all-targets --all-features --locked`(warning 0)
- lib 시험 목록(`--lib --list`) 전체의 byte 동일성. 통합 시험 target은 컴파일만 했고 목록은 만들지 않았다(바뀐 항목이 `pub(crate)`이므로 영향 없음)
- 변경 파일의 `rustfmt --check`와 `git diff --check`
- 경로를 쓰는 mixed owner probe 5개의 컴파일 동등 확인

시험 본문은 바꾸지 않았으므로 NOT_RUN이다. R1·R3는 제품 diff가 없다. 독립 리뷰를 받은 뒤 그 결과를 증거 절에 기록한다.

### 완료 조건과 문서 정본

| 판정 범위 | 완료 조건 |
| --- | --- |
| 이번 문서 반영 | 최신 기준과 후보·순서·한계·소유 문서의 연결을 반영하고 독립 D2 필수 지적0. 제품 이동·검증 결과와 구별 |
| 배치 정리 단위 | 파일별 이동표와 실제 경로가 일치. 해당 논리 모듈·접근 권한·cfg·시험 발견/selector 보존, 영향받은 컴파일/선택 검증·현재 참조 갱신·독립 리뷰 완료 |
| 논리 의존 변경 단위 | 허용 의존·제한 API·원 상태/권한·오류/자원/복구 계약을 검토하고 실제 소비 경로의 변경 국소성·필요한 선택 검증을 확인. 파일 이동 결과로 대체하지 않음 |
| M4/M5 | 기존 공통 공급·MI01–06의 실제 인수 조건 유지. 문서 반영과 폴더 정리가 완료되어도 M4/M5를 자동 완료로 표시하지 않음 |

작업 순서·후보·결정은 이 절이 소유한다. [저장소 배치](../policies/repository-build-plan.md#module-folder-layout)는 기존 코딩/병렬 규칙의 적용과 실제 inventory 갱신 시점을 연결하고, [관리 JSON](../policies/repository-build-plan.json)의 `module_folder_reorganization`은 정본 참조·기준·단위별 실행 상태만 색인한다. [소유 계약](../contracts/dev3-module-boundaries.md#target-imports)은 현재 초기화 조상·접근 제한을 유지하며 구체적인 논리 변경 때 영향 절만 재검토한다. 코딩 컨벤션·병렬 정책의 기존 규칙은 그대로 사용한다.

문서 진입점·계획/정책 README에서 이 절과 배치 원칙을 연결한다. 기존 문서의 ID·경로·제목은 유지하므로 `documents.json`의 등록 변경은 없다. 새 정식 문서를 만들지 않고 임시 초안의 제안을 이 절로 통합했다. 후속 사용자의 commit·불필요한 임시 문서 삭제 지시에 따라, 내용과 조사·검토 근거가 정식 문서에 반영된 임시 초안은 삭제한다. 과거 source별 실행 증거는 유지한다.

### 결정과 실행 기록

| 항목 | 기록 |
| --- | --- |
| ID·일자·분류 | `XADB-MODULE-LAYOUT-001` · 2026-10-29 · D2 ACCEPTED / 후속 계획에 한정 |
| 근거·담당 | 사용자의 임시 계획 작성·진행 작업 종료 뒤 문서 갱신 요청과 후속 진행 지시. 작성·계약·통합 `/root` |
| 선택·이유 | 참조·소유·API 경계를 먼저 대조하여 기존 책임별 배치 규칙을 구체화. 논리 경로 보존 이동과 의존 변경을 분리해 유한한 이행·검증 범위를 확보 |
| 비교 대안 | 평면 파일 분리 유지에는 탐색·소유 이름 불일치가 남음. 모든 파일을 접두어별로 일괄 이동하면 서로 다른 논리 부모와 private 접근을 혼동. 역할별 crate 전환은 실제 필요·검증 범위를 넘어가므로 이번 계획에서 채택하지 않음 |
| 대체·영향 | 기존 M2의 원 초기화 조상·권한 계약, M4/M5 종료 조건과 12절 적용 시점을 대체하지 않음. 미래 논리 변경은 해당 owner 계약의 별도 D2 단위에서 채택 |
| 독립 검토 | `/root/module_layout_plan_review`가 실제7개 문서/JSON diff·임시 초안·DP01/DP02·DOC-002·코딩/병렬 정책·채택 경계 계약·후보 소스 선언과 종료 보고를 직접 읽기 대조하여 문서 반영 범위의 D2 수용·미해결 필수 수정0으로 판정. `/root`가 수신·반영 확인. 구체적 이동표/의존 재설계의 시행 검토와 M4/M5 인수는 포함하지 않음 |
| 실행 | 문서 반영·독립 검토 시점에는 제품 이동·빌드·시험·문서 자동 검사·commit/push 모두 NOT_RUN. 후속 사용자 지시에 따라 문서7파일을 commit하고 통합된 임시 초안을 정리하는 범위로 진행. 제품·검사 실행과 push는 포함하지 않으며 기존 source별 검증 결과 유지 |
| 재검토 조건 | 후보가 이미 해결됨, 다른 작업과 소유 중복, 논리 부모/visibility 변경 필요, 원 권한·복구·자원 계약 약화, 공통 변경 확대 또는 새로운 사용자 방향 |

검토 입력은 위 기준 HEAD의7파일 diff이며, 이 판정·채택 상태 반영 전 `git diff --binary HEAD -- <7파일>`의 SHA-256은 `98825b49b1817959d35ce15b19cab1725029201dbbb6eae049c7d970f8f3254f`, 이 계획 파일은 `21359ae314332df4dee0fe9ebafa690f9abd30c9f270c5e71f7919064a4af3df`다. 검토 후 변경은 수용 판정·문서 채택 상태와 후속 commit·임시 초안 정리 지시의 기록이다. 제품·시험·자동 문서 검사 결과를 새로 생성하거나 과거 결과를 재분류하지 않았다.
