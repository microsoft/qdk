// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol: parts that compose with `>>` into a chain that
//! runs shots.
//!
//! The crate's `README.md` and its `docs/` pages say what the protocol is
//! for, the problem it solves and how, and give its rules.

mod batch;
mod composition;
mod instruction_set;
mod needs;
mod offer;
mod roles;
mod seed;

#[cfg(test)]
#[expect(
    clippy::unused_async_trait_impl,
    reason = "a test part that never waits keeps the trait's `async fn` form, the form the \
              protocol documents; the `core::future::ready` the lint suggests would run the \
              body when the method is called, not when its future is polled"
)]
mod tests;

pub use batch::{
    Batch, BatchReadouts, ReadoutFailure, ReadoutSlot, ReadoutValue, Readouts, ShotConditions,
    ShotId, ShotSet, ShotSetFault,
};
pub use composition::{
    Adapter, AdmissionFaults, Compose, Composed, CompositionFaults, Driver, Executor, PartId,
    RunError, RunRefusal, Runner, ShortOffer,
};
pub use instruction_set::{
    Angle, Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, FitFaults,
    Guard, Instruction, InstructionDef, InstructionFault, InstructionMismatch, InstructionSet,
    Meaning, MeaningFault, Observable, Opcode, OpcodeMap, Parameter, ParameterKind, Pauli,
    ReadoutParity, SelectPattern,
};
pub use needs::{Needs, UnmetNeed};
pub use offer::Offer;
pub use roles::{
    Adapt, AdapterSetup, Drive, DriveError, DriverSetup, Execute, ExecuteError, ExecuteRefusal,
    ExecutorSetup, Failure, NotReady, RetryClass,
};
pub use seed::Seed;
