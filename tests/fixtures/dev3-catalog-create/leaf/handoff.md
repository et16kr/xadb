# 독립 입력 전달

2026-10-25 · `/root/dev3_review` 작성. 독립 raw 생성 1회 성공, 제품·compiler·시험 runner 호출 0회. `generation.log`가 최초 생성 출력이다.

- raw 119개, 1,296,915 bytes.
- manifest SHA256: `84b8aa4c0607d314dc373a956e6d75c7dcdca701209ef18ab0470c23826a6d5d`.
- index SHA256: `728977808b9c1abd66939bce4989c487d7c3ebc6c71182b97222419dd113b35c`.
- generator SHA256: `67d8168a2f8596aa5e00af30265a8ae12388d0d2688fe807555f4db141882eaa`.
- plan SHA256: `60845e2807c7093b844c0728a5c092680133e41126f8d4696bcb1e28b789ca51`.

국소 실행 후보는 core 정상20+형태 거절34=54, projection 정상6+값/context 거절12=18, wire shape 정상17+재봉인 거절patch18=35로 총107이다. 여기에 별도 stable semantic 의무4개(NC09/NS01..03), Catalog value/cell support8개를 합하면 raw119다. 이 수는 이미 실행한 제품 case 수가 아니다. 실제 source·selector·정확 result/cause 요구·자원 상한을 root가 동결해야 실행 가능하다.

`P*` raw는 해시할 projection 원문이다. value 인자는 manifest의 value_file을 사용하고 Missing은 None을 전달한다. expected object/namespace는 manifest의 외부 값으로 공급한다. NP10..12는 context 부정이며 NP11/12의 빈 raw를 Some(empty)로 해석하지 않는다.

`C*`/`NC*`는 decode_core의 input이며 expected operation을 manifest에서 별도로 공급한다. NC09는 정상 typed slot1이므로 core 구조 통과 후 실제 slot0 source 대조에서 실패하는 의무다. 이를 core 거절 선택에 넣지 않는다.

`W*`는 shape-only LayoutView 입력이다. expected stream은 DB bytes1..16/timeline1/epoch1/previous100, supplied Profile는 same namespace의 format2를 사용한다. Profile encoder를 기대값 generator로 쓰지 않는다. 완전한 before state와 original ledger는 없으며 descriptor/native sentinel은 유효 의미를 가장하지 않는다.

`NW*`/`NS*` raw는 작은 대체 byte 문자열이다. base W frame을 복사하여 manifest offset에 치환하고, XWF header의 앞128B SHA를 [128,160)에 다시 쓰고 마지막32B에 prefix 전체 SHA를 쓴다. 최종 materialized_sha256와 대조한 뒤 소비한다. 그 외 frame byte나 길이 field를 임의 변경하지 않는다. NS01..03은 현재 LayoutView 이후의 native/inventory 의미 거절 의무다.

추가 source 읽기에서 같은 physical cell에 다른 native kind를 두면 tuple 정렬만으로 통과할 수 있는 사례를 root에 전달했다. 해당 proof 범위의 결정·원 source 수정 뒤 별도 새 입력으로 추가하며 현재 frozen 원자료는 변경하지 않는다.
