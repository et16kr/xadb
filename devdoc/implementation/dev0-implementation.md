# DEV0 — Rust 빌드와 XADBTest 최소 구현

**이관 상태:** 아래의 API·동작·제약 설명을 현재 코드와 함께 유지한다. 이전 실행 횟수·검토 기록은 새 XADB qualification이 아니다. 새 실행 여부는 [구현 상태](implementation-status.json)와 [증거 색인](../evidence/README.md)을 따른다.


2026-10-18 · **XADB-002/XADB-003 로컬 구현·시험 완료 / 엔진 미구현 / 공식 배포 아님**

[개발 단계](../plans/development-stages.md) · [실행 결과 JSON](../evidence/README.md) · [기계 판독 계약](../contracts/dev0-contract.json)

<a id="section-1"></a>

## 1. 구현 범위와 현재 상태

DEV0의 유한한 범위는 Rust workspace, 독립 XADBTest controller/worker, 명시적 oracle, 구조화 판정, 실행 증거, 소유 프로세스·디렉터리 회수다. `B0_LOCAL_VERIFIED`는 이 범위의 로컬 실행 결과이며 보호 CI의 승격이나 제품 출시 판정이 아니다. DEV1 공통 계약을 구현할 기반을 마련했다.


| 작업 | 구현 | 검증 |
|---|---|---|
| XADB-002 | Rust 1.95.0·edition 2024·lockfile, safe Rust, source/build identity | 고정 dependency 빌드, 경고 없는 clippy, 기존 target·Git이 없는 소스 복사본 빌드 |
| XADB-003 | schema, controller, 독립 기대값 oracle, fixture worker, verdict, receipt, cleanup | 14개 controller 통합·6개 process unit 시험, 기본 worker smoke 7개 assertion, 소스 없는 빈 디렉터리 실행 |

현재 로컬 환경은 Ubuntu 26.04.1 LTS, x86_64 Linux다. CI workflow는 Ubuntu 24.04 runner에서 동일 entry point를 실행하도록 추가했지만 원격 CI는 아직 실행하지 않았다. 호스트/runner 이미지 전체의 digest pin, 지원 filesystem qualification, 보호 corpus와 release 승격은 후속 의무다. Rust toolchain과 dependency checksum은 고정되어 있다.

<a id="section-2"></a>

## 2. 코드 경계와 실행

| 위치 | 책임 |
|---|---|
| `crates/xadbtest-protocol` | versioned typed operation/request/observation. engine·oracle 의존 없음 |
| `crates/xadbtest` | case 검증, worker identity, 실행·회수, 독립 기대값 비교, 증거·판정. `lib.rs`는 공개 조립, `controller.rs`는 실행·증거 소유 |
| `crates/xadbtest-fixture-worker` | signed INT64 checked-add와 overflow 관측을 반환하는 프레임워크 시험 fixture |
| `tests/cases/dev0-smoke.json` | 손으로 계산한 7개 기대값, 필수 witness, 실행 상한 |
| `scripts/check_dev0.py` | 실제 검사 실행·로그 보존·clean build/runtime·artifact hash 확인 |


저장소 루트에서 실행한다. 최초 실행에는 고정 toolchain 및 Cargo dependency 다운로드가 필요하다.

```bash
python3 scripts/check_dev0.py
```

