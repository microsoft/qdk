// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PauliSum`]: the observable an [`MpsEngine`](crate::MpsEngine) takes an
//! expectation of, `O = c_I·I + Σₖ cₖ·Pₖ`, on sites.
//!
//! The crate owns this type because the engine sits below every simulator:
//! it cannot borrow a simulator's observable without depending on it. A
//! caller that speaks of qubits converts its observable into this one at the
//! boundary, placing each qubit's factor on the qubit's site.

use num_complex::Complex64;

/// A single-site Pauli operator other than the identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pauli {
    /// σx.
    X,
    /// σy.
    Y,
    /// σz.
    Z,
}

impl Pauli {
    /// The operator as a row-major 2×2 matrix, `M[out][in]`, the layout
    /// [`MpsEngine::apply`](crate::MpsEngine::apply) takes.
    #[must_use]
    pub fn matrix(self) -> [Complex64; 4] {
        const ZERO: Complex64 = Complex64::new(0.0, 0.0);
        const ONE: Complex64 = Complex64::new(1.0, 0.0);
        const MINUS_ONE: Complex64 = Complex64::new(-1.0, 0.0);
        const I: Complex64 = Complex64::new(0.0, 1.0);
        const MINUS_I: Complex64 = Complex64::new(0.0, -1.0);
        match self {
            Self::X => [ZERO, ONE, ONE, ZERO],
            Self::Y => [ZERO, MINUS_I, I, ZERO],
            Self::Z => [ONE, ZERO, ZERO, MINUS_ONE],
        }
    }
}

/// Why a term or a sum was not built.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum PauliError {
    /// A term with no factor. An identity term is the sum's identity
    /// coefficient instead, so that no engine needs an identity operator.
    #[error("a Pauli term needs at least one non-identity factor")]
    NoFactors,
    /// A site named twice in one term; the product would not be a Pauli
    /// string on distinct sites.
    #[error("site {site} is named twice in one Pauli term")]
    RepeatedSite {
        /// The site named a second time.
        site: usize,
    },
    /// A coefficient whose real or imaginary part is NaN or infinite.
    #[error("the coefficient {0} is not finite")]
    NonFinite(Complex64),
}

/// One product `c·P_{s₁}⊗P_{s₂}⊗…` with at least one factor, on distinct
/// sites.
#[derive(Clone, Debug, PartialEq)]
pub struct PauliTerm {
    coefficient: Complex64,
    factors: Vec<(usize, Pauli)>,
}

impl PauliTerm {
    /// `coefficient·Π P` over `factors`, each a (site, Pauli) pair, in any
    /// site order.
    ///
    /// # Errors
    ///
    /// [`PauliError::NonFinite`] for a coefficient that is not finite;
    /// [`PauliError::NoFactors`] for no factor; and
    /// [`PauliError::RepeatedSite`] for the first site named a second time.
    /// Which error a term with several faults gets is unspecified.
    pub fn new(coefficient: Complex64, factors: Vec<(usize, Pauli)>) -> Result<Self, PauliError> {
        if !coefficient.is_finite() {
            return Err(PauliError::NonFinite(coefficient));
        }
        if factors.is_empty() {
            return Err(PauliError::NoFactors);
        }
        for (at, &(site, _)) in factors.iter().enumerate() {
            if factors[..at].iter().any(|&(earlier, _)| earlier == site) {
                return Err(PauliError::RepeatedSite { site });
            }
        }
        Ok(Self {
            coefficient,
            factors,
        })
    }

    /// The term's coefficient.
    #[must_use]
    pub fn coefficient(&self) -> Complex64 {
        self.coefficient
    }

    /// The factors, as (site, Pauli), in the order they were given.
    #[must_use]
    pub fn factors(&self) -> &[(usize, Pauli)] {
        &self.factors
    }
}

/// A validated Pauli sum `O = c_I·I + Σₖ cₖ·Pₖ`. The coefficients are
/// complex, so a non-Hermitian `O` (σ⁺ = (X + iY)/2) has a complex ⟨O⟩;
/// each Pauli string is Hermitian, so `⟨O⟩ = c_I + Σₖ cₖ·⟨Pₖ⟩` with every
/// ⟨Pₖ⟩ real.
///
/// Terms are kept as given: two terms on the same Pauli string are not
/// merged, which changes no expectation.
#[derive(Clone, Debug, PartialEq)]
pub struct PauliSum {
    identity_coefficient: Complex64,
    terms: Vec<PauliTerm>,
}

impl PauliSum {
    /// `identity_coefficient·I + Σ terms`.
    ///
    /// # Errors
    ///
    /// [`PauliError::NonFinite`] for an identity coefficient that is not
    /// finite (each term's was checked when it was built).
    pub fn new(identity_coefficient: Complex64, terms: Vec<PauliTerm>) -> Result<Self, PauliError> {
        if !identity_coefficient.is_finite() {
            return Err(PauliError::NonFinite(identity_coefficient));
        }
        Ok(Self {
            identity_coefficient,
            terms,
        })
    }

    /// The identity's coefficient, `c_I`.
    #[must_use]
    pub fn identity_coefficient(&self) -> Complex64 {
        self.identity_coefficient
    }

    /// The terms with at least one factor, in the order they were given.
    #[must_use]
    pub fn terms(&self) -> &[PauliTerm] {
        &self.terms
    }
}
