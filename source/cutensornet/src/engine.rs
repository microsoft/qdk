// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CudaMpsEngine`]: the `tensornet::MpsEngine` on one CUDA device. It is
//! `canonical_mps`'s canonical-form MPS engine on [`CudaAlgebra`], whose
//! site tensors live in device memory and whose operations cuTensorNet
//! runs.
//!
//! An MPS simulation method (the algorithm that runs a program on an MPS)
//! needs an engine that holds the chain, applies gates, truncates and reads
//! probabilities. `canonical_mps` writes that engine once, over the five
//! tensor operations of a `SiteAlgebra`. This module gives it a
//! `SiteAlgebra` on the GPU, so a method written against `MpsEngine` runs
//! there unchanged.
//!
//! ```text
//! CudaMpsEngine = CanonicalMpsEngine<CudaAlgebra>   (canonical_mps)
//! CudaAlgebra   the site tensors and their operations on one device,
//!               with its device session (cuda_algebra.rs, its map)
//!   └─ every native call ──► NativeCalls        CUDA and cuTensorNet
//!                                               (native_calls.rs)
//! failures ─────────────────► CudaAlgebraError  (cuda_algebra_error.rs)
//! ```
//!
//! The algebra makes every native call through `NativeCalls`, so its real
//! code also runs in CI, which has no GPU. The crate's `docs/engine.md`
//! ("Native calls of each phase") lists the calls in order.
//!
//! An engine, its algebra and its tensors are `!Send`: the stream and the
//! handle stay on the thread that created them.

pub(crate) mod cuda_algebra;
mod cuda_algebra_error;
pub(crate) mod native_calls;

pub use cuda_algebra::{CudaAlgebra, DeviceTensor};
pub use cuda_algebra_error::CudaAlgebraError;

/// The `tensornet::MpsEngine` on one CUDA device:
/// `canonical_mps::CanonicalMpsEngine` on a [`CudaAlgebra`].
///
/// Open one on a discovery's [`Availability`](crate::Availability) (Linux
/// x86-64) with `CudaMpsEngine::try_from(&availability)`, at the engine's
/// default cutoff; `.with_cutoff(c)` sets another. Its promises beyond the
/// contract (how it fuses gates, the bounds its truncations stay within)
/// are `canonical_mps`'s.
///
/// Open one in each run's first step, on the thread that runs it, and drop
/// it at the run's end. Until then it holds device 0's stream and
/// cuTensorNet handle, the memory it lent cuTensorNet (grown to the run's
/// largest call) and the contraction networks it prepared.
///
/// It is `!Send`, as its algebra is, so it cannot leave that thread. This
/// does not compile:
///
/// ```compile_fail,E0277
/// fn require_send<T: Send>() {}
/// require_send::<qdk_cutensornet::CudaMpsEngine>();
/// ```
///
/// Its states' sites are its algebra's tensors, which another algebra
/// refuses. A method given another engine's state that passes its sites to
/// the algebra (a fork, for instance) returns
/// `CanonicalMpsError::Algebra(CudaAlgebraError::ForeignTensor)`. A
/// poisoned algebra returns `Poisoned` instead, as every method then does.
///
/// It runs only where its libraries do, so this example compiles on Linux
/// x86-64 and no test runs it:
///
/// ```no_run
/// # #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// # fn main() {
/// use qdk_cutensornet::{CudaMpsEngine, discover};
/// use tensornet::{MpsEngine, MpsShape};
///
/// let availability = discover().expect("the audited libraries are installed");
/// let engine = CudaMpsEngine::try_from(&availability).expect("a CUDA device");
/// let capacity = MpsShape::qubit_chain(3, 2).expect("three sites");
/// let state = engine.prepare(&capacity).expect("a chain of qubits");
/// assert_eq!(engine.probability_of_one(&state, 0).expect("a site"), 0.0);
/// # }
/// # #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
/// # fn main() {}
/// ```
pub type CudaMpsEngine = canonical_mps::CanonicalMpsEngine<CudaAlgebra>;

/// An engine on device 0, on the libraries `availability` loaded and
/// checked: [`CudaAlgebra::new`]'s algebra.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl TryFrom<&crate::Availability> for CudaMpsEngine {
    type Error = CudaAlgebraError;

    /// # Errors
    ///
    /// Fails as [`CudaAlgebra::new`] does.
    fn try_from(availability: &crate::Availability) -> Result<Self, CudaAlgebraError> {
        CudaAlgebra::new(availability).map(Self::new)
    }
}

#[cfg(test)]
mod tests;
