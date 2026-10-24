//! Test-only observation of the actual retained byte bank. This lends no
//! mutation, constructor, requirement, or authority and copies no caller loan.
use super::*;

impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn recovery_test_bank(&self) -> &[u8] {
        &self.control.bytes[..65_536]
    }
}
