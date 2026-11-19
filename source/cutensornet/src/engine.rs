// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CudaMpsEngine`]: the `tensornet::MpsEngine` on one CUDA device,
//! `canonical_mps`'s canonical-form MPS engine on [`CudaAlgebra`], its
//! site tensors in device memory and their operations run by cuTensorNet.
//!
//! An MPS simulation method (the algorithm that runs a program on an MPS)
//! needs an engine that holds the chain, applies gates, truncates, and reads
//! probabilities. `canonical_mps` writes that engine once, over the five
//! tensor operations of a `SiteAlgebra`; this module gives it one on the
//! GPU, so an MPS simulation method written against the trait runs on the
//! GPU with no change to its code, and reads as it does on the CPU's
//! algebra up to rounding.
//!
//! ```text
//! CudaMpsEngine = CanonicalMpsEngine<CudaAlgebra>   (canonical_mps)
//! CudaAlgebra ── holds ──► DeviceSession   device, stream, handle
//!                                          (device_session.rs)
//!             ── makes ──► DeviceTensor    one site in a device buffer
//!                                          (cuda_algebra/)
//! DeviceSession and CudaAlgebra
//!   └─ every native call ──────► NativeCalls     CUDA and cuTensorNet
//!                                                (native_calls.rs)
//! all three fail with ─────────► CudaAlgebraError
//!                                                (cuda_algebra_error.rs)
//! ```
//!
//! The files here hold the cuTensorNet mechanics: no circuit or shot, only
//! site tensors and their operations. The session and the algebra make
//! their native calls through `NativeCalls`, so the algebra's real code
//! also runs in CI, which has no GPU. The crate's `docs/engine.md`
//! ("Native calls of each phase") shows the native calls in order.
//!
//! An engine, its algebra and its tensors are `!Send`: the stream and the
//! handle stay on the thread that created them. The compiler enforces it;
//! each block below is a test that must fail to compile, one for the
//! engine, one for its algebra and one for its tensors.
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CudaMpsEngine>();
//! ```
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CudaAlgebra>();
//! ```
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::DeviceTensor>();
//! ```
//!
//! Each call of the algebra selects its device first, since CUDA's current
//! device is per thread and another algebra's call may have changed it.

pub(crate) mod cuda_algebra;
pub(crate) mod cuda_algebra_error;
mod device_session;
pub(crate) mod native_calls;

pub use cuda_algebra::{CudaAlgebra, DeviceTensor};
pub use cuda_algebra_error::CudaAlgebraError;

// What the tests under `src/tests` name of the engine's private modules:
// the session.
#[cfg(test)]
pub(crate) use self::device_session::DeviceSession;

/// The `tensornet::MpsEngine` on one CUDA device:
/// `canonical_mps::CanonicalMpsEngine` on a [`CudaAlgebra`].
///
/// Open one on a discovery's [`Availability`](crate::Availability) (Linux
/// x86-64) with `CudaMpsEngine::try_from(&availability)`, at the engine's
/// default cutoff; `.with_cutoff(c)` sets another. What the engine
/// promises beyond the contract (how it fuses gates, the bounds its
/// truncations stay within) is `canonical_mps`'s; what this crate adds is
/// the algebra.
///
/// Its states' sites are its algebra's tensors, which another algebra
/// refuses: a method given another engine's state that passes its sites to
/// the algebra (a fork, for instance) returns
/// `CanonicalMpsError::Algebra(CudaAlgebraError::ForeignTensor)`.
///
/// It runs only where its libraries do, so this example is compiled on
/// Linux x86-64, and run by no test:
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

/// An engine on device 0, at the engine's default cutoff, on the libraries
/// `availability` loaded and checked: [`CudaAlgebra::new`]'s algebra.
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
