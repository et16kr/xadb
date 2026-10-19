# DEV7 — 비교 문맥·CALL·XA 설계 재검토

2026-10-19 · **DEV7-DESIGN-002 / D2 설계 보완 채택·독립 정적 검토 PASS / 제품 구현·복구·DBMS 비교 실행 NOT_RUN**

[설계와 결정](../contracts/dev7-sql-extensions-design.md#review-002) · [인수 계획](../plans/dev7-implementation-plan.md#acceptance) · [이전 조사 기록](dev7-source-research.md#review) · [단계 완료 계약](../plans/stage-completion.json)

## 범위와 작업 소유

사용자의 Oracle·ARIES·로컬 DBMS 참고 리뷰 및 발견 문제 즉시 수정 요청에 따라 네 계약 경계를 보완한다. 작업·계약·통합 담당은 `/root`, 분류는 DP01의 D2다. 별도 검토 전에는 이번 보완을 독립 PASS로 표시하지 않는다. 최초 DEV7-DESIGN-001과 그 검토는 당시 범위의 기록으로 보존하며 이번 판단을 소급 적용하지 않는다. 추가 로컬 checkout은 `SOURCE-L1`으로 식별한다.

착수 기준 XADB HEAD는 `57d25d3e67b24c2e4e39b757d20b5814de42f6d4`, 당시 `git diff --binary HEAD` SHA-256은 `1d554eee203d1885a23ffa3e6e1a08fc69c21a3836e89749c6fd17f5f3b90b1a`다. 이 지문은 tracked diff만 식별하며 untracked 파일을 포함한 제품 source/clean export identity가 아니다. 검토 입력은 다음 세 문서의 SHA-256으로 고정했다.

| 입력 | SHA-256 |
| --- | --- |
| dev7-sql-extensions-design.md | `cc65bd6df6907e0b6dcb8cb895204a35792148c11dc4e74d98b1640e09859ff6` |
| dev7-implementation-plan.md | `92f80598c5ab54efcd250ef4df98cff00761329fc1db1fa271e12152d7ab6b8e` |
| dev7-source-research.md | `95a29beeb26f720799ab8a1e33a33795e10176348c7516b3c40bc66d6c2098a6` |

수정 소유는 `devdoc/contracts/dev7-sql-extensions-design.md`, `devdoc/plans/dev7-implementation-plan.md`, 이 기록이다. 공유 편집은 `devdoc/plans/stage-completion.json`, `devdoc/README.md`, 해당 세 폴더 README와 `devdoc/documents.json`의 DEV7 항목으로 한정하며 `/root`가 순차 반영한다. 기존 engine/testing/COMM·DEV2/DEV6 등의 dirty 변경은 이번 소유 밖이다. 새 검토자는 읽기 전용이며 같은 파일을 동시에 수정하지 않는다. 합류 순서는 계약 → 인수 계획/완료 JSON → 기록/색인 → 독립 검토 → 담당 반영이다. 제품 코드·소스 checkout·과거 증거·생성물·빌드 manifest는 수정하지 않는다.

## 실제 참고한 자료

공개 문서/논문의 열람과 로컬 소스의 특정 책임 비교다. 외부 DBMS를 실행해 동등성을 인증한 것이 아니며 코드를 이식하지 않았다.

| 자료 | 실제 열람과 적용 |
| --- | --- |
| [Oracle 26ai Data Type Comparison Rules](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Data-Type-Comparison-Rules.html), 2026-10-19 | CHAR 계열의 blank-padded와 VARCHAR 계열이 포함된 nonpadded 비교 구분. XADB의 비교 문맥별 key/optimizer 전제를 도출 |
| [Oracle Java SE 21 XAResource](https://docs.oracle.com/en/java/javase/21/docs/api/java.transaction.xa/javax/transaction/xa/XAResource.html), 2026-10-19 | `forget`은 heuristic 완료 대상, `recover`는 prepared/heuristic XID를 반환. 표준 정상 완료에 별도 ACK API가 없음을 확인하고 확장 receipt profile을 분리 |
| [Mohan 외, ARIES, TODS 17(1), 1992](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf), 2026-10-19 | 원문 PDF를 직접 읽어 prepare record의 잠금 기록/재획득, checkpoint 인계, redo의 history 반복과 in-doubt/loser 분리를 확인. DEV3/7의 CLR·prepared 보호 방향을 지지하며 제품 복구 검증을 대신하지 않음 |

아래 revision은 이번 리뷰에서 실제 확인한 HEAD다. 공개 checkout 다섯 개는 clean이었고 SOURCE-L1에는 untracked `tests/`가 있었으나 해당 파일은 연구 입력으로 사용하지 않았다. 로컬 경로는 source catalog와 추가 checkout root 아래의 상대 소스 위치다.

| 소스 / 실제 revision | 열람 위치와 관측 |
| --- | --- |
| PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | `src/backend/executor/spi.c` 225–255·330–346: atomic 문맥의 commit/rollback 차단. `src/backend/utils/adt/varchar.c` 746–783·987–1043: CHAR 비교와 trailing-blank를 일치시킨 hash |
| MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | `sql/sp_head.cc` 315–341·`sql/sp_head.h` 973–993: COMMIT/ROLLBACK 분류와 function/trigger의 거래 제어 제한 |
| MySQL `3b99be40f8a31d9396cad10accfd3ba51647d613` | `sql/xa/sql_xa_commit.cc` 80–210: prepared commit·실패/재시도 경계. 일부 구현의 unilateral rollback 분기는 XADB의 durable prepared 결정 불변 규칙으로 가져오지 않음 |
| SQLite `43a48f77585a88940c727f33682151418f544059` | `src/fkey.c` 19–99: immediate/deferred 제약과 statement rollback 경계. counter 구현 자체를 정답으로 복사하지 않음 |
| CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | `src/query/serial.c` 1180–1260: sequence 변경 redo와 system operation, non-cycle 범위 경계. 이 부분만으로 전체 발급 내구성을 인증하지 않음 |
| SOURCE-L1 `0682bdd8610c3728810df616c95a858cda675706` | `/home/et16/work/altidev4`의 `src/qp/qsx/qsxEnv.cpp` 849–925·`qsxExecutor.cpp` 6287–6333: routine rollback·문맥별 제한. `src/mm/mmd/mmdXa.cpp` 723–777·1945–1985: heuristic 전용 forget과 prepared/heuristic recover 합집합 |

## 발견 사항과 반영

| ID / 중요도 | 반례·문제 | 반영한 계약과 필수 검증 |
| --- | --- | --- |
| D7-R2-01 / P1 | 호출 전 INSERT 후 CALL 안의 전체 ROLLBACK 또는 호출자 savepoint 조작은 CALL 이전 변경을 지운다. COMMIT/DDL만 열거한 제한으로는 불충분 | atomic 문맥의 모든 거래 종료/재시작·mode 변경 차단. invocation 소유 marker와 implicit mark 분리, 직접/동적/중첩/trigger 동일 검사. F05·T12/T15/T16 |
| D7-R2-02 / P1 | 정상 XA 완료에도 TM forget/ACK를 기다리면 표준 TM의 성공 workload만으로 terminal ledger가 포화된다 | StandardXa 정상 receipt의 안전한 bounded retirement·회수 후 NOTA, XID 비재사용은 TM 의무. DecisionReceiptAck1의 ACK/frontier는 별도 협상. F07/F09·T25/T26/T30·C07 |
| D7-R2-03 / P2 | CHAR(1) A='a', CHAR(2) B='a ', VARCHAR(2) C='a '에서 A=B와 B=C가 TRUE지만 A=C는 FALSE. 전역 equality 추론은 행을 누락 | ComparisonDescriptor별 타입/변환·padding·collation·NULL, 양쪽 hash/merge 정규화와 index 문맥 호환. 불법 추이성 규칙 known-bad. F01/F02·T02/T05/T10 |
| D7-R2-04 / P2 | force 해소 응답 유실 뒤 prepared만 recover하면 TM이 heuristic 처리 대상을 열거하지 못한다 | Prepared+미forget HeuristicCompleted 합집합, bounded scan 소유와 상태 재검사, heuristic 결과 보존·영속 forget 뒤 부활 금지. F07·T25/T26/T28·C07 |

네 사항은 구현 전 의미 계약의 보완이다. 숫자/ABI/byte·전체 state/fault variant 동결은 여전히 F01–F10의 미완료 입력이며 이 기록의 반례 수로 전체 인수를 대체하지 않는다. StandardXa에서 NOTA는 adapter의 성공 합성 근거가 아니다. TM의 durable journal/peer identity/profile로 입증하지 못한 결과는 UNKNOWN을 유지한다. 확장 ACK가 없어도 표준 TM 지원을 제공해야 하며 확장 profile 성공만으로 그 요구를 닫지 않는다.

## 확인한 반례와 실행 범위

문자 비교의 작은 독립 Python 모델에서 아래 기대값을 확인했다. XADB evaluator나 외부 DBMS를 호출하지 않았으며 구현·optimizer PASS로 보고하지 않는다.

```python
values = {"A": ("CHAR", "a"), "B": ("CHAR", "a "), "C": ("VARCHAR", "a ")}
def equal(left, right):
    lt, lv = left
    rt, rv = right
    if lt == rt == "CHAR":
        size = max(len(lv), len(rv))
        return lv.ljust(size) == rv.ljust(size)
    return lv == rv
assert equal(values["A"], values["B"])
assert equal(values["B"], values["C"])
assert not equal(values["A"], values["C"])
```

CALL/XA는 계약·소스·명세의 정적 반례 검토다. 실제 CALL rollback, XA ledger 반복 workload, 두 RM/TM crash/recover/forget, bounded state 전수 탐색과 성능 인수는 NOT_RUN이다. 제품 build/test·문서 자동 검사·commit/push·배포를 실행하지 않았다. 문서 검사는 기존 pre-push gate에서 수행한다.

## 독립 검토와 적용 상태

독립 검토자 `/root/dev7_revision_review`가 원래 네 반례·최신 계약/인수 diff·단계 완료 JSON·색인을 읽고, Oracle 문자 비교/XAResource 공식 명세와 PostgreSQL·SOURCE-L1 해당 소스를 직접 대조했다. 결과는 **정적 설계 검토 범위 PASS, P1/P2 잔여 및 필수 수정 없음**이다. 비교 문맥/추이성, CALL atomic 문맥과 marker 소유, StandardXa retirement와 확장 ACK 분리, heuristic recover/forget 및 인수 연결을 확인했다. 검토자는 파일 수정·문서 자동 검사·제품 시험·commit을 수행하지 않았다.

| 검토자가 확인한 수정안 | SHA-256 |
| --- | --- |
| 설계 | `7438ac4f84effde8d2a0b9b5404448e77d6a6663058dcd00f5bb46b0b949e8eb` |
| 인수 계획 | `e435900cb68eb90b0b0cb84054710c74f7219a4b4703344e2478fd1984a12428` |
| 단계 완료 JSON | `6104ddad410a616036df385b811d4ae2550d06dc1018614a7dd585234bccf509` |
| 이 기록의 검토 전 본문 | `38426cdf8e2e36a69b78907b2b3bb296681a376cb8eeaa87c06b8c6ed7920491` |

계약/통합 `/root`가 위 네 지문과 작업 트리의 일치를 확인한 뒤 DEV7-DESIGN-002를 ACCEPTED로 전환했다. 지문은 상태 전환·검토 결과를 덧붙이기 전 snapshot이며 그 뒤 변경은 설계/계획/이 기록의 상태·검토 기록뿐이다. 제품 인수·F01–F10 동결·SC07–SC11·SQL_EXT1·배포는 여전히 NOT_RUN이다.
