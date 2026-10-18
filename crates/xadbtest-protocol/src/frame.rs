//! Bounded native framing. A length prefix is checked before body allocation.
//! The streaming decoder returns one complete validated message per feed call.
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fmt,
    io::{self, Read, Write},
    marker::PhantomData,
};

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BODY_BYTES: usize = MAX_FRAME_BYTES - 4;

pub trait Envelope: Serialize + DeserializeOwned {
    /// Reject unsupported versions and invalid identities before dispatch.
    fn validate(&self) -> Result<(), Error>;
}

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Io(io::Error),
    Json(serde_json::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Io(error) => write!(f, "frame I/O: {error}"),
            Self::Json(error) => write!(f, "frame JSON: {error}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn length(header: [u8; 4]) -> Result<usize, Error> {
    let count = u32::from_be_bytes(header) as usize;
    if !(1..=MAX_BODY_BYTES).contains(&count) {
        return Err(Error::Invalid(
            "frame exceeds total 4 MiB or has empty body",
        ));
    }
    Ok(count)
}

fn decode<T: Envelope>(body: &[u8]) -> Result<T, Error> {
    let message: T = serde_json::from_slice(body).map_err(Error::Json)?;
    message.validate()?;
    Ok(message)
}

/// Include the prefix in the bounded result; no partially encoded message is returned.
pub fn encode<T: Envelope>(message: &T) -> Result<Vec<u8>, Error> {
    struct Limited(Vec<u8>);
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_FRAME_BYTES - self.0.len() {
                return Err(io::Error::other("encoded frame exceeds 4 MiB"));
            }
            let needed = self.0.len() + bytes.len();
            if needed > self.0.capacity() {
                let capacity = needed.max(256).next_power_of_two().min(MAX_FRAME_BYTES);
                self.0
                    .try_reserve_exact(capacity - self.0.len())
                    .map_err(io::Error::other)?;
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    message.validate()?;
    let mut output = Limited(vec![0; 4]);
    serde_json::to_writer(&mut output, message).map_err(Error::Json)?;
    let count = (output.0.len() - 4) as u32;
    output.0[..4].copy_from_slice(&count.to_be_bytes());
    Ok(output.0)
}

/// Read exactly one frame, allowing clean EOF only before its first header byte.
/// Blocking readers need an owner-imposed deadline; this function does not create threads.
pub fn read<T: Envelope>(reader: &mut impl Read) -> Result<Option<T>, Error> {
    let mut header = [0; 4];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    reader.read_exact(&mut header[1..])?;
    let count = length(header)?;
    let mut body = Vec::new();
    body.try_reserve_exact(count)
        .map_err(|_| Error::Invalid("frame allocation failed"))?;
    body.resize(count, 0);
    reader.read_exact(&mut body)?;
    decode(&body).map(Some)
}

/// Incremental framing over a nonblocking owner-controlled stream.
/// Any error poisons this decoder. Already consumed bytes must never be retried.
pub struct Decoder<T> {
    header: [u8; 4],
    header_used: usize,
    body: Vec<u8>,
    expected: usize,
    poisoned: bool,
    closed: bool,
    message: PhantomData<T>,
}
impl<T: Envelope> Default for Decoder<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Envelope> Decoder<T> {
    pub const fn new() -> Self {
        Self {
            header: [0; 4],
            header_used: 0,
            body: Vec::new(),
            expected: 0,
            poisoned: false,
            closed: false,
            message: PhantomData,
        }
    }

    /// Return consumed byte count and at most one complete envelope.
    /// The caller must feed the unconsumed suffix, even if it contains another full frame.
    pub fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<T>), Error> {
        if self.poisoned || self.closed {
            return Err(Error::Invalid("decoder poisoned or closed"));
        }
        let result = self.feed_inner(input);
        if result.is_err() {
            self.poisoned = true;
            self.body = Vec::new();
        }
        result
    }

    fn feed_inner(&mut self, input: &[u8]) -> Result<(usize, Option<T>), Error> {
        let mut used = 0;
        if self.header_used < 4 {
            let count = (4 - self.header_used).min(input.len());
            self.header[self.header_used..self.header_used + count]
                .copy_from_slice(&input[..count]);
            self.header_used += count;
            used += count;
            if self.header_used != 4 {
                return Ok((used, None));
            }
            self.expected = length(self.header)?;
            self.body
                .try_reserve_exact(self.expected)
                .map_err(|_| Error::Invalid("frame allocation failed"))?;
        }
        let count = (self.expected - self.body.len()).min(input.len() - used);
        self.body.extend_from_slice(&input[used..used + count]);
        used += count;
        if self.body.len() != self.expected {
            return Ok((used, None));
        }
        let message = decode(&self.body)?;
        self.body = Vec::new();
        self.header_used = 0;
        self.expected = 0;
        Ok((used, Some(message)))
    }

    /// Mark stream EOF. Every subsequent feed/finish is rejected, including empty input.
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.poisoned || self.closed || self.header_used != 0 {
            self.poisoned = true;
            self.body = Vec::new();
            return Err(Error::Invalid(
                "incomplete frame or poisoned decoder at EOF",
            ));
        }
        self.closed = true;
        Ok(())
    }
}
