# COMM 코드 리뷰와 수정 기록

2026-10-19 · **D1 / 두 결함 수정·관련 검증·독립 검토 PASS / 하위 통합 gate FAIL · C1 INCOMPLETE**

[소유 계약](../contracts/comm-design.md) · [구현 상태](../implementation/comm-implementation.md) · [이전 소스 조사](comm-source-research.md)

## 범위와 변경 소유

사용자가 COMM 코드 리뷰, 발견 문제의 즉시 수정, 다른 DBMS 통신 구현 참조를 요청했다. 담당·통합 담당은 `/root`, DP01 별도 검토자는 `/root/comm_independent_review`다. 기존 계약 안의 취소·오류 격리 수정으로 D1을 적용한다. wire bytes·공개 API·의존성·단계 인수 기준은 바꾸지 않는다.

- 기준 commit: `fe8d3f5d2a7744a1c54a26fa9d30c656b974fa7c`.
- 검토 사본의 초기 source SHA-256: `85ee4ca0af07660d618caf5245633dabaed8c881d8e08943915fe0153020934d`.
- 사본: `artifacts/comm/review-20261019-3r89oy1w/source`; 초기 tracked diff와 실행 root는 같은 디렉터리 상위의 `baseline.json`에 보존한다.
- 제품 소유 경로: `crates/transport/src/driver.rs`, `connection.rs`, `sender.rs`의 아래 결함에 해당하는 구간과 신규 `crates/transport/tests/driver_failures.rs`.
- 공유 문서 변경: 이 기록과 evidence README·documents.json의 등록 항목만 순차 반영한다. COMM 인수 작업의 구현 기록·diagnostics·실패 주입, DEV2 작업은 보존한다.
- 동시 작업: 같은 checkout에서 COMM 인수·DEV2 구현/리뷰가 진행 중임을 확인했다. source fingerprint를 복사 전후 대조한 사본에서 재현·수정하고, 최신 원문에 좁은 diff만 적용한다. 사본 결과를 이후 변경된 전체 checkout의 PASS로 승계하지 않는다.
- 순서: 계약·소스 비교 → 수정 전 반례 → 사본 수정·독립 검토 → 현재 파일에 좁은 patch → 관련 실행 검사와 증거 대조. 새 package·의존·생성 소스는 없다. 새 시험은 기존 `crates` fingerprint/export root에 포함된다.

## 확인한 결함과 수정

| 우선순위 | 반례·영향 | 수정·회귀 |
| --- | --- | --- |
| P1 | driver 취소 token과 연결 token이 분리되어 있었다. 첫 write에서 driver 취소를 요청해도 추가 write를 수행하고 뒤 frame까지 local flush로 확정한다. 수신 중 취소도 다음 read·payload 공개·TX를 계속하고 기존 sender는 다음 driver step 전까지 새 frame을 승인한다 | 기존 admission gate에 parent 취소 token을 연결하고 sender·각 연결 I/O 검사에서 local/driver token을 함께 확인. partial TX의 실제 3 bytes만 실패 결과로 보존, RX 미공개·추가 TX 차단, 기존 sender의 enqueue 거절. 다른 연결의 local 취소는 driver·형제 연결에 전파하지 않음 |
| P1 | listener의 `ConnectionAborted`/`ConnectionReset`이 driver 오류로 전파되어 정상 연결의 pending TX와 모든 listener까지 종료한다 | peer 중단 오류는 해당 listener만 기존 20 ms backoff로 재시도. 정상 연결의 partial TX·FIFO·다른 진행을 보존하며 지속적인 listener 오류는 원래 cause로 종료 |

driver token을 연결마다 복제한 별도 상태로 갱신하거나 새 lock을 추가하는 대신, 기존 gate에서 parent token을 보관한다. 먼저 생성된 sender도 같은 gate를 참조하므로 별도 재등록이 필요 없다. 오류를 전부 무시하지 않고 peer 연결 중단 두 종류에만 재시도를 적용한다.

## 공개 DBMS 소스 비교

