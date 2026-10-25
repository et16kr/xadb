# DEV1 — 공통 의미와 Linux effect 기반

**이관 상태:** 아래의 API·동작·제약 설명을 현재 코드와 함께 유지한다. 이전 실행 횟수·검토 기록은 새 XADB qualification이 아니다. 새 실행 여부는 [구현 상태](implementation-status.json)와 [증거 색인](../evidence/README.md)을 따른다.


2026-10-18 · **XADB-005/026 최소 구현 / 로컬 실행 결과는 별도 JSON / DB 엔진 미구현**

[개발 단계](../plans/development-stages.md) · [구현 계약](../contracts/implementation-contracts.md) · [기계 판독 계약](../contracts/dev1-contract.json) · [실행 결과](../evidence/README.md)


<a id="section-1"></a>

## 1. 범위와 모듈 경계

DEV1은 DEV2가 실제로 사용할 공통 계약의 작은 첫 구현이다. `crates/foundation`은 공통 의미와 effect 계약을 소유하며 다른 workspace crate를 import하지 않는다. normal 외부 dependency는 SHA-256이다. `crates/platform`이 foundation 계약을 구현하고 Linux 파일 I/O와 실제 clock/scheduler를 제공한다. safe nix fcntl wrapper는 platform의 Linux 전용 dependency다. Rust 표준 자료구조·동기화 기능을 사용하며 모든 workspace Rust 코드는 `unsafe_code = "forbid"`다. 현재 지원·실제 시험은 Linux뿐이며 타 OS 추가 구조는 [플랫폼 분리 정책](../policies/repository-build-plan.md)을 따른다.

`F1_LOCAL_VERIFIED`는 아래의 유한한 범위를 실제 로컬에서 검증했다는 뜻이다. DB의 row/transaction/WAL/recovery/checkpoint, 제품 파일시스템 지원, 보호 CI, 공식 배포가 완료되었다는 뜻이 아니다. 사내 배포는 매번 새 DB·fixture이며 이전 사내 버전의 codec/API 호환·변환은 요구하지 않는다.

| 작업 | 실제 구현 | 주요 검증 |
|---|---|---|
| XADB-026 의미 계약 | typed ID·generation·schema epoch, 오류 의미, 최소 값/비교, index key·frame codec | 0/overflow/stale 거절, 독립 경계 vector, 모든 frame truncation·single-bit 손상 |
| XADB-005 | 파일 effect port·실제 Linux adapter·fault adapter, clock/scheduler port·typed event | 실제 write/sync/reopen, partial write 후 error·sync 불명, 결정적 schedule 경계 |
| XADB-026 자원·조립 | 독립 crate, quota·취소·start/drain/stop, production API worker | 독립 build·dependency closure, 32개 thread 자원 경쟁, controller 독립 기대값 |

이전 기록에서 ID·값·codec 시험을 XADB-004로 표기한 것은 작업 귀속 오류였다. 해당 구현·시험은 XADB-026의 공통 의미 증거다. **XADB-004는 DEV2의 독립 row/transaction reference**다. 현재 P0 일부가 구현되었으며 known-bad history를 거절한다. 범위와 한계는 [DEV2 구현](dev2-implementation.md)을 따른다. 기존 실행 JSON·source hash·시험 결과는 변경하지 않는다.

<a id="section-2"></a>

## 2. 식별자와 오류

`ObjectId`, `RowId`, `Generation`, `SchemaEpoch`, `CommitSeq`는 서로 다른 u64 newtype이며 0을 거절한다. `next()`는 overflow에서 오류를 반환하고 wrap하지 않는다. `ObjectHandle`의 object/generation/schema_epoch가 현재 객체와 하나라도 다르면 stale 오류다. 객체 번호의 발급·전역 유일성·persist/reopen, snapshot pin과 row 생명주기는 DEV2의 책임이다. 숫자 handle을 포인터로 해석하지 않는다.

오류는 안정적인 `XADB-FOUND-0001`–`0014` code, 정적 context, 선택적인 OS `ErrorKind`를 담는다. 현재의 typed ID는 내부 개발 계약이며 고객 공개 오류 catalog는 후속 SQL/server/client에서 확정한다. 인터페이스 기획의 영향·재시도·확실성 분리를 따른다.

| 오류 | effect / certainty | retry |
|---|---|---|
| validation·type·overflow·quota·입장 전 취소 | Unchanged / Known | Never; 입력·상태를 바꾼 새 시도는 caller 판단 |
| read 오류 | 영속 상태 Unchanged / Known | Operation; buffer에는 prefix가 남을 수 있으므로 폐기 |
| write·resize·sync 오류, 내부 lock poison | CallerMustResolve / Unknown | AfterRecovery; 자동 재전송·rollback 확정 금지 |

foundation은 transaction을 소유하지 않아 statement/transaction rollback을 수행했다고 주장하지 않는다. DEV2가 오류 위치와 commit 상태를 결합해 transaction effect를 추가한다. 취소 token은 caller가 효과를 내기 전에 확인하며 이미 발생한 write를 되돌리지 않는다.

