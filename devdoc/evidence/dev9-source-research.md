# DEV9 — Oracle·ARIES·로컬 소스 조사와 설계 검토

2026-10-19 · **실제 문서·소스 열람 / DEV9-DESIGN-001 독립 수정본 검토 PASS·채택 / 제품·비교 시험 NOT_RUN**

[상세 설계](../contracts/dev9-qualification-design.md) · [구현·검증 계획](../plans/dev9-implementation-plan.md) · [공개 로컬 소스 catalog](../reference/opensource-references.md)

<a id="method"></a>
## 1. 질문·방법·변경 범위

사용자는 DEV9 설계에 Oracle·ARIES·작업 환경의 DBMS와 추가 로컬 소스를 참고하도록 요청했다. 질문은 (1) 기능·환경·운영 조합의 누락을 어떻게 막는가, (2) 실제 복구·업무 결과·성능을 어떤 독립 증거로 인수하는가, (3) 설치·문서·버전의 제품 인수와 외부 출시를 어떻게 구분하는가다.

작업·계약·통합 담당은 `/root`, 분류는 D2다. 기준 XADB commit은 `7aeafe4d32b116fda0ebb95140347088c0003850`, 작성 전 tracked diff의 SHA-256은 `1189a26f59d63c8cc914373eae360137209ae389574a500199e9b660a9d90389`다. 이는 dirty 작업 트리를 구분하기 위한 관측이며 제품 source fingerprint·실행 증거가 아니다. DEV2/COMM 코드·시험·보고서·기존 문서 변경이 진행 중임을 `git status`로 확인했다.

소유 변경은 새 DEV9 설계·계획·이 조사 문서다. 공유 편집은 단계 Markdown/JSON·capability·기능 작성 목록·각 README·문서 catalog의 DEV9 연결이며 최신 파일을 읽어 좁게 반영한다. 기존 제품 코드·과거 증거·다른 설계 원문을 바꾸지 않는다. 조사 → 계약/계획 → 공유 참조 → 독립 검토 순서로 반영한다. 문서 검사기는 pre-push에만 적용하며 이번 편집 중 실행하지 않는다.

공개 소스는 실제 HEAD·원격·작업 트리를 확인하고 아래 지정 범위만 읽었다. 네 공개 checkout은 열람 시 clean이었다. catalog의 모든 10개 소스를 조사했다고 주장하지 않는다. 추가 조회한 일부 catalog 경로는 현재 없었으며 조사 근거에 포함하지 않았다. 외부 DBMS build·비교 workload·복구 시험은 실행하지 않았다. 코드·장문 원문·기존 expected 결과를 가져오지 않고 관측과 XADB 선택을 나누어 기록한다.

<a id="primary"></a>
## 2. Oracle와 ARIES 원문

