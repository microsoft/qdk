// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Backend-neutral Pauli-sum observables, `O = c_I·I + Σₖ cₖ·Pₖ`.
//!
//! A backend evaluates `⟨ψ|O|ψ⟩` for its own state representation; this type
//! only fixes what `O` means, so every backend accepts the same validated input.

use crate::QubitID;
use num_complex::Complex64;
use std::fmt;

/// A single-qubit Pauli operator other than the identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Pauli {
    X,
    Y,
    Z,
}

impl Pauli {
    /// Parses `X`, `Y` or `Z`; `I` and every other character yield `None`.
    #[must_use]
    pub const fn from_label(label: char) -> Option<Self> {
        match label {
            'X' => Some(Self::X),
            'Y' => Some(Self::Y),
            'Z' => Some(Self::Z),
            _ => None,
        }
    }

    /// The textbook 2×2 matrix in row-major order, `[m00, m01, m10, m11]`,
    /// where `m_ij = ⟨i|P|j⟩`. Y is the only non-symmetric Pauli, so it is the
    /// one that detects a transposed layout.
    #[must_use]
    pub fn matrix(self) -> [Complex64; 4] {
        let zero = Complex64::new(0.0, 0.0);
        let one = Complex64::new(1.0, 0.0);
        match self {
            Self::X => [zero, one, one, zero],
            Self::Y => [
                zero,
                Complex64::new(0.0, -1.0),
                Complex64::new(0.0, 1.0),
                zero,
            ],
            Self::Z => [one, zero, zero, -one],
        }
    }
}

/// One product `c·P_{q₁}⊗P_{q₂}⊗…` with at least one non-identity factor, on
/// distinct qubits.
#[derive(Clone, Debug, PartialEq)]
pub struct PauliTerm {
    coefficient: Complex64,
    factors: Box<[(QubitID, Pauli)]>,
}

impl PauliTerm {
    #[must_use]
    pub const fn coefficient(&self) -> Complex64 {
        self.coefficient
    }

    /// The non-identity factors, in the order they were given.
    #[must_use]
    pub fn factors(&self) -> &[(QubitID, Pauli)] {
        &self.factors
    }
}

/// A validated Pauli sum. Identity-only terms are folded into one coefficient,
/// so a backend needs no native identity operator: `⟨ψ|c·I|ψ⟩ = c` for a
/// normalized state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PauliSum {
    identity: Complex64,
    terms: Vec<PauliTerm>,
}

impl PauliSum {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `coefficient·P`, where `labels[i]` (one of `I`, `X`, `Y`, `Z`)
    /// acts on `qubits[i]`. Qubits must be distinct, including those under
    /// `I`, so a term names each qubit at most once.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty or unknown label, a label/qubit count
    /// mismatch, a repeated qubit or a non-finite coefficient. The sum is
    /// unchanged on error.
    pub fn push_labels(
        &mut self,
        coefficient: Complex64,
        labels: &str,
        qubits: &[QubitID],
    ) -> Result<(), PauliSumError> {
        if labels.is_empty() {
            return Err(PauliSumError::EmptyTerm);
        }
        let label_count = labels.chars().count();
        if label_count != qubits.len() {
            return Err(PauliSumError::LengthMismatch {
                labels: label_count,
                qubits: qubits.len(),
            });
        }
        for (position, &qubit) in qubits.iter().enumerate() {
            if qubits[..position].contains(&qubit) {
                return Err(PauliSumError::RepeatedQubit { qubit });
            }
        }
        let mut factors = Vec::with_capacity(qubits.len());
        for (label, &qubit) in labels.chars().zip(qubits) {
            if label == 'I' {
                continue;
            }
            let pauli = Pauli::from_label(label).ok_or(PauliSumError::UnknownLabel { label })?;
            factors.push((qubit, pauli));
        }
        self.push(coefficient, factors)
    }

    /// Adds `coefficient·Π factors`; an empty product is the identity.
    ///
    /// # Errors
    ///
    /// Returns an error for a repeated qubit or a non-finite coefficient. The
    /// sum is unchanged on error.
    pub fn push(
        &mut self,
        coefficient: Complex64,
        factors: impl IntoIterator<Item = (QubitID, Pauli)>,
    ) -> Result<(), PauliSumError> {
        if !(coefficient.re.is_finite() && coefficient.im.is_finite()) {
            return Err(PauliSumError::NonFiniteCoefficient);
        }
        let factors = factors.into_iter().collect::<Box<[_]>>();
        for (position, &(qubit, _)) in factors.iter().enumerate() {
            if factors[..position].iter().any(|&(other, _)| other == qubit) {
                return Err(PauliSumError::RepeatedQubit { qubit });
            }
        }
        if factors.is_empty() {
            self.identity += coefficient;
        } else {
            self.terms.push(PauliTerm {
                coefficient,
                factors,
            });
        }
        Ok(())
    }

    /// The accumulated coefficient of the identity terms.
    #[must_use]
    pub const fn identity_coefficient(&self) -> Complex64 {
        self.identity
    }

    /// The non-identity terms, in insertion order.
    #[must_use]
    pub fn terms(&self) -> &[PauliTerm] {
        &self.terms
    }

    /// The largest qubit any non-identity factor acts on.
    #[must_use]
    pub fn max_qubit(&self) -> Option<QubitID> {
        self.terms
            .iter()
            .flat_map(|term| term.factors.iter().map(|&(qubit, _)| qubit))
            .max()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauliSumError {
    EmptyTerm,
    LengthMismatch { labels: usize, qubits: usize },
    UnknownLabel { label: char },
    RepeatedQubit { qubit: QubitID },
    NonFiniteCoefficient,
}

impl fmt::Display for PauliSumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTerm => write!(formatter, "a Pauli term needs at least one label"),
            Self::LengthMismatch { labels, qubits } => write!(
                formatter,
                "a Pauli term has {labels} labels but {qubits} qubits"
            ),
            Self::UnknownLabel { label } => {
                write!(formatter, "unknown Pauli label {label:?}; use I, X, Y or Z")
            }
            Self::RepeatedQubit { qubit } => {
                write!(formatter, "a Pauli term names qubit {qubit} more than once")
            }
            Self::NonFiniteCoefficient => write!(formatter, "a Pauli coefficient is not finite"),
        }
    }
}

impl std::error::Error for PauliSumError {}
