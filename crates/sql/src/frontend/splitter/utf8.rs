//! A fixed-size incomplete scalar, validated by the standard UTF-8 implementation.

use super::{Code, Diagnostic, error};

pub(super) struct Utf8Carry {
    bytes: [u8; 4],
    length: usize,
    opening: usize,
}

impl Utf8Carry {
    pub(super) fn new() -> Self {
        Self {
            bytes: [0; 4],
            length: 0,
            opening: 0,
        }
    }

    pub(super) fn push(&mut self, byte: u8, offset: usize) -> Result<(), Diagnostic> {
        if self.length == 0 {
            self.opening = offset;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        match std::str::from_utf8(&self.bytes[..self.length]) {
            Ok(_) => {
                self.length = 0;
                Ok(())
            }
            Err(invalid) => {
                if let Some(length) = invalid.error_len() {
                    let start = self
                        .opening
                        .checked_add(invalid.valid_up_to())
                        .ok_or_else(|| error(Code::Limit, offset, offset))?;
                    let end = start
                        .checked_add(length)
                        .ok_or_else(|| error(Code::Limit, offset, offset))?;
                    Err(error(Code::Syntax, start, end))
                } else {
                    // A standard UTF-8 scalar needs at most four bytes; after
                    // four bytes it is valid or invalid, never still incomplete.
                    // Thus a successful return retains at most three bytes.
                    Ok(())
                }
            }
        }
    }

    pub(super) fn finish(&self, offset: usize) -> Result<(), Diagnostic> {
        if self.length == 0 {
            Ok(())
        } else {
            Err(error(Code::Syntax, self.opening, offset))
        }
    }
}
