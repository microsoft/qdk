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
mod lowering;
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
    Batch, BatchReadouts, ReadoutSlot, ReadoutSlots, Readouts, ShotId, ShotSet, ShotSetFault,
};
pub use boundary_compatible::BoundaryCompatible;
pub(crate) use capabilities::Boundary;
pub use capabilities::{
    BatchFault, Capabilities, CapabilitiesMismatch, FeatureMismatch, Features, ResourceMismatch,
    Resources, RunRequirement, StatesLimits,
};
pub use composition::{
    AdapterJoins, BoundaryMismatch, CompositionFault, CompositionFaults, DriverJoins, RunError,
    RunRefusal, Runner,
};
pub use instruction_set::{Binding, InstructionSet};
pub use lowering::{Lowered, Lowering};
pub use part_id::PartId;
pub use roles::{
    Adapter, AdapterLimits, DriveError, Driver, DriverReadings, DriverRequirements, ExecuteError,
    ExecuteRefusal, Executor, ExecutorLimits, ExecutorReadings, Failure, NotReady,
    TranslatingAdapter,
};
pub use seed::Seed;
