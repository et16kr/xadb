# DEV3 복구 이미지·제어 레코드의 순수 표현

계약 ID: **D3-RECOVERY-CONTROL-BASE-001**. XRB1 복구 이미지와 XRC1 CURRENT·XWE2 WAL header·XWS2 WRITER의 byte 표현, 닫힌 plan4 census와 공통 검산 bridge를 다룬다. [wire 결정](../../artifacts/dev3/plan4-g6-empty-retention-wire-decision-20261024.md)의 SHA256은 `0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2`다. 실제 파일 선택·공개·복구 권한과 분리하며, 디코딩 성공으로 VerifiedBase·Fresh·WalRequirement를 만들지 않는다.

신규 필수272개(static3·semantic269)를 원 source/ELF에서 검증했다. 최종 독립 D1을 순수 codec 범위로 제한 수용했다. [실행 증거](../evidence/dev3-recovery-control-base-20261024.md)와 [단계별 선택](../plans/dev3-test-plan.md#recovery-control-base)을 따른다. live owner·새 epoch 복구·DEV3/S5는 미완료다.

## XRB1의 닫힌 이미지

제품은 [base codec](../../crates/engine/src/storage/plan4_recovery_base.rs)에서 caller의 정확1MiB를 빌린다. Header와6개 section의 길이·count·entry·digest, 사용하지 않은 영역의0, 전체 trailer SHA를 검산한다. full-file SHA는 trailer를 포함하므로 내부 trailer SHA와 구별한다.

| section | offset | bytes | 구성 |
| --- | ---: | ---: | --- |
| descriptor |4096|55296|36×1536, decoded descriptor 배열을 만들지 않음 |
| allocator |65536|16384|ordinal0, 원 allocation codec |
| Root/Jobs |81920|98304|Root1·Job2..6의6page |
| Status |180224|22528|64×352, 실제 Status9..63 |
| certificate |202752|6144|64×96, exact canonical page full SHA |
| ownership |208896|16384|embedded XOI1 root/chunk와0 padding |

고정 Catalog7/8은 원 genesis 표현과 같아야 한다. metadata0..8의 정체성·kind·owner/generation과 Status9..63의 object2·generation1을 검산한다. Status incarnation은2 이상이다. 최대 ATT32, non-Free Job55, 전체 Job ordinal64를 서로 구별한다. active ATT↔Leased Job↔Occupied Status, Ready Job↔Aborted Status와 각 ref/generation/LSN이 양방향으로 맞아야 한다. Free와 unused record는 정규0 표현을 따른다.

Wire bank의 allocation slot32와 Aging slot34는 이 표현의 고정 위치다. 다른 runtime row 배치는 별도 명시 변환이 필요하다. allocation은 None/Cancelled/InitializedStatus의 닫힌 terminal만 허용한다. 남은 Reserved는 하나이고 capture L은 allocator의 reserve L과 같아야 한다. reserve를 보유한 동안 다른 append를 허용하지 않으며 복구의 cancel은 먼저 수행한다. 이 조건이 새 epoch에서도 birth+1 terminal 인접성을 보존한다.

INIT claim 대조는 원 recipe의 초기 full image를 확인하는 순수 callback이다. symbolic prefix SHA·epoch2 header claim은 실제 WAL/선택된 base/INIT provenance를 증명하지 않는다. 실제 owner는 파일 정체성·CURRENT closure·원 INIT 또는 인증된 predecessor를 추가로 확인해야 한다.

## 제어 레코드

[control codec](../../crates/engine/src/storage/plan4_recovery_control.rs)은 각4096byte, 정규 reserved 영역과 checksum, profile/namespace·세대·LSN·epoch 관계를 검산한다. Current와 previous의 closure가 나타내는 BaseOnly/WalOpen/WalSealed 상태는 wire 계약을 따른다.

- WalOpen의 두 closure는 물리 base slot만 다를 수 있다. continuation epoch·offset/LSN·header/WRITER 결합이 달라지는 fallback을 거절한다.
- WalSealed는 두 closure가 같고 intent가 없어야 한다. sealed prefix 경계 뒤의 bytes를 채택하는 권한을 부여하지 않는다.
- BaseOnly의 intent는 issued epoch, base generation/LSN/digest, 다음 active generation과 맞아야 한다. intent 없이 남는 필드는0이다.
- XWE2는 이전 source와 선택 base를 결합하고, XWS2는 CURRENT generation을 가리킨다. CURRENT는 XWS2 full SHA를 가져 hash 순환을 피한다.

실제 intent 영속화→create→header/WRITER 공개→readback, old source 회수, unknown 세 번째 WAL의 HOLD, crash fallback은 후속 owner 의무다. 이 순수 codec은 namespace I/O를 수행하지 않는다.

## 공통 검산의 재사용

`ownership.rs`는 고정 slice/borrowed 두 policy로 기존 verifier body를 공유한다. borrowed callback의 owner 범위를 먼저 확인하고 ordinal별 descriptor를 빌리며36개 wide descriptor를 복사하지 않는다. 기존 slice 함수의 인자·호출 계층·실행 body는 유지한다. 호출자의 callback과 동시 생존 값은 별도 계상한다.

`allocation_operation.rs`의 base bridge는 plan4 전체 population을 검사한 뒤 원 reserve/finished 검산을 사용한다. `plan4_genesis.rs`의 Catalog bridge도 원 정규 생산자를 사용한다. 실제 live reserve/INIT/cancel 및 새로운 owner는 이 bridge 단위의 완료 범위가 아니다.

## 자원과 시험 경계

원 static 값은 base64,564bytes(공통 bridge 포함), control14,660bytes다. 이 값만으로 caller 포함 한도나 live F05를 선언하지 않는다. [독립 phase 검토](../../artifacts/dev3/plan4-g6-recovery-20261024/review/pure-call-phase-admission-r0.md)는 실제 순수 호출의 deep/return 단계와 caller를 합쳐 base63,204·control21,620bytes로 계산했다. 각65,536bytes 이하이며 machine stack/RSS 측정은 아니다.

BaseView/Result는 깊은 inventory 검산이 끝난 뒤 생성한다. encode는 decode를 tail 호출한다. 따라서 deep phase의64,564에서 아직 없는 반환 carrier5,440을 제외한59,124에 실제 caller 상계4,080을 더한다. 반환 phase는 종료된 최대 bridge24,212를 제외한40,352에 같은 caller를 더한다. 다른 live/recovery caller에는 이 수치를 자동 전용하지 않는다.

caller backing은 base input1MiB·encode output1MiB·scratch4page64KiB, control input/output 각각4KiB다. fixture의 Vec와 보존 raw는 시험 host backing이며 제품의 고정 공간과 구별한다. 정상/거절 byte 검산은 page·WAL·DWB·pool 또는 namespace 효과가 없다.

중간 필수272는 base128와 control144, S5 추가23,886은 base256와 control23,630이다. S5에는 필수272도 포함한다. 완료 사례를 반복하지 않으며 원 source/ELF·실패·HOLD 기록을 보존한다. 실제 durable recovery·같은 pool의 live 재진입·3PID·Catalog CREATE/CLR·첫 행 ACK·전체 DEV3 인수는 별도다.
