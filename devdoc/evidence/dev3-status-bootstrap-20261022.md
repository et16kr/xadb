# DEV3 불변 Status bootstrap 선택 인수

2026-10-22 · **필수 선택 PASS · 최종 독립 D1 수용 · 전체 DEV3 미완료**

[소유 계약](../contracts/dev3-status-bootstrap.md) · [기계 판독 실행 색인](dev3-status-bootstrap-20261022.json) · [구현 현황](../implementation/dev3-implementation.md#status-bootstrap)

plan2를 전체 설치/readback한 세션에서 ObjectId2/gen1/kind9/level0의 immutable role을 빌리도록 구현했다. plan1과 plan2를 expected plan으로 구별하며 old allocator adapter는 plan2를 효과 전에 거절한다. 이 결과는 Status payload INIT나 현재 membership·거래 directory·recovery/READY를 공급하지 않는다.

## 선택 범위와 실제 실행

기준 commit은 `09688384da7e4f2f34b9f3f2938c1f9c07a072b3`이다. 새 역할·plan decoder·old adapter guard·typed resource preflight와 그 오류 projection이 실제 변경되었다. 이 변경의 새32case와 기존 변경 decoder의 보존 corpus1만 선택했고, 정상 full creator는 native P1 한 DB에서 실행했다. 변경 없는 하위 suite와 전체 wrapper는 실행하지 않았다.

제품 source는 `cf219808ec8357b3f35ec4ac3646825ea9b2563e0fcbf6faa210f7cd2780a768`, ELF는 `7e9c5a1f98f8f796887047966b598dbfa50bde239de5855ae2b4ab3b3243cbc7`이다. 별도 export·no-run build·binary·fixture·raw stdout/stderr와 명령은 JSON에 원 identity로 보존한다. 문서/등록 변경은 이 source의 실행 증거로 섞지 않는다.

| 범위 | 결과와 분모 |
| --- | --- |
| 새 runtime |SB01–05의5 Rust leaf/32 case PASS. resource named variant2와 forwarding variant3 별도 기록 |
| 변경 parser |기존 `bootstrap_descriptor_rejects_damage` selector1 보존 corpus PASS |
| 실제 파일 |native P1 profile1 DB1 PASS.350 mutation effects,9파일,5,296,128 bytes를 독립 전체 bytes/zero padding과 대조 |
| 수명 compiler |NC00/NC01/NC02 semantic3 및 infrastructure baseline1 PASS. 실제 호출4, 합계6.268245초. NC01 E0451 primary5, NC02 E0505 primary7 |
| 새 checker |compiler checker4 + runtime/native checker3 PASS. resource variant 검출을 수정한 기존1method만 추가 선택 재실행; unique8로 집계하지 않음 |
| 정적 검사 |같은 product source의 fmt 및 engine all-targets/all-features Clippy PASS. 제품 시험 실행을 포함하지 않음 |
| 최종 확대 |matrix78에서 mandatory alias13을 뺀 추가65 작성/NOT_RUN. 이 subset 최종 새 runtime inventory97 |

native의 wrapped File peak3/final0, 실제 DB FD live 관측3/final0이다. 이는 명시한 관측 경계이며 physical exact FD peak 또는 성능 측정으로 확대하지 않는다. elapsed와 raw 관측은 실행 원본을 따른다. 성공한 role fixture는 private test seed와 actual native creator를 구분한다. SB04는 각 실패 위치의 원 typed cause/phase/effect, cut hit, 후속효과0, partial-file hash와 Drop 이후 handle0을 검산한다. caught panic의 stderr는 지정 unwind/poison 입력의 원 로그이며 지우지 않는다.

## 독립 입력과 자원

독립 literal3은 기존 plan1의 offset120/280/checksum만 struct/hashlib로 수정했다. 효과350 및 partial-file47행은 기존 독립 storage model과 입력 manifest에 연결한다. 제품 encoder를 정답 계산에 사용하지 않았다. S5 matrix78/alias13의 독립 읽기 감사를 보존하며 입력 manifest의 PREPARED_NOT_EXECUTED는 생성 당시 상태 그대로다. 실제 실행 상태는 위 receipt가 소유한다.

실제 값은 kind1,role16,plan176,Fresh560,Mutable848,prepare nested12208,readback nested14788,Fresh role424,Mutable role792 bytes다. 원65536 component·49152 caller scratch 예산을 증액하지 않았다. type 크기와 동시에 살아 있는 source 값의 수량, generic reader padding의 effect 전 거절을 함께 검산했다. source 수명 식의 보수 중복을 compiler stack/RSS 측정으로 표현하지 않는다.

건강한 plan2의 consuming old adapter는 owner를 소비하고 CallerMustResolve를 반환한다. runtime은 scratch/files/I/O 불변과 typed 결과를 관측하며, 소비 뒤 외부에 남지 않는 fence는 독립 source 검토로 확인한다. 이를 runtime fence 직접 관측이라고 주장하지 않는다.

## 원본·검토·남은 작업

원 D2, 입력/소스 D1, compiler raw 감사 및 resource source ledger를 JSON에서 path/bytes/SHA256으로 식별한다. 독립 검토는 채택 설계·실행 입력과 실제 실행 결과를 구분한다. 보존 source가 달라진 새 checker1의 결과를 다른 source에서 새 실행한 것처럼 합치지 않는다. 이 단위의 제품 실행 FAIL은 없으며 이전 단위의 FAIL/NOT_RUN은 원래 증거에 남는다.

다음 작업은 실제 reserve→Status INIT, 닫힌 frame 등록/2-page DPT 및 WAL 보호의 연결이다. batch8/64·다중 file·일반 tree/undo/transaction·recovery/READY·S5 전수와 전체 DEV3는 여전히 미완료다. 문서 verifier는 push gate 전용이므로 이 편집/commit 과정에서 실행하지 않았다.
