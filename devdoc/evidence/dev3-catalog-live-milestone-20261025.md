# DEV3 Catalog CREATE 구현 중간 저장

2026-10-25 실행 단위 · 2026-10-26 중간 저장 · 최소 Catalog CREATE/Commit/새 프로세스 복구·조회 범위의 검토된 구현을 저장한다. 첫 Row INSERT는 시작하지 않았다. **이 커밋은 전체 DEV3·S5 또는 현재25항목 inventory의 완료 선언이 아니다.**

## 구현과 완료한 검증

원 Owner/WAL/pool에 CREATE의 Begin·Reserve·Forward·Commit을 연결하고, 취소·rollback·CLR·정리와 v3 Base/scope2 복구를 인계했다. page 예약·실제 reader LOAD·visibility·retirement·Commit의 기존 권한과 backing을 사용한다. cap은 source65,536/metadata196,608/whole262,144 그대로다.

- N01: r29의 실제3개 process에서 T Commit9/외부 ACK 뒤 강제종료, 새 process의 복구·조회/U Commit19/외부 ACK 뒤 강제종료, 세 번째 process의 복구·T/U 조회와 정상종료를 기록했다. 마지막 Base3 판정은 checker가 model cycle2를 actor local2로 오해해 FAIL이었다. 동결 계획은 epoch2/local1이며 raw actor도 같다. 독립 검토 뒤 source_local과 INIT provenance의 기대값만 수정하고 원 bytes를 읽기 전용으로 감사하여 PASS했다. 새 제품 실행0회, 원 FAIL 유지, 감사0.795초 및 원 실행22.908초와 합쳐30초 이내다.
- N02: r32에서 Forward8의 DWB Pending/home 설치 전 종료 → 새 process의 durable CLR10 직후 종료 → 재복구/조회 Missing까지15.326초 PASS.
- N03: r32에서 Commit9의 실제 sync 뒤 오류/UNKNOWN·ACK 없음 → 강제종료 → 새 process의 winner 복구/조회까지11.856초 PASS.
- 실제 메모리14개 고유 사례 PASS: MC01·MC02·MC04, MF02–MF12. 예약 취소·재사용, rollback/CLR/정리, Reserved Drop fence, write/sync 오류·중단·부분 쓰기, Commit 중간 unwind와 fresh recovery를 포함한다. MF10 r29 FAIL은 보존한다. 원 install_page는 bytes와 pageLSN을 함께 변경하므로 두 번째 page의 pageLSN9/이전 counter·DPT 기대를 바로잡은 r32에서 남은 단언까지 통과했다.
- 검사기 새 반례2개 PASS. 기존14개와 완료한 하위 시험은 반복하지 않았다. commit 후보의 별도 index export는 컴파일 성공, 사용하지 않는 import/변수 경고8개가 남았다. 실제 실행 결과는 r28/r29/r32 원 source·ELF에 귀속하며 commit 후보의 새 runtime PASS로 바꾸지 않는다.

## 남은 범위와 저장 경계

MC03, MT01–MT07, MF14의9개 실제 입력은 NOT_RUN이다. MF01/MF13은 원 single-writer/private-slot 경로에서 도달할 수 없어 UNREACHABLE/NOT_RUN이며 PASS로 세지 않는다. 전체 runtime F05 인수도 아직 완료하지 않았다.

이 커밋은 검토된 r29 구현과 r32의 좁은 MF10 기대 수정 및 실제 검증 소스를 포함한다. 이후 작성한 CatalogPreparation/Writer 준비 API와 MC03 초안은 작업 트리에 보존하고 포함하지 않는다. 원 실패·중단·미실행 결과를 삭제하거나 소급 합격으로 바꾸지 않는다.

## 검토와 재현 증거

[세부 source·ELF·실행·검토 해시](dev3-catalog-live-milestone-20261025.json)에 실제 파일과 근거를 기록했다. 초기 slot 없음의 정상 상태 처리, CREATE7개 target 동시 보호, 복구 allocator 재로드 제외/CLR9개 target 보호를 각각 독립 검토했고, root가 cap 유지와 원 authority 조건을 대조하여 채택했다. checker actor remap와 MF10은 제품 동작 변경이 아닌 동결 계약에 맞춘 기대값 수정으로 D2 채택했다.

고정 원본은 `artifacts/dev3/catalog-create-20261025/integration-r28`, `integration-r29`, `integration-r32`와 `implementation-commit-r0`에 있다. 원 ELF·source manifest·로그·DB snapshot은 보존한다. 사용이 끝난 compiler target만 해시 확인 뒤 제거한다. [선행 독립 입력/제어기 증거](dev3-catalog-input-controller-20261025.md)와 [순수 전이 증거](dev3-catalog-transition-20261025.md)는 원 범위 그대로 유지한다.
