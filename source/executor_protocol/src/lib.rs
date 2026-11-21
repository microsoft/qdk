// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor protocol: parts joined with `through` and `on` into a
//! chain that runs shots.
//!
//! The crate's `README.md` and its `docs/` pages say what the protocol is
//! for, the problem it solves and how, and give its rules.

mod batch;
mod boundary_compatible;
mod capabilities;
mod composition;
mod instruction_set;
mod part_id;
pub mod prelude;
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
pub use boundary_compatible::BoundaryCompatible;
pub use capabilities::{
    BatchFault, Boundary, Capabilities, CapabilitiesMismatch, Feature, FeatureMismatch, Features,
    Resource, ResourceMismatch, Resources, RunProvision, StatementMismatch, Statements,
};
pub use composition::{
    AdapterJoins, CompositionFault, CompositionFaults, DriverJoins, RunError, RunRefusal, Runner,
};
pub use instruction_set::{
    Angle, Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, Guard,
    Instruction, InstructionDef, InstructionFault, InstructionMismatch, InstructionSet, Meaning,
    MeaningFault, Observable, Opcode, OpcodeMap, Parameter, ParameterKind, Pauli, ReadoutParity,
    SelectPattern,
};
pub use part_id::PartId;
pub use roles::{
    Adapter, DriveError, Driver, ExecuteError, ExecuteRefusal, Executor, Failure, NotReady,
};
pub use seed::Seed;
