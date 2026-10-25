# DEV3 실제 CREATE의 exact-image 복구 base

2026-10-25 · `D3-CATALOG-CREATE-001` 결합 D2 채택·구현 착수 수용 · [실제 소비 계약](dev3-catalog-create.md). 제품 효과·실행 인수 미완료.

2026-10-25 · /root · 결합 D2 채택 정본. 실제 source F05·입력 동결 전 제품 효과·시험 미입장. fresh v2 Root/Job/profile와 actual CREATE prefix의 selected source owner를 전제로 한다. pure bytes decode는 선택·복구·INIT·page reuse 권한이 아니다.

## 고정 배치와 복구 source

XRB1 version3, 정확2,097,152B. 기존 Header의 namespace/profile/FileMap/bootstrap identity, generation/previous, capture/sourceepoch/offset/prefixdigest/localhigh, systemhigh, bank/census/rootrevision/Aginggroups, selectedDWB/generation, remaining/physical/issuedepoch/sourceheaderdigest 의미를 유지한다. v3 section directory와 모든 미사용 영역을 명시하여 old section offset으로 암묵 읽지 않는다. 각section은 전체digest를 가지며 전체trailer는마지막32B, CURRENT는전체파일SHA와exactlength를결합한다.

| section | offset | bytes | count/width |
| --- | ---: | ---: | --- |
|header/directory|0|4096|1|
|descriptor bank|4096|55296|36×1536|
|page certificates|65536|6144|64×96|
|INIT provenance|73728|22528|64×352|
|ownership inventory|98304|16384|원 XOI1 root+boundedchunk 배치|
|exact image arena|262144|1048576|64×16384|
|trailer|2097120|32|1|

v3 section directory는 기존 시작 offset1280과 entry64B layout을 유지하고 정확5개다. table 순서로 id1 descriptors, id2 certificates, id3 INIT provenance, id4 inventory, id5 images를 둔다. 여섯 번째 entry는 없으며 `[1600,4096)`은 전부zero다. 기존 header scalar offset은 유지한다. header240의 object-high5는 immutable bootstrap object high이며 현재 dynamic ObjectHigh는 exact Root400에서 검산한다. active_transactions는 TerminalLeased 수와 대조한다. RootReserved도 leased이므로 전체 leased bitmap popcount와 ATT 수의 equality를 계승하지 않는다.

나머지gap은zero. allocator ordinal0, Root1, Jobs2..6, CatalogAnchor7, CatalogLeaf8와Allocatedpayload의exactimage를ordinal로직접배치한다. Free/Reserved page의arena slot/certificate/provenance는전부zero. allocatorimage가논리identity와membership의정본이며physicalordinal만으로권한을주지않는다. Root/queue/Catalog를legacygenesis로재생성하거나home의현재값을과거capture값으로재사용하지않는다. decodedbase는borrowed view, image재적재는같은bytes를검산해caller의원pagearena로복사한다.

Certificate96은현형의present/kind/level/PageRef/owner/ownergen/pageLSN/fullSHA를유지한다. v3는정적owner맵에더해원Catalog에서확인된dynamicTable Anchor/RowRoot만허용한다. fixedpage0..8의role/identity는immutableplan과같아야한다. payload9..63의Status owner2/Undo owner3는원systemINIT, dynamicpair는exactobjecthandle과원ForwardINIT에결합한다. genericPageKind를그냥허용하지않는다.

INIT provenance352는v2의압축Statusbody/FreeUndorecipe로해석하지않는다. v3의정확offset은아래추가표를따른다. 초기고정page0..8은genesis, 후속Status/Undo는systemINITscope1, tablepair는CREATEscope2동일Forward2INIT의원Tx/object/pair에결합한다. base observer의self-sealedCertificate만으로원INIT를발급하지않는다. selectedowner가직전acceptedbase또는retainedWALsemanticReplay에서원INIT를추적해다음base로인계한다.

## Reserved inventory와 before/after closure

