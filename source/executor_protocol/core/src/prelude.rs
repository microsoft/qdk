// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The traits a part's writer and a chain's builder use, to import at
//! once: `use executor_protocol_core::prelude::*;`.
//!
//! The roles ([`Driver`], [`Adapter`], [`Executor`]) are what a part
//! implements; the joins ([`DriverJoins`], [`AdapterJoins`]) give every
//! part `through` and `on`, and [`BoundaryCompatible`] gives
//! [`Capabilities`](crate::Capabilities) and
//! [`InstructionSet`](crate::InstructionSet) their `fits`, which a method
//! call reaches only when its trait is in scope.

pub use crate::{Adapter, AdapterJoins, BoundaryCompatible, Driver, DriverJoins, Executor};
