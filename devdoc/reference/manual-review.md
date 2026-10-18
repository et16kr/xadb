# XADB 매뉴얼 리뷰와 구현 추적

2026-10-18 · 초안 0.2 반영 · **문서 리뷰 / 제품 시험 미실행**

[개선한 매뉴얼](user-manual-draft.md) · [SQL 명세](sql-language-draft.md) · [기계 판독 리뷰·근거·인수 계획](manual-review.json)

<a id="section-1"></a>

## 1 리뷰 결과와 반영 범위


CLI 필드·encoding·권한표 등 신규 상세는 제안이다. 커널·개발 순서·호환 정책을 변경하지 않으며, DEV0·DEV1 로컬 검증 외 DB 제품 지원이나 새 시험 성공을 주장하지 않는다.

| ID | 발견한 문제 | 반영한 수정 | 연결 인수 계획 | 본문 |
|---|---|---|---|---|
| MR-01 | 설치·시작/종료가 순서 설명에 그쳐 준비·실패 판정이 모호함 | 단계별 전제·완료·실패 상태와 데이터 폐기 분리 | MA-01, MA-02 | [수정 위치](user-manual-draft.md#manual-lifecycle) |
| MR-02 | 문장 오류·세션 상태·commit 결과·UNKNOWN의 관계가 부족함 | 상태 전이·재시도 단위·client/engine 불명 분리와 실패 예제 | MA-03 | [수정 위치](user-manual-draft.md#manual-transactions) |
| MR-03 | offline DDL·pin·quota의 운영 영향과 실패 후 자원 상태 부족 | DB 전체 drain·실패 원자성·소유 admission·회수 지연 계약 | MA-04, MA-05 | [수정 위치](user-manual-draft.md#manual-storage) |
| MR-04 | 설정 사전의 필수 항목과 적용 결과를 구현자가 다르게 해석할 수 있음 | 설정 카드·timeout 측정 범위·가려진 값·동시 변경 예제 | MA-06 | [수정 위치](user-manual-draft.md#manual-settings) |
| MR-05 | 기계 출력 값 encoding·완료·오류의 분리 기준이 부족함 | 열/행 배열·정수 문자열·NULL/binary·레코드 종류·exit 범주 제안 | MA-07, MA-08 | [수정 위치](user-manual-draft.md#manual-cli-results) |
| MR-06 | driver batch·표준 API 부작용·pool 반환이 개념 수준임 | 원자 단위별 반례와 driver 계약표·연결 재사용 기준 | MA-09, MA-10 | [수정 위치](user-manual-draft.md#manual-clients) |
| MR-07 | 권한 분리·철회·인증서 교체 절차가 부족함 | 업무 권한표·실행 시 재검사·교체 검증 절차 | MA-11 | [수정 위치](user-manual-draft.md#manual-security) |
| MR-08 | 관측 사전·결측·진단 종료/묶음 완료 조건이 부족함 | 뷰별 행 단위·reset 예제·수집 예산과 manifest | MA-12, MA-13 | [수정 위치](user-manual-draft.md#manual-observation) |
| MR-09 | backup/checkpoint/export·PITR·WAL 의존 자료가 혼동될 수 있음 | 복구 목적별 closure·cut·timeline·pin/공간·재개 반례 | MA-14, MA-15 | [수정 위치](user-manual-draft.md#manual-recovery) |
| MR-10 | 복제 수신·영속 인계·적용·writer 전환 판정이 부족함 | 단계별 위치·restart 보존·격리·failback 조건 | MA-16, MA-17 | [수정 위치](user-manual-draft.md#manual-replication) |
| MR-11 | 이관의 파일 의미·부분 commit·resume와 버전 변환 실패 경계 부족 | 파일 profile·chunk 결과·snapshot/값 비교·shadow 변환 절차 | MA-18, MA-19, MA-20 | [수정 위치](user-manual-draft.md#manual-transfer) |
| MR-12 | 확장 기능이 이름만 나열되고 요구별 누락·미정·시험 연결이 부족함 | 후속 기능 계약표·104개 요구별 문서 깊이·12개 OPEN 결정·25개 SQL 사례 | MA-03, MA-09, MA-18, MA-20 | [수정 위치](user-manual-draft.md#manual-future-contracts) |

전체 매뉴얼이나 capability inventory가 완성된 상태는 아니다. 특히 후속 SQL/PSM/공간/외부 연계의 정확한 문법·overload·API 조합, 포트·한도·RPO/RTO의 수치는 해당 기능 구현 전에 확정해야 한다. 19장의 MS-01–12가 산출물과 책임 영역을 추적한다.

<a id="section-2"></a>

## 2. 이관한 설계 제안의 적용 범위

기존 초안에서 유용한 운영 질문과 인수 사례를 이관했다. 원래 제품 매뉴얼 조사·revision·줄 번호는 이 문서에서 제외하며 공개 DBMS에서 새로 검증한 결과로 바꾸지 않는다. 아래 사례는 모두 계획이고 실제 제품 시험 증거가 아니다. 이후 [공개 참고 자료](opensource-references.md)와 XADB 계약에 따라 각 의미를 검토한다.

<a id="section-3"></a>

## 3 매뉴얼에서 도출한 인수 사례

**20개 모두 PLANNED_NOT_RUN**이다. 아래는 실제 test runner 출력이 아니라 구현 후 실행할 입력·장애·기대 관측이다. fixture·단계·연결된 UX·미정 결정은 JSON에 있으며, 관련 단계가 준비되기 전에 상위 기능을 선행 조건으로 요구하지 않는다. MA-20의 고객 교차 버전 부분은 이전 안정 릴리스 기준선이 있을 때만 적용하고 사내 배포는 fresh 구축이다.

| ID | 과제 | 입력·장애 | 기대 관측 |
|---|---|---|---|
| MA-01 | 빈 경로 생성·복구 준비 | 기존 경로 생성 요청; recovery VERIFY 전 일반 SQL 접속; VERIFY 완료 후 fixture 조회 | 기존 경로는 변경 없이 거절; VERIFY 전 일반 SQL 거절; READY 후 대상 identity와 정확한 행 확인 |
| MA-02 | 종료 대기와 재시작 | 정상 종료 대기 만료를 관측; 별도 강제 종료 후 소유 worker를 실제 recovery로 재시작 | 대기 만료는 종료 완료가 아님; ACK 거래 보존; 미확인 거래는 허용 이력으로 대조 |
| MA-03 | 문장·거래·client UNKNOWN 분리 | SQL-06·08·13·14 실행; 별도 fixture에서 commit 응답 유실과 engine sync 결과 불명 주입 | 문장 실패는 이전 변경 유지; 충돌은 전체 abort; client 불명과 engine 쓰기 차단을 별개 관측 |
| MA-04 | offline build 중단과 원자 공개 | drain timeout으로 build 요청 거절; reader 종료 후 SQL-24 build 실패 주입 | timeout이면 build 미착수; 실패 후 원래 행/catalog 유지; 임시 자원과 자기 admission만 해제 |
| MA-05 | pin과 quota의 자원 수명 | quota 초과, rollback, cursor close, backup 해제를 순서대로 수행 | 문장/거래 효과와 예약 반환을 ledger로 확인; 아직 pin된 자료 유지; 마지막 owner 해제 후 회수 |
| MA-06 | 설정 출처와 동시 변경 | 변경안 검토 후 revision 변경; 별도 TTL override 만료 중 기본값 변경 | 옛 revision 적용 거절; 저장/현재/대기/출처 구별; 만료는 현재 기반으로 재계산 |
| MA-07 | CLI 큰 값·NULL·중복 열 | JSON/JSONL로 내보내고 독립 decoder로 타입·순서·정확한 값 비교; 빈 결과도 조회 | 정수 문자열/타입으로 정밀도 보존; 모든 값 구별; 중복 열 보존; 빈 결과도 완료 표식 |
| MA-08 | 스크립트 중단·출력 장애 | 오류 중단 및 commit 후 broken pipe; 유한 JSONL request_end 전 중단 | 문장별 효과·미실행 범위 보고; 전달 실패와 committed 공존; 최종 표식 없으면 불완전 |
| MA-09 | driver batch와 표준 부작용 | autocommit 원소별 batch, 명시 거래 batch, 단일 다중 행 INSERT를 각각 실행 | 각 계약의 원소별 성공/실패/미실행·거래 상태 일치; 표준 mode 전환의 commit 효과도 공개 |
| MA-10 | pool 반환과 결과 자원 | 연결 반환 중 rollback/reset 실패와 disconnect 주입; 다음 사용자 checkout | 정상 반환만 clean state; 실패/UNKNOWN 연결은 재사용 금지; 자원 회수와 옛 handle 거절 |
| MA-11 | 권한 재검사와 인증서 교체 | 같은 관리 요청을 SQL/CLI/화면으로 실행; 교체의 정상·실패 경로 확인 | 철회된 관리 요청 거절; 비밀·객체 존재 노출 없음; 새 연결 인증과 기존 연결 적용 시점 보고 |
| MA-12 | 관측 시점·결측·reset | CLI/SQL/화면에서 같은 범위 조회; 이력 만료·권한 제한·수집 중단도 관측 | 다른 boot rate 계산 안 함; 같은 구간 3/초; p99/결측은 상태로 구별 |
| MA-13 | 진단 예산·중첩·묶음 | client 유실·restart·한도 초과 후 자기 trace 종료; 수집 일부 실패 bundle 생성 | 예산/손실/실제 종료 보고; 다른 수집 유지; manifest에 포함/제외/실패; 비밀 원문 fallback 없음 |
| MA-14 | backup cut와 디스크 저장 경로 복원 | C만 빈 목적지 복원; 별도 extended restore에 후속 closure 제공; 필수 COW 파일 누락도 주입 | C 복원=(2,2), 목표까지 복원=(5,5); 누락은 명시 오류·서비스 차단; 부분 거래 금지 |
| MA-15 | 보존 pin·공간 부족·재개 | pin 해제 순서를 바꾸며 회수; 공간 고갈; 다른 manifest/목적지로 resume 요청 | 필요 closure 보존; 신규 작업 거절과 이유; 불일치 resume 거절; 미완료 목적지 READY 금지 |
| MA-16 | 복제 handoff·재동기화 | read/sent 이후 영속 handoff 이전 장애; consumer restart; 초기 sync와 변경 stream 연결 | restart에 필요한 상태/WAL 유지; 누락/중복 없는 일관된 적용점; received/durable/applied 분리 |
| MA-17 | 장애 전환과 failback | 전환 도중 연결 유실; 격리 실패 경로와 성공 경로; 이전 노드 복귀 | 격리 불가 시 새 writer 개방 거절; 확인 적용점·유실 범위 보고; failback은 재동기화 후 |
| MA-18 | import chunk 실패·응답 유실 | 오류 중단; 다른 fresh fixture에서 chunk2 commit 응답 유실; 입력 변경 후 resume 시도 | 오류는 committed2/미반영2; 유실은 chunk2 불명; 입력/대상/규칙 바뀐 resume 거절 |
| MA-19 | 이관 값·snapshot·비교와 쓰기 | profile export/import와 독립 값 비교; compare만 실행; preview 후 동시 변경한 sync | profile round trip 동일; 다중 시점 한계 표시; compare 무변경; 오래된 sync 거절 |
| MA-20 | 내부 fresh와 고객 업그레이드 | 사내 기존 경로 생성 거절 확인; 고객 변환 manifest 전후 crash·새 버전 재쓰기/복구 | 사내는 호환 비요구·자동 삭제 없음; 고객은 완전 old/new 선택·ACK/논리 보존; 기준선 부재는 N/A |

<a id="section-4"></a>

## 4 제품 요구별 매뉴얼 위치와 남은 상세

16개 기능군의 현재 104개 요구 ID를 빠짐없이 연결한다. **초기 일부**는 최소 기능의 구체 초안만 존재, **절차 초안**은 운영 흐름과 실패 경계가 있으나 명령·수치·조합 미정, **주제 개요**는 후속 계약 질문만 작성된 상태다. 어느 것도 제품 지원·전체 leaf 명세 완성·기능 대체 PASS를 뜻하지 않는다.

각 행의 남은 인수 상세는 [제품 요구 원문](../plans/product-requirements.md)과 JSON의 `remaining_work`를 따른다. recovery/logging은 기존 조사에서 유보된 별도 설계를 따라야 하며 이 표가 해당 설계를 승인하거나 시험을 대신하지 않는다.

| 요구 ID | 기능 | 매뉴얼 장 | 문서 깊이 |
|---|---|---|---|
| R01-01 | 관계 조회와 중복·NULL 의미 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R01-02 | 내부·외부·교차 조인 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-03 | 상관 subquery와 lateral 조회 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-04 | 집계·다중 grouping | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-05 | window 분석 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-06 | 집합 연산·정렬·부분 결과 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R01-07 | 재귀·계층 탐색 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-08 | 행·열 전환 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-09 | 복합 DML·upsert·변경 결과 반환 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-10 | view·이름 간접 참조 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-11 | 조회 결과의 물리화·갱신 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R01-12 | 객체 생성·변경·삭제·메타데이터 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R02-01 | 정수·고정소수·근사수 표현 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R02-02 | 문자·국제 문자·길이 의미 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R02-03 | byte·가변 byte·bit 값 | [5장](user-manual-draft.md#section-5) | 초기 일부 |
| R02-04 | 날짜·시간·표현 변환 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R02-05 | 수치·문자·시간·변환 함수 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R02-06 | 암호·해시 관련 SQL 기능 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R02-07 | 조건·cast·정규식·표현식 | [5장](user-manual-draft.md#section-5), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R03-01 | 명시 transaction·autocommit·read-only | [6장](user-manual-draft.md#section-6) | 초기 일부 |
| R03-02 | statement 실패·savepoint | [6장](user-manual-draft.md#section-6) | 초기 일부 |
| R03-03 | READ COMMITTED·REPEATABLE READ·SERIALIZABLE의 업무 능력 | [6장](user-manual-draft.md#section-6) | 초기 일부 |
| R03-04 | 명시 잠금·대기 제어 | [6장](user-manual-draft.md#section-6), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R03-05 | 세션 상태·동시 사용 | [6장](user-manual-draft.md#section-6) | 절차 초안 |
| R03-06 | 구조화 오류·재시도 범위 | [6장](user-manual-draft.md#section-6) | 초기 일부 |
| R04-01 | 영속 DISK 데이터 접근 | [7장](user-manual-draft.md#section-7) | 초기 일부 |
| R04-02 | 비영속 저장 기능 | [7장](user-manual-draft.md#section-7), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R04-03 | transaction·session 임시 table | [7장](user-manual-draft.md#section-7), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R04-04 | 압축을 통한 공간 절감 | [7장](user-manual-draft.md#section-7), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R04-05 | 저장 공간의 배치·증설·상한 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R04-06 | table 이동·공간 재조직 | [7장](user-manual-draft.md#section-7), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R05-01 | range·list·hash 분할 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R05-02 | partition pruning | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R05-03 | 디스크 partition과 행 이동 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R05-04 | partition 추가·삭제·분할·병합·교환 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R05-05 | 분할 객체 관리와 관측 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R06-01 | 정렬·보조·복합 index 접근 | [7장](user-manual-draft.md#section-7) | 초기 일부 |
| R06-02 | primary·unique·NOT NULL 제약 | [7장](user-manual-draft.md#section-7) | 초기 일부 |
| R06-03 | 참조 무결성과 CHECK | [7장](user-manual-draft.md#section-7), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R06-04 | local·global index 범위 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R06-05 | index lifecycle·유효성·재구성 | [7장](user-manual-draft.md#section-7) | 절차 초안 |
| R07-01 | procedure·function·호출 인자 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-02 | 제어 흐름·예외·dynamic SQL | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-03 | cursor·record·배열·사용자 타입 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-04 | package·session 상태·의존 객체 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-05 | 독립 transaction을 사용하는 routine | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-06 | trigger | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-07 | sequence | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-08 | 작업 스케줄러 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-09 | 내장 package의 관리·통신 기능 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-10 | 온라인 객체 변경 능력 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R07-11 | transactional queue·메시지 소비 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R08-01 | LOB 값과 부분 읽기·쓰기 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R08-02 | 공간 값·교환 표현 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R08-03 | 공간 관계·분석·색인 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R08-04 | 공간 client API와 파일 적재 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R09-01 | C/C++ 호출형 client | [10장](user-manual-draft.md#section-10) | 절차 초안 |
| R09-02 | JDBC 표준 업무 | [10장](user-manual-draft.md#section-10) | 절차 초안 |
| R09-03 | ODBC 표준 업무 | [10장](user-manual-draft.md#section-10) | 절차 초안 |
| R09-04 | embedded SQL 개발 능력 | [10장](user-manual-draft.md#section-10), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R09-05 | PHP/PDO·.NET 및 언어 연결 | [10장](user-manual-draft.md#section-10), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R09-06 | XA·외부 transaction manager 연동 | [10장](user-manual-draft.md#section-10), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R09-07 | 큰/배열 인자·결과·취소 | [10장](user-manual-draft.md#section-10) | 절차 초안 |
| R09-08 | 도구·framework 상호운용 | [10장](user-manual-draft.md#section-10), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R09-09 | 로컬·원격 접속과 전송 제약 | [10장](user-manual-draft.md#section-10) | 절차 초안 |
| R10-01 | 외부 DB 조회·원격 실행 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R10-02 | 외부 C/C++ routine 호출 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R10-03 | 이종 DB 변경 전달 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R10-04 | 분산 분석 환경 데이터 교환 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R10-05 | 변경 분석·구독 기능 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R11-01 | 단일·다중 쓰기 노드 운영 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-02 | 동기·비동기 복제 모드 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-03 | 충돌 탐지·처리·관측 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-04 | 초기 동기화·부분 대상·재동기화 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-05 | 복제 중 schema·sequence 운영 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-06 | 접속 전환·failback·다중 주소 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R11-07 | 복제 관리·상태 조회 | [15장](user-manual-draft.md#section-15) | 절차 초안 |
| R12-01 | 백업·복원·시점 복구·DISK recovery | [14장](user-manual-draft.md#section-14) | 절차 초안 |
| R13-01 | 계정·인증·password 정책 | [11장](user-manual-draft.md#section-11) | 절차 초안 |
| R13-02 | 역할·객체/시스템 권한 | [11장](user-manual-draft.md#section-11) | 절차 초안 |
| R13-03 | 전송 보호와 인증서 관리 | [11장](user-manual-draft.md#section-11) | 절차 초안 |
| R13-04 | 데이터 보호·보안 모듈 연계 | [11장](user-manual-draft.md#section-11), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R13-05 | 감사 정책·조회 | [11장](user-manual-draft.md#section-11) | 절차 초안 |
| R14-01 | catalog·운영 metadata 조회 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R14-02 | 설정·관리 session·시작/종료 | [8장](user-manual-draft.md#section-8) | 절차 초안 |
| R14-03 | 대화형·batch SQL 도구 | [9장](user-manual-draft.md#section-9) | 절차 초안 |
| R14-04 | 논리 schema·데이터 export/import | [16장](user-manual-draft.md#section-16) | 절차 초안 |
| R14-05 | 이종 데이터·객체 이관 | [16장](user-manual-draft.md#section-16) | 절차 초안 |
| R14-06 | 데이터 비교·선택 동기화 | [16장](user-manual-draft.md#section-16) | 절차 초안 |
| R14-07 | 성능 진단·plan·통계 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R14-08 | plan/result cache 제어 | [12장](user-manual-draft.md#section-12), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R14-09 | 모니터링 API·알림·SNMP 연계 | [12장](user-manual-draft.md#section-12), [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R14-10 | 보호된 진단·관리 도구 묶음 | [13장](user-manual-draft.md#section-13) | 절차 초안 |
| R14-11 | 외부 관리·directory 연계 | [18장](user-manual-draft.md#section-18) | 주제 개요 |
| R15-01 | 대표 업무 corpus와 결과 기준 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R15-02 | 처리량·tail latency·동시성 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R15-03 | 표현·구조적 용량 경계 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R15-04 | 큰 데이터·bounded resource | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R15-05 | 혼합 workload·운영 중 기능 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R15-06 | 성능 판정의 사전 등록 | [12장](user-manual-draft.md#section-12) | 절차 초안 |
| R16-01 | Linux 설치·배포·제거 | [3장](user-manual-draft.md#section-3) | 절차 초안 |
| R16-02 | 사내 새 구축과 재현 | [16장](user-manual-draft.md#section-16) | 절차 초안 |
| R16-03 | 안정 릴리스 이후 XADB 버전 계약 | [16장](user-manual-draft.md#section-16) | 절차 초안 |
| R16-04 | 사용자 문서·기능 대응표 | [19장](user-manual-draft.md#section-19) | 절차 초안 |

문서 검증은 ID·참조·표·상태·예제 지문이 서로 일치하는지 확인한다. 인수 사례의 실행, 실제 값·권한·내구성 검증, usability 평가를 수행하지 않는다.
