//! Streaming SQL source boundaries, independent of statement parsing or execution.
//!
//! One call consumes through at most one nonempty statement. The caller retains
//! any unconsumed suffix; each returned source owns its bytes and quota leases.
//! D4-FE-SPLIT-001 defines the chunk, EOF, absolute-span, and cleanup contracts.

mod utf8;

use super::{
    control::CallControl,
    diagnostic::{Code, Diagnostic, Module, Span},
    source::{SourceBudget, SourceBuffer, StatementSource},
};
use utf8::Utf8Carry;
use xadb_foundation::runtime::Quota;

const MAX_SOURCE_BYTES: usize = 1 << 20;
const CHECKPOINT_BYTES: usize = 256;

/// The next source boundary or exhaustion of the supplied chunk.
#[derive(Debug)]
pub enum SplitOutcome {
    /// The complete chunk was consumed; only `finish` can declare actual EOF.
    NeedMore,
    /// One nonempty source, including its ending external semicolon.
    Statement(StatementSource),
}

/// Progress within one borrowed input chunk.
#[derive(Debug)]
pub struct SplitStep {
    /// Number of bytes consumed from this chunk; resubmit the remaining suffix.
    pub consumed: usize,
    /// A completed boundary or a request for another chunk.
    pub outcome: SplitOutcome,
}

#[derive(Clone, Copy)]
enum LexState {
    Normal,
    Minus,
    Slash { opening: usize },
    Quote { delimiter: u8, opening: usize },
    QuoteEnd { delimiter: u8, opening: usize },
    LineComment,
    BlockComment { opening: usize },
    BlockStar { opening: usize },
}

#[derive(Clone, Copy)]
enum Status {
    Active,
    Finished,
    Failed(Diagnostic),
}

/// Incrementally collect sources while retaining quote, comment, and UTF-8 state.
///
/// Quota handles share caller-owned ledgers. Calls borrow independent controls;
/// neither a cancellation token nor a deadline is retained. An error discards
/// only the pending source and becomes sticky. Previously returned sources keep
/// their own bytes and reservations until their owners release them.
pub struct Splitter {
    buffer: SourceBuffer,
    budget: SourceBudget,
    state: LexState,
    status: Status,
    utf8: Utf8Carry,
    absolute_offset: usize,
    segment_start: usize,
    has_nontrivia: bool,
}

impl Splitter {
    /// Borrow shared session and database quota identities by cloning their handles.
    /// No backing allocation or reservation occurs until input is supplied.
    pub fn new(session: &Quota, database: &Quota) -> Self {
        Self {
            buffer: SourceBuffer::new(),
            budget: SourceBudget::new(session, database),
            state: LexState::Normal,
            status: Status::Active,
            utf8: Utf8Carry::new(),
            absolute_offset: 0,
            segment_start: 0,
            has_nontrivia: false,
        }
    }

    /// Consume through the first nonempty external-semicolon boundary, if any.
    ///
    /// Empty segments are skipped, and all returned source bytes are preserved.
    /// Parentheses do not hide a semicolon. This finds lexical boundaries only;
    /// callers must still pass the returned source to the lexer/parser.
    ///
    /// # Errors
    /// Returns Syntax for invalid consumed UTF-8 or input after successful EOF,
    /// Limit for offset/source/quota/allocation limits, or the current call's
    /// cancellation/deadline diagnostic. Failure discards pending storage and
    /// becomes terminal. No bytes beyond a returned statement are inspected.
    pub fn push(
        &mut self,
        chunk: &[u8],
        control: &CallControl<'_>,
    ) -> Result<SplitStep, Diagnostic> {
        if let Status::Failed(error) = self.status {
            return Err(error);
        }
        let result = self.push_active(chunk, control);
        if let Err(error) = &result {
            self.fail(*error);
        }
        result
    }

