//! Independent DEV4 lifecycle records and canonical state/input serialization.
//! This module does not establish SQL execution, engine durability, or FE14 exploration completion.

mod codec;
mod input_schema;
mod schema;
mod workspace;

#[cfg(test)]
mod literal;
#[cfg(test)]
mod literal_tests;
#[cfg(test)]
mod workspace_tests;

pub use codec::{CodecError, decode_input, decode_state, encode_input, encode_state};
pub use input_schema::*;
pub use schema::*;
pub use workspace::*;