| ID·원문 | 실제 확인 범위·관측 | XADB 설계 선택·인수 연결 |
| --- | --- | --- |
| OR-Q01 [Oracle Database 19c, Introduction to Database Replay](https://docs.oracle.com/en/database/oracle/oracle-database/19/ratug/introduction-to-database-replay.html) | Workload Capture/Preprocessing/Replay, Analysis and Reporting. client 요청·bind·transaction의 재생, 배경 job 미포함, replay 차이와 별도 업무 검증 필요성을 확인 | fixture/session/dependency를 고정하고 background 운영 작업을 별도 matrix로 명시. 결과/오류·업무 불변식·성능을 분리; M01–07·T11–22·T29 |
| OR-Q02 [Oracle Database 19c, Validating Database Files and Backups](https://docs.oracle.com/en/database/oracle/oracle-database/19/bradv/validating-database-files-backups.html) | 16.1 physical/logical corruption, 16.4 RESTORE VALIDATE. 검증 과정과 출력 파일을 쓰는 실제 restore가 구별됨을 확인 | 파일 integrity만으로 복구 인수하지 않고 실제 restore→논리/구조 대조→새 commit→재시작; T16/20/28/32 |
| AR-Q01 C. Mohan 외, 1992, [IBM 논문 정보](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging), [ARIES 원문 PDF](https://www.cs.cmu.edu/~15849g/readings/mohan92.pdf) | §6 analysis/redo/undo·pageLSN/dirty page·CLR, §7 restart 중 checkpoint, §8 media recovery의 fuzzy image copy와 redo 시작점. 원문 해당 절을 텍스트 추출하여 읽음 | redo/undo/CLR 중 재장애와 복원 closure를 독립 history로 인수. 실제 kernel은 DEV3의 UndoRef·예약·REBASE 소유 계약을 따름; T14–17/32 |

ARIES PDF는 웹 도구의 timeout 후 HTTP로 받아 Poppler 텍스트 추출로 해당 절을 확인했다. 파일 SHA-256은 `6131bcee36c21e669d41204ef2b2320cb458b5d3bd9b7cf3e7758afe8ed3495f`다. 논문의 전체 구현이나 증명·benchmark를 재현한 것은 아니다. 위 자료에서 XADB의 수치 SLA·공개 SQL 호환·wire 형식·동일 알고리즘을 도출하지 않는다. Oracle replay 기능 자체를 XADB 제품에 추가하는 결정도 아니다.

<a id="public-sources"></a>
## 3. 공개 로컬 소스의 실제 관측

아래 링크는 확인한 revision에 고정한다. 각 파일의 지정 범위·책임만 확인했으며 제품 전체 testing 전략을 인증하지 않는다.

| ID·실제 revision | 읽은 소스·관측 | 채택/기각·검증 |
| --- | --- | --- |
| PG-Q01 PostgreSQL `45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851` | [isolation/README](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/test/isolation/README) 1–110행. 여러 session·step·permutation, prepared 설정에 따른 실행 범위 구별 | 명시 schedule과 실제 연결·적용성 분류. 기본 suite만 통과했다고 선택 기능을 인수하지 않음; T03/13/14/17 |
| PG-Q02 같은 revision | [001_stream_rep.pl](https://github.com/postgres/postgres/blob/45fc51fcbf4534ea2f6f85e36f8155dbd5ba9851/src/test/recovery/t/001_stream_rep.pl) 1–100행. primary/standby/cascade 구축·catch-up·실제 SQL 결과 검사 | node lifecycle·진행 witness·내용 검증을 구분; T18. XADB 초기 standby read 지원을 이 소스만으로 추가하지 않음 |
| MA-Q01 MariaDB `327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4` | [mysql-test/README](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/mysql-test/README) 및 [instant_alter_crash.test](https://github.com/MariaDB/server/blob/327437ade1d6dc6e4e57a6f1723b4abe4ac6c4b4/mysql-test/suite/innodb/t/instant_alter_crash.test) 1–95행. private server, 외부 server로 실행 불가한 lifecycle 시험, debug 동기화 후 DDL/commit/crash/restart와 조회 | runner가 소유 process를 제어하고 fault 도달을 확인; T06/08/14. 기록된 product 출력의 자동 expected 승격은 채택하지 않음 |
| CU-Q01 CUBRID `a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5` | [util_sa.c](https://github.com/CUBRID/cubrid/blob/a1c3506c375c8dd207dbf33feaf6f82fa4ec7dc5/src/executables/util_sa.c) 1040–1148행, restoredb. backup 목록 조회와 실제 boot_restart_from_backup 경로·복원 시각/위치/key 입력을 구별 | inspect와 restore/runbook 시험을 분리하고 복원 후 public 경로까지 검증; T16/20/28. 관측만으로 해당 구현 전체의 안전성을 주장하지 않음 |
| SQ-Q01 SQLite `43a48f77585a88940c727f33682151418f544059` | [crash.test](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/test/crash.test) 1–100행, [ioerr.test](https://github.com/sqlite/sqlite/blob/43a48f77585a88940c727f33682151418f544059/test/ioerr.test) 1–95행. 변형 I/O로 sync 시점 장애·후속 내용 검사, I/O 실패와 reference/자원 확인, capability skip | process kill과 저장 장애 모델 분리, 오류 뒤 데이터/자원 검증·미도달/skip 비통과; T06/15/21. rollback-journal 알고리즘을 XADB WAL/undo로 가져오지 않음 |

<a id="local-neutral"></a>
## 4. 추가 로컬 자료 LOCAL-Q01

실제 revision은 `0682bdd8610c3728810df616c95a858cda675706`이다. 작업 트리에 untracked `tests/`가 있었으며 그 내용은 이번 근거로 사용하지 않았다. 아래 읽은 tracked 파일은 각각 HEAD bytes와 같음을 확인했다. checkout·Git 이력·원본 파일은 수정하지 않았으며 기존 script도 실행하지 않았다.

원래 파일 경로의 대응은 저장소 밖 `$HOME/.local/state/xadb/research-archive/f34788ba01910b69c0bc31f4b4b05406fdffaf511d4263bc860b7f6fece62b22/source-inventory.json`에 보존했다. inventory SHA-256은 디렉터리명과 같다. XADB 문서에는 중립 ID·revision·file hash만 기록하며 원문 제품명·코드·기존 경로 inventory를 전사하지 않는다.

| 중립 파일 ID·SHA-256 | 열람한 범위·관측 | XADB 반영 |
| --- | --- | --- |
| LOCAL-Q01-A `06828a7aac8b2cdabdda7bc9fdfd0f0c270d0e2e27679c63bbc1eae9149d62bb` | 전체 17행. 복구 시험의 초기 DB 구성과 fault 목록·driver 실행 조립 | run별 소유 fresh fixture·fault inventory. 전역 service kill·기존 DB 제거 방식은 채택하지 않음; T08/24 |
| LOCAL-Q01-B `1d684925b05f6f481bfe22e28759051503769355e903b8d04f3db44f275dddda` | 전체 28행. fault 목록 분할·여러 시험 process·결과 로그 합류 | shard별 원래 분모·완결 receipt를 확인하는 집계. 로그 파일 연결만으로 성공 처리하지 않음; T02/05/33 |
| LOCAL-Q01-C `bbe463ff961fbce917c48636e29066dac06a5d507c75bee37d177aa488e2bc7b` | 1–155행. 등록 kill point·scenario 실행·다음 startup와 마지막 scenario startup 검사; 일부 start 오류 경로가 exit 0 | fault 도달·최종 재시작·독립 내용 검증 필수. exit code/로그만으로 성공하지 않는 negative checker; T06/15 |
| LOCAL-Q01-D `12a7af414d2fb11162945f3430e631c8326594d098c79e997853e90fe3e154fe` | 1–105행. 실제 thread 생성/join과 반복 begin·commit/rollback | thread lifecycle stress와 업무 correctness를 별도 판정. 빈 거래 stress로 행/복구 인수하지 않음; T13/21/31 |
| LOCAL-Q01-E `05a11f547663106aeafb59f77f5167b12a8f1baa7a5ae688027904b40c508727` | 1–105·173–202행. OS별 설정 안내와 Linux 자원 tuning 안내 | 플랫폼 preflight와 사용자 안내의 필요를 참고. 기존 수치·비Linux 지원·특정 저장 방식 전제를 복사하지 않음; F08·T23/24 |

코드 도입은 없다. 향후 외부 코드·문구를 실제 도입하면 DP04의 출처·license/notice 의무를 별도로 적용해야 하며 중립 연구 식별자는 그 의무를 대체하지 않는다.

<a id="choices"></a>
## 5. 관측에서 선택으로 연결

1. replay의 운영 범위와 내용 검증을 분리해 SQL·driver·background job·복구·보안을 하나의 typed matrix로 연결한다. Oracle 자료의 관측을 XADB 자체 schema·oracle와 혼동하지 않는다.
2. ARIES와 현재 DEV3 계약에 따라 commit ACK뿐 아니라 undo/CLR·복구 중 재장애·media closure·prepared 의무를 확인한다. 알고리즘 재설계는 DEV9 소유 밖이다.
3. 공개 suite의 설정/skip와 로컬 fault 목록 분할에서 누락 위험을 도출해 사전 분모·실제 도달·완결 receipt를 요구한다. 실행된 case만 세는 coverage는 기각한다.
4. process·fixture·설치 경로를 소유 run으로 제한한다. 기존 환경 전체를 kill/삭제하는 script 형태는 이식하지 않는다.
5. 제품 인수와 외부 출시·버전 기준선을 분리하고 actual candidate의 모든 required 결과를 유지한다. 제품별 통과 결과나 설계 채택을 XADB PASS로 재사용하지 않는다.

<a id="review"></a>
## 6. 독립 검토와 실행 상태

2026-10-19, 별도 AI 실행 `/root/dev9_design_review`가 새 설계/계획/조사·DEV6/7/8와 DEV3 소유 계약·capability/stage JSON·README·문서 색인의 실제 변경을 읽기 전용으로 검토했다. 구현자의 결론만 전달받은 승인이 아니며 파일 수정·제품 시험·문서 검증기를 실행하지 않았다. 작업·계약·통합 담당 `/root`가 아래 지적을 반영하고 수정본을 재검토받았다.

| 지적·반례 | 반영 | 수정본 검토 |
| --- | --- | --- |
| P2: 설계 §3의 새 candidate 전체 인수와 §6의 시험/환경 오류 영향 셀 재실행이 충돌. 공통 oracle/worker 수정 후 이전 PASS를 섞는 우회 가능 | source/artifact 변경은 새 CandidateId+전체 SC17, baseline 내용 변경은 새 SC16+전체 SC17. 영향 재실행은 candidate/baseline/oracle bytes·ProfileId 불변인 일시적 인프라 복원과 독립 오염 closure 확인으로 한정; T07 반례 추가 | 반영 확인·해결 |
| 경미한 인용 위치: Oracle RESTORE VALIDATE 절을 §16.5로 기록 | 공식 19c 원문의 §16.4로 정정 | 반영 확인·해결 |

최종 결론은 **독립 D2 설계 검토 PASS, 남은 통합 차단 P1/P2 없음**이다. 검토자는 104개 requirement ID·required 값과 baseline/inventory 미동결·NOT_RUN 유지, 조기 준비/선행 gate/제품 인수/출시 구별, DEV3/7/8·버전 정책의 의무 보존을 확인했다. 공개 소스 4개의 revision·clean 상태, LOCAL-Q01 외부 inventory digest·5개 파일 hash/HEAD 일치, ARIES PDF digest도 별도로 확인했다. 통합 담당은 이 결과에 따라 DEV9-DESIGN-001을 ACCEPTED로 기록한다. F01–F10의 실제 입력 동결·제품 인수는 남는다.

제품 코드·runner·schema·package는 구현하지 않았다. 104개 부모 배정과 F01–F10·T01–T34·P01–P05는 설계/계획이며 SC16/SC17·PRODUCT_QUALIFIED·Oracle/공개 DBMS 비교 실행·공식 배포는 NOT_RUN이다. 문서 자동 검사는 push gate에서만 수행한다.
