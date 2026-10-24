//! New selection and pre-effect seal inputs. Fixtures come from actual owners;
//! byte mutations use fixed independently described offsets and SHA only.
use super::literals::{Oracle, put, seal};
use super::support::*;
use super::*;
use xadb_foundation::namespace::NamespaceErrorKind;

fn get(raw: &[u8], at: usize, width: usize) -> u64 {
    let mut out = [0; 8];
    out[..width].copy_from_slice(&raw[at..at + width]);
    u64::from_le_bytes(out)
}
fn established() -> Images {
    let d = clone_directory(&fresh_memory(4));
    let state = d.state();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    {
        let runtime = actual_runtime(&d, &mut maintenance, &mut dwb, &mut slot);
        assert_eq!(runtime.observation().source_lsn, 4);
    }
    assert_closed(&state);
    images(&state)
}
fn paired_base_only(files: &mut Images) {
    let current = files.get_mut("CURRENT").unwrap();
    for at in [256, 512] {
        assert_eq!(current[at], 2);
        current[at + 1] = 0;
        current[at + 4..at + 8].fill(0);
        current[at + 64..at + 192].fill(0);
    }
    current[92..100].fill(0);
    current[104..224].fill(0);
    seal(current);
    files.remove("d3-wal-00000002").unwrap();
}
fn rejected(
    directory: &port::Directory,
    state: &State,
    check: impl FnOnce(&Plan4RecoveryError, &Plan4RecoveryObservation),
) {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let failed = match Plan4Recovery::acquire(
        directory,
        Some(namespace()),
        &mut maintenance,
        &mut dwb,
        &mut slot,
    ) {
        Err(Plan4RecoveryAcquireFailure::Acquired(failed)) => failed,
        Err(Plan4RecoveryAcquireFailure::Before { error, .. }) => {
            panic!("unexpected prelease error: {error:?}")
        }
        Ok(owner) => match owner.recover() {
            Err(failed) => failed,
            Ok(_) => panic!("negative recovery input succeeded"),
        },
    };
    assert_eq!(failed.error().effect, Effect::CallerMustResolve);
    assert!(failed.error().lease_acquired);
    assert!(failed.owner.fence.is_fenced());
    check(failed.error(), failed.observation());
    drop(failed);
    assert_closed(state);
}
fn unchanged_error(
    error: &Plan4RecoveryError,
    observation: &Plan4RecoveryObservation,
    phase: Plan4RecoveryPhase,
) {
    assert_eq!(
        (error.phase, error.database_effect),
        (phase, Effect::Unchanged)
    );
    assert_eq!(
        (
            observation.mutations_attempted,
            observation.mutations_completed
        ),
        (0, 0)
    );
}

