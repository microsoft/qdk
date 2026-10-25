// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tooling built on the execution core for checking simulation methods: tests
//! and GPU qualification use it as a reference, and no method depends on it.

mod conditioned_circuit;

pub use conditioned_circuit::{
    ConditionedCircuit, ConditionedCircuitError, ConditionedOperation, ConditioningError,
};
