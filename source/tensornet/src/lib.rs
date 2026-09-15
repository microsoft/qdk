//! A backend-agnostic description of a tensor network.
//!
//! Two families of description, and no more. A general network is built from
//! four concepts: an [`Index`] is an axis, [`Indices`] is an ordered list of
//! them, a [`TensorNetwork`] is nodes joined by index identity, and a
//! [`ContractionQuery`] says what to compute from one. Alongside them, and not
//! beneath them, an [`Mps`] describes a chain of site tensors.
//!
//! [`Mps`] is a peer of [`TensorNetwork`] rather than a case of it. A network
//! can express a chain's topology but not the invariants that make it a matrix
//! product state, and a chain cannot express an arbitrary topology, so neither
//! type subsumes the other.
//!
//! Nothing here knows how a network is contracted. There are no paths, no
//! slicing, no cost model and no execution, so the same description can be
//! handed to a GPU library, to a CPU contractor, or to a reference
//! implementation written purely for a test. It carries no tensor elements
//! either: planning a contraction needs the shape of the problem and nothing
//! else, so a caller can ask whether a network is feasible without allocating
//! anything.
//!
//! The semantics are einsum's, with one departure from the physicists'
//! reading worth stating up front: a repeated index is *identification*, not
//! contraction. Carrying the same index in two nodes says they are the same
//! axis; it is [`ContractionQuery`] that decides which axes are summed, by
//! leaving them out of `keep`.

mod contraction;
mod error;
mod index;
mod mps;
mod network;

pub use contraction::ContractionQuery;
pub use error::{ContractionError, MpsError, NetworkError};
pub use index::{Index, Indices};
pub use mps::Mps;
pub use network::TensorNetwork;
