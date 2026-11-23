// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The traits a part's writer and a chain's builder use, to import at
//! once: `use executor_protocol_core::prelude::*;`.
//!
//! The roles ([`Driver`], [`Adapter`], [`Executor`]) are what a part
//! implements, with their limits ([`DriverRequirements`], [`AdapterLimits`],
//! [`ExecutorLimits`]) when it states any, and what the protocol reads of any
//! driver or executor ([`DriverReadings`], [`ExecutorReadings`]), which a part
//! that wraps one forwards; [`InstructionSet`] is what the instructions they
//! exchange implement, and [`Lowering`] what runs one set's instructions on
//! another's; the joins ([`DriverJoins`], [`AdapterJoins`]) give every part
//! `through` and `on`, and [`BoundaryCompatible`] gives
//! [`Capabilities`](crate::Capabilities) and every instruction set their
//! `fits`. A method call reaches a trait's methods only when the trait is in
//! scope.

pub use crate::{
    Adapter, AdapterJoins, AdapterLimits, BoundaryCompatible, Driver, DriverJoins, DriverReadings,
    DriverRequirements, Executor, ExecutorLimits, ExecutorReadings, InstructionSet, Lowering,
};