<a id="section-3"></a>

## 3. 값·key·frame의 구체값

최소 타입은 NULL, bool, signed i64, UTF-8 문자열, binary다. 문자열은 정규화 없는 UTF-8 byte 비교이며 locale collation·숫자 암묵 변환·decimal·float·날짜는 아직 없다. 가변 길이 값은 UTF-8 byte 기준 64 KiB까지다. i64 덧셈은 overflow 오류이며 wrap/saturate하지 않는다.

`Value::compare`는 NULL이 포함되면 UNKNOWN(`None`), 서로 다른 non-NULL 타입이면 type 오류다. index의 전체 순서는 NULL < bool < i64 < UTF-8 < binary이며 SQL NULL 비교와 구분한다. unique index의 NULL 정책·복합 key·내림차순은 이후 index 계약이다.

key는 타입 tag 0–4, bool 0/1, i64 sign bit 반전 후 big endian 8 bytes로 구성한다. UTF-8/binary의 0 byte는 `00 FF`, 끝은 `00 00`으로 인코딩하여 prefix·embedded zero를 구별한다. 동일한 값은 동일한 key를 만들고 byte 순서가 명시한 전체 값 순서와 일치한다. key는 정렬용이며 원래 typed value의 저장 codec을 대신하지 않는다. row/value 저장 형식은 DEV2에서 정의한다.

frame은 `AXF1` + format u16 LE(1) + payload 길이 u32 LE + payload + 앞부분 전체의 SHA-256 32 bytes다. payload는 최대 1 MiB, trailing bytes·손상·잘린 입력·지원하지 않는 format은 명시적으로 거절한다. decode는 할당 없이 빌린 payload를 반환한다. Rust struct layout을 저장하지 않는다. 이 범용 frame은 **WAL/commit record 형식이 아니며** transaction ID·chain·commit 경계는 DEV2가 추가한다. checksum은 인증·악의적 변경 방지 장치가 아니다.

<a id="section-4"></a>

## 4. 파일·fault·시간 경계

`foundation::io::FileIo`는 positional read/write, sync_all, set_len을 제공한다. 공통 read/write 재시도·범위·오류 처리는 foundation에 유지한다. `platform::linux::LinuxFile`은 실제 std File을 사용한다. Linux의 O_APPEND는 positional write의 offset을 무시하므로 매 write 전에 fcntl로 검사하여 거절한다. caller는 공유 handle의 file status flag를 동시에 변경해서는 안 된다. `write_all_at`은 short write를 남은 offset부터 이어 쓰며 0 write는 오류다. read/write는 Interrupted를 재시도한다. signed 64-bit offset 범위를 첫 effect 전에 검사한다. read 실패 시 caller buffer 전체를 유효하다고 사용하지 않는다.

파일 소유 경로·create_new·writer 배제·rename/publication 순서는 caller 책임이다. `platform::linux::sync_directory`는 디렉터리 handle을 확인한 뒤 sync한다. 생성·잠금·rename/publication을 엔진에 제공할 공통 filesystem port는 DEV2에서 확정하며 이번 분리가 그 계약의 완료는 아니다. 실제 시험은 private temp directory에서 create_new → frame write → file sync → rename → directory sync → reopen을 수행한다. 기존 사용자 경로를 삭제하거나 덮어쓰지 않는다. API 성공·process reopen은 power loss 내구성 증명이 아니다.

`test-support` feature에서만 fault adapter와 controlled clock/scheduler를 공개한다. 1부터 시작하는 특정 FileIo 호출에 short write, 실제 prefix write 뒤 error, 실제 sync 뒤 error, read error, zero write를 주입한다. 다른 operation에 지정하면 명시적 오류이며 모든 다른 호출은 실제 adapter에 위임한다. receipt는 최대 256개이고 초과 건수를 기록한다. scheduled는 주입 계획, injected는 실제 적용을 구분한다. 실제 sync 실패를 sync 성공 뒤의 주입으로 기록하지 않는다. 시나리오 끝의 `FaultFile::finish()`는 미도달·잘못된 operation·효과 없는 short write·선행 I/O 실패·receipt 유실을 거절한다. 시험은 이 완료 검사, receipt, 실제 파일 내용을 함께 확인한다.

`Clock`/`Scheduler` 계약과 controlled 구현은 foundation에 있고, `platform::runtime`의 native clock은 Instant 기반 단조 증가 nanos, native scheduler checkpoint는 thread yield다. manual clock은 overflow를 거절하며 scripted scheduler는 명시한 actor/point 순서와 미소비 step을 검사한다. 한 번 잘못된 checkpoint를 관측하면 그 실패를 유지하여 이후 올바른 호출로 finish 성공을 만들 수 없다. 이 scheduler는 driver가 주도한 순서를 확인하는 최소 port로, 일반 thread를 강제로 제어하거나 모든 interleaving을 탐색하지 않는다. blocking syscall을 취소하는 runtime도 아직 없다.

