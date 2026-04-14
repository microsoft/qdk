// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Public QIR facade.

mod build;
pub(crate) mod inspect;
mod spec;

pub use build::{double_op, i64_op, qubit_op, result_op, void_call};
pub use inspect::{
    extract_float, extract_id, find_entry_point, get_function_attribute, operand_key,
};
pub use spec::*;