첫소비자의liveoriginalbound는exclusive linear CatalogCreate reservation 하나를보유할수있다. 그원token이&mutPlan4Retention을빌리므로동시Systemreservation/두번째CREATEreservation을만들수없다. 따라서v3선택inventory상태는None, 기존System count1, 또는동일Tx의CatalogCreate(PageToken2+UndoSlot1)이다. 기존Systempath는기존의originalSystem owner/gen검산을유지한다. 이한정은영속input에도정확히적용하고그밖의validgeneralXOI1를지원된CREATEprefix로받지않는다. 후속다중statement/병렬creator는실제consumer와함께확대한다.

CatalogCreate Reserved의각page는원 allocator Reserved(claimant=Tx,generation=reserved incarnation)이며image가없다. Reserved allocator entry에는 kind/object/pair field가없다. 그역할과exactobject/gen/ordinalpair는RootJobReserved+Tx inventory+immutable semantic plan의교차검산으로확인하며allocatorwire를늘리지않는다. 원RootJobReserved는같은pair와plannedU/seq를갖는다. Undo container는Allocated(owner3)이며exactimage가Free(g2)다. 그Free bytes가예약자체는아니며UndoSlot inventory항목+Job+Txdescriptor가예약권한의표현이다. U page/slot/generation,SourceStatus/Txn,Roothigh가모두일치해야한다. 원XOI1 checker로ownerindex/physicaloverlap/totalhash를검사하고typedconsumer가3항목의분류와budgettoken2+1을추가검산한다.

Forward after는세inventory항목0, 두pairAllocated/두INITimage, Catalogactorstamp/undoU, StatusheadU/incoming1, UndoLiveincoming2, RootJobRetained와Agingdebt가함께있다. Cancel after는pairFree(예약incarnation유지), UndoFree(g2)와RootJobFree, 발급high는유지한다. Reservedrecovery는영속Job/descriptor/inventory로originalrecoveryowner의sealedcancel경로를만들어Cancel을기록·설치한다. 일반decode에서livePageToken을위조하지않는다.

CLRafter는Catalog없음, pairFree, Statushead/nextNone와incoming0, U Releasing0/incoming0, RootJobReleasingCompensated이다. pairarena/certificate/provenance가이미지없음으로닫혀야하며선택된PendingDWB/oldimage를새Freepage의권한으로받지않는다. Jobpairfield0은해당CLR원자반환이채택된뒤만가능하다. Commitafter는Catalogactorstamp/undoU와LiveU를그대로보존하고StatusCommitted(q)/RootC≥q/TerminalReady、Agingdebt만바뀐다. Pruneafter는CatalogCommitted(q)/undoNone, StatusheadNone/incoming0, UReleasing0와RootJobReleasingCommitted다.

Root/Job/Status/Undo/nativeedge의양방향census, actualCatalogobject→exactAnchor→emptyRowRoot관계, Roothigh/CommitC와각actor/local/seq의상계를검산한다. 각actualUndoFieldmultiset→counts를계산하되Jobcleanupref는incoming에넣지않는다. firstCREATEprefix에는index0/RowRootempty/inlineMissing한root만허용한다. source에서아직지원하지않는row/change종류는해당effect전에거절하고validgeneralbytes를지원된base로주장하지않는다. 다음별도Row INSERT가공급되면그소비자diff와함께이grammar를확장한다.

## capture/selection/physical/epoch 수명

현재originalrecoveryowner의quiescent admitted capture를먼저연결한다. immutablecaptureprefix와동일한bank/inventory/64arena이미지로stagingbase를완성하고검산·sync한뒤CURRENT교체/dirsync한다. livehomes를나중에재읽어capture를보충하지않는다. 일반fuzzycapture는별도후속의무다. selectedbase만oldWAL제거를허용하며선택/epoch교체/dirsync불명은기존fence와debt를유지한다.

physical repairarena는1MiB를유지하고BASE_BYTES2MiB와상수를분리한다. old/new두base를원12MiBmaintenance안에배치하며총namedend7,274,496B/잔여5,308,416B 후보를source별재검산한다. retaineddataframe/after/before425984B는별도originalcallerloan이고syncPending때sharedmaintenance/FIFOturn을점유하지않는다. component65536/metadata196608/whole262144의source-valuecap은증액하지않는다.

