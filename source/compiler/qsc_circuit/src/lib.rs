// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod builder;
mod circuit;
pub mod operations;

pub use builder::{CircuitTracer, TracerConfig};
pub use builder::{
    LexicalScope, LogicalStackEntryLocation, LoopIdCache, PackageOffset, Scope, SourceLookup,
};
pub use circuit::{
    CURRENT_VERSION, Circuit, CircuitGroup, ComponentColumn, Operation, SourceLocation,
    operation_list_to_grid,
};
pub use operations::Error;
pub use qsc_rir::debug::DbgInfo;
pub mod circuit_to_qsharp;
pub mod json_to_circuit;
pub mod rir_to_circuit;
