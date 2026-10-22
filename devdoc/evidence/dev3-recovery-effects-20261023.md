# DEV3 현재 genesis의 실제 닫힌 복구 인수

2026-10-23 · **변경 범위 선택 검증·최종 독립 D1 수용 / 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-recovery-effects.md) · [source별 실행 색인](dev3-recovery-effects-20261023.json) · [구현 상태](../implementation/dev3-implementation.md#recovery-effects)

실제 reserve/cancel·Status INIT producer의 현행 genesis prefix에 고정 두 zero 슬롯 provisioning, source 안정화, historical Pending 수리와 whole retirement, 예약의 한 번 인계, page별 REDO와 별도 recovery DWB flush를 연결했다. consuming owner는 writer lease·source File·caller maintenance12MiB/flush4MiB의 전체 독점 대여를 유지한다. 성공은 닫힌 inspection이며 실패 owner도 읽기 관측과 Drop만 허용한다. retry·raw writer·READY를 공개하지 않는다.

## 지원과 실제 변경

기준 HEAD는 `e9cc2737a392b2a1ce726346bb36298c62ca438c`, 앞선 제품 공급은 `e6153995bdc6a708346db4039efaf963d5c7b618`이다. 사이의 기존 CI 변경은 보존했다. 제품·시험·도구37경로를 구현했고 dependency·source root·wire 형식은 추가하지 않았다. 기존 readonly Prefix의 공개 reserved-file 거절과 초기 SELECT 제한을 유지한다. 새 내부 경로와 직접 변경 수신은 새 시험에서 검산하며 무변경 하위 시험을 반복하지 않는다.

profiles1/2/3·XBS plan1/2·초기 FILEMAP의 단일1MiB extent·epoch1에서 현재 producer의 reserve/cancel·첫 Status INIT만 지원한다. 두 `d3-rebase-*` 이름은 이 단위의 고정 zero 공간이며 REBASE body는 없다. 두 슬롯을 모두 사전 검산한 뒤 매 진입에서 각각1MiB를64KiB씩 다시 쓰고 sync·close·RO reopen·identity/length를 확인한다. source file sync와 namespace sync 뒤에만 home 효과를 시작한다.

선택 Pending은 해당 historical LSN의 canonical full image·typed identity·digest와 대조한다. healthy equal-LSN의 다른 bytes, 더 높은 LSN, identity/kind/온전한 비지원 형식은 거절하며 checksum-torn은 지원 범위에서 수리한다. healthy allocator/Status의 typed codec을 추가 검산하고 healthy older Status의 LSN을 encoded INIT prior_lsn(현행 grammar0)과 대조한다. 물리 home cursor와 canonical replay를 분리해 allocator/Status 각각 skip/apply한다. 잔여 의무는 private reservation에서 한 번만 이동하며 page credit/topup을 중복 charge하지 않는다.

## 실행과 source 귀속

| 범위 | 실제 결과 |
| --- | --- |
| 신규 runtime393 | r3 완결 job381, 원 실패 job 내 EN001–007 완료 단언7, r4 EN008–012 완결 job5. 고유 완료 단언393이며 완결 job 사례386과 구별 |
| native31이력 | r3 NR01–14, r4 NR15–25·NB01–06. 완결 이력의95worker=producer31+effect64이며 전체 보존 실행은 원 실패 producer1을 더한96 |
| native 결과 분류 | recover26·verify26·의도한 recovery 거절3·의도한 acquire 거절2·장애 주입7.31을 모두 새 정상 설치 PASS로 바꾸지 않음 |
| compiler | r3 semantic9(EC00–08)+unmodified control1 PASS. positive와8개 정확한 오류 code/path/line을 확인. 이후 제품/API·compiler fixture/tool 불변으로 반복하지 않음 |
| checker | r1 신규3module·30method PASS. r4에서 변경된 producer-cut method1만 재검증 PASS. 고유 method 분모는30 |
| 정적 검사 | 최종 r4 fmt·engine all-target/all-feature Clippy PASS. test target 컴파일과 사례 실행을 구별 |
| S5 | runtime matrix3504/alias36/추가3468, native matrix63/alias7/추가56 작성·NOT_RUN |

전체393을 마지막 binary에서 새로 실행한 결과가 아니다. 원 실패 job의 EN001–007도 독립 PASS job으로 바꾸지 않는다. r3 runtime/native controller의 loop 뒤 최종 identity gate는 실패 때문에 미도달했으며 원 run은 **FAIL**로 유지한다. outer command receipt의 source 불변 확인과 별도 `late-integrity-audit-r3.json`의 보존 source/ELF/log/DB hash 대조는 후속 읽기 감사 시점 증거다. 원 gate PASS로 소급하지 않는다. r4 선택 실행은 자체 최종 identity gate까지 PASS다.

source-r0는 실행 전 반례로 미채택됐다. r1에서 첫 compile/checker, r2에서 변환 수정 compile, r3에서 lint 수정 뒤 최초 제품 사례, r4에서 시험 장치만 수정했다. 최종 source SHA는 `b02136125e51902c166d453835466c0150fbaeabb18e255ecbf0b4e9053d216a`, ELF SHA는 `d6107928b47368e8c1d929d8cebdd8e05825afbc7aba7f1c6b940e8d82f357c5`다. 제품 로직은 r2 이후 불변이며 각 source·ELF·fixture·로그·원 FAIL은 `artifacts/dev3-recovery-effects-20261022-r0`에 보존한다. artifact 이름의 날짜는 설계 시작일이고 실제 실행일은2026-10-23이다.

runtime 두 controller 경과 합은4.855149초/180초이고 각 job30초 안이다. native 원 실패 포함 두 controller 경과 합은43.994045초/300초이며 각 완결 이력30초 안이다. compiler 총 경과20.610645초/60초와 각 checker 선택30초 이내를 확인했다. 기능 시험 관측이며 성능 인수나 모델 효율 비교가 아니다.

## 원 실패와 최소 수정

| 원본 | 원인과 후속 |
| --- | --- |
| r0 사전 검토 미채택 | EH005/008이 reserved header 손상을 다른 의미로 기대하고 EH010 incarnation 변경은 no-op. r1에서 typed-valid 다른 body와 정확 Corrupt/IdentityMismatch 경계로 정정. 제품 실행 전 발견 |
| r1 compile FAIL | mutable slice에서 immutable page-array borrow로 직접 변환하는 E0277. r2에서 불변 재대여만 추가. 사례 미실행 |
| r2 Clippy FAIL | 새 시험의 manual range 조건. r3 동등 Range::contains로 수정. 사례 미실행 |
| r3 EN008 FAIL | 의도한 panic 뒤 reset이 fault plan을 유지해 clean reentry에도 panic 재주입. r4 해당 분기에서만 plan을 해제. EN008 재검증과 미도달 EN009–012 첫 실행만 선택 |
| r3 NR15 FAIL | producer cut18의 실제 FileIo SelectorSync 오류를 controller가 Namespace 문구로 기대. r4 정확 IoWriteUnknown·sync context·selection index/step·raw 마지막 sync를 검산. 실패/미도달17이력과 변경 checker1만 선택 |

완료된381사례·7단언·14native이력을 재실행하지 않았다. compiler·나머지 checker와 무변경 하위 wrapper도 반복하지 않았다. 생략·미도달 사례를 새 실행의 PASS로 표시하지 않는다. 원 수동 physical counterexample의 header 크기 설명 오류(96/104)는128/136으로 별도 정정했으며, 그 수동 r0 입력은 실행 증거가 아니다.

## 자원과 독립 관측

native post-lease62관측의 typed source census는 outer9008+max(prefix33408, inventory3320, work31160)=**42416bytes≤64KiB**, W16/File48이다. 실제 caller maintenance12582912·flush4194304bytes 독점 대여는 별도다. typed 동시 값 회계이며 OS stack/RSS 측정이나 일반 OOM 보장은 아니다. bounded namespace18·실제 Vec capacity≤18와 전체 borrowed arenas·private per-W reservation의 선행 한도를 확인했다.

완결 이력의95worker는 FD peak5·최종 FD/File0이다. closed recovery File peak2와 producer 포함 peak3을 구별한다. local File·inventory stream의 permit 회수와 실패/unwind 뒤 owner 보존을 검산했다. native full file bytes/hash·length·identity, source 보존, 독립 mutation payload/order, full new port-call order와 read hashes·최종 home readback을 대조했다. failed owner의 actual_owned/outstanding은 마지막 확인된 stamp의 charge와 아직 보호 중인 credit이며 실패 후 측정한 실제 물리 길이로 소급하지 않는다. raw 물리 snapshot은 별도 보존한다.

고유31 DB 최종525file·747824793bytes, 최대24322304bytes≤24MiB다. 원 실패 NR15 DB까지 보존한 전체541file·769957337bytes≤768MiB, failed snapshots20301713bytes≤32MiB, 나머지 native 증거5322960bytes≤64MiB다. 별도 보존 ELF184499616bytes와 source/compile 산출물은 이 native evidence 한도와 구별한다. 이는 일반 orphan 전수 회계나 전역 disk 공간 예약의 인증이 아니다.

## 검토와 종료 경계

별도 `/root/recovery_design_review`가 제한 구현설계 D2와 source-r1 첫 실행 gate를 수용했다. r0 미채택·r1 수용 및 이후 원 실패는 별도로 보존했다. 같은 별도 검토자가 최종 source별 결과·최소 수정·부분 단언·자원/파일 원본을 대조해 **D1 ACCEPTED**, 남은 필수 수정 없음으로 판정했다.37개 소유 경로·36개 색인 receipt·1085개 독립 snapshot 기록과525개 실제 최종 파일을 대조했으며 통합 담당이 반영을 확인했다. 원 보고서는 artifact의 `final-review-r4.md/json`에 보존한다. 검토자의 compile/test/checker/docchecker 실행은0이다. 요청 모델 역할은 구현 Sol/high·핵심 독립 검토 Astra/high이며 실제 runtime identity를 독립 확인했다고 주장하지 않는다.

사용자 지시에 따라 이 단위의 최종 검토·local commit 뒤 멈추며 다음 leaf를 시작하지 않는다. 일반 BOOT/F05·REBASE body/continuation/reclaim·새 epoch·CLR/undo/READY·전체 task12/DEV3·최종 S5는 미완료다. 모든 S5·무변경 하위·DEV0/1/2/workspace 제품 시험은 NOT_RUN이다. 문서 verifier는 push 전용이며 이번 편집·commit에서는 실행하지 않는다.
