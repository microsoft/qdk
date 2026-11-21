// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The traits a part's writer and a chain's builder use, to import at
//! once: `use executor_protocol::prelude::*;`.
//!
//! The roles ([`Driver`], [`Adapter`], [`Executor`]) are what a part
//! implements; the joins ([`DriverJoins`], [`AdapterJoins`]) give every
//! part `through` and `on`, which a method call reaches only when its
//! trait is in scope.

pub use crate::{Adapter, AdapterJoins, Driver, DriverJoins, Executor};
