# XADB 초기 제품 기준선

2026-10-18 · **범위 채택 / 구현·검증은 단계별 구분**

[제품 방향](project-direction.md) · [범위 JSON](phase1-scope.json) · [개발 단계](development-stages.md) · [기능 목록](capability-inventory.json)

<a id="section-1"></a>
## 1. 제품 범위

Linux/Rust 단일 프로세스의 디스크 관계형 DBMS로 시작한다. 단일 DB 안의 여러 table·index·catalog 변경은 한 transaction에서 원자적이어야 한다. 버퍼와 locator cache는 명시된 예산 안에 있으며 전체 DB가 RAM에 들어간다고 가정하지 않는다. 선언한 SQL 기능은 독립 expected result와 실제 저장 경로에서 검증한다.

<a id="section-2"></a>
## 2. 공통 불변식

ACK는 선언한 fault model에서 complete durable commit을 뜻한다. commit 결과 불명은 UNKNOWN으로 표시하고 writer를 fence한다. snapshot은 한 committed view를 고정한다. row·index·catalog·locator를 따로 공개하지 않는다. WAL·checkpoint·backup은 명시적 형식과 integrity 검사를 갖추며 required corruption을 조용히 건너뛰지 않는다.

<a id="section-3"></a>
## 3. 기능 개발 범위

| 단계 | 범위 |
| --- | --- |
| DEV0/DEV1 | 독립 검증 도구, 공통 ID·값·오류·codec·Linux 파일 효과·자원 경계 |
| DEV2 | 최소 행/catalog와 transaction·WAL·recovery·checkpoint/restore |
| DEV3 | 실제 디스크 페이지·bounded buffer/locator·PK/unique/nonunique index |
| DEV4/DEV5/DEV6 | embedded SQL, 서버 조립, native/JDBC/ODBC 클라이언트 |
| DEV7/DEV8 | SQL·타입·실행 확장, 복제·운영·성능·복구 기능 |
| PART/NG | 디스크 파티션, 필요 기능으로 명시한 disk global index |
| DEV9 | 선언한 제품 capability와 운영·성능의 인수 |

<a id="section-4"></a>
## 4. 디스크 qualification

행·index·catalog의 원자 변경, 실제 파일 sync와 재시작, snapshot lifetime, 손상/부분 쓰기/자원 고갈, checkpoint와 동시 변경, backup/restore를 포함한다. DEV3 working set은 buffer의 최소 4배로 고정하고 실제 I/O witness와 page/locator/RSS 예산을 확인한다. 이 비율은 시험 기준이며 제품 최대 용량이나 성능 보장이 아니다.

<a id="section-5"></a>
## 5. 공개·호환

개발 버전은 새 DB·fixture를 사용한다. 연구용 소스 공개에는 실제 상태와 제약을 명시한다. 안정 릴리스의 이전 형식·SQL·client 호환 기준선은 [버전 정책](../policies/version-compatibility-policy.json)으로 별도 선언한다. 개발 저장소는 공개하지만 안정 배포·운영 인수 시험은 이번 이관으로 완료되지 않는다.

<a id="section-7"></a>
## 7. D0의 일곱 경계

D0는 volatile K0에서 디스크 저장으로 발전할 수 있는 계약 경계를 검증한다. 아래 일곱 항목은 각각 코드/API, 독립 fixture, 실패 반례를 연결해야 한다. 실제 디스크 구현·복구 시험은 DEV3/H1의 별도 의무다.

| 경계 | 필수 조건과 검증 |
| --- | --- |
| D0-1 논리 정체성 | RowUID는 포인터·page/slot과 분리한다. 재사용·drop/recreate의 incarnation을 검증하고 오래된 ID가 새 행을 가리키지 않는다. |
| D0-2 snapshot 수명 | 행 handle은 snapshot에 묶인 opaque 접근이다. executor와 index가 volatile 배열이나 물리 포인터 수명에 의존하지 않는다. |
| D0-3 index 범위 | LOCAL/GLOBAL 논리 범위와 물리 index 구현을 분리한다. global unique의 사용자 key에 partition ID를 붙여 중복을 허용하지 않는다. |
| D0-4 원자 공개 | row/index/catalog/locator/partition 변경은 한 commit view로 공개·복구한다. 부분 공개·부분 복구를 거절하는 fixture를 둔다. |
| D0-5 형식·capability | WAL/catalog/backup의 version·capability를 명시하고 미지원 필수 형식을 거절한다. 개발 빌드는 새 fixture를 쓰며 안정 릴리스만 호환 기준선이다. |
| D0-6 RAM 상한 | K0의 O(N_rows) 임시 행 map이 디스크의 O(N_disk) 상주 locator 요구로 굳지 않는다. buffer/locator/pin/작업 메모리의 예산과 실패 소유자를 구분한다. |
| D0-7 독립 검증 | 계층 의존과 API 경계를 검사하고 production 알고리즘과 독립된 fixture로 검증한다. mock·volatile 경계 시험을 실제 disk 지원 PASS로 기록하지 않는다. |
