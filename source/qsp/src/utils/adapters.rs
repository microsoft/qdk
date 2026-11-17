// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod result_adapter;
mod transpiling_adapter;

pub use result_adapter::ResultAdapter;
pub use transpiling_adapter::TranspilingAdapter;

use crate::{QuantumBackend, interface::Driver};

/// Adapter is something that is both `Driver` and `QuantumBackend`.
pub trait Adapter: Driver + QuantumBackend {}

impl<T: Driver + QuantumBackend> Adapter for T {}
