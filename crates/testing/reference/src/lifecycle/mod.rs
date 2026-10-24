//! Independent DEV4 lifecycle records and canonical state/input serialization.
//! This module does not establish SQL execution, engine durability, or FE14 exploration completion.

mod codec;
mod input_schema;
mod raw_action;
mod schema;
mod target_selector;
mod workspace;

#[cfg(test)]
mod literal;
#[cfg(test)]
mod literal_tests;
#[cfg(test)]
mod workspace_tests;

#[cfg(test)]
mod raw_test_support;
#[cfg(test)]
mod raw_tests;
#[cfg(test)]
mod selector_literal;
#[cfg(test)]
mod selector_tests;

pub use codec::{CodecError, decode_input, decode_state, encode_input, encode_state};
pub use input_schema::*;
pub use raw_action::{
    PrepareChoice, RAW_ACTION_COUNT, RawAction, RawActionError, action_at, index_of,
};
pub use schema::*;
pub use target_selector::{SelectorError, TargetKind, TargetSelector, resolve_selector};
pub use workspace::*;
