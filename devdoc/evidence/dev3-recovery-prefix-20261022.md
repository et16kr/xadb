# DEV3 현행 WAL prefix 분석 인수

2026-10-22 · **변경 범위 선택 검증·최종 독립 D1 수용 / 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-recovery-prefix.md) · [source별 실행 색인](dev3-recovery-prefix-20261022.json) · [구현 상태](../implementation/dev3-implementation.md#recovery-prefix)

실제 reserve/cancel·Status INIT producer의 WAL을 처음부터 읽어 canonical allocator·선택적 Status·최신 descriptor와 잔여 의무를 재구성한다. 닫힌 owner는 exclusive lease·열린 source WAL·caller256KiB borrow를 유지하고 관측만 제공한다. 기존 초기 SELECT API와 live producer 권한은 유지한다. home 읽기·변경·sync, Pending raw image 수리, BOOT_RESERVE·실제 예약 복원·물리 REDO·READY는 공급하지 않는다.

## 실제 변경과 지원

기준 HEAD는 `412bd7b8477cde8abee86de4c5d18611f9d66385`이며 시작 worktree는 깨끗했다. 새 semantic envelope prefix scanner와 `wal_io::RecoveryPrefix`를 작성했다. 기존 allocation/Status 검산에서 pure expected context를 추출하고 immutable bootstrap preparation을 공유했다. live role·fence·WAL durable proof를 위조하지 않으며 canonical before와 물리 mixed home 검증을 구별한다.

profile1/2/3·XBS1 plan1/2·초기 FILEMAP·genesis epoch1에서 실제 exclusive/dirty reserve→cancel 반복과 Status reserve→cancel/INIT를 지원한다. count1/63·선행 owner chain·단조 high-water·descriptor/inventory·native/redo 결합을 검산한다. active EOF를 terminal로 바꾸지 않는다. 선택 DWB의 generation/batch/bank와 Pending의 **해당 historical LSN** canonical digest를 대조한다. raw slot/home의 유효성 인증은 아니다.

불완전한 physical EOF는 보이는 header 필드를 검산한 뒤 미채택 tail로 보존한다. 완결 checksum 오류·unknown opcode·중간 손상을 버리지 않는다. 최대 지원 frame38000을 넘는 완결 frame은64KiB chunk로 checksum을 검산한 뒤 거절하며 source를 truncate하지 않는다. source identity/length를 마지막에 다시 대조한다.

## 실행과 source 귀속

| 범위 | 실제 결과 |
| --- | --- |
| 새 scanner42 | r3에서6 exact 함수 PASS. 기존 legacy scanner 시험은 실행하지 않음 |
| 새 core189 | r3의 PPN001/002 완료 단언2, r4의 완결 job179·실패 job 내 PPW001–006 완료 단언6, r5 수정2case PASS로 고유189를 확인 |
| 공유 bootstrap 수신 | r3에서 변경 helper와 직접 관련한 기존 RS01–RS06만 PASS. 나머지 SELECT164를 반복하지 않음 |
| native31 | r3의 실제 producer·별도 PID 분석64worker PASS. canonical inspection29와 의도한 입력 거절2를 구별 |
| compiler | r3에서 semantic8+unmodified control1 PASS. positive 및7개 정확한 오류 code/path/line 검산 |
| checker | r1의 신규3module·15method PASS. 이후 runner의 manifest hash pin만 갱신했으며 실행 로직·시험 method는 불변이므로 반복하지 않음 |
| 정적 검사 | r5 fmt와 engine all-target/all-feature Clippy PASS. 사례 실행과 구별 |
| S5 | core matrix60/alias24/추가36, scanner matrix244/alias7/추가237, native PL01 작성·NOT_RUN |

새 runtime 고유 분모는231이며 **완결 job PASS223와 원 실패 job에서 완료된 단언8**을 구별한다. PPN001/002와 PPW001–006을 독립 PASS job이나 마지막 binary의 재실행으로 표시하지 않는다. 이후 수정은 해당 분기의 입력·단언을 바꾸지 않아 반복하지 않았다. source별 원 결과의 strict BEGIN/PASS/COMPLETE·libtest 결과와 canonical case map은 JSON에 보존한다. 전체231을 최종 binary에서 새로 실행한 결과가 아니다.

r0는 중복 workspace 경계를 사전 검토에서 보완하기 전 source이며 compile/test NOT_RUN이다. r1은 첫 컴파일 및 checker source, r2는 native compile 수정 source, r3는 정적 수정 뒤 최초 제품 실행 source다. r4는 독립 literal chain 조립 정정, r5는 실패한 두 oracle 경계 정정이다. 마지막 source fingerprint는 `2468c48df6c8821d301c30130e6702c847494d1f646fa1e5f16ecb091c8fcd42`, ELF SHA는 `0559cf83a551796504d74d112eb0d7274b9295f04f88212143c12da088cbee95`다. 제품 core는 r3 이후 바뀌지 않았다. 동시 작업의 `.github/workflows/dev0.yml`·문서 검사 정책 변경은 보존하고 이번 commit에서 제외한다. 관측 당시 root와 r5의 source 입력 차이는 해당 workflow1개뿐이며 전체 root fingerprint를 r5 검증 결과로 표시하지 않는다.

runtime 다섯 선택 실행의 controller 경과 합은1.277619초다. 각 필수 job30초·semantic/core 각각 합120초의 별도 budget을 유지했다. compiler9호출은13.808438초/60초, native31은10.742923초이며 가장 긴 case0.623986초로 각30초/합300초 안이다. 기능 실행 관측이며 성능 인수나 모델 효율 비교가 아니다.

## 원 실패와 최소 수정

| 원본 | 원인과 후속 |
| --- | --- |
| r0 사전 검토 | PPB005가 PPB001과 중복. 같은 workspace를 덮은 뒤 재대여해 전체 이미지/descriptor·lease 반환을 검산하도록 보완. 실행 전 발견이며 runtime FAIL이 아님 |
| r1 compile FAIL | native 어댑터의 foundation→I/O 오류 변환 누락 E0277와 events/counter borrow 충돌 E0502. 원 오류를 `io::Error`의 source로 보존하고 counter를 먼저 복사. 제품 사례 미실행 |
| r2 Clippy FAIL | 미사용 PageLsn import와 unit의 수동 range pattern. 최소 동등 수정 뒤 r3·최종 r5 정적 검사 PASS |
| r3 runtime FAIL | PPN003의 n63 literal은 previousLSN4인데 genesis 뒤 단독 조립. 실제 n1→n8→n63 선행 literal을 그대로 연결하고 work3/LSN5·6/hold510을 기대. 제품 order 검사는 유지 |
| r4 runtime FAIL | PPW007의 owner1 XOR1이 invalid ObjectId0으로 먼저 거절. valid owner2로 바꾸어 원래 identity mismatch 목표를 검산. 앞선6개 완료 단언 보존 |
| r4-unrun runtime FAIL | PPL001에서 token subtotal1,032,192,000을1GiB 초과로 잘못 기대. 실제 잘못된 총계는 fixed+WAL+누적 token=1,121,408,768, 올바른 총계는89,233,152로 고정. 앞선 PPW008/009 PASS 보존 |

r4의 중단 뒤는 아직 도달하지 않은117개, 다음 중단 뒤는 남은114개만 처음 실행했다. 후속 r5는 PPW007/PPL001만 재검증했다. 완료된 무변경 사례를 재실행하거나 생략·미도달을 PASS로 바꾸지 않았다. 모든 source/ELF/fixture/log/원 FAIL은 `artifacts/dev3-recovery-prefix-20261022-r0`에 보존한다.

## 자원과 독립 관측

mock source census는 outer6400+max(bootstrap18024,replay26936,stream3136)=33336bytes다. native64worker는 outer6472+max(18024,26936,3136)=33408bytes, W16/File48이며 caller262144bytes는 별도 독점 대여다. source-owned64KiB 및 maintenance12MiB 내부의 caller 몫으로 계산한다. 실제 OS stack/RSS 측정이나 BOOT_RESERVE 선확보 증거가 아니다.

native의 폐쇄 분석33worker는 File peak2·mutation0이며 producer를 포함한 DB FD peak5, 모든 worker의 최종 File/FD0이다. DB31개의 파일466개, 총685456984bytes≤768MiB, 최대22132544bytes≤24MiB다. 전후 전체 file bytes/hash·length·device/inode/link, canonical RAM 전체 bytes와 독립 literal을 대조했다. home·비선택 bank·raw slot·stage 내용은 분석에 읽지 않았다. lease hold 중 다른 PID의 Busy와 Drop 뒤 새 PID 분석도 확인했다.

known closure의 actual owned bytes·미실현 selector-stage credit·retained WAL·remaining protected의 기존 page credit·allocator/Status topup을 한 번씩 계산한다. terminal 주소 union을 반복 generation마다 누적 quota로 charge하지 않는다. 원천 WAL 안정화나 terminal release 권한은 없고 active owner를 보존한다. 이는 일반 DB orphan 전수 회계·물리 공간 선확보·예약 인계의 완료가 아니다.

## 검토와 남은 경계

별도 `/root/recovery_design_review`가 D2·추가 회계·S5 PL01·실행 전 source/type/입력 검토를 수용했다. 같은 별도 검토자가 최종 source별 결과·수정 diff·부분 완료8·ELF/receipt·native 파일/자원 자료를 대조해 **D1 ACCEPTED**, 남은 필수 수정 없음으로 판정했다. raw64worker·실제466file·25개 evidence hash와 canonical case map의 불일치0을 확인했고 통합 담당이 반영 확인했다. 검토자는 제품/시험 편집과 compile/test·문서 verifier 실행을 하지 않았다. 최종 원 보고서는 artifact의 `final-review-r5.md/json`에 보존한다.

요청 모델 역할은 구현 `gpt-6.1-sol`/high, 핵심 독립 검토 `gpt-6-astra`/high다. 실제 runtime identity/추론 설정을 독립 확인했다고 주장하지 않는다. S5의 unique runtime은 필수231+추가273=504이며 PL01은 별도 실제1000cycle/128MiB/300초로 최종 인수에 배정한다. 모든 S5·무변경 하위 wrapper·DEV0/1/2/workspace 제품 시험은 NOT_RUN이다.

다음은 실제 bootstrap 자원·source sync·선택 Pending 수리·예약의 한 번 인계·page별 REDO와 다른 PID 재장애 검산이다. REBASE/새 epoch/CLR·일반 recovery/READY·전체 DEV3와 최종 S5는 미완료다. 문서 verifier는 push 전용이며 편집·commit을 이유로 실행하지 않는다.
