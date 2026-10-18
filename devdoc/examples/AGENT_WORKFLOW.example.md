# XADB agent workflow — 제안 템플릿

아래 운영 사실은 XADB의 profile·경계 인수 시험을 통과한 뒤에만 실제 repository 지침에 반영한다.

<a id="section-1"></a>

## 작업 범위

- 목적은 XADB의 기능 구현, correctness·내구성 검증, 원인 분석과 성능 개선이다.
- 주 언어는 사용자 확정 사항인 Rust다. safe Rust core와 제한된 unsafe/FFI 경계를 유지한다. 상세 계약은 [implementation-contracts.md](../contracts/implementation-contracts.md)에 있다.
- 작업 계약은 exact revision, contract ID, synthetic fixture, registered case/recipe, allowed effects와 완료 기준을 제공한다.
- 정상 local suite는 검증된 disposable worker와 합성 데이터를 사용한다. production DB, 실제 credential, 공유 process, host 보안 설정은 접근하지 않는다.
- 기존에 허가된 이 범위 안에서는 구현·영향 시험·수정·재검증을 진행한다. 매 routine 단계마다 새 허가를 요청할 필요는 없다. 플랫폼과 관리 정책의 추가 요구는 그대로 적용한다.

<a id="section-2"></a>

## 실행

1. M1 모듈 → M2 component → 필요한 M3 system 순으로 반례를 좁힌다. 변경 계약에 필요한 실제 환경 시험은 생략하지 않는다. 실행 profile의 doctor 결과와 resource ownership을 확인한다. 불일치하면 broad access로 대체하지 않고 fixture·환경 계약을 수정한다.
2. DB에서 발견한 오류도 catalog·causal context로 module capsule을 먼저 만든다. 오류가 출력된 파일을 원인으로 단정하지 않고 같은 불변식·witness의 재현을 확인한다. registered case를 실행한다. crash·overlay 변경은 recipe에 명시하고 해당 worker 소유 자원에 한정한다.
3. internal fault·typed oracle·실제 production adapter 검증을 구분한다. simulation만으로 실제 정전 검증을 대체하지 않는다.
4. unit/SQL/복구 등 필요한 검증을 완료한다. 실행하지 못한 항목을 PASS로 바꾸지 않는다.
5. B0/F1 기반 → K0–K5/B2 커널 → S1 저장 → Q1 SQL → B3 server 순서와 COMM/PART/NG 독립 경로를 따른다. 하위 core는 SQL/network/server 없이 build/test 가능해야 한다. 아직 없는 server에 의존해 초기 작업의 완료 조건을 순환시키지 않는다. 미검증 통합 의무를 기록하고 해당 기능을 제공하는 release 전에 필수 gate를 모두 충족한다. 초기 사내 버전에는 D0 확장 계약을 적용하고 DISK H1은 DEV3에서, PART/NG는 저장 기준선 뒤 별도로 구현한다. 작은 실제 row를 포함한 transaction/WAL/recovery를 함께 키우며 mock-only PASS를 금지한다. 사내 배포는 매번 새 DB·fixture로 구축하며 버전 간 호환을 요구하지 않는다. same-major 호환은 안정 릴리스 버전 간에 적용한다. 개발 소스 공개와 안정 제품 출시를 구분하며, 개발 우선순위와 capability 범위는 `development-stages.json`을 따른다. Linux·DISK·native global index 범위는 `phase1-scope.json`을 따른다. 디스크 경로·global unique·resolver 시험을 메모리 모델이나 local index fanout으로 대체하지 않는다.

<a id="section-3"></a>

## 원인 범위 한정

- 오류를 감지한 module·source와 실제 원인 후보를 구분한다. data/object version에서 producer·writer·owner·control·동기화·영속 의존을 역추적한다.
- 보수적 후보와 우선 조사 후보를 나누고, 후보 제외에는 같은 계약의 경계 증거를 연결한다. trace 누락·미확인 writer·누락된 의무를 숨기지 않는다.
- 작은 실제 core 시험으로 가설을 구분한다. 구분 실험·fix 전후·필수 회귀가 없으면 원인 검증 완료로 보고하지 않는다.
- 새 기능에서 입력/출력·ownership·흐름 manifest·오류 catalog·재현 entry와 진단 인수 사례를 함께 갱신한다.

<a id="section-4"></a>

## Astra / Luna

- 초기 제안: Astra는 설계·복잡한 원인 분석·최종 검토, Luna는 좁은 재현·반복 시험·증거 추출.
- 테스트 FAIL은 유효한 반례일 수 있다. 실패한 agent라고 간주해 모델을 무작정 교체하지 않는다.
- 확정된 일반 기술 오류나 분석 품질 문제의 재배정은 같은 scope·oracle에서 bounded하게 수행한다.
- safety 중단·policy 거절·원인 불명 종료는 자동 모델 전환 조건이 아니다. 실제 사유와 허용 범위를 확인한다. 다른 model/chat/tool을 우회 경로로 사용하지 않는다.

<a id="section-5"></a>

## 자료와 결과

- runner가 필요한 최소 artifact를 기록한다. 환경 전체·실제 비밀·고객 row 전체를 수집하지 않는다.
- agent는 승인된 typed evidence view를 사용한다. 원본 artifact와 view의 접근 권한을 OS나 서비스로 실제 분리한다.
- 자식은 raw dump 대신 상태, failure class, product verdict, 검증 범위, witness/oracle 요약, disclosure 검사 결과, evidence ID, 미해결 질문을 전달한다.
- 최종 보고서도 검증된 view에서 작성한다. raw를 파일로 옮겼다는 이유로 공개·접근 허가가 생기지 않는다.
- 범위·effect·거절 사실은 정확히 표시한다. 필요한 audit와 안전 심사를 숨기지 않는다.
- context와 evidence의 source/build/case/oracle hash가 현재 작업과 일치하는지 확인한다. candidate 출력으로 expected를 자동 승격하지 않는다. 테스트 수정은 가능하지만 독립 근거와 보호된 판정 경계를 유지한다.
- case validity, product verdict, execution, cleanup, workflow를 따로 기록한다. cleanup 실패나 재시도 성공으로 앞서 관측한 제품 실패를 지우지 않는다.

<a id="section-6"></a>

## 완료

source/build/case/recipe 지문, 필수 witness·oracle, 자원 정리, 정보 공개 기준을 확인한다.
구현·제품 판정·에이전트 상태를 구분하고 남은 제한을 보고한다.
반례·국소 수정·독립 검증·재진입 기록과 AI pilot 절차는 [ai-development-method.md](../policies/ai-development-method.md)를 따른다.

참고: [OpenAI의 Astra 작업 지침](https://learn.chatgpt.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra).
이 템플릿은 OpenAI의 인증이나 무중단 보증이 아니다.