    /// Declare actual EOF and return a remaining nonempty source without a semicolon.
    ///
    /// A closing-quote candidate or a line comment may finish at EOF. Incomplete
    /// UTF-8, an open quote, or an open block comment cannot. Repeated successful
    /// calls return None after checking their own control; any later `push`, even
    /// with an empty chunk, becomes a terminal Syntax diagnostic.
    ///
    /// # Errors
    /// Returns Syntax for incomplete UTF-8/quote/comment state or the current
    /// cancellation/deadline diagnostic, discarding all pending storage. An
    /// already terminal diagnostic is returned unchanged without inspecting control.
    pub fn finish(
        &mut self,
        control: &CallControl<'_>,
    ) -> Result<Option<StatementSource>, Diagnostic> {
        if let Status::Failed(error) = self.status {
            return Err(error);
        }
        let result = self.finish_active(control);
        if let Err(error) = &result {
            self.fail(*error);
        }
        result
    }

    /// Inject a Limit failure at this splitter's next backing-allocation point.
    #[cfg(feature = "test-support")]
    pub fn fail_next_allocation(&mut self) {
        self.budget.fail_next_allocation();
    }

    /// Add checked bytes to backing requests to exercise actual-capacity accounting.
    #[cfg(feature = "test-support")]
    pub fn set_allocation_padding(&mut self, bytes: usize) {
        self.budget.set_allocation_padding(bytes);
    }

    /// Observe the pending source's actual backing capacity without its contents.
    #[cfg(feature = "test-support")]
    pub fn buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }

    fn push_active(
        &mut self,
        chunk: &[u8],
        control: &CallControl<'_>,
    ) -> Result<SplitStep, Diagnostic> {
        check_at(control, self.absolute_offset)?;
        if let Status::Finished = self.status {
            return Err(error(
                Code::Syntax,
                self.absolute_offset,
                self.absolute_offset,
            ));
        }
        let mut since_checkpoint = 0;
        for (index, &byte) in chunk.iter().enumerate() {
            let offset = self.absolute_offset;
            let next_offset = offset
                .checked_add(1)
                .ok_or_else(|| error(Code::Limit, offset, offset))?;
            if self.buffer.len() == MAX_SOURCE_BYTES {
                return Err(error(Code::Limit, offset, next_offset));
            }
            self.utf8.push(byte, offset)?;
            self.buffer.push(
                byte,
                &mut self.budget,
                control,
                Span {
                    start: self.segment_start,
                    end: offset,
                },
            )?;
            self.absolute_offset = next_offset;
            let boundary = self.consume_lexical(byte, offset);
            since_checkpoint += 1;
            if since_checkpoint == CHECKPOINT_BYTES {
                check_at(control, self.absolute_offset)?;
                since_checkpoint = 0;
            }
            if boundary {
                check_at(control, self.absolute_offset)?;
                since_checkpoint = 0;
                let buffer = std::mem::replace(&mut self.buffer, SourceBuffer::new());
                let span = Span {
                    start: self.segment_start,
                    end: self.absolute_offset,
                };
                self.segment_start = self.absolute_offset;
                self.state = LexState::Normal;
                if std::mem::take(&mut self.has_nontrivia) {
                    return Ok(SplitStep {
                        consumed: index + 1,
                        outcome: SplitOutcome::Statement(StatementSource::new(buffer, span, true)),
                    });
                }
                // Empty segments release their buffer and leases immediately.
                drop(buffer);
            }
        }
        check_at(control, self.absolute_offset)?;
        Ok(SplitStep {
            consumed: chunk.len(),
            outcome: SplitOutcome::NeedMore,
        })
    }

    fn finish_active(
        &mut self,
        control: &CallControl<'_>,
    ) -> Result<Option<StatementSource>, Diagnostic> {
        check_at(control, self.absolute_offset)?;
        if let Status::Finished = self.status {
            check_at(control, self.absolute_offset)?;
            return Ok(None);
        }
        self.utf8.finish(self.absolute_offset)?;
        match self.state {
            LexState::Quote { opening, .. }
            | LexState::BlockComment { opening }
            | LexState::BlockStar { opening } => {
                return Err(error(Code::Syntax, opening, self.absolute_offset));
            }
            LexState::Minus | LexState::Slash { .. } => self.has_nontrivia = true,
            LexState::Normal | LexState::QuoteEnd { .. } | LexState::LineComment => {}
        }
        check_at(control, self.absolute_offset)?;
        self.status = Status::Finished;
        let buffer = std::mem::replace(&mut self.buffer, SourceBuffer::new());
        if self.has_nontrivia {
            Ok(Some(StatementSource::new(
                buffer,
                Span {
                    start: self.segment_start,
                    end: self.absolute_offset,
                },
                false,
            )))
        } else {
            drop(buffer);
            Ok(None)
        }
    }

    fn consume_lexical(&mut self, byte: u8, offset: usize) -> bool {
        // A pending opener or closing quote may need to interpret this byte in
        // Normal once more. Only lexical state repeats, never validation/copy.
        loop {
            self.state = match self.state {
                LexState::Normal => match byte {
                    b'-' => LexState::Minus,
                    b'/' => LexState::Slash { opening: offset },
                    b'\'' | b'"' => {
                        self.has_nontrivia = true;
                        LexState::Quote {
                            delimiter: byte,
                            opening: offset,
                        }
                    }
                    b';' => return true,
                    b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' => LexState::Normal,
                    _ => {
                        self.has_nontrivia = true;
                        LexState::Normal
                    }
                },
                LexState::Minus if byte == b'-' => LexState::LineComment,
                LexState::Slash { opening } if byte == b'*' => LexState::BlockComment { opening },
                LexState::Minus | LexState::Slash { .. } => {
                    self.has_nontrivia = true;
                    self.state = LexState::Normal;
                    continue;
                }
                LexState::Quote { delimiter, opening } if byte == delimiter => {
                    LexState::QuoteEnd { delimiter, opening }
                }
                LexState::Quote { delimiter, opening } => LexState::Quote { delimiter, opening },
                LexState::QuoteEnd { delimiter, opening } if byte == delimiter => {
                    LexState::Quote { delimiter, opening }
                }
                LexState::QuoteEnd { .. } => {
                    self.state = LexState::Normal;
                    continue;
                }
                LexState::LineComment if matches!(byte, b'\r' | b'\n') => LexState::Normal,
                LexState::LineComment => LexState::LineComment,
                LexState::BlockComment { opening } if byte == b'*' => {
                    LexState::BlockStar { opening }
                }
                LexState::BlockComment { opening } => LexState::BlockComment { opening },
                LexState::BlockStar { .. } if byte == b'/' => LexState::Normal,
                LexState::BlockStar { opening } if byte == b'*' => LexState::BlockStar { opening },
                LexState::BlockStar { opening } => LexState::BlockComment { opening },
            };
            return false;
        }
    }

    fn fail(&mut self, diagnostic: Diagnostic) {
        let buffer = std::mem::replace(&mut self.buffer, SourceBuffer::new());
        drop(buffer);
        self.state = LexState::Normal;
        self.utf8 = Utf8Carry::new();
        self.has_nontrivia = false;
        self.status = Status::Failed(diagnostic);
    }
}