현재Profile만XDCversion2,ID1/2/3의기존buffer/owned8GiB/protected1GiB/기타튜플은유지하고base@168을2MiB로정확고정한다. FileMap등다른ControlKind의version을변경하지않는다. Profile typed값은version을보존하고currentfresh/selection에서명시v2를소비한다. oldv1profile/basev1v2purecodec·원증거는보존하며currentruntime에서암묵continued-write하지않는다. 두basephysicalcharge는2×2MiB로한번추가회계하고CURRENT/epochencodedlength/selectionstat도2MiB를검산한다.

이후같은ELF의native producer/recover/reentry에서실제CREATECommit→handle읽기→nonemptybase선택→oldWAL실제unlink/dirsync→재장애를검증한다. loserReserve/Forward/Commitdurability/installcuts도원prefix와sameELF로검사한다. 실행입력/independentliteral/sourcebound/자원한도동결은아직전이며이문서는PASS증거가아니다.

## INIT provenance352 exact candidate

ordinal0..8은record352 전체zero이며originalimmutablegenesis의membership/role/identity provenance를검산한다. 현재Root/Jobs/Catalog/allocator bytes는변경될수있으므로현재certificate/image SHA를genesis와같게요구하지않는다. 현재certificate/image는acceptedcapture의정확변경bytes로별도검산한다. payload의Allocatedpage만다음presentrecord를가지며Free/Reserved는zero다.

| offset | bytes | 의미 |
| ---: | ---: | --- |
|0|1|present1|
|1|1|acceptedrole: Status1, Undo2, TableAnchor3, RowRoot4|
|2,3|1each|actual PageKind, level0|
|4|1|source owner tag: System1 / Transaction2|
|5|3|zero|
|8|24|exact PageRef|
|32,40|8each|object owner ID/generation|
|48|8|original initialization group LSN|
|56|4|original initialization writer epoch, nonzero|
|60|4|zero|
|64|32|whole original INIT/Forward WAL frame SHA|
|96|32|exact original initial page SHA|
|128|48|source SystemWorkRef or TxnRef, tag4 selects existing codec|
|176|8|original Reserve group LSN, 0<reserve<init|
|184|24|dynamic ObjectHandle(object/generation/schemaepoch); System roles zero|
|208|48|table Anchor/RowRoot PageRef pair; System roles zero|
|256|96|reservedzero|

Systemroles는source SystemWorkKind::Allocation/원namespace와object2또는3/gen1, canonicalinitialStatus/FreeUndo생성image/INITscope1을검산한다. Txroles는sourceepoch=TxnRef.epoch, sourceidentity/currentnamespace, dynamicObjectHandle(object≥6,gen1,epoch1),pair2의상이한physicaladdress와row/anchor역할,같은ForwardframeSHA/epoch/initLSN/reserveLSN/sourceTx/object/pair의일치를검산한다. 현재certificate의pageLSN≥initLSN이고현재arena이미지는그certificate와동일하다. provenance의initialimageSHA와현재imageSHA는변경뒤서로달라도정상이다. 초기imageSHA는originalsemanticINIT 또는previousacceptedbase에서온claim이지현재page의자기봉인hash를수용한것이아니다.

과거sourceTx/epoch/Reserve/INITidentity는historicalprovenance이며지금Status/Job/Undo를pin하거나재조회할권한이아니다. Terminal/RootJob이정상회수된후에도validtablepair의provenance는유지한다. pair반환은같은adoptedCLR에서두record를zero로만들고재할당에는새incarnation/provenance가필요하다. 현재scope에는drop/recreate가없다.

원 recovery 내부 전달도changed8과initialized2의고정배열+실제count로확장하고INIT순서를exactForwardpair와검산한다. 원Targetrow는owner/gen16B를추가한72B로하여동적Tableidentity를kind로추론하지않는다. 이는디스크wire가아닌기존calleranalysislane의internalrepresentation이며certificate6144+targets4608=10752B로기존16KiB영역내다. source-valuecap은새배열/Result를포함하여재검산한다.
