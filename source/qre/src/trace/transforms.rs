// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod lattice_surgery;
mod psspc;

pub use lattice_surgery::LatticeSurgery;
pub use psspc::PSSPC;

use crate::{Error, Trace};

pub trait TraceTransform {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error>;
}
