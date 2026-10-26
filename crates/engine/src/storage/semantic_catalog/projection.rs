//! Logical Catalog projection uses validated XCT1 bytes and a stable target.
//! Stream hashing avoids an owned maximum-size Catalog copy in the call stack.
use super::{Error, Result};
use crate::{
    api::Namespace,
    storage::{
        catalog_record::{self, CatalogExpectation},
        undo_record::{self, UndoTargetKind, UndoTargetView},
    },
};
use foundation::{
    codec::ContentHasher,
    identity::{Generation, ObjectId},
};

pub(crate) fn catalog_projection_digest(
    namespace: Namespace,
    object: ObjectId,
    value: Option<&[u8]>,
) -> Result<[u8; 32]> {
    if namespace.timeline != 1 || object.get() < 6 {
        return Err(Error::InvalidInput);
    }
    if let Some(value) = value {
        let record =
            catalog_record::decode_catalog_record(value, CatalogExpectation { namespace, object })?;
        if record.header().identity.generation.get() != 1
            || record.header().identity.schema_epoch.get() != 1
        {
            return Err(Error::IdentityMismatch);
        }
    }
    let mut prefix = [0; 48];
    prefix[..4].copy_from_slice(b"XCP1");
    prefix[4..6].copy_from_slice(&1u16.to_le_bytes());
    prefix[6..8].copy_from_slice(&3u16.to_le_bytes());
    let target = UndoTargetView {
        kind: UndoTargetKind::Catalog,
        owner: ObjectId::new(5).expect("Catalog owner"),
        generation: Generation::new(1).expect("Catalog generation"),
        entity: object.get(),
        arity: 0,
        key: &[],
    };
    undo_record::encode_undo_target(target, target, &mut prefix[8..40])?;
    prefix[40] = u8::from(value.is_some());
    let mut hash = ContentHasher::new();
    hash.update(&prefix);
    if let Some(value) = value {
        hash.update(value);
    }
    Ok(hash.digest())
}
