# DEV3 Catalog 검토 후 읽기 수정과 오류 분기 검증

2026-10-26 · 사용자가 Claude의 DEV3 작업 검증에 이어 문제 해결·작업 완료·commit을 요청했다. 범위는 [Catalog CREATE 남은 작업](dev3-catalog-remaining-20261026.md)의 읽기 결함과 pair 준비 오류 검증이다. 첫 Row INSERT·runtime F05 전체 인수·DEV3/S5 완료로 확대하지 않는다. 작업·통합 담당은 `/root`, 독립 D1 검토는 `/root/dev3_fix_review`다.

## 수정과 검증 범위

`RetentionReader::read`가 v2 Profile에도 Root/Jobs v1 decoder를 사용해 정상 페이지를 `UnsupportedFormat`으로 fence하던 결함을 수정했다. 검증된 원 Profile의 format으로 decoder를 선택한다. decoder 실패 뒤 다른 버전을 시도하지 않으며 membership·원 guard·fatal fence·checked release를 유지한다. v2 codec의 borrowed view와 반환값도 source 계산에서 청구한다. wire·Profile 형식·상한·소유 관계를 변경하지 않는다.

신규 입력과 유한 범위는 `artifacts/dev3/catalog-create-20261025/review-fix-20261026/input-plan.json`에 실행 전에 고정했다. 신규 runtime4·selection4와 직접 바뀐 source 계산2만 exact selector로 실행했다. 각30초·주소 공간1GiB·출력 파일8MiB·전체 실행300초 상한이며 기존 완료 MC/MT/MF 및 하위·전체 workspace 시험을 반복하지 않았다. 기존 SHA 고정 memory-gap와 v1 fixture를 관측/손상 입력으로 사용하고 새 oracle를 제품 decoder로 생성하지 않았다.

| ID | 확인 내용 | r38 결과 |
| --- | --- | --- |
| CR01 | 실제 fresh v2 owner의 AgingStart 후 Root 읽기, 독립 golden bytes, guard/pin 반환 | PASS |
| CR02 | 동일 실제 owner의 Jobs index0–4 전부 읽기, golden bytes·상태 보존 | PASS |
| CR03 | 유효 checksum을 가진 v1 Root/Jobs를 각각 fresh v2 owner에 혼입: UnsupportedFormat·fence, guard/pin·DPT·파일/WAL 보존 | PASS |
| CR04 | Root/Jobs bytes 손상: fatal 거절·fence, guard/pin·DPT·파일/WAL 보존 | PASS |
| PR01 | 보호7/전체8 frame에서 둘째 victim 없음: Busy·Unchanged, 예약/세대/bytes/DPT/pin 보존 | PASS |
| PR02 | 둘째 victim generation 최대값: CounterExhausted·Unchanged, 동일 상태 보존 | PASS |
| PR03 | victim write latch 경합: Busy·Unchanged, 동일 상태 보존 | PASS |
| PR04 | dirty frame의 recLSN 누락: 하위 invariant에서 fence, 후보 발급 전 거절·상태 보존 | PASS |
| SC01/SC02 | 변경된 v2 reader source 계산을 포함한 memory/native 타입별 admission | PASS |

PR01–04는 metadata selection 단위 입력이며 실제 저장/WAL/복구의 근거가 아니다. `load.rs`의 Flush 후보 recLSN 없음 분기는 하위 `pair → stamp_in → FrameContent::valid` 검사를 통과할 수 없어 실제 입력으로 도달하지 못한다. PR04는 이 실제 거절 지점을 확인했으며 상위 방어 분기 실행으로 표시하지 않는다. dirty victim flush 후 두 번째 CREATE 성공은 기존 r36 MT02 증거에 그대로 속한다.

SC01/02는 `undo_read_source_bytes`에 v2 codec 비용을 추가한 직접 diff 때문에 선택했다. native source live peak64,074·metadata132,170·whole197,706B, recovery peak65,426B로 기존 상한 안이다. runtime F05·machine stack/RSS 측정 결과는 아니다.

## 반례와 source별 보존

[기록](dev3-catalog-review-fix-20261026.json)에 source/ELF·입력·로그·receipt·검토 해시와 exact selector를 연결한다.

- r37: 새 테스트 import 경로와 flush 인자의 compile 오류2건. 빌드 FAIL 보존, 제품 시험 실행0. 빌드 한도는 기존 controller를 재사용했으며 이 사본의 plan 설명에 이전 r36 문구가 남았지만 실제 입력은 새 overlay다. r38부터 설명을 정정했다.
- r38: compile PASS, 신규8개와 source2개 PASS. 이후 Clippy가 새 시험 tuple의 `type_complexity`를 거절한 원 로그를 보존했다.
- r39: r38 소스에서 `read.rs`만 원 r36 버전으로 교체한 별도 대조 사본. 신규 CR01/CR02만 각각1회 실행해 둘 다 `UnsupportedFormat`의 예상 FAIL을 확인했다. 제품 작업 트리를 되돌리지 않았으며 이 FAIL을 정상 PASS로 재분류하지 않는다.
- r40: 시험의 기존 tuple에 `FrameContents` 타입 별칭만 붙인 최종 소스다. 실행 동작·값·검사·제품 코드는 r38과 같다. compile·fmt·Clippy 결과는 별도 보존하고 완료 runtime/source 시험은 반복하지 않는다. r38 PASS를 r40 실행으로 표시하지 않는다.

r33–r36의 원 PASS·FAIL·UNREACHABLE/NOT_RUN은 보존한다. MF01/MF13 및 native·장기 부하 미실행을 이 작업으로 완료 처리하지 않는다. 문서 verifier는 사용자 정책에 따라 commit 전에 실행하지 않으며 실제 push의 gate에 남긴다.

## 독립 검토와 종료점

독립 검토는 Profile 기반 단일 decoder·오류와 guard 해제·source 청구, 신규 input/selector·oracle와 반례 대조 범위를 확인했다. 최초 지적된 테스트 타입·비교·동일 ID 중복을 수정한 뒤 실행 전 검토를 받았다. 최종 독립 판정은 **ACCEPTED_LIMITED_D1, 차단 지적0건**이다. 원문은 `artifacts/dev3/catalog-create-20261025/review-fix-20261026/final-independent-review.md`이며 source별 기록에 해시를 연결한다. 검토자는 source24,052개·최종 Rust19개·연결36개와 실제 실행 로그/ELF를 독립 대조했다.

이 단위는 기존 Claude 작업의 Live Undo·순차 CREATE 보완과 함께 commit한다. 전체 DEV3 단계 상태와 첫 Row INSERT 미착수는 유지한다.

최종 r40의 compile·fmt·engine all-targets/all-features Clippy가 PASS다. r37–r40의 compiler target만 검산 후 정리했으며 source·ELF·원 로그·receipt는 보존했다.
