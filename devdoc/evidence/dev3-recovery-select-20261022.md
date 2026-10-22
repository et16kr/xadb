# DEV3 초기 복구 입력 선택 인수

2026-10-22 · **선택 실행·최종 독립 D1 수용 / 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-recovery-select.md) · [source별 실행 색인](dev3-recovery-select-20261022.json) · [구현 상태](../implementation/dev3-implementation.md#recovery-select)

기존 디렉터리에서 exclusive lease를 취득하고 초기 FILEMAP·Empty DWB·genesis header-only WAL을 읽기 전용으로 선택한다. 지원 profile1/2/3과 XBS1 plan1/2를 검산한 닫힌 owner는 borrowed observation만 제공한다. 기존 생성 경로의 pure XBS1/XDI1 preparation을 공유하며 home bytes·raw DWB를 읽거나 writer/reader·BOOT_RESERVE·repair·READY 권한을 반환하지 않는다. 비어 있지 않은 실제 reserve/cancel WAL과 진행된 선택은 명시 거절한다.

## 실제 변경과 선택 범위

기준 commit은 `fdd7ae61ca6b03417a4d40482d4c32d971a3361d`이고 시작 tracked diff는 비어 있었다. 새 SELECT owner·고정 geometry·opened/named metadata 검산·기존 FILEMAP reader의 전용 read-only wrapper가 제품 변경이다. 기존 생성 API는 유지하고 pure preparation과 원 typed cause를 새 소비자에 연결했다. 새 단위의 unit/native/compiler·exact selector/receipt 도구와 known-bad를 함께 작성했다.

| 범위 | 실제 결과와 source |
| --- | --- |
| 필수170 | r0 `848501575d…`에서10함수·170variant PASS. RP6/RU12/RB8/RC20/RG48/RF61/RL5/RQ3/RN1/RS6 |
| 변경 helper4 | r2 `e9f946fd52…`에서 fake metadata replacement가 쓰이는 RG43/44/47/48만 PASS. 다른166은 반복하지 않음 |
| compiler | r0에서 실제 control1+semantic5 PASS. positive·E0382·E0505·E0451·E0599를 정확 diagnostic/path/line과 대조 |
| native | r2에서 NP11/12/21/22/31/32·NW01·NC01의8scenario/28별도 process PASS. 초기 선택6개와 nonempty producer/손상 profile 예상 거절2개를 구별 |
| checker | r0의 신규15method PASS. 빈/오타/중복·잘못된 binary/receipt·0실행·순서/분모·미도달·timeout·틀린 compiler 진단 거절 |
| 정적 검사 | r2의 fmt와 engine all-targets/all-features Clippy PASS. 사례 실행과 구별 |
| S5 | matrix1176 작성·NOT_RUN. exact alias48을 제외한 추가1128, 이 단위 unique runtime1298. 최종 source의 mandatory170 실행과 alias 대응 필요 |

원본170과 후속4의 실제 variant 실행 합은174이며 고유 필수 분모는170이다. 모든 필수를 마지막 binary에서 다시 실행했다고 표현하지 않는다. r0→r1은 native fixture caller directory 수명과 실패 자료 보존만, r1→r2는 fake namespace의 동등한 let-chain 조건식만 바뀌었다. 제품 SELECT·pure helper·인수 assertion·입력 manifest·Python driver는 그대로다. 각 source export/ELF/fixture/log hash와 canonical case→source는 JSON에 보존한다. DEV0/1/2·workspace·하위 wrapper와 완료된 무변경 시험은 실행하지 않았다.

필수170은0.381408초, 후속4는0.003815초로 합120초 budget 안이다. compiler6호출은9.304169초/60초, native8은2.286074초이며 각30초/합240초 안이다. build/setup·checker·정적 검사는 별도다. 기능 실행의 경과 관측이며 성능 인수나 모델 간 효율 비교가 아니다.

## 원 실패와 좁은 수정

| 원본 | 원인·보존·후속 |
| --- | --- |
| compile r0 FAIL | 새 helper 원인을 기존 BootstrapCause에 추가해 기존 adapter exhaustive match E0004. helper 전용 InitialRecoveryError로 분리하여 adapter·기존 producer 의미를 유지했고 compile-only r1 PASS. 제품 사례 실행 전 실패 |
| native r0 FAIL | 첫 NP11 fixture producer가 WAL+FILEMAP reader+data와 caller directory를 함께 보유하여 FD5 검사 실패. 원 stderr는 `>5`만 관측하며 실제6 측정값으로 꾸미지 않음. SELECT 미도달·나머지7개 NOT_RUN |
| fixture 수정 | writer가 cloned directory/LOCK을 소유하므로 producer의 불필요한 caller directory를 취득 직후 Drop. 입력·분모·FD5 budget 유지, raw 자원 자료를 assertion 전에 보존. 수정 source native8 실행 |
| static r1 FAIL | 새 fake metadata helper의 collapsible_if1건. 동등한 let-chain으로 바꾼 뒤 활성4case만 재검증하고 최종 정적 검사 PASS. compiler/checker 반복 없음 |

모든 원 FAIL과 source별 사본은 `artifacts/dev3-recovery-select-20261022-r0`에 보존한다. 실패·중단·미도달을 원 실행의 PASS로 변경하지 않았다. 사용하지 않는 FD의 수명을 줄여 원 상한을 맞췄으며 실패 뒤 상한을 늘리지 않았다.

## 자원과 독립 관측

RQ01 concrete deterministic port의 source/type 값은 owner96·File32·outer4256, preparation nested17672·reader nested14788 bytes다. join은 preparation22024/reader19468/I/O19172로 peak22024≤metadata65536이고 caller scratch49152를 더한71176≤전체 allowance114688이다. oversized W/File은 lease 전에 거절한다. native port도 동일 prelease 용량식을 통과한다. source 소유·타입 회계이며 compiler stack/RSS 실측이나 전체 BOOT_RESERVE가 아니다.

native SELECT 실제 File peak1·DB FD peak4, fixture producer File peak3·DB FD peak5이며 모두 final0이다. source 구조의 SELECT 상한은 File2·process DB FD5다. `/proc` 경계 관측과 연속 peak 보장을 구별한다. SELECT 계열 mutation0·home read0과 borrowed observation을 검산했다.

8개 새 DB의 실제 파일 합176596384 bytes, 원 실패 DB22069248까지 보존한 합198665632≤192MiB다. 각 DB는24MiB 이내다. ELF/source/log·receipt 공간은 DB quota와 별도다. 전후 file bytes/hash·이름·inode/link를 독립 literal과 대조했다. lease hold 중 다른 PID는 Busy이고 Drop 뒤 새 PID에서 같은 초기 선택을 재검산했다. nonempty WAL 예상 거절은 성공한 replay/복구가 아니다.

## 검토와 남은 경계

D2는 `/root/recovery_design_review`의 별도 실행이 지원 범위·XDI1 staging 재사용·오류 순서·자원·S5 alias 포함 관계를 읽고 수용했다. 같은 별도 검토자가 최종 source/diff·원 반례·실제 로그/진단·ELF/receipt·native 파일과 자원 기록을 대조하여 **D1 ACCEPTED**, 필수 수정 없음으로 판정했고 통합 담당이 반영 확인했다. source fingerprint와167건의 hash/파일 식별 대조가 일치한다는 검토 보고를 보존한다. 검토자는 제품 시험·컴파일·문서 검사를 반복하지 않았다.

주 개발 요청 설정은 `gpt-6.1-sol`/high, 핵심 검토 요청 설정은 `gpt-6-astra`/high다. runtime model ID/추론 설정의 독립 확인 수단은 없어 요청 설정과 실제 실행 identity를 구별한다. 첫 평가 단위는 제한 범위로 수용했으며 전체 복구 역량·모델 간 품질/비용 우위를 주장하지 않는다. 이후3.12.1–4 실제 복원·검산의 증거로 배정 판단을 갱신한다.

첫3.12.1.a 선택과 실제 producer nonempty prefix·혼합 home·Pending/torn 복원 목표3.12.1–4를 구별한다. BOOT_RESERVE·analysis/예약 복원·redo·REBASE·epoch/CLR·일반 READY·전체 DEV3/S5는 미완료다. 다음은 선택 owner의 수명·실제 profile 예약과 producer prefix 지원 의미를 확정해 후속 장벽으로 연결하는 일이다. 문서 verifier는 push gate 전용이며 이번 편집·commit 중 실행하지 않는다.
