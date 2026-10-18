use crate::{Error, ErrorCode, Result};
use xadb_foundation::identity::{ObjectId, RowId};

pub(crate) struct Issuer {
    epoch: u32,
    object: u32,
    row: u32,
}
impl Issuer {
    pub fn new(epoch: u32) -> Result<Self> {
        if epoch == 0 {
            return Err(Error::new(ErrorCode::InvalidInput, "zero writer epoch"));
        }
        Ok(Self {
            epoch,
            object: 0,
            row: 0,
        })
    }
    fn take(epoch: u32, counter: &mut u32) -> Result<u64> {
        *counter = counter.checked_add(1).ok_or(Error::new(
            ErrorCode::IdentityExhausted,
            "epoch range exhausted",
        ))?;
        Ok((u64::from(epoch) << 32) | u64::from(*counter))
    }
    pub fn object(&mut self) -> Result<ObjectId> {
        ObjectId::new(Self::take(self.epoch, &mut self.object)?)
            .map_err(|_| Error::new(ErrorCode::IdentityExhausted, "invalid object range"))
    }
    pub fn row(&mut self) -> Result<RowId> {
        RowId::new(Self::take(self.epoch, &mut self.row)?)
            .map_err(|_| Error::new(ErrorCode::IdentityExhausted, "invalid row range"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epoch_local_vectors_and_exhaustion_never_wrap_or_reuse() {
        assert!(Issuer::new(0).is_err());
        let mut issuer = Issuer::new(1).unwrap();
        assert_eq!(issuer.object().unwrap().get(), 0x0000_0001_0000_0001);
        assert_eq!(issuer.row().unwrap().get(), 0x0000_0001_0000_0001);
        issuer.epoch = u32::MAX;
        issuer.row = u32::MAX - 1;
        assert_eq!(issuer.row().unwrap().get(), u64::MAX);
        assert_eq!(issuer.row().unwrap_err().code, ErrorCode::IdentityExhausted);
        assert_eq!(issuer.row().unwrap_err().code, ErrorCode::IdentityExhausted);
        issuer.object = u32::MAX;
        assert_eq!(
            issuer.object().unwrap_err().code,
            ErrorCode::IdentityExhausted
        );
    }
}
