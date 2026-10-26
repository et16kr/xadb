#![allow(dead_code)]
use super::*;
fn probe<W: WriterNamespace + 'static>(
    storage: MutableStorageOwner<'static, 'static, W>,
) -> Plan4SystemRole<'static> {
    Plan4SystemRole::new(&storage, RetentionPageKind::FreeUndo).unwrap() // expected diagnostic
}