이 명령은 Python evidence identity·배타적 보고서 출력 시험 10개, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --all-features --locked`, 기본 feature 빌드, fixture smoke, 깨끗한 실행 환경과 소스 복사본 빌드를 수행한다. `--all-features`는 고의 결함 worker와 cleanup 실패 주입을 포함하는 **self-test 전용** feature를 활성화한다. 기본 CLI에는 mutation·shell 옵션이 없다.


```bash
evidence_run=$(date -u +%Y%m%dT%H%M%SZ)
python3 scripts/check_dev0.py --report "devdoc/evidence/dev0-${evidence_run}.json"
```

저장 후 [현재 증거 갱신 절차](../evidence/README.md)에 따라 현재 결과 ID와 링크를 갱신한다. 이관한 결과 파일은 덮어쓰지 않는다.


<a id="section-3"></a>

## 3. 입력·oracle·프로토콜 계약

scenario/request/result/manifest/receipt의 현재 schema version은 각각 1이다. 이 형식은 **미배포 개발 형식**이며 공식 XADB/XADBTest 배포 baseline이 아니다. [버전 정책](../policies/version-compatibility-policy.json)에 따라 사내 배포는 매번 새로 구축하며 이전 사내 배포 호환 시험은 `NOT_REQUIRED_INTERNAL_FRESH_BUILD`이다. 안정 릴리스한 버전부터 same-major 호환 기준선이 생긴다.

scenario는 최대 64 KiB, 1–128개 step, step마다 정확히 한 개의 typed assertion을 요구한다. unknown field/operation/type/schema, 중복 ID/assertion, 잘못된 reference, 빈 oracle/witness, 상한 밖 budget을 거절한다. timeout은 50–10,000 ms, stdout/stderr는 각각 1,024–1,048,576 bytes로 제한한다. 기본 smoke는 2,000 ms와 stream별 65,536 bytes다.

worker에는 run ID, case hash, typed operation만 보낸다. 기대값과 oracle은 controller에 남고 worker 출력으로 갱신하지 않는다. worker는 newline으로 끝나는 JSON observation을 step별로 정확한 순서에 맞춰 출력해야 한다. controller는 version·run/case identity·sequence·step ID·witness와 typed 값을 검증한다. plain `PASS`나 worker가 제출한 verdict는 잘못된 observation이다. required witness는 경로 관측이며 값의 정확성을 대신하지 않는다.

case/binary 기대 지문을 실행 전에 확인한다. worker는 private 디렉터리에 복사한 뒤 그 복사본을 해시하고 실행한다. `scripts/check_dev0.py`·`scripts/check_dev1.py`의 단계 검사 PASS는 실행 전·후 실제 source fingerprint와 controller의 build fingerprint가 모두 일치해야 한다. 단독 `xadbtest run`의 case PASS는 보존 case/worker bytes와 관측을 검증하며 현재 checkout을 읽거나 최신 소스와의 동등성을 검사하지 않는다. 소스 없는 재현 실행도 이 구분을 따른다. smoke도 별도로 보존한 binary·case로 실행한다. source identity는 정렬한 경로·길이·내용으로 계산한 SHA-256이며 Cargo manifest/lockfile/toolchain, crates, tests, scripts, CI 파일을 포함한다. 문서와 생성 artifact는 별도 검증 대상이다. controller/worker binary, case, oracle와 oracle 구현에도 지문을 남긴다. 동일 source가 서로 다른 경로의 debug binary를 byte-identical하게 만든다는 보장은 없다.

<a id="section-4"></a>

## 4. 판정과 소유 자원

| 축 | 현재 값과 의미 |
|---|---|
| `case_validity` | VALID / INVALID / UNKNOWN: identity와 관측 경로의 유효성 |
| `product_verdict` | PASS / FAIL / UNKNOWN / NOT_EVALUATED: DEV0에서는 fixture 계약만 판단 |
| `execution_state` | PREPARED / RUNNING / COMPLETED / ERROR / TIMEOUT |
| `cleanup_state` | PASS / FAIL / UNKNOWN: process wait와 private directory 회수 |
| `case_verdict` | PASS=0, FAIL=1, INVALID=2, ERROR=3, TIMEOUT=4의 CLI exit code와 일치 |

관측한 typed mismatch는 후속 timeout·프로토콜 오류·cleanup 실패로 지우지 않는다. 전체 실행이 정상 종료했더라도 cleanup 실패는 성공 판정을 막는다. 올바른 값 뒤 nonzero worker exit는 product PASS와 execution ERROR를 각각 남기며 최종 ERROR다. 미완료·잘못된 관측을 성공으로 승격하지 않는다. case/worker 파일의 읽기·복사 실패는 잘못된 case와 구분하여 execution ERROR/exit 3이다. verdict를 저장할 수 없는 harness 오류도 exit 3이며 완료 증거가 아니다.

controller는 새 임시 work directory, 실제 `Child` handle, 새 process group만 소유한다. worker 환경은 비우고 stdin은 크기가 제한된 request 파일로 전달한다. stdout/stderr를 nonblocking으로 수집하고 output budget·deadline을 검사한다. worker 종료는 Linux `waitid(WNOWAIT)`로 관측하며 leader를 reap하기 **전에** 소유 group에 종료 신호를 보낸다. leader PID의 재사용 이후 신호를 보내지 않는다. leader 종료 후에도 stdout/stderr 양쪽 EOF까지 수집하며 deadline이 지나면 TIMEOUT이다. 원래 group이 사라졌으면 직접 소유한 Child도 종료한 후 기다린다. 마지막에는 `wait`와 work directory 삭제를 수행한다. 이름 검색으로 다른 프로세스를 종료하지 않는다.

worker는 독립 프로세스지만 **동일 OS 사용자에 대한 보안 sandbox는 아니다**. 적대적인 worker의 임의 host 접근·다른 session으로 이탈한 descendants를 봉쇄하지 않는다. 현재 고정된 fixture는 다른 프로세스를 생성하지 않는다. worker가 스스로 출력한 정리 성공을 믿지 않고 controller의 실제 handle/삭제 결과로 판단한다. timeout은 협력적인 OS에서의 실행 상한이며 uninterruptible kernel I/O에 대한 강제 종료 시간 보장은 아니다.

각 run에는 `manifest.json`, `case.json`, `request.json`, `observations.jsonl`, `stderr.txt`, `receipts.jsonl`, `verdict.json`, `artifacts.json` 중 도달한 단계의 파일이 남는다. preflight 실패는 worker 출력 파일이 없다. receipt는 실행·정리 전 intent와 관측 결과를 순서대로 sync한다. controller 자체의 갑작스러운 종료 뒤 자동 orphan reconciliation은 DEV0 범위 밖이다. 완료 verdict/index 없는 bundle은 미완료이며 PASS가 아니다. 이 로컬 bundle은 서명이나 불변 저장소가 아니다.

<a id="section-5"></a>

## 5. 실제 인수 결과와 남은 의무

[실행 결과](../evidence/README.md)는 실제 명령 exit code, 로그·binary·case·source 지문, 기본 worker 판정, 보존한 bundle 수를 기록한다. negative self-test의 FAIL/INVALID/ERROR/TIMEOUT은 예상한 반증 결과이며 전체 검증 실패와 혼동하지 않는다.

14개 통합 시험은 입력 파일 부재의 실행 오류 분류를 포함하여 정상 기대값, 잘못된 값, 위조/잘못된 observation 13종, binary/case/oracle 변조, schema 오류 13종과 oversize/duplicate field, timeout 회수, stdout/stderr 폭주, nonzero exit, cleanup 실패 뒤 판정 보존, 실패 후 timeout, 동시 실행·재시도 증거 보존, spawn 실패, CLI exit code를 다룬다. cleanup 실패는 self-test feature의 명시적 주입이며 OS 권한을 변경해 인위적으로 leak를 만들지 않는다.

기존 PA01/PA13은 보호 controller와 trusted promotion까지 포함한 제안이다. 이번 실행은 그 로컬 판정·negative-test 부분의 증거로 연결하며 원래 전체 인수 계획을 PASS로 바꾸지 않는다. GitHub workflow 작성은 원격 CI 실행·branch protection 설정의 증거가 아니다.

남은 범위는 보호 CI/baseline 승격, controller crash reconciliation, 완전한 replay/minimization, scheduler·fault adapter, filesystem qualification, 실제 엔진·복구·SQL·안정 릴리스 버전 간 호환 시험이다. 이를 DEV0 fixture PASS로 승계하지 않는다. DEV1의 최소 공통 경계는 별도 구현·시험했고 다음은 DEV2 실제 row/catalog 커널이다.

## 현재 방침 적용

[완료 기록](README.md)에 따라 Drop 정리 경로는 allocation 없는 typed cleanup 오류를 사용한다. 문자열 진단은 명시 종료 결과를 controller가 처리할 때 생성한다. Drop은 실패를 PASS로 바꾸지 않으며 정상 판정에는 명시적인 종료·증거 확인이 계속 필요하다. 신규 Drop/finish 3개 사례는 각각 새 child로 3회 실행한다.
