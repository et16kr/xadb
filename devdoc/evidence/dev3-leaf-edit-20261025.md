# XADB 첫 행 공급 — bounded leaf 후보 편집

2026-10-25 · **D2 수용·신규 선택 검증 완료 / 최종 독립 D1 수용 / 첫 행·DEV3/S5 미완료**

[계약](../contracts/dev3-tree-format.md#leaf-edit) · [기계 기록](dev3-leaf-edit-20261025.json) · [트리 소비자](../contracts/dev3-task-06-tree-rows.md#leaf-edit-supply)

기준 commit은 `688b9af`, 작업·계약·통합은 `/root`, 별도 검토는 `/root/dev3_review`다. 사용자는 DEV3 완료·단계별 commit·사용 완료 자료 정리를 요청했고, 진행 중 가능한 병렬 작업을 추가 요청했다. DEV2와 카탈로그 reader의 병렬 변경은 이 단위의 diff에 포함하지 않는다.

첫 Catalog CREATE/Row INSERT와 해당 CLR가 소비할 `TreePageView::prepare_leaf_edit`를 추가했다. caller-owned 참조 scratch로 Insert/Replace/Remove 후보를 만들고 기존 codec과 byte-balanced split을 소비한다. 원본·정체성·정렬을 보존하며 중복/부재/잘린 입력/공간 부족에서 scratch와 output을 보존한다. Remove는 사용자 DELETE의 tombstone을 대체하지 않는다. 새 pool·I/O·WAL 권한·dependency·wire format은 없다.

내부에1013개 참조 배열이나 page 복사를 숨기는 대안 대신 caller scratch를 선택했다. source는 immutable borrow, scratch는 plan 수명에 묶이며 일반 producer가 자기 pin/latch·before·LSN을 검증해야 한다. 최대 backing57066 bytes와 typed carrier128/24/104/104를 구분해 기록했다. 이는 일반 opcode의 전체 자원 인수가 아니다.

| 검증 | 실제 결과와 범위 |
| --- | --- |
| 제품 | 새 test5 PASS. 세 family 처음/중간/끝·empty, duplicate/missing·internal·malformed, scratch 경계1012/1013/1014, C/H 경계, overflow split·원본/output 보존.64 seeds×128 operation×3family=24576 이력 동작 |
| oracle | 수동 wire-envelope와 literal cell, typed BTreeMap 기대값. 제품 page/tree encoder나 편집 mapping으로 expected를 만들지 않음 |
| borrow | 원본 source·입력 cell·scratch 각각 positive/negative 및 baseline, 총7 PASS. 실패 code와 정확 primary span 대조. 원 source와 probe patch/log 보존 |
| 도구 | 새 inventory missing/duplicate/ignored/failure 거절, 새 compiler probe code/span 결합, layout witness의 interleaving/중복/누락/값 변조 거절, 고유3 PASS |
| 정적 | 변경 tree source rustfmt PASS. 보존 source의 engine lib/tests/all-features Clippy `-D warnings` PASS. 제품 사례 재실행0 |
| 최종 | S5 확대256×1024×3는 별도 ignored selector로 작성, NOT_RUN. 첫 행·live writer·복구 연결은 미완료 |

원 자료는 `artifacts/dev3/leaf-edit-20261025/`에 있다. source 지문은 `1cbed969ae706ccf7e315a35bdd0873a3c11d4865bcb9e19f92b2c84e0e79494`, 실제 binary SHA-256은 `28f752b6734d4b5cf8735523a1ed72f908046186205688323ae6413e186de9f0`이다. 신규 helper/checker는 JSON의 별도 파일 hash로 결합하며 이를 당시 제품 source 지문에 소급 포함하지 않는다. generator 규칙·seed 목록·시험 source hash는 실행 전 `frozen-input.json`에 고정했다.

실패를 삭제하지 않았다. 최초 compile은 시험의 임시 key 수명 E0716로 거절되어 local binding으로 수정했다(`build.log`→`build-r1.log`). 최초 generic 결과 검사기는 nocapture layout 출력이 named-result 줄에 끼어든 원 로그를 거절했다. 정확 witness만 검산·제거하는 새 검사로 원5개 결과를 확인했으며 제품을 반복 실행하지 않았다. live checkout Clippy는 병렬 reader의 아직 미완성 source를 읽어 실패했다(`clippy.log`). 원 leaf source 사본의 Clippy는 통과했으며 병렬 reader의 완료 근거로 확대하지 않는다.

검토자는 D2에서 최대 scratch·LSN 권한·오류 효과를 수용하고 seed 사전 동결과 output 생존량 범위 정확화를 요청했다. 이를 반영했다. 1차 D1은 제품 diff·독립 기대값·원 source/binary·고유5 PASS를 직접 확인하여 차단 결함 없음으로 보고했다. 최종 D1은 checker/borrow·최종 inventory와 source·binary·로그 해시를 직접 대조하여 필수 수정 없음으로 수용했다. 병렬 카탈로그 변경은 검토 범위에서 제외했다.

컴파일 probe의 소유 임시 source·target166384441 bytes를 종료 시 제거했다. 원 source·binary·실패·입력·probe patch·raw log는 보존한다. 시작 여유는 약132GiB이며 기존 증거·다른 작업 산출물은 크기만으로 삭제하지 않았다.

다음 소비는 동일 authoritative owner의 카탈로그/행 변경과 live Undo/status·terminal·N>0 recovery다. 표현 후보가 검증됐다는 이유로 실제 DB CREATE/INSERT/COMMIT/reopen이나 DEV3 S5를 완료 처리하지 않는다.

독립 리뷰 뒤 root 전용 Cargo target의 재생성 자료113782993 bytes도 제거했다. 보존 ELF는 별도 copy로 inode가 다름을 확인했으며 다른 담당 target은 건드리지 않았다.
