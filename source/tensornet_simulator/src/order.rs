// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order: which site of the engine's chain holds each qubit.

use qdk_simulators::QubitID;

/// Where a terminal places each qubit on the chain of sites of its matrix
/// product state: a permutation of the qubits, qubit `q` on site
/// [`site(q)`](Self::site).
///
/// An MPS is a chain, so its cost depends on where the qubits sit: a
/// two-qubit operator on sites at distance d is applied through about
/// 2(d − 1) swaps of neighbouring sites, and the bond at each cut is the
/// Schmidt rank of the state across it, which depends on which qubits fall
/// on each side. The same state is held by every order, so the values a
/// terminal draws and reads do not depend on it (to rounding, and within
/// the truncation error of an engine that truncates); only the bonds the
/// state needs, and so the time and the weight a bond cap discards, do.
///
/// The [`identity`](Self::identity) order, qubit `q` on site `q`, is the
/// default; a terminal with it hands the engine the same sites as a
/// terminal without an order. Every value a terminal reports stays keyed by
/// qubit and result tag; the sites appear only where the engine's state
/// does ([`TensorNetTerminal::state`](crate::TensorNetTerminal::state),
/// [`MeasurementRun::sites`](crate::MeasurementRun::sites)).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QubitOrder {
    /// The site of each qubit.
    sites: Vec<usize>,
}

impl QubitOrder {
    /// Qubit `q` on site `q`, for `qubits` qubits.
    #[must_use]
    pub fn identity(qubits: usize) -> Self {
        Self {
            sites: (0..qubits).collect(),
        }
    }

    /// The order that places qubit `q` on site `sites[q]`.
    ///
    /// # Errors
    ///
    /// When `sites` is not a permutation of `0..sites.len()`, which a chain
    /// of one site per qubit needs (each site holds one qubit, each qubit
    /// one site): [`QubitOrderError::SiteBeyond`] for the first qubit on a
    /// site beyond the chain, else [`QubitOrderError::SharedSite`] for the
    /// first site two qubits share.
    pub fn new(sites: Vec<usize>) -> Result<Self, QubitOrderError> {
        let qubits = sites.len();
        let mut holder: Vec<Option<QubitID>> = vec![None; qubits];
        for (qubit, &site) in sites.iter().enumerate() {
            let Some(held) = holder.get_mut(site) else {
                return Err(QubitOrderError::SiteBeyond {
                    qubit,
                    site,
                    qubits,
                });
            };
            if let Some(first) = *held {
                return Err(QubitOrderError::SharedSite {
                    first,
                    second: qubit,
                    site,
                });
            }
            *held = Some(qubit);
        }
        Ok(Self { sites })
    }

    /// The site of `qubit`. A qubit beyond the order's is on its own site
    /// (the order extends to every qubit as the identity), so that a
    /// terminal never needs a fallback; terminals refuse such qubits before
    /// they place them.
    #[must_use]
    pub fn site(&self, qubit: QubitID) -> usize {
        self.sites.get(qubit).copied().unwrap_or(qubit)
    }

    /// The site of each qubit, qubit 0 first.
    #[must_use]
    pub fn sites(&self) -> &[usize] {
        &self.sites
    }

    /// The sites of `qubits`, in the same order: the sites an operator on
    /// `qubits` is applied to, so that its matrix, defined over its qubits
    /// in that order, stays valid on them.
    #[must_use]
    pub fn sites_of(&self, qubits: &[QubitID]) -> Vec<usize> {
        qubits.iter().map(|&qubit| self.site(qubit)).collect()
    }

    /// The qubits it places.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// Whether it places no qubit.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Whether each qubit is on its own site.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.sites
            .iter()
            .enumerate()
            .all(|(qubit, &site)| qubit == site)
    }
}

/// Why a [`QubitOrder`] cannot place the qubits it was given.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QubitOrderError {
    /// A qubit on a site beyond the chain: an order of n qubits places them
    /// on sites 0..n.
    #[error(
        "qubit {qubit} is placed on site {site}, beyond the {qubits} sites of a chain of \
         {qubits} qubits: an order is a permutation of 0..{qubits}"
    )]
    SiteBeyond {
        /// The qubit.
        qubit: QubitID,
        /// Its site.
        site: usize,
        /// The order's qubits, and so the chain's sites.
        qubits: usize,
    },
    /// Two qubits on one site: each site of the chain holds one qubit.
    #[error(
        "qubits {first} and {second} are both placed on site {site}: an order is a \
         permutation, each site holding one qubit"
    )]
    SharedSite {
        /// The first qubit on the site.
        first: QubitID,
        /// The second.
        second: QubitID,
        /// The site.
        site: usize,
    },
    /// An order of another number of qubits than the terminal's.
    #[error("the order places {order} qubits, but the terminal has {qubits}")]
    Qubits {
        /// The order's qubits.
        order: usize,
        /// The terminal's.
        qubits: usize,
    },
    /// An order given to a terminal that has already run a batch: its state
    /// is on the sites of the order it had.
    #[error(
        "the terminal has already run a batch: its state is placed by its order, which \
         cannot change after the first batch"
    )]
    Started,
}
