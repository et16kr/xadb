# PR #54 후속 설계·코드 검토와 신규 검증

2026-11-05. 대상은 `et16kr/xadb` PR #54, base `2102038784400743eab550cc08d72e15ebd06d74`,
원 head `69899a1a9fba97b167685616b9a4f7eb3ddeeb3f`다.
사용자가 설계·방향성·코드 리뷰, 기존 시험 외 신규 시험 및 문제 수정 commit을 요청했다.
담당 `/root`, 별도 검토자 `/root/independent_review`. 확정 계약 안의 시험 보강은 D1,
원 PR의 경계·공통 API 설계 검토는 D2다. main 병합은 하지 않는다.

## 검토 판단

- A가 실패한 행의 재시도·문장 rollback을 조립하고, C가 현재 leaf의 공간·예약 D·부채·split 대상과
  WAL 효과를 판정하는 방향은 기존 DISK 엔진·모듈 경계에 맞는다. 구조 split은 redo-only system 작업이고
  사용자 행의 실패와 수명이 다르므로 재시작 시 트리 형태까지 확인해야 한다.
- route LOAD의 anchor/current 보호, 선택된 split 집합 재적재, INIT victim look-ahead,
  shell의 checked 반환, 정확한 Capacity trigger·clean preflight finish, split 16회 상한과
  flush의 비계수, F4 source census 및 A literal 시험을 검토했다. 새 제품 결함은 확인하지 못했다.
- 제품 공급 한계는 유지한다. CREATE Catalog Job의 live cleanup 호출자가 없어 일반 제품 표는
  split을 Unsupported로 거절한다(일정 10-1 잔여). RUP-DEV01의 찬 leaf UPDATE 검증 편차는
  기존 코드의 문제이며 이 PR에서 해결하지 않는다. 따라서 내부 연결 단계로 평가하며 제품의
  자동 확장 완료로 주장하지 않는다.
- 문서 결함: automatic-split의 상태가 D2 2회차 CONDITIONAL에 머물렀다. 원 검토 기록의
  3회차 ACCEPT·필수0으로 바로잡되 1·2회차 경과는 보존했다.

## 신규 시험과 결과

제품 소스는 원 head 그대로다. 새 시험만 선택하고 완료된 기존 시험·전체 workspace는 실행하지 않았다.
`cargo test -p xadb-engine --lib --locked spl_review_ -- --nocapture`의 최초 호출은 build에 성공했지만
필수 `XADB_CATALOG_SOURCE_SHA256` 누락으로 **3 FAIL, 본문 미실행**이었다.
이는 제품 반례가 아니라 실행 준비 오류다. 원 로그는 별도 보존했다.

동일 ELF에 실제 source manifest·ELF·선택 입력 SHA-256과 `RUST_MIN_STACK=33554432`를 설정하고,
`<ELF> spl_review_ --nocapture --test-threads=1`로 실행했다. 입력은 3개 사례, 단일 thread,
전체 timeout 1800초로 먼저 고정했다. 각 사례의 900초 상한은 소스에 고정되어 있다.

| 사례 | 독립 기대값 | 결과 |
| --- | --- | --- |
| SPL-REVIEW-01 | committed 7행 위 split 직후 파일만 재시작; Q=8, CLR=0, 기존 값7·leaf2 유지, 새 행 없음 | PASS |
| SPL-REVIEW-02 | Active 7행 위 split 직후 파일만 재시작; Q=1, CLR=7, Row Job=0, 행0·빈 leaf2 | PASS |
| SPL-REVIEW-03 | 앞 statement 7행, 다음 statement의 split·INSERT 뒤 NULL 거절; 그 문장만 rollback, 후속 INSERT·Commit Q=2/8행, 재시작 값·leaf2 | PASS |

실제 출력: **3 passed; 0 failed; 2356 filtered out; 103.87s**.
RowId·값의 기대는 입력 이력에서 정하고, 새 recovery 뒤 B cursor로 읽은 값과 leaf 형태를 검산했다.
SPL-REVIEW-02는 holder drop 전에 I/O 파일을 캡처하여 active loser 경계를 보존한다.
시험은 메모리 I/O double의 파일을 새 recovery에 넣는 검증이며, native process crash나 실제 fsync
검증을 새로 수행한 것은 아니다. 최소 pool·FlushRequired 왕복 경합 등은 여전히 미검증 잔여다.

## 실행 identity와 보존

- source manifest SHA-256: `dea6249ae991ab391125085a6c556cd564e6469ad0fdbb3734830c3ffdebd455`
- ELF SHA-256: `2f532edf6e3faa90bc07d8bbc32f9785cdd9e75596738546c591d964a5a2e340`
- 고정 실행 입력 SHA-256: `39ceaaedb13f93c50d033599df1ace7414b01efc059615e645ce07446b96e02b`
- 새 시험 파일 blob: `6b7a91d6eeecd108e1bcb6f021aa7983877d4b85`
- 계약 문서 최종 blob: `07bd33ffca67adaec11f8edaf1df145c38166209`
- 원 실패 로그 SHA-256: `7a9dba14c661d77fb803471cdb112c185af3b067e7fe17d2dab9db88e5106b8b`
- 실제 3 PASS 로그 SHA-256: `8218be696976bd80273285fbfa37fd3ba7742aeaf0a3b3e3a7189a5da1419f05`

로컬 증거는 `/workspace/.xadb-env/logs/pr54-review/`의 `new-tests.log`,
`new-tests-configured.log`, `source-files.sha256`, `identity.json`, `execution-plan.json`, `run.py`다.
실행 뒤 바뀐 계약 설명·gate 기록은 이 source manifest의 일부로 소급 주장하지 않는다.
시험·제품 소스가 그대로이므로 문서 변경만으로 시험을 반복하지 않는다.
생성 target/ELF는 hash 기록 뒤 삭제하며 원 로그·manifest·입력은 보존한다.

## 별도 검토

`/root/independent_review`가 원 PR의 제품 diff와 literal·budget 시험, 신규 시험의 기대값·실패 경계,
계약 수정, 두 실행 로그를 읽고 **ACCEPT, required_open=0**으로 판정했다. 재실행은 하지 않았다.
신규 시험 blob `6b7a91d6eeecd108e1bcb6f021aa7983877d4b85`와 계약 blob `07bd33ffca67adaec11f8edaf1df145c38166209`에서 확인한 판정이다.
SplitScan의 resident leaf 검산은 이번 두-leaf fixture에 한정되며 일반 tree inventory oracle로 주장하지 않는다.
원 PR의 기존 D2 검토 및 실행 증거는 보존하고 이번 검토를 별도 record 항목으로 추가한다.

fmt·Clippy·문서 검증과 module gate는 전송할 commit에 기존 pre-push 절차로 적용한다.
그 결과는 실제 push 로그와 최종 보고에서 구분한다.