fn error(code: Code, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code,
        module: Module::Split,
        span: Span { start, end },
    }
}

fn check_at(control: &CallControl<'_>, offset: usize) -> Result<(), Diagnostic> {
    control.check(
        Module::Split,
        Span {
            start: offset,
            end: offset,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xadb_foundation::runtime::{Cancellation, Clock};

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_nanos(&self) -> u64 {
            0
        }
    }

    #[test]
    fn absolute_offset_overflow_releases_without_allocation() {
        let session = Quota::new(0);
        let database = Quota::new(0);
        let cancellation = Cancellation::default();
        let control = CallControl::new(&cancellation, &FixedClock, None);
        let mut splitter = Splitter::new(&session, &database);
        splitter.absolute_offset = usize::MAX;
        splitter.segment_start = usize::MAX;
        let expected = error(Code::Limit, usize::MAX, usize::MAX);
        assert_eq!(splitter.push(b"a", &control).err(), Some(expected));
        assert_eq!(splitter.push(b"", &control).err(), Some(expected));
        assert_eq!(splitter.finish(&control).err(), Some(expected));
        assert_eq!(splitter.buffer.len(), 0);
        assert_eq!(splitter.buffer.capacity(), 0);
        assert_eq!(session.used(), 0);
        assert_eq!(session.peak(), 0);
        assert_eq!(database.used(), 0);
        assert_eq!(database.peak(), 0);
    }
}