<a id="section-5"></a>

## 5. 자원·수명·진단

quota는 AtomicU64의 CAS로 동시 예약을 제한한다. non-clone Reservation을 자원 수명 동안 유지하고 Drop에서 반환한다. overflow·초과는 상태 변경 없이 실패한다. 32개 실제 thread가 100 단위 quota에 각각 10을 예약할 때 최대 10개만 성공하는 사례를 시험한다. 이는 명시적으로 회계한 자원만의 budget이며 프로세스 전체 OOM 복구 보장이 아니다. Arc와 기존 Value 생성 등의 숨은 allocation은 남아 있다.

lifecycle은 Running → Draining → Stopped다. stop과 admit은 같은 mutex로 선형화하며 stop 이후 새 입장을 거절한다. 기존 OperationGuard가 모두 해제되면 Stopped다. restart는 새 lifecycle을 생성한다. poison 후 정상 서비스를 재개하지 않는다. Drop은 accounting만 처리한다.

typed event는 operation ID, 선택적 object handle, monotonic time, event ID, 선택적 error code를 담는다. EventBuffer는 1–4096개로 제한하며 가득 차면 새 event를 버리고 dropped를 증가시킨다. emit은 미리 예약된 공간을 사용한다. 이벤트 출력·집계·운영 message catalog는 후속 인터페이스 구현에 연결한다. 이벤트는 독립 correctness oracle이 아니다.

<a id="section-6"></a>

## 6. 실제 검증과 XADBTest 연결

`crates/foundation/tests/contracts.rs`의 16개와 `crates/platform/tests/contracts.rs`의 9개, 합계 25개 시험은 production API를 직접 호출한다. 기존 시험의 이름·입력·판정은 보존하고 실제 Linux I/O·clock과 합성하는 시험의 소유 위치를 platform으로 옮겼다. i64 oracle은 i128 계산 후 범위 변환을 사용하고, key는 손계산 hex·명시적 순서 목록, frame은 독립 Python hashlib로 계산한 고정 vector와 비교한다. 모든 truncation과 byte별 single-bit 손상, 1 MiB 한계, 실제 파일·fault·quota·수명·취소·진단을 검사한다. default feature에서는 fault 지원을 제외한 foundation 13개와 platform 4개, 합계 17개가 실행된다.

`xadbtest-foundation-worker`는 기존 fixture와 별도 executable이며 실제 `foundation::value::checked_add`를 호출한다. controller는 worker에 기대값을 전달하지 않고 `tests/cases/dev1-foundation.json`의 7개 literal 기대값과 비교한다. 기대값 하나를 43으로 고의 변경한 실행은 FAIL/exit 1이어야 검증 성공이다. XADBTest process protocol로 연결한 DEV1 연산은 현재 i64 덧셈뿐이다. 다른 foundation 계약은 Rust 통합 시험으로 검증하며 모든 연산이 process adapter에 연결되었다고 표현하지 않는다.

```bash
python3 scripts/check_dev1.py
```


```bash
evidence_run=$(date -u +%Y%m%dT%H%M%SZ)
python3 scripts/check_dev1.py \
  --report "devdoc/evidence/dev1-${evidence_run}.json" \
  --dev0-report "devdoc/evidence/dev0-${evidence_run}.json"
```


[DEV0·DEV1 리뷰](../evidence/README.md)는 수정 전 실제 실패 로그, 발견한 문제와 수정, 현재 검증 범위를 기록한다.

<a id="section-7"></a>

## 7. 다음 단계

다음은 DEV2의 XADB-004 독립 정답 모델과 K0의 작은 실제 row/catalog 저장소다. typed ID·값·오류·quota를 연결하고 snapshot pin과 객체 발급을 검증한다. 이후 K1 private batch, K2 WAL, K3 recovery, K4 동시성, K5 checkpoint/restore를 실제 경로로 연결한다. 다른 세션에서 작성한 recovery·인터페이스 설계의 변경은 해당 구현 시 구체 계약으로 대조한다. DEV1은 그 전체 제품 설계를 대신 승인하지 않는다.

## 현재 방침 적용과 다중 OS 경계

현재 구현에서 공통·Linux API의 효과·오류·수명·동시성 계약을 Rustdoc에 연결했고 두 제품 crate는 공개 설명 누락을 컴파일 오류로 거절한다. 기존 25개 계약 시험과 default 17개, 독립 build·정상 의존 경계 검사를 유지한다.

DEV1의 다중 OS 확장 구조는 구현했다. `xadb-foundation`의 `FileIo`·`Clock`·`Scheduler`를 OS 구현이 제공하고 상위 조립 지점이 선택한다. 실제 adapter·지원·실행 검증은 Linux만 완료했다. 다른 OS의 빈 stub이나 지원 완료 표시는 없다. 파일 생성/open·writer 배제·rename/publication 공통 port는 DEV2, transport·대기/취소는 COMM의 실제 사용 계약과 함께 확장한다.
