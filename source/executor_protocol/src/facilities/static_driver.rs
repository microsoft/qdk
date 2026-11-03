// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The static driver ([`StaticDriver`]), which
//! [`static_driver()`](fn@static_driver) makes a chain's prefix: it runs a
//! [`StaticProgram`]. Its impl is with the program ([`StaticProgram`]).

#[cfg(doc)]
use crate::{Chain, Driver, DriverExt, StaticProgram};
use crate::{ChainPrefix, Instruction, NoAdapter};

/// The static driver, as a chain's prefix with no adapter yet: composed
/// with an executor, through adapters or not, it gives a [`Chain`] that
/// runs a [`StaticProgram`] of `I`s, `chain.run(program, shots)`. `I` is
/// fixed by what it is composed with.
#[must_use]
pub fn static_driver<I: Instruction>() -> ChainPrefix<StaticDriver, NoAdapter<I>> {
    ChainPrefix::new(StaticDriver)
}

/// The static driver, the one [`static_driver()`](fn@static_driver) makes
/// a chain's prefix: it drives a [`StaticProgram`] only, each batch to
/// every shot, in order ([`Driver`]). A program of another form has a
/// driver of its own, whose chain starts at `runtime.into_chain()`
/// ([`DriverExt`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticDriver;
