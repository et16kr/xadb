# XADB 원 plan4의 Free Undo container 수명

2026-10-25 · **D2 수용·신규 선택 실행 완료 / 최종 독립 D1 수용 / 실제 행 거래·DEV3/S5 미완료**

[소유 계약](../contracts/dev3-plan4-free-undo.md) · [source별 기계 기록](dev3-plan4-free-undo-20261025.json) · [S5 연결 inventory](../plans/dev3-test-plan.md#plan4-free-undo-inventory)

**D3-PLAN4-FREE-UNDO-001**은 원 plan4의 같은 InstalledStorageCore·MutableStorageOwner·WAL·fence·SharedPool에서 System-owned Free Undo 페이지를 발급하고 저장·재적재·복구한다. private plan4 role와 linear reservation이 원 allocator claim을 소비한다. 별도 namespace·WAL·pool·페이지 권한을 만들지 않는다. Status와 Undo 모두 명시적인 XSG1 v2/flags1 scope와 최대38208B terminal 예약을 사용하며, Free Undo의 canonical432B cell을 기존 INIT algebra로 설치한다.

선택 base v2는 고정1MiB 안의 FreeUndo352B record와 원 INIT provenance로 정확한 페이지를 복원한다. 내부 recovery target row는 accepted role를 보존한다. home의 kind나 스스로 재봉인한 digest를 권한으로 사용하지 않는다. 기존 v1 순수 decoder와 원 증거는 유지하며 current-build plan4 runtime은 v2만 계속 사용한다.

| 검증 | source별 결과 |
| --- | --- |
| 정적2 | r3 PASS. memory recovery peak65320/native65456, 상한65536 유지 |
| 메모리7 | r3의 base3·실제 수명·역할/손상·Reserved 복구 취소6함수 PASS; r4의 실패 경계1함수 내부7분기 PASS |
| 컴파일러7 | r3 baseline1·positive1·negative5 PASS. double consume·Owner 재대여·role 탈출·plan3 role 혼용·initialize 뒤 cancel 재사용을 정확 오류 code/span과 대조 |
| native3단계 | r5의 같은 ELF, PID249737/249752/249769, 외부 SIGKILL/SIGKILL/정상 종료. 전체7.276초, 최종 감사0.300초 |
| 도구4 | 고유4 PASS. 이후 변경된 no-force 검사와 arm 구간 검사만 각각1회 추가 검증 |
| 품질 | r3/r4/r5의 engine lib/tests all-features Clippy·fmt PASS. 문서 verifier는 push 전용이므로 실행하지 않음 |

native producer는 Status9 L2와 Undo10 L4를 만들고 실제 flush·clean eviction·두 번의 physical read를 거친다. Undo11 L6의 Pending DWB 선택·directory sync 뒤 첫 home write 직전에 종료한다. 두 번째 PID는 이를 복구하고 capture6의 v2 base를 선택하여 old WAL1을 실제 unlink/dirsync한 뒤 Status12 L8·Undo13 L10을 durable로 만든다. 새 page12/13의 home을 쓰지 않고 다시 종료한다. 세 번째 PID는 old WAL1 없이 base+suffix를 복구하고 Status14 L12·Undo15 L14를 추가 발급한 뒤 Undo15의 flush·eviction/load를 완료한다. Status14의 home은 아직 설치되지 않았으며 그 INIT는 durable WAL에 있다. 각 단계의 실제 파일 snapshot과 trace를 보존한다. File peak3·DB FD peak6을 지켰고 정상 마지막 PID의 handles0을 확인했다. 종료된 앞 두 PID를 handles0 정상 반환으로 표시하지 않는다.

제품·정적·컴파일러의 원 source는 r3 `02a0e27216a42342cede8163a8f01a4417afbd647c75dc593baa8329b2614f61`, 수정된 실패 입력은 r4 `b27c65a242f1327ec505e036deb05b20acd7c72b0d08da2146341bf6cd2bfd41`, 최종 native는 r5 `2b0fb7df40f481e23f1a68794c4fa9db36f17670e0c2dc7f9e1b4ff14fcdd44d`다. r5 ELF는 `ead9b292588bce25eaf8c885df67eda46fd47f4db79581c2c74e8bff9a87e3ba`다. r3→r4는 실패 시험의 입력/선택과 provenance, r4→r5는 native 시험 adapter의 arm trace와 controller만 바뀌었다. 원 static/compiler/memory PASS를 새 source에서 재실행한 결과로 바꾸지 않는다.

최초 r0 compile 오류, r1 자원·Clippy 실패, r2 검토 보류·시험 NOT_RUN, r3 시험 입력 실패와 r4 native controller 실패를 모두 보존한다. r1은 서로 배타적인 Catalog·Status·Undo 복원 생존량을 합산하여 native66070B를 계산했다. 실제 helper 경계를 분리하고 borrowed view와 typed branch max를 독립 검토한 뒤 상한을 유지한 채 통과했다. r3은 frame counter만MAX이고 DPT counter는2인 모순된 입력이어서 의도한 overflow 앞에서 거절됐다. 두 counter를 일치시키고 원 LSN·recLSN·requirement 및 기대 결과는 유지했다. r4는 arm 이전의 초기 zero-fill 쓰기를 Undo image 쓰기로 잘못 분류했다. 명시적인 arm→cut→barrier 구간으로 고쳤으며 실제 Pending/image/home 검산은 유지했다.

원 증거는 `artifacts/dev3/free-undo-20261025/`에 있다. r0–r5 source 전체 member·bytes·hash를 검산한 압축본과 manifest를 보존한다. 컴파일러7개의 overlay도 원 patch·진단·metadata와 함께 검증된 압축본을 남겼다. 실패한 native DB와 성공한3단계 snapshot은 전체 파일을 검산한 압축본으로 보존하고 펼친 사본을 정리했다. 마지막 live DB는 최종 snapshot과 모든 파일이 같음을 확인한 뒤 제거했다. 이 담당 전용 Cargo target114466933B는 보존 ELF5개의 hash를 확인한 뒤 제거했다. 다른 작업 cache는 수정하지 않았다. archive·정리 기록의 실제 경로와 hash는 기계 기록에 연결한다. r1–r5 cleanup의 `preserved` 문구에는 r0의 “original build failure” 설명이 재사용됐지만 이 build들은 PASS다. 실제 판정은 각 원 build-receipt의 exit_code이며 cleanup 원본은 변경하지 않는다.

독립 검토자 `/root/dev3_review`는 최종 source4921개, 변경 범위, raw50항목과1951392B 신규 literal, 원 source별 결과를 확인했다. native의 실제 selector/control/image seal, prior page 재복구, exact epoch suffix, old WAL unlink/dirsync, 미강제 home과 마지막 정상 반환을 snapshot·trace로 대조하고 이 Free container 단위의 최종 D1 PASS를 보고했다. 통합 담당 `/root`가 채택했으며 reviewer의 제품 재시험은0회다.

이 단위는 Free container의 실제 수명 공급이다. Live/Releasing Undo·UndoSequence 발급, Catalog CREATE·Row INSERT·Commit/publication·nonempty REBASE와 loser 복구는 후속 구현이다. 최종 S5에서는 새 inventory와 형식 변경의 영향을 받는 기존 plan4 실제 소비자 입력을 함께 처리하며, 전체 DEV3 완료로 표시하지 않는다.
