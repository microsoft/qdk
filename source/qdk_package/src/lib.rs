// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![expect(
    clippy::clone_on_copy,
    reason = "PyO3's from_py_object generates Clone calls even for Copy pyclasses"
)]

allocator::assign_global!();

mod displayable_output;
mod fs;
mod generic_estimator;
mod interop;
mod interpreter;
mod noisy_simulator;
mod openqasm;
mod qir_simulation;
mod qre;
