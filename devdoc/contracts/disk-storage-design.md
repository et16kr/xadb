# 디스크 저장 엔진 기준

2026-10-18 · **XADB-SCOPE-001 적용 / 상세 페이지 형식 미동결 / 미구현**

[커널 계약](implementation-contracts.md) · [DEV2 설계](dev2-kernel-design.md) · [복구 계약](recovery-logging-design.md) · [인수 항목](disk-test-matrix.json)

## 경계

제품 storage는 DISK 하나다. K0의 volatile row/catalog는 direct API·snapshot·quota와 독립 이력 비교를 위한 현재 구현이다. 현재 배열 복사·coarse validation 비용은 정확성 개발의 초기 제약이며 디스크 성능 기준선으로 주장하지 않는다. K2–K5에 실제 WAL·파일·checkpoint를 연결하고 DEV3에서 bounded page buffer와 disk index를 완성한다.

## 행·페이지·인덱스

논리 row identity는 물리 page 주소와 분리한다. table/catalog generation과 snapshot을 검증해 stale handle을 거절한다. page header·checksum·length·type·generation과 key ordering/NULL 의미를 byte vector로 동결한다. 행 변경과 PK·unique·nonunique index, locator 변경은 같은 commit에 속한다. 전체 disk locator를 RAM에 강제 상주시키지 않는다.

초기 방향은 private 변경과 immutable/COW publication이다. 변경 page·namespace의 내구성 의존성이 complete commit보다 먼저 충족되어야 한다. 실제 형식·write amplification과 recovery 비용을 검토한 뒤 구현한다. 이후 WAL-first/in-place 대안을 채택하려면 별도 설계 검토와 실제 crash 검증이 필요하다.

## 자원과 수명

page buffer·locator cache·dirty/prepared page·snapshot pin·WAL·checkpoint scratch·recovery 공간을 별도 계상한다. eviction은 pin과 durable dependency를 깨지 않는다. 공유 물리 자원은 중복 계상하지 않고, 예산 초과 시 bounded backpressure 또는 명시 오류를 반환한다. 정상 버퍼 사용과 별도 resident DBMS 제품을 혼동하지 않는다.

## 파티션·global index

PART는 disk partition routing/pruning·local index·행 이동을 구현한다. NG는 필요할 때 disk-buffered global index를 추가한다. media 선택 옵션은 없으며 partition membership·행·index 갱신과 unique 검사는 같은 snapshot/commit 계약을 사용한다. PART/NG는 최초 embedded SQL의 선행 조건이 아니다.

## 검증과 성능

실제 I/O witness가 있는 buffer 4배 이상의 working set, 순차/랜덤 접근, cold/warm workload를 사용한다. 처리량뿐 아니라 p99·RSS·page I/O·쓰기 증폭·checkpoint/복구 비용을 비교한다. 캐시 적중만으로 저장·복구 정확성을 주장하지 않는다. 신규 disk 경로에서 DEV2의 ACK/UNKNOWN/atomicity·crash/reopen 인수를 다시 실행한다.
