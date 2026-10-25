//! N=0 CURRENT and new-epoch byte records. These observations grant no file,
//! replay, deletion, writer or recovery capability; the owner binds actual I/O.
#![allow(dead_code)] // The selected recovery owner is being connected.

use super::control::Profile;
use foundation::codec::content_digest;

pub(super) const BYTES: usize = 4096;
pub(super) const BASE_BYTES: u32 = 1 << 20;
const TRAILER: usize = BYTES - 32;
const WAL_CAP: u64 = 256 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    InvalidInput,
    Corrupt,
    Unsupported,
    IdentityMismatch,
    OrderMismatch,
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnchorKind {
    Genesis,
    Base,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    BaseOnly,
    WalOpen,
    WalSealed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Closure {
    pub(super) anchor: AnchorKind,
    pub(super) mode: Mode,
    pub(super) slot: u8,
    pub(super) epoch: u32,
    pub(super) generation: u64,
    pub(super) lsn: u64,
    pub(super) digest: [u8; 32],
    pub(super) epoch_digest: [u8; 32],
    pub(super) replay_offset: u64,
    pub(super) sealed_offset: u64,
    pub(super) sealed_lsn: u64,
    pub(super) sealed_digest: [u8; 32],
    pub(super) writer_digest: [u8; 32],
    pub(super) writer_generation: u64,
}
impl Closure {
    fn validate(self) -> Result<()> {
        if self.slot > 1 || self.generation == 0 {
            return Err(Error::InvalidInput);
        }
        match self.anchor {
            AnchorKind::Genesis => {
                if self.generation != 1
                    || self.lsn != 0
                    || self.slot != 0
                    || self.epoch != 1
                    || self.mode != Mode::WalSealed
                {
                    return Err(Error::InvalidInput);
                }
            }
            AnchorKind::Base if self.lsn == 0 => return Err(Error::InvalidInput),
            AnchorKind::Base => {}
        }
        match self.mode {
            Mode::BaseOnly => {
                if self.anchor != AnchorKind::Base
                    || self.epoch != 0
                    || self.epoch_digest != [0; 32]
                    || self.replay_offset != 0
                    || self.sealed_offset != 0
                    || self.sealed_lsn != 0
                    || self.sealed_digest != [0; 32]
                    || self.writer_digest != [0; 32]
                    || self.writer_generation != 0
                {
                    return Err(Error::InvalidInput);
                }
            }
            Mode::WalOpen | Mode::WalSealed => {
                if self.epoch == 0
                    || (self.anchor == AnchorKind::Base && self.epoch < 2)
                    || self.replay_offset != BYTES as u64
                    || self.writer_generation == 0
                {
                    return Err(Error::InvalidInput);
                }
                if self.mode == Mode::WalOpen {
                    if self.sealed_offset != 0
                        || self.sealed_lsn != 0
                        || self.sealed_digest != [0; 32]
                    {
                        return Err(Error::InvalidInput);
                    }
                } else if !(self.replay_offset..=WAL_CAP).contains(&self.sealed_offset)
                    || self.sealed_lsn < self.lsn
                    || ((self.sealed_offset == self.replay_offset) != (self.sealed_lsn == self.lsn))
                {
                    return Err(Error::OrderMismatch);
                }
            }
        }
        Ok(())
    }

    fn same_except_slot(self, other: Self) -> bool {
        Self {
            slot: other.slot,
            ..self
        } == other
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EpochIntent {
    pub(super) epoch: u32,
    pub(super) base_generation: u64,
    pub(super) base_lsn: u64,
    pub(super) active_generation: u64,
    pub(super) base_digest: [u8; 32],
    pub(super) epoch_digest: [u8; 32],
    pub(super) writer_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Current {
    pub(super) generation: u64,
    pub(super) issued_epoch: u32,
    pub(super) retired_through_epoch: u32,
    pub(super) writer_high_water: u64,
    pub(super) base_high_water: u64,
    pub(super) intent: Option<EpochIntent>,
    pub(super) current: Closure,
    pub(super) previous: Closure,
}
impl Current {
    fn validate(self) -> Result<()> {
        self.current.validate()?;
        self.previous.validate()?;
        if self.generation == 0
            || self.issued_epoch == 0
            || self.retired_through_epoch > self.issued_epoch
            || self.current.epoch > self.issued_epoch
            || self.previous.epoch > self.issued_epoch
            || self.current.writer_generation > self.writer_high_water
            || self.previous.writer_generation > self.writer_high_water
        {
            return Err(Error::OrderMismatch);
        }
        for closure in [self.current, self.previous] {
            if closure.anchor == AnchorKind::Base && closure.generation > self.base_high_water {
                return Err(Error::OrderMismatch);
            }
        }
        match self.current.mode {
            Mode::WalOpen => {
                // A fallback may differ in physical base slot, never in its
                // continuation epoch: otherwise acknowledged suffix is lost.
                if !self.current.same_except_slot(self.previous) || self.intent.is_some() {
                    return Err(Error::IdentityMismatch);
                }
            }
            Mode::WalSealed => {
                if self.current != self.previous || self.intent.is_some() {
                    return Err(Error::IdentityMismatch);
                }
            }
            Mode::BaseOnly => match self.previous.mode {
                Mode::BaseOnly => {
                    if !self.current.same_except_slot(self.previous) {
                        return Err(Error::IdentityMismatch);
                    }
                }
                Mode::WalSealed => {
                    if self.intent.is_some() || self.previous.sealed_lsn != self.current.lsn {
                        return Err(Error::OrderMismatch);
                    }
                }
                Mode::WalOpen => return Err(Error::InvalidInput),
            },
        }
        // Keep the optional intent checks grouped in the frozen validation order.
        #[allow(clippy::collapsible_if)]
        if let Some(intent) = self.intent {
            if self.current.mode != Mode::BaseOnly
                || self.previous.mode != Mode::BaseOnly
                || intent.epoch < 2
                || intent.epoch != self.issued_epoch
                || intent.base_generation != self.current.generation
                || intent.base_lsn != self.current.lsn
                || intent.base_digest != self.current.digest
                || self.generation.checked_add(1) != Some(intent.active_generation)
                || self.writer_high_water == 0
            {
                return Err(Error::OrderMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EpochHeader {
    pub(super) epoch: u32,
    pub(super) base_lsn: u64,
    pub(super) base_generation: u64,
    pub(super) base_digest: [u8; 32],
    pub(super) previous_epoch: u32,
    pub(super) previous_header_digest: [u8; 32],
}
impl EpochHeader {
    fn validate(self) -> Result<()> {
        if self.epoch < 2
            || self.base_lsn == 0
            || self.base_generation == 0
            || self.previous_epoch == 0
            || self.previous_epoch >= self.epoch
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WriterSelection {
    pub(super) generation: u64,
    pub(super) epoch: u32,
    pub(super) epoch_digest: [u8; 32],
    pub(super) base_generation: u64,
    pub(super) base_lsn: u64,
    pub(super) base_digest: [u8; 32],
    pub(super) active_generation: u64,
}
impl WriterSelection {
    fn validate(self) -> Result<()> {
        if self.generation == 0
            || self.epoch < 2
            || self.base_generation == 0
            || self.base_lsn == 0
            || self.active_generation == 0
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("checked fixed record"))
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("checked fixed record"))
}
fn digest_at(bytes: &[u8], at: usize) -> [u8; 32] {
    bytes[at..at + 32].try_into().expect("checked fixed record")
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn zeros(bytes: &[u8]) -> bool {
    bytes.iter().all(|value| *value == 0)
}
fn prepare(bytes: &mut [u8; BYTES], magic: &[u8; 4], profile: Profile) {
    bytes.fill(0);
    bytes[..4].copy_from_slice(magic);
    bytes[4..8].copy_from_slice(&[1, 0, 1, 0]);
    put32(bytes, 8, BYTES as u32);
    put32(bytes, 12, 256);
    bytes[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
    put64(bytes, 32, profile.spec().namespace.timeline);
}
fn check(bytes: &[u8], magic: &[u8; 4], profile: Profile) -> Result<()> {
    if bytes.len() != BYTES || bytes[..4] != *magic {
        return Err(Error::Corrupt);
    }
    if bytes[4..8] != [1, 0, 1, 0] {
        return Err(Error::Unsupported);
    }
    if u32_at(bytes, 8) != BYTES as u32
        || u32_at(bytes, 12) != 256
        || digest_at(bytes, TRAILER) != content_digest(&bytes[..TRAILER])
    {
        return Err(Error::Corrupt);
    }
    if bytes[16..32] != profile.spec().namespace.database.bytes()
        || u64_at(bytes, 32) != profile.spec().namespace.timeline
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn seal(bytes: &mut [u8; BYTES]) {
    let digest = content_digest(&bytes[..TRAILER]);
    bytes[TRAILER..].copy_from_slice(&digest);
}
fn write_profile(bytes: &mut [u8], at: usize, profile: Profile) {
    put64(bytes, at, profile.spec().generation);
    bytes[at + 8..at + 40].copy_from_slice(&profile.digest());
}
fn check_profile(bytes: &[u8], at: usize, profile: Profile) -> Result<()> {
    if u64_at(bytes, at) != profile.spec().generation
        || digest_at(bytes, at + 8) != profile.digest()
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn write_closure(bytes: &mut [u8], value: Closure) {
    bytes[0] = match value.anchor {
        AnchorKind::Genesis => 1,
        AnchorKind::Base => 2,
    };
    bytes[1] = match value.mode {
        Mode::BaseOnly => 0,
        Mode::WalOpen => 1,
        Mode::WalSealed => 2,
    };
    bytes[2] = value.slot;
    bytes[3] = 1;
    put32(bytes, 4, value.epoch);
    put64(bytes, 8, value.generation);
    put64(bytes, 16, value.lsn);
    put32(
        bytes,
        24,
        if value.anchor == AnchorKind::Base {
            BASE_BYTES
        } else {
            BYTES as u32
        },
    );
    bytes[32..64].copy_from_slice(&value.digest);
    bytes[64..96].copy_from_slice(&value.epoch_digest);
    put64(bytes, 96, value.replay_offset);
    put64(bytes, 104, value.sealed_offset);
    put64(bytes, 112, value.sealed_lsn);
    bytes[120..152].copy_from_slice(&value.sealed_digest);
    bytes[152..184].copy_from_slice(&value.writer_digest);
    put64(bytes, 184, value.writer_generation);
}
fn read_closure(bytes: &[u8]) -> Result<Closure> {
    let anchor = match bytes[0] {
        1 => AnchorKind::Genesis,
        2 => AnchorKind::Base,
        _ => return Err(Error::Unsupported),
    };
    let mode = match bytes[1] {
        0 => Mode::BaseOnly,
        1 => Mode::WalOpen,
        2 => Mode::WalSealed,
        _ => return Err(Error::Unsupported),
    };
    if bytes[3] != 1 {
        return Err(Error::Unsupported);
    }
    if !zeros(&bytes[28..32])
        || !zeros(&bytes[192..256])
        || u32_at(bytes, 24)
            != if anchor == AnchorKind::Base {
                BASE_BYTES
            } else {
                BYTES as u32
            }
    {
        return Err(Error::Corrupt);
    }
    let value = Closure {
        anchor,
        mode,
        slot: bytes[2],
        epoch: u32_at(bytes, 4),
        generation: u64_at(bytes, 8),
        lsn: u64_at(bytes, 16),
        digest: digest_at(bytes, 32),
        epoch_digest: digest_at(bytes, 64),
        replay_offset: u64_at(bytes, 96),
        sealed_offset: u64_at(bytes, 104),
        sealed_lsn: u64_at(bytes, 112),
        sealed_digest: digest_at(bytes, 120),
        writer_digest: digest_at(bytes, 152),
        writer_generation: u64_at(bytes, 184),
    };
    value.validate()?;
    Ok(value)
}

pub(super) fn encode_current(
    value: Current,
    profile: Profile,
    output: &mut [u8; BYTES],
) -> Result<()> {
    value.validate()?;
    prepare(output, b"XRC1", profile);
    put64(output, 40, value.generation);
    write_profile(output, 48, profile);
    put32(output, 88, value.issued_epoch);
    put32(output, 100, value.retired_through_epoch);
    if let Some(intent) = value.intent {
        output[92] = 1;
        put32(output, 96, intent.epoch);
        put64(output, 104, intent.base_generation);
        put64(output, 112, intent.base_lsn);
        put64(output, 120, intent.active_generation);
        output[128..160].copy_from_slice(&intent.base_digest);
        output[160..192].copy_from_slice(&intent.epoch_digest);
        output[192..224].copy_from_slice(&intent.writer_digest);
    }
    put64(output, 224, value.writer_high_water);
    put64(output, 232, value.base_high_water);
    write_closure(&mut output[256..512], value.current);
    write_closure(&mut output[512..768], value.previous);
    seal(output);
    Ok(())
}
pub(super) fn decode_current(input: &[u8], profile: Profile) -> Result<Current> {
    check(input, b"XRC1", profile)?;
    check_profile(input, 48, profile)?;
    if !zeros(&input[93..96]) || !zeros(&input[240..256]) || !zeros(&input[768..TRAILER]) {
        return Err(Error::Corrupt);
    }
    let intent = match input[92] {
        0 => {
            if !zeros(&input[96..100]) || !zeros(&input[104..224]) {
                return Err(Error::Corrupt);
            }
            None
        }
        1 => Some(EpochIntent {
            epoch: u32_at(input, 96),
            base_generation: u64_at(input, 104),
            base_lsn: u64_at(input, 112),
            active_generation: u64_at(input, 120),
            base_digest: digest_at(input, 128),
            epoch_digest: digest_at(input, 160),
            writer_digest: digest_at(input, 192),
        }),
        _ => return Err(Error::Unsupported),
    };
    let value = Current {
        generation: u64_at(input, 40),
        issued_epoch: u32_at(input, 88),
        retired_through_epoch: u32_at(input, 100),
        writer_high_water: u64_at(input, 224),
        base_high_water: u64_at(input, 232),
        intent,
        current: read_closure(&input[256..512])?,
        previous: read_closure(&input[512..768])?,
    };
    value.validate()?;
    Ok(value)
}

pub(super) fn encode_epoch(
    value: EpochHeader,
    profile: Profile,
    output: &mut [u8; BYTES],
) -> Result<()> {
    value.validate()?;
    prepare(output, b"XWE2", profile);
    put32(output, 40, value.epoch);
    write_profile(output, 48, profile);
    put64(output, 88, value.base_lsn);
    put64(output, 96, BYTES as u64);
    put32(output, 104, BASE_BYTES);
    put64(output, 112, WAL_CAP);
    put64(output, 120, value.base_generation);
    output[128..160].copy_from_slice(&value.base_digest);
    put32(output, 160, value.previous_epoch);
    put64(output, 168, value.base_lsn);
    output[176..208].copy_from_slice(&value.previous_header_digest);
    put32(output, 208, 1);
    seal(output);
    Ok(())
}
pub(super) fn decode_epoch(input: &[u8], profile: Profile) -> Result<EpochHeader> {
    check(input, b"XWE2", profile)?;
    check_profile(input, 48, profile)?;
    if !zeros(&input[44..48])
        || !zeros(&input[108..112])
        || !zeros(&input[164..168])
        || !zeros(&input[212..TRAILER])
        || u64_at(input, 96) != BYTES as u64
        || u32_at(input, 104) != BASE_BYTES
        || u64_at(input, 112) != WAL_CAP
        || u32_at(input, 208) != 1
        || u64_at(input, 168) != u64_at(input, 88)
    {
        return Err(Error::Corrupt);
    }
    let value = EpochHeader {
        epoch: u32_at(input, 40),
        base_lsn: u64_at(input, 88),
        base_generation: u64_at(input, 120),
        base_digest: digest_at(input, 128),
        previous_epoch: u32_at(input, 160),
        previous_header_digest: digest_at(input, 176),
    };
    value.validate()?;
    Ok(value)
}
pub(super) fn encode_writer(
    value: WriterSelection,
    profile: Profile,
    output: &mut [u8; BYTES],
) -> Result<()> {
    value.validate()?;
    prepare(output, b"XWS2", profile);
    put64(output, 40, value.generation);
    put32(output, 48, value.epoch);
    put32(output, 52, 1);
    write_profile(output, 56, profile);
    output[96..128].copy_from_slice(&value.epoch_digest);
    put64(output, 128, value.base_generation);
    put64(output, 136, value.base_lsn);
    output[144..176].copy_from_slice(&value.base_digest);
    put64(output, 176, BYTES as u64);
    put64(output, 184, value.active_generation);
    seal(output);
    Ok(())
}
pub(super) fn decode_writer(input: &[u8], profile: Profile) -> Result<WriterSelection> {
    check(input, b"XWS2", profile)?;
    check_profile(input, 56, profile)?;
    if u32_at(input, 52) != 1 || u64_at(input, 176) != BYTES as u64 || !zeros(&input[192..TRAILER])
    {
        return Err(Error::Corrupt);
    }
    let value = WriterSelection {
        generation: u64_at(input, 40),
        epoch: u32_at(input, 48),
        epoch_digest: digest_at(input, 96),
        base_generation: u64_at(input, 128),
        base_lsn: u64_at(input, 136),
        base_digest: digest_at(input, 144),
        active_generation: u64_at(input, 184),
    };
    value.validate()?;
    Ok(value)
}

#[cfg(test)]
#[path = "plan4_recovery_control_tests.rs"]
mod tests;
