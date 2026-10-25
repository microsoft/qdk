//! A backend-agnostic description of tensor networks, by shape alone.
//!
//! Today it describes one family: an [`MpsShape`] is the shape of a chain
//! of site tensors (a matrix product state), the extents of its sites.
//! Nothing here knows how a chain is built, truncated or contracted, and
//! nothing here holds tensor elements, so the same description can be
//! handed to a GPU library, to a CPU library, or to a reference
//! implementation written purely for a test. A method describes its state
//! with it, and a caller compares a requested capacity with the shape a
//! backend produced, without touching the data.
//!
//! Beside the shape, it states what a library that holds matrix product
//! states does for a simulator ([`MpsEngine`]), with the observable it reads
//! ([`PauliSum`]) and an exact engine every other one is checked against
//! ([`reference::ExactReference`]). None of these knows a simulator's
//! protocol, a device or a vendor.
//!
//! It depends only on `thiserror` and `num-complex`. The crate's
//! `README.md` explains the model and its choices. When a module or a public
//! type is added, moved or renamed, update the README's "Files" table with
//! it.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod engine;
mod error;
mod mps;
mod pauli;
pub mod reference;

pub use engine::{MpsEngine, SampleSeed, SeedOutOfRange};
pub use error::MpsShapeError;
pub use mps::MpsShape;
pub use pauli::{Pauli, PauliError, PauliSum, PauliTerm};