#[test]
fn recovery_selection() {
    let ids: Vec<_> = (1..=15).map(|i| format!("P4-REC-S{i:02}")).collect();
    let names: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&names);
    for case in 1..=15 {
        h.case(&format!("P4-REC-S{case:02}"),|| {
        if case==2 {
            let d=port::Directory::fresh();let state=d.state();let mut maintenance=boxed::<12_582_912>();maintenance.fill(0xa5);let mut dwb=boxed::<4_194_304>();dwb.fill(0xa5);let mut slot=boxed::<1_048_576>();slot.fill(0xa5);
            let error=match Plan4Recovery::acquire(&d,Some(Namespace{timeline:0,..namespace()}),&mut maintenance,&mut dwb,&mut slot) {Err(Plan4RecoveryAcquireFailure::Before{error,..})=>error,Err(Plan4RecoveryAcquireFailure::Acquired(_))=>panic!("invalid expected acquired"),Ok(_)=>panic!("invalid expected accepted")};
            assert_eq!((error.phase,error.effect,error.database_effect,error.lease_acquired),(Plan4RecoveryPhase::Preflight,Effect::Unchanged,Effect::Unchanged,false));assert!(matches!(error.cause,Plan4RecoveryCause::Capacity));
            assert!(maintenance.iter().chain(dwb.iter()).chain(slot.iter()).all(|b|*b==0xa5));assert_eq!(state.lock().unwrap().acquire_calls,0);return;
        }
        let mut files=if case==1 {fresh_memory(0)} else if case==9 {fresh_memory(4)} else {established()};
        match case {
            3=>{files.remove("d3-wal-00000002").unwrap();}
            4=>{
                paired_base_only(&mut files);let slot=usize::from(files["CURRENT"][258]);files.get_mut(BASE_NAMES[slot]).unwrap()[900_000]^=1;
                let d=clone_directory(&files);let state=d.state();clear(&state);let mut maintenance=boxed::<12_582_912>();let mut dwb=boxed::<4_194_304>();let mut wal=boxed::<1_048_576>();
                {let owner=Plan4Recovery::acquire(&d,Some(namespace()),&mut maintenance,&mut dwb,&mut wal).unwrap();assert!(owner.state.selected_previous);let selected=usize::from(files["CURRENT"][514]);assert_eq!(&owner.state.maintenance[..BASE_BYTES],files[BASE_NAMES[selected]]);assert_eq!(owner.observation().mutations_attempted,0);}
                assert_eq!(images(&state),files);assert_closed(&state);return;
            }
            5=>{
                paired_base_only(&mut files);
                let original=fresh_memory(4);let slot=usize::from(files["CURRENT"][258]);let base=files.get_mut(BASE_NAMES[slot]).unwrap();put(base,4,2,2);seal(base);let bad_digest=content_digest(base);
                let current=files.get_mut("CURRENT").unwrap();current[288..320].copy_from_slice(&bad_digest);
                // A distinct historical Genesis/WalSealed previous closure is
                // legal next to a BaseOnly current and independently valid.
                let previous=&mut current[512..768];previous.fill(0);previous[..4].copy_from_slice(&[1,2,0,1]);put(previous,4,1,4);put(previous,8,1,8);put(previous,24,4096,4);previous[32..64].copy_from_slice(&content_digest(&original["d3-bootstrap"]));
                let source=&original["d3-wal-00000001"];previous[64..96].copy_from_slice(&content_digest(&source[..4096]));put(previous,96,4096,8);put(previous,104,source.len() as u64,8);put(previous,112,4,8);previous[120..152].copy_from_slice(&content_digest(source));previous[152..184].copy_from_slice(&content_digest(&original["WRITER"]));put(previous,184,1,8);seal(current);
                files.insert("d3-wal-00000001".into(),source.clone());
            }
            6=>{
                let epoch=files.get_mut("d3-wal-00000002").unwrap();epoch[176]^=1;seal(epoch);let epoch_digest=content_digest(epoch);
                let writer=files.get_mut("WRITER").unwrap();writer[96..128].copy_from_slice(&epoch_digest);seal(writer);let writer_digest=content_digest(writer);
                let current=files.get_mut("CURRENT").unwrap();for at in [256,512] {current[at+64..at+96].copy_from_slice(&epoch_digest);current[at+152..at+184].copy_from_slice(&writer_digest);}seal(current);
            }
            7=>{let mut third=files["d3-wal-00000002"].clone();put(&mut third,40,254,4);seal(&mut third);files.insert("d3-wal-000000fe".into(),third);}
            9=>{files.get_mut("d3-wal-00000001").unwrap().extend_from_slice(&Oracle::new().raw("rollback.wal")[..159]);}
            10=>{
                let suffix=Oracle::new().cleanup("rollback.wal",2);let wal=files.get_mut("d3-wal-00000002").unwrap();wal.extend_from_slice(&suffix[..159]);let digest=content_digest(wal);let end=wal.len() as u64;
                let current=files.get_mut("CURRENT").unwrap();current[257]=2;put(current,360,end,8);put(current,368,5,8);current[376..408].copy_from_slice(&digest);let closure=current[256..512].to_vec();current[512..768].copy_from_slice(&closure);seal(current);
            }
            _=>{}
        }
        let d=clone_directory(&files);let state=d.state();clear(&state);
        if case==8 {state.lock().unwrap().fault=Some(port::Fault{key:port::Key::new("open","d3-wal-00000002",0,1),action:port::Action::OpenedIdentity});}
        if case>=11 {
            let edit=match case {
                11=>{let mut bytes=files["CURRENT"].clone();bytes[40]^=1;seal(&mut bytes);port::Edit::Bytes("CURRENT".into(),bytes)}
                12=>{let path=BASE_NAMES[usize::from(files["CURRENT"][258])];let mut bytes=files[path].clone();bytes[900_000]^=1;seal(&mut bytes);port::Edit::Bytes(path.into(),bytes)}
                13=>{let mut bytes=files["DWB"].clone();bytes[32]^=1;seal(&mut bytes);port::Edit::Bytes("DWB".into(),bytes)}
                14=>{let mut bytes=files["FILEMAP"].clone();let generation=get(&bytes,32,8);put(&mut bytes,32,generation+1,8);seal(&mut bytes);port::Edit::Bytes("FILEMAP".into(),bytes)}
                15=>port::Edit::Identity("CURRENT".into()),_=>unreachable!(),
            };
            state.lock().unwrap().hook=Some(port::Hook{key:port::Key::new("list","",0,5),edit});
        }
        if case==9 {
            let mut maintenance=boxed::<12_582_912>();let mut dwb=boxed::<4_194_304>();let mut slot=boxed::<1_048_576>();
            {let runtime=actual_runtime(&d,&mut maintenance,&mut dwb,&mut slot);assert_eq!((runtime.observation().source_lsn,runtime.observation().source_end),(4,139_680));assert_eq!(file(&state,"d3-wal-00000002").len(),4096);}
            // The original incomplete suffix remains in the immutable input;
            // only the complete adopted source is eligible for rebase.
            assert_eq!(files["d3-wal-00000001"].len(),139_839);assert_closed(&state);return;
        }
        rejected(&d,&state,|error,observation|{
            let phase=if matches!(case,1|10..=15){Plan4RecoveryPhase::Analysis}else{Plan4RecoveryPhase::Selection};unchanged_error(error,observation,phase);
            let correct=match case {
                1|7=>matches!(error.cause,Plan4RecoveryCause::Unsupported),
                3=>matches!(&error.cause,Plan4RecoveryCause::Namespace(e) if e.kind==NamespaceErrorKind::NotFound),
                5=>matches!(error.cause,Plan4RecoveryCause::Base(base::Error::Unsupported)),
                6|8|15=>matches!(error.cause,Plan4RecoveryCause::IdentityMismatch),
                10=>matches!(error.cause,Plan4RecoveryCause::Frame(WalError::Truncated)),
                11..=14=>matches!(error.cause,Plan4RecoveryCause::ContentMismatch),
                _=>unreachable!(),
            };assert!(correct,"wrong primary rejection: {error:?}");
        });
        let observed={let s=state.lock().unwrap();(s.mutations(),s.fault_hits,s.hook_hits,s.events.iter().filter(|e|e.op=="list").count())};
        assert_eq!(observed.0,0);if case==8 {assert_eq!(observed.1,1);}if case>=11 {assert_eq!(observed.2,1);assert!(observed.3>=5);}
        if case<11 {assert_eq!(images(&state),files);}
    });
    }
    h.finish();
}
