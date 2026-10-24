# DEV3 복구 이미지·제어 레코드 선택 검증

2026-10-24 · **신규 필수272개(static3·semantic269) PASS, 최종 독립 D1 제한 수용**. [소유 계약](../contracts/dev3-recovery-control-base.md)·[시험 계획](../plans/dev3-test-plan.md#recovery-control-base)·[기계 색인](dev3-recovery-control-base-20261024.json)을 따른다. 실제 live·새 epoch 복구·전체 DEV3/S5는 미완료다.

XRB1의 전체 section/certificate·ATT/Job/Status census와 XRC1/XWE2/XWS2의 byte 생성·검산을 구현했다. 원 ownership·allocation·Catalog 검산을 연결하고 Reserved capture의 terminal 인접 조건을 보강했다. 이 단위는 파일 선택 권한을 발급하지 않는다.

## 원 입력과 범위

독립 stdlib generator가 base15×1MiB와 원 reference14개167,936bytes, control13×4096과 원 reference4×4096을 생성/복사했다. 원 generator를 재실행하거나 제품 encoder 출력을 oracle로 사용하지 않았다. 두 생성은 외부30초/10초 및 메모리64MiB 제한 안에서 각각0.365314/0.215448초로 완료했다. base index는 `b3f7e7f018baf54284ac062ef1f7cf39b0c6da4991748097ce9431889d05fef1`, control index는 `24fc3ed635ae36177021e696d1ac14e0f3396fa6b04e72fa5f329875b667e7f0`다. 전체 fixture50파일15,982,393bytes를 보존했다.

| 보존 source | SHA256 | 결과 |
| --- | --- | --- |
| codec-tests-r0 | `b5f1598e8be4b54af1fe2dd698a3f7a0602218a3edc929f6d3763cae6728721c` | 실제 fresh=false compile·static3·semantic269 PASS; fmt import 배치 FAIL 보존 |
| codec-quality-r0 | `a9cf1d6617cc48176e58c727c9ceefb482461ee6c60b9b1006c3725d9ce23fd5` | import 배치 fmt PASS; Clippy4 lint FAIL 보존; runtime0 |
| codec-quality-r1 | `96a212ef4a2832c9da9d79d871a994ece5359eb354653910a622cf2fff4f7aa5` | 좁은 lint annotation·동등 array-size 식, 변경 파일 fmt·engine all-targets/all-features Clippy PASS; runtime0 |

시험 ELF는 `8ac348abdbdf98a8b283d2def60bc5544cb3932b3a20a7469974d41f8ac1b68c`(45,343,704bytes), 실행 입력은 `4a6123649a6f681f399766a28281fcf3701ff1bac70b00f138ed028f2fd6dbed`다. [결과 색인](../../artifacts/dev3/plan4-g6-recovery-20261024/codec-mandatory-results.json)은272개 고유 PASS·완료 반복0을 기록한다. static receipt SHA `4d7781ba2f79ac716f5f2b342e4cfc8f845fc1297dcaba6712f2fa4650406491`, semantic receipt SHA `204f2a1dcd2c7015fdd53341502720c4e46e3dbe7e67530d0a8d20b121567e29`다.

전체 charge11.279초 중 제품 process7.394892초이며 static30초(worker20/감사10)·semantic180초(worker150/감사30) 및 개별 함수5/30초 안이다. 최초 source/hash·prior 검토와 최종 기록을 포함했다. exact selector별 explicit ID를 사용하고 기존 하위·pure140·전체 wrapper·S5를 실행하지 않았다. 새 XB/G6RC namespace checker12는 별도 PASS이며 기존 checker를 반복하지 않았다.

## 자원·원 실패·품질

base 순수 호출의 deep/return 수명과 caller를 포함한 독립 상계는63,204bytes, control은21,620bytes다. [phase 검토](../../artifacts/dev3/plan4-g6-recovery-20261024/review/pure-call-phase-admission-r0.md)는 원 static 값을 보존하며 아직 없는 반환값과 종료된 nested bridge를 각 phase에서 구분한다. 실제 owner F05·stack/RSS 측정이 아니다.

초기 input 메타데이터 편집은 optional category 누락으로 쓰기 전에 실패했다. 원 기록을 보존하고 static의 순환 F05 의존과 과거 bridge 미공급 설명을 successor에서 정리했다. 이후 fmt 실패는3개 import 배치뿐, Clippy 실패는 macro의 unused count/인자 수와 size 식·optional intent의 스타일 lint였다. [lint source 대조](../../artifacts/dev3/plan4-g6-recovery-20261024/codec-lint-source-proof.json)는 실행문·타입·동시값을 바꾸지 않은 좁은 annotation과 `8×sizeof(usize)==sizeof([usize;8])`를 확인한다. 기존272개를 새 source PASS로 재명명하거나 반복하지 않았다.

같은 작업 공간의 후속 live build가 공유 Cargo cache의 이전 ELF를 재사용한 사실도 별도 [HOLD 감사](../../artifacts/dev3/plan4-g6-recovery-20261024/live-tests-r0/cached-artifact-audit.json)에 보존했다. 그 ELF로 live 시험을 실행하지 않았다. private engine cache를 비우고 실제 fresh=false 빌드를 요구하도록 helper를 보강했다. 원 source/ELF 경로·bytes는 유지했고 동일한 immutable ELF 복사만 hardlink로 합쳤다. 이 사건은 실제 fresh=false로 컴파일한 위 codec 결과와 구별한다.

준비 상한은 source160MiB/ELF128MiB/compile256MiB/records16MiB/fixture20MiB/native0, 최소 free256MiB다. mutable cache와 보존 source/ELF의 inode 교집합은0이다. 별도 live의 첫 static2 실패와 개발 중인 source는 이번 순수 단위에 포함하지 않는다.

## 남은 범위

S5 추가23,886은 NOT_RUN이며 최종 DEV3 인수에서 필수272와 함께 실행한다. 실제 CURRENT 공개·intent·base capture/INIT provenance·REBASE/LOG_OPEN·loser 정리·VERIFY/STABILIZE·같은 pool 재진입과3PID는 아직 미완료다. Catalog CREATE/CLR·첫 행 ACK·AI 소비자2/3·DEV3 전체 완료를 이 결과로 승격하지 않는다. 문서 verifier·push는 실행하지 않았다.
