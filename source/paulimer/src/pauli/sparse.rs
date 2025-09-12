use rustc_hash::FxHashMap;

use crate::bits::{self, Bitwise, IndexAssignable, IndexSet};
use crate::pauli::generic::{PauliCharacterError, PauliUnitary};
use crate::quantum_core::{self, PositionedPauliObservable};

use super::{Pauli, PauliUnitaryProjective};

pub type SparsePauli = PauliUnitary<IndexSet, u8>;
pub type SparsePauliProjective = PauliUnitaryProjective<IndexSet>;

// Note: Can be improved using PauliMutable trait
// Question: Should 'i' be interpreted as I or complex phase ?
impl TryFrom<FxHashMap<usize, char>> for SparsePauli {
    type Error = PauliCharacterError;

    fn try_from(characters: FxHashMap<usize, char>) -> Result<Self, Self::Error> {
        let mut x_bits = IndexSet::new();
        let mut z_bits = IndexSet::new();
        let mut exponent: u8 = 0;
        for (index, character) in characters {
            match character {
                'X' | 'x' => x_bits.assign_index(index, true),
                'Z' | 'z' => z_bits.assign_index(index, true),
                'Y' | 'y' => {
                    exponent += 1;
                    x_bits.assign_index(index, true);
                    z_bits.assign_index(index, true);
                }
                'I' => {}
                _ => return Err(PauliCharacterError {}),
            }
        }
        Ok(SparsePauli::from_bits(x_bits, z_bits, exponent))
    }
}

// Note: Allow repeated indicies so that conversion never fails and replace with generic that relies on PauliMutable
impl From<&[PositionedPauliObservable]> for SparsePauli {
    fn from(pauli_observable: &[PositionedPauliObservable]) -> Self {
        let mut obs_copy = Vec::from(pauli_observable);
        obs_copy.sort_unstable();
        if obs_copy.len() > 1 {
            for j in 0..obs_copy.len() - 1 {
                assert!(
                    obs_copy[j].qubit_id < obs_copy[j + 1].qubit_id,
                    "Repeated qubit positions"
                );
            }
        }

        let mut x_indices = IndexSet::new();
        let mut z_indices = IndexSet::new();
        let mut phase = 0u8;

        for quantum_core::PositionedPauliObservable {
            qubit_id,
            observable,
        } in obs_copy
        {
            match observable {
                quantum_core::PauliObservable::PlusI => (),
                quantum_core::PauliObservable::MinusI => phase += 2,
                quantum_core::PauliObservable::PlusX => x_indices.assign_index(qubit_id, true),
                quantum_core::PauliObservable::PlusZ => z_indices.assign_index(qubit_id, true),
                quantum_core::PauliObservable::MinusX => {
                    x_indices.assign_index(qubit_id, true);
                    phase += 2;
                }
                quantum_core::PauliObservable::MinusZ => {
                    z_indices.assign_index(qubit_id, true);
                    phase += 2;
                }
                quantum_core::PauliObservable::PlusY => {
                    x_indices.assign_index(qubit_id, true);
                    z_indices.assign_index(qubit_id, true);
                    phase += 1;
                }
                quantum_core::PauliObservable::MinusY => {
                    x_indices.assign_index(qubit_id, true);
                    z_indices.assign_index(qubit_id, true);
                    phase += 3;
                }
            }
        }
        PauliUnitary::from_bits(x_indices, z_indices, phase)
    }
}

impl From<&[PositionedPauliObservable]> for SparsePauliProjective {
    fn from(pauli_observable: &[PositionedPauliObservable]) -> Self {
        let mut obs_copy = Vec::from(pauli_observable);
        obs_copy.sort_unstable();
        if obs_copy.len() > 1 {
            for j in 0..obs_copy.len() - 1 {
                assert!(
                    obs_copy[j].qubit_id < obs_copy[j + 1].qubit_id,
                    "Repeated qubit positions"
                );
            }
        }

        let mut x_indices = IndexSet::new();
        let mut z_indices = IndexSet::new();

        for quantum_core::PositionedPauliObservable {
            qubit_id,
            observable,
        } in obs_copy
        {
            match observable {
                quantum_core::PauliObservable::PlusI | quantum_core::PauliObservable::MinusI => (),
                quantum_core::PauliObservable::PlusX | quantum_core::PauliObservable::MinusX => {
                    x_indices.assign_index(qubit_id, true);
                }
                quantum_core::PauliObservable::PlusZ | quantum_core::PauliObservable::MinusZ => {
                    z_indices.assign_index(qubit_id, true);
                }
                quantum_core::PauliObservable::PlusY | quantum_core::PauliObservable::MinusY => {
                    x_indices.assign_index(qubit_id, true);
                    z_indices.assign_index(qubit_id, true);
                }
            }
        }
        PauliUnitaryProjective::from_bits(x_indices, z_indices)
    }
}

impl<const LENGTH: usize> From<[PositionedPauliObservable; LENGTH]> for SparsePauli {
    fn from(pauli_observable: [PositionedPauliObservable; LENGTH]) -> Self {
        pauli_observable.as_slice().into()
    }
}

impl From<Vec<PositionedPauliObservable>> for SparsePauli {
    fn from(value: Vec<PositionedPauliObservable>) -> Self {
        value.as_slice().into()
    }
}

impl<const LENGTH: usize> From<[PositionedPauliObservable; LENGTH]> for SparsePauliProjective {
    fn from(pauli_observable: [PositionedPauliObservable; LENGTH]) -> Self {
        pauli_observable.as_slice().into()
    }
}

impl From<Vec<PositionedPauliObservable>> for SparsePauliProjective {
    fn from(value: Vec<PositionedPauliObservable>) -> Self {
        value.as_slice().into()
    }
}

pub fn remapped_sparse(pauli: &SparsePauli, support: &[usize]) -> SparsePauli {
    let x_bits: IndexSet = bits::remapped(pauli.x_bits(), support);
    let z_bits: IndexSet = bits::remapped(pauli.z_bits(), support);
    SparsePauli::from_bits(x_bits, z_bits, pauli.xz_phase_exponent())
}

pub fn as_sparse(pauli: &impl Pauli<PhaseExponentValue = u8>) -> SparsePauli {
    let x_bits = pauli.x_bits().support().into();
    let z_bits = pauli.z_bits().support().into();
    SparsePauli::from_bits(x_bits, z_bits, pauli.xz_phase_exponent())
}

pub fn as_sparse_projective(pauli: &impl Pauli) -> SparsePauliProjective {
    let x_bits = pauli.x_bits().support().into();
    let z_bits = pauli.z_bits().support().into();
    SparsePauliProjective::from_bits(x_bits, z_bits)
}
