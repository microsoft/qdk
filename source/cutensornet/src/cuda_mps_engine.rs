// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): [`CudaMpsEngine`], `qdk_cpu_mps`'s
//! MPS algorithm on cuTensorNet's tensor API, to measure whether its
//! splits, run as `GateSplit` calls on the GPU, beat both the CPU engine
//! and [`CuTensorNetEngine`](crate::CuTensorNetEngine) (the state API) on
//! the large bonds of the polyene and H₂ runs.
//!
//! It keeps the CPU engine's rules, each marked where it is ported:
//! - SwapBack routing (`swap-back-2x`) of gates on distant sites, and the
//!   orthogonality centre moved into each pair before its gate;
//! - the relative discarded-weight cutoff (1e-14, the state-API engine's
//!   `DISCARDED_WEIGHT_CUTOFF`, which is the CPU engine's default) and the
//!   capacity's bond, the outbound SWAPs applying the cutoff alone;
//! - fusion: the gates on one pair, one-site gates included, are one
//!   `GateSplit` at the pair's first other operation;
//! - the reads: each value's probability from its own weight at the
//!   centre, `project`'s scaled normalisation, conditions on a fork, and
//!   expectations divided by the centre's weight;
//! - `shape` from the host's extents, and the retained weight multiplied by
//!   `1 − discarded` at each truncating split.
//!
//! What differs, because the device has no small kernels of its own:
//! - one `GateSplit` per split, which contracts the pair and applies the
//!   gate itself, so a fused pair is held as a 4×4 gate on the host, not
//!   as a contracted block;
//! - one-site gates wait on the host, per site, until the next `GateSplit`
//!   on the site takes them into its gate, or a read applies them to its
//!   downloaded copy;
//! - the centre moves by a `GateSplit` with no cutoff, whose bond is at
//!   most `min(2·left, 2·right)` and the old bond, not by a QR;
//! - reads download the centre site (and, for an expectation, the sites
//!   between the observable and the centre) and compute on the host.
//!
//! The tensor calls are behind [`TensorCalls`], whose one implementation
//! is `DeviceTensors` on the GPU ([`CudaMpsEngine::new`]). The engine is
//! tested on the GPU only (`tests/cuda_mps_on_the_gpu.rs`).

mod device_chain;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod device_tensors;
mod error;
mod gates;
mod reads;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod self_checks;
mod split;
mod tensor_calls;

pub use device_chain::DeviceMps;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub use device_tensors::DeviceTensors;
pub use error::CudaMpsError;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub use self_checks::{SelfCheck, self_check};
pub use tensor_calls::{GateSplitCall, Partition, SplitOutcome, TensorCalls};

use device_chain::DeviceChain;
use num_complex::Complex64;
use std::cell::{Cell, RefCell};
use std::time::Instant;
use tensornet::{MpsEngine, MpsShape, PauliSum, checks, sampling};

/// `qdk_cpu_mps`'s MPS algorithm on cuTensorNet's tensor API (see the
/// module's documentation). Neither the engine nor its state is `Send`.
pub struct CudaMpsEngine<T> {
    tensors: T,
    cutoff: f64,
    counters: Cell<CudaMpsCounters>,
}

