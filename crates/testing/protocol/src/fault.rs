//! One-shot native fault control. Reached is an adapter observation, never an
//! oracle verdict. Missing/mismatched witnesses and channel loss are incomplete.
use crate::frame::{Envelope, Error};
use serde::{Deserialize, Serialize};
pub const VERSION: u32 = 1;
pub const MAX_CONTROL_BYTES: usize = 128 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Any,
    WalCommit,
    RecoveryOldFile,
    Current,
    Writer,
    FileMap,
    Doublewrite,
    Retirement,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Boundary {
    CreateBefore,
    CreateAfter,
    WriteBefore,
    WritePartial,
    WriteAfter,
    SyncBefore,
    SyncAfter,
    RenameBefore,
    RenameAfter,
    DirectorySyncBefore,
    DirectorySyncAfter,
    UnlinkBefore,
    UnlinkAfter,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cut {
    pub target: Target,
    pub boundary: Boundary,
}
impl Cut {
    pub fn validate(self) -> Result<(), Error> {
        use Boundary as B;
        use Target as T;
        let valid = match self.target {
            T::Any => true,
            T::WalCommit => matches!(
                self.boundary,
                B::WriteBefore | B::WritePartial | B::WriteAfter | B::SyncBefore | B::SyncAfter
            ),
            T::RecoveryOldFile => matches!(self.boundary, B::SyncBefore | B::SyncAfter),
            T::Current | T::Writer | T::FileMap | T::Doublewrite => matches!(
                self.boundary,
                B::WriteBefore
                    | B::WritePartial
                    | B::WriteAfter
                    | B::SyncBefore
                    | B::SyncAfter
                    | B::RenameBefore
                    | B::RenameAfter
                    | B::DirectorySyncBefore
                    | B::DirectorySyncAfter
            ),
            T::Retirement => matches!(
                self.boundary,
                B::UnlinkBefore | B::UnlinkAfter | B::DirectorySyncBefore | B::DirectorySyncAfter
            ),
        };
        if valid {
            Ok(())
        } else {
            Err(Error::Invalid("unsupported native fault cut"))
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub cut: Cut,
    /// Matching boundary occurrence, counted after optional filters, 1..=4096.
    pub occurrence: u32,
    pub actor: Option<u8>,
    pub directory: Option<u8>,
}
impl Plan {
    pub fn validate(self) -> Result<(), Error> {
        self.cut.validate()?;
        if !(1..=4096).contains(&self.occurrence)
            || self.actor.is_some_and(|v| v >= 32)
            || self.directory.is_some_and(|v| v >= 8)
        {
            return Err(Error::Invalid("native fault plan bound"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Continue,
    ReturnError,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Create,
    Open,
    Restore,
    Commit,
    Checkpoint,
    Backup,
    GarbageCollect,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reached {
    pub plan: Plan,
    pub matched: u32,
    pub actor: u8,
    pub request: u64,
    pub directory: u8,
    pub phase: Phase,
    pub observed_target: Target,
    pub file: Option<String>,
    pub offset: Option<u64>,
    pub requested_bytes: Option<u64>,
    pub completed_bytes: Option<u64>,
    pub frame_start: Option<u64>,
    pub frame_end: Option<u64>,
    pub frame_written_bytes: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Connected { process_id: u32 },
    Reached { witness: Reached },
    Released { decision: Decision },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub version: u32,
    pub run: [u8; 16],
    pub token: [u8; 16],
    pub event: Event,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub version: u32,
    pub run: [u8; 16],
    pub token: [u8; 16],
    pub decision: Decision,
}
fn identity(version: u32, run: [u8; 16], token: [u8; 16]) -> Result<(), Error> {
    if version != VERSION || run == [0; 16] || token == [0; 16] {
        Err(Error::Invalid("native fault control identity"))
    } else {
        Ok(())
    }
}
impl Envelope for Release {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.token)
    }
}
impl Envelope for Observation {
    fn validate(&self) -> Result<(), Error> {
        identity(self.version, self.run, self.token)?;
        match &self.event {
            Event::Connected { process_id } if *process_id == 0 => {
                Err(Error::Invalid("native fault process id"))
            }
            Event::Reached { witness } => {
                witness.plan.validate()?;
                if witness.plan.cut.target != Target::Any
                    && witness.observed_target != witness.plan.cut.target
                    || witness
                        .plan
                        .actor
                        .is_some_and(|actor| actor != witness.actor)
                    || witness
                        .plan
                        .directory
                        .is_some_and(|directory| directory != witness.directory)
                    || witness.observed_target == Target::RecoveryOldFile
                        && witness.phase != Phase::Open
                {
                    return Err(Error::Invalid("native fault witness classification"));
                }
                if witness.plan.cut.boundary == Boundary::WritePartial
                    && (witness.completed_bytes != Some(1)
                        || witness.requested_bytes.is_none_or(|count| count <= 1)
                        || witness.offset.is_none())
                {
                    return Err(Error::Invalid("native partial write lacks actual prefix"));
                }
                if witness.plan.cut.target == Target::WalCommit
                    && matches!(
                        witness.plan.cut.boundary,
                        Boundary::WriteAfter | Boundary::SyncBefore | Boundary::SyncAfter
                    )
                {
                    let complete = witness
                        .frame_start
                        .zip(witness.frame_end)
                        .and_then(|(start, end)| end.checked_sub(start));
                    if complete.is_none_or(|count| count == 0)
                        || complete != witness.frame_written_bytes
                    {
                        return Err(Error::Invalid("native WAL frame is not completely written"));
                    }
                }
                if witness.matched != witness.plan.occurrence
                    || witness.actor >= 32
                    || !(1..=1024).contains(&witness.request)
                    || witness.directory >= 8
                    || witness.file.as_ref().is_some_and(|name| {
                        name.len() > 128
                            || name.is_empty()
                            || !name.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
                            })
                    })
                {
                    return Err(Error::Invalid("native fault witness bound"));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reached(target: Target, boundary: Boundary) -> Observation {
        Observation {
            version: VERSION,
            run: [1; 16],
            token: [2; 16],
            event: Event::Reached {
                witness: Reached {
                    plan: Plan {
                        cut: Cut { target, boundary },
                        occurrence: 1,
                        actor: Some(0),
                        directory: Some(0),
                    },
                    matched: 1,
                    actor: 0,
                    request: 1,
                    directory: 0,
                    phase: Phase::Commit,
                    observed_target: target,
                    file: Some("wal-00000001-0000000000000001.axf".into()),
                    offset: Some(100),
                    requested_bytes: Some(50),
                    completed_bytes: Some(1),
                    frame_start: Some(100),
                    frame_end: Some(150),
                    frame_written_bytes: Some(1),
                },
            },
        }
    }
    #[test]
    fn cut_catalog_and_occurrence_bounds_reject_ambiguous_plans() {
        assert!(
            Cut {
                target: Target::RecoveryOldFile,
                boundary: Boundary::WritePartial
            }
            .validate()
            .is_err()
        );
        assert!(
            Cut {
                target: Target::WalCommit,
                boundary: Boundary::RenameAfter
            }
            .validate()
            .is_err()
        );
        assert!(
            Plan {
                cut: Cut {
                    target: Target::Any,
                    boundary: Boundary::CreateBefore
                },
                occurrence: 0,
                actor: None,
                directory: None
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn reached_partial_requires_one_real_byte_and_full_cut_requires_complete_extent() {
        let mut partial = reached(Target::WalCommit, Boundary::WritePartial);
        assert!(partial.validate().is_ok());
        if let Event::Reached { witness } = &mut partial.event {
            witness.completed_bytes = Some(0);
        }
        assert!(partial.validate().is_err());
        let mut full = reached(Target::WalCommit, Boundary::WriteAfter);
        assert!(full.validate().is_err());
        if let Event::Reached { witness } = &mut full.event {
            witness.completed_bytes = Some(50);
            witness.frame_written_bytes = Some(50);
        }
        assert!(full.validate().is_ok());
    }

    #[test]
    fn dev3_selector_cut_matrix_and_wire_names() {
        let boundaries = [
            Boundary::CreateBefore,
            Boundary::CreateAfter,
            Boundary::WriteBefore,
            Boundary::WritePartial,
            Boundary::WriteAfter,
            Boundary::SyncBefore,
            Boundary::SyncAfter,
            Boundary::RenameBefore,
            Boundary::RenameAfter,
            Boundary::DirectorySyncBefore,
            Boundary::DirectorySyncAfter,
            Boundary::UnlinkBefore,
            Boundary::UnlinkAfter,
        ];
        let mut accepted = 0;
        let mut rejected = 0;
        for (target, literal) in [
            (Target::FileMap, "\"file_map\""),
            (Target::Doublewrite, "\"doublewrite\""),
        ] {
            assert_eq!(serde_json::to_string(&target).unwrap(), literal);
            assert_eq!(serde_json::from_str::<Target>(literal).unwrap(), target);
            for boundary in boundaries {
                let valid = !matches!(
                    boundary,
                    Boundary::CreateBefore
                        | Boundary::CreateAfter
                        | Boundary::UnlinkBefore
                        | Boundary::UnlinkAfter
                );
                let cut = Cut { target, boundary };
                assert_eq!(cut.validate().is_ok(), valid, "{target:?}/{boundary:?}");
                if valid {
                    accepted += 1;
                    let observation = reached(target, boundary);
                    observation.validate().unwrap();
                    let encoded = crate::frame::encode(&observation).unwrap();
                    assert_eq!(
                        crate::frame::read::<Observation>(&mut encoded.as_slice()).unwrap(),
                        Some(observation.clone())
                    );
                    let mut wrong = observation;
                    if let Event::Reached { witness } = &mut wrong.event {
                        witness.observed_target = if target == Target::FileMap {
                            Target::Doublewrite
                        } else {
                            Target::FileMap
                        };
                    }
                    assert!(wrong.validate().is_err());
                } else {
                    rejected += 1;
                }
            }
        }
        assert_eq!((accepted, rejected), (18, 8));
        assert_eq!(VERSION, 1);
        println!(
            "XADB_DEV3_SELECTOR_PROTOCOL {{\"cases\":26,\"accepted\":18,\"rejected\":8,\"scope\":\"port_only\"}}"
        );
    }
}
