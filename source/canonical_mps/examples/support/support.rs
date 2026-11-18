// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the examples share: the circuits they measure the engine on, the
//! runs of those circuits on the engine and on the sparse simulator, the
//! exact state vector and recorded distributions they compare with, and
//! their command lines. Each example uses a part.

#![allow(dead_code, reason = "each example uses a part")]

// The workspace's lints refuse `mod.rs` files, and Cargo would take an
// `examples/support.rs` for an example of its own: so this file sits in
// its folder, and names its parts' files, beside it.
#[path = "agreement.rs"]
pub mod agreement;
#[path = "args.rs"]
pub mod args;
#[path = "circuit.rs"]
pub mod circuit;
#[path = "dense.rs"]
pub mod dense;
#[path = "families.rs"]
pub mod families;
#[path = "gates.rs"]
pub mod gates;
#[path = "readout.rs"]
pub mod readout;
#[path = "recordings.rs"]
pub mod recordings;
#[path = "rng.rs"]
pub mod rng;
#[path = "runs.rs"]
pub mod runs;

use num_complex::Complex64;
use std::error::Error;

pub type C = Complex64;
pub type Res<T> = Result<T, Box<dyn Error>>;

pub fn c(re: f64, im: f64) -> C {
    C::new(re, im)
}