/// What a [`CudaMpsEngine`] did since it was made, or since
/// [`CudaMpsEngine::reset_counters`].
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CudaMpsCounters {
    /// The truncating splits (a gate's, or a route's SWAP's).
    pub splits: u64,
    /// The centre's moves, one site each.
    pub centre_moves: u64,
    /// The route's SWAPs.
    pub swaps: u64,
    /// The seconds spent in `GateSplit` calls, splits and moves.
    pub gate_split_seconds: f64,
    /// The seconds spent in reads, downloads and host work included.
    pub read_seconds: f64,
    /// The largest bond a split kept.
    pub largest_bond: usize,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl CudaMpsEngine<DeviceTensors> {
    /// An engine on device 0, on the libraries `availability` loaded.
    ///
    /// # Errors
    ///
    /// When a function the engine needs is missing, or the device's setup
    /// fails.
    pub fn new(availability: &crate::Availability) -> Result<Self, CudaMpsError> {
        Ok(Self::with_tensors(DeviceTensors::open(availability)?))
    }
}

impl<T: TensorCalls> CudaMpsEngine<T> {
    /// The engine on `tensors`, with the cutoff `1e-14`.
    #[doc(hidden)]
    pub fn with_tensors(tensors: T) -> Self {
        Self {
            tensors,
            cutoff: crate::engine::policy::DISCARDED_WEIGHT_CUTOFF,
            counters: Cell::new(CudaMpsCounters::default()),
        }
    }

    /// What the engine did since it was made or reset.
    #[doc(hidden)]
    pub fn counters(&self) -> CudaMpsCounters {
        self.counters.get()
    }

    /// Resets [`Self::counters`].
    #[doc(hidden)]
    pub fn reset_counters(&self) {
        self.counters.set(CudaMpsCounters::default());
    }

    fn count(&self, update: impl FnOnce(&mut CudaMpsCounters)) {
        let mut counters = self.counters.get();
        update(&mut counters);
        self.counters.set(counters);
    }

    /// One `GateSplit`, timed.
    fn timed_gate_split(
        &self,
        call: &GateSplitCall<'_, T::Buffer>,
    ) -> Result<SplitOutcome<T::Buffer>, CudaMpsError> {
        let started = Instant::now();
        let outcome = self.tensors.gate_split(call)?;
        let seconds = started.elapsed().as_secs_f64();
        self.count(|counters| counters.gate_split_seconds += seconds);
        Ok(outcome)
    }

    /// P(Zero) and P(One) of `site` given `given`, on a fork when `given`
    /// is not empty, as `qdk_cpu_mps`'s `conditional_probabilities`.
    fn conditional_probabilities(
        &self,
        state: &DeviceMps<T::Buffer>,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<[f64; 2], CudaMpsError> {
        if given.is_empty() {
            return self.probabilities(&mut state.chain.borrow_mut(), site);
        }
        let mut fork = self.fork(state)?;
        let chain = fork.chain.get_mut();
        for &(at, one) in given {
            self.project_on(chain, at, one)?;
        }
        self.probabilities(chain, site)
    }
}

impl<T: TensorCalls> MpsEngine for CudaMpsEngine<T> {
    type State = DeviceMps<T::Buffer>;
    type Error = CudaMpsError;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, CudaMpsError> {
        checks::prepare(capacity)?;
        let capacity_bonds = (0..capacity.site_count() - 1)
            .map(|cut| capacity.bond_dim(cut).expect("a cut of the capacity"))
            .collect();
        Ok(DeviceMps {
            chain: RefCell::new(DeviceChain::zero(&self.tensors, capacity_bonds)?),
        })
    }

    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), CudaMpsError> {
        let chain = state.chain.get_mut();
        checks::apply(chain.sites.len(), sites, matrix)?;
        match *sites {
            [site] => {
                chain.apply_one(site, matrix);
                Ok(())
            }
            [first, second] => self.apply_pair(chain, first, second, matrix),
            _ => unreachable!("`checks::apply` refuses an operator on no site or more than two"),
        }
    }

    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, CudaMpsError> {
        let mut chain = state.chain.borrow_mut();
        checks::probability_of_one(chain.sites.len(), site)?;
        Ok(self.probabilities(&mut chain, site)?[1])
    }

    fn conditional_probability_of_one(
        &self,
        state: &Self::State,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, CudaMpsError> {
        let site_count = state.chain.borrow().sites.len();
        checks::conditional_probability_of_one(site_count, site, given)?;
        Ok(self.conditional_probabilities(state, site, given)?[1])
    }

    fn project(
        &self,
        state: &mut Self::State,
        site: usize,
        one: bool,
    ) -> Result<f64, CudaMpsError> {
        let chain = state.chain.get_mut();
        checks::project(chain.sites.len(), site)?;
        self.project_on(chain, site, one)
    }

    /// Splits a fused pair first, so the forks do not each split it.
    fn fork(&self, state: &Self::State) -> Result<Self::State, CudaMpsError> {
        let mut chain = state.chain.borrow_mut();
        let centre = chain.centre;
        self.split_pair_toward(&mut chain, centre)?;
        Ok(DeviceMps {
            chain: RefCell::new(chain.duplicate(&self.tensors)?),
        })
    }

    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, CudaMpsError> {
        checks::sample(state.chain.borrow().sites.len(), sites)?;
        sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            Ok(self.conditional_probabilities(state, site, given)?[1])
        })
    }

    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, CudaMpsError> {
        let mut chain = state.chain.borrow_mut();
        checks::expectation(chain.sites.len(), observable)?;
        self.expectation_of(&mut chain, observable)
    }

    fn shape(&self, state: &Self::State) -> Result<MpsShape, CudaMpsError> {
        let mut chain = state.chain.borrow_mut();
        let centre = chain.centre;
        self.split_pair_toward(&mut chain, centre)?;
        let n = chain.sites.len();
        let sites = chain
            .sites
            .iter()
            .enumerate()
            .map(|(at, site)| {
                let left = (at > 0).then_some(site.left);
                let right = (at + 1 < n).then_some(site.right);
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        Ok(MpsShape::new(sites).expect("a chain whose neighbours share each bond"))
    }

    fn retained_weight(&self, state: &Self::State) -> Result<f64, CudaMpsError> {
        let mut chain = state.chain.borrow_mut();
        let centre = chain.centre;
        self.split_pair_toward(&mut chain, centre)?;
        Ok(chain.retained_weight)
    }
}