아래 로컬 checkout의 origin·HEAD·작업 트리를 다시 확인했다. 모두 작업 트리 변경이 없었고, 소스 수정·DBMS build/test는 수행하지 않았다. 로컬 catalog의 경로를 사용했으며 실제 읽은 revision을 아래에 고정한다.

| 대상·실제 revision | 읽은 파일·구간 | 관측·XADB 적용 |
| --- | --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/libpq/pqcomm.c` 792–820, 1200–1268, 1360–1426; `src/backend/postmaster/postmaster.c` 1710–1745 | 길이 검증 뒤 allocation, 부분 write cursor 유지, accept 실패를 해당 시도로 제한하고 대기 후 재시도. XADB는 driver 전체 sleep 대신 listener별 bounded backoff로 정상 연결을 계속 진행 |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql/net_serv.cc` 200–251, 805–980; `sql/mysqld.cc` 6910–6968 | packet 상한·남은 read 길이·취소와 오류 처리, bounded accept loop에서 실패 통계 후 다음 loop 진행. XADB frame/queue·전체 연결 취소 계약은 별도로 유지 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/connection/connection_support.cpp` 377–565, 1020–1145 | short I/O 누적, WouldBlock 뒤 poll, EINTR 재시도 중 abort 확인, peer EOF/reset 구분. XADB의 효과 전후 취소·절대 deadline·실제 진행량 보존을 대조 |

origin은 각각 [postgres/postgres](https://github.com/postgres/postgres), [MariaDB/server](https://github.com/MariaDB/server), [CUBRID/cubrid](https://github.com/CUBRID/cubrid)다. 외부 코드·protocol·SQL 의미를 도입하지 않았다. PostgreSQL의 OOM 뒤 body discard, DBMS별 wire layout·thread 모델은 채택하지 않는다.

| 파일 | 열람 파일 전체 SHA-256 |
| --- | --- |
| PostgreSQL `pqcomm.c` | `d6de729509a2876d6a1c2aacbe9e4fcb17b1420de684f46242b4848a4ecc40cd` |
| PostgreSQL `postmaster.c` | `79b36f9074e954c2c64aa4ec5591633cd5869daffa885d5a3e972ff49f0689b1` |
| MariaDB `net_serv.cc` | `7d7b8b8a8de1834099f350912ed9255a1f376565da407e8dc461f97c72f12c93` |
| MariaDB `mysqld.cc` | `e28a6f0e8c9a49e320098c7b3a73aec2419ebbe7f27391e9c0d12dcaa5e60802` |
| CUBRID `connection_support.cpp` | `a2248046cab91c0ab7d588658e077a3f5ae55353698fa5c1253673ebae00183a` |

## 실행과 독립 검토

수정 전 `cargo test -p xadb-transport --locked --offline --test driver_failures`는 5개 중 취소·accept 반례 4개 FAIL, 인접 fatal-error 사례 1개 PASS였다. 수정 후 같은 5개가 PASS했고, 실제 별도 thread가 partial write 반환 전에 driver 취소를 요청하는 schedule와 형제 연결의 local 취소 격리 사례를 추가해 총 7개 PASS를 확인했다. 원본 로그는 사본 상위의 `repro-before-all.log`, `repro-after.log`, `regressions-after.log`다.

별도 검토자는 원래 계약·구현·반례·DBMS 코드와 좁은 diff를 직접 읽고, 사본에서 초기 5개 회귀를 독립 실행하여 모두 PASS를 확인했다. 이후 강화한 partial-TX 사례와 실제 thread·형제 연결을 포함한 최종 7개 시험, 실제 파일 일치와 아래 통합 실패 원인까지 재검토하여 두 수정에 잔여 차단 결함이 없다고 판정했다. 실제 native Unix 소유 경로 정리도 지원 전제 안에서 추가 확정 결함을 찾지 못했다. 같은 모델의 별도 실행이며 통계적 독립성 주장은 아니다.

`python3 -B scripts/check_comm.py --with-regressions --report ../validation-r2.json`을 위 보존 사본에서 실행했다. 최종 실행 source SHA-256은 `89b6e2baee8a339cc9be026810787417c88b8c017545df2a19b049705dcf9ce4`이며 실행 종료 후 다시 계산한 지문도 같다. 수정 후 `foundation`·`platform`·`transport`·`transport-worker`의 모든 파일이 검증 사본과 현재 checkout에서 같음을 확인했다. 이 확인 이후의 동시 작업은 해당 검증 범위에 포함하지 않는다.

| 실제 실행 | 결과·범위 |
| --- | --- |
| transport default / all-features | 각각 38개 / 41개 시험 PASS. 신규 driver 회귀 7개·기존 native 시험·control layout 포함 |
| COMM Clippy·의존 경계·독립 build·worker default build | PASS. 새 의존·wire 변경 없음 |
| 독립 Python peer | IPv4·IPv6·Unix × echo/corrupt/truncated/kill 12개 사례 PASS. 기존 process-kill receipt의 partial-RX 도달 한계는 유지 |
| Python checker 부정 시험 | 기존 3개 test method PASS |
| foundation·platform 직접 회귀 | `cargo test -p xadb-foundation -p xadb-platform --locked --offline` 및 `--all-features`: 각각 22개 / 30개 PASS |
| DEV2 P0/K0 → DEV1 → DEV0 하위 gate | FAIL. DEV0 workspace Clippy가 별도 DEV2 작업의 `crates/testing/engine-worker/tests/kernel_stress.rs` 114·207·387행에서 `collapsible_if`, `needless_range_loop`, `manual_is_multiple_of`를 거절. 이후 하위 gate는 완료되지 않음 |

전체 R2 보고서는 **FAIL**로 보존한다. COMM 관련 검사 PASS가 이를 대체하지 않는다. 최초 전체 실행 `validation.json`도 신규 시험의 `.err().expect()` Clippy 오류로 FAIL이었으며, 이를 `expect_err()`로 수정한 뒤 R2를 실행했다. 이전 실패와 `integration.log`·`integration-r2.log`, R2의 하위 보고서를 삭제하거나 PASS로 바꾸지 않았다. 직접 foundation/platform 실행 로그는 `foundation-platform-default.log`와 `foundation-platform-all.log`에 있다.

최종 좁은 patch는 사본 상위 `review.patch`, 현재 파일 적용 확인 hash는 `applied-source.json`에 보존했다. `/root`가 현재 checkout에 반영했고 독립 검토는 완료했지만 V2 전체 통합 승격은 하위 gate 실패 때문에 보류한다. 별도 DEV2 작업 파일은 수정하지 않았다. 전체 C1 model/fault/fuzz/stress/성능 인수, 내구성·SQL·공식 배포의 완료 증거로 사용하지 않는다. 문서 검사는 DOC-002에 따라 push gate로 남겼으며 이 작업에서 문서 검사·commit·push를 수행하지 않았다.

## 후속 commit 요청의 분리 검증

같은 날 사용자의 후속 `commit` 요청에 따라 위 두 수정과 7개 회귀·이 기록/색인만 분리했다. COMM 인수 계측·실패 주입과 DEV2의 미커밋 변경은 제외했다. 기준 commit `fe8d3f5d2a7744a1c54a26fa9d30c656b974fa7c`에 commit 대상 8개 파일만 적용한 사본은 `artifacts/comm/commit-20261019-hpoo6x0_/source`다.

이 사본에서 `cargo fmt -p xadb-transport --check`, `cargo test -p xadb-transport --locked --offline`, 같은 test의 `--all-features`, `cargo clippy -p xadb-transport --all-targets --all-features --locked --offline -- -D warnings`가 PASS했다. 기본/전체 feature 각각 35개이며, 위 혼합 사본의 38/41개와 다른 범위다. 로그는 사본 상위의 `transport-default.log`, `transport-all.log`, `transport-clippy.log`에 보존했다. 이전 혼합 사본의 하위 gate FAIL이나 전체 C1 상태를 대체하지 않는다. 문서 자동 검사와 push는 수행하지 않았다.
