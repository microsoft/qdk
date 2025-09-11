use super::{anti_commutes_with, DensePauli, Pauli, PauliBinaryOps};
use crate::{
    bits::{BitMatrix, Bitwise},
    setwise::complement,
    NeutralElement,
};
use itertools::enumerate;

/// # Panics
/// Will panic if the input `paulis` are not independent
pub fn complete_to_full_pauli_basis<PauliLike: Pauli>(paulis: &[PauliLike], qubit_count: usize) -> Vec<DensePauli>
where
    DensePauli: PauliBinaryOps<PauliLike>,
{
    let mut paulis_as_bitmatrix = bitmatrix_from_paulis(paulis, qubit_count);
    let rank_profile = paulis_as_bitmatrix.echelonize();
    assert_eq!(rank_profile.len(), paulis.len());
    let rank_profile_complement = complement(&rank_profile, 2 * qubit_count);
    let mut result = Vec::new();
    for pauli in paulis {
        let mut dense_pauli = DensePauli::neutral_element_of_size(qubit_count);
        dense_pauli.assign(pauli);
        result.push(dense_pauli);
    }
    for column_index in rank_profile_complement {
        if column_index < qubit_count {
            result.push(DensePauli::x(column_index, qubit_count));
        } else {
            result.push(DensePauli::z(column_index - qubit_count, qubit_count));
        }
    }
    result
}

pub fn bitmatrix_from_paulis<PauliLike: Pauli>(paulis: &[PauliLike], qubit_count: usize) -> BitMatrix {
    let mut result = BitMatrix::zeros(paulis.len(), 2 * qubit_count);
    for (row_index, pauli) in enumerate(paulis) {
        for x_column_index in pauli.x_bits().support() {
            result.set((row_index, x_column_index), true);
        }
        for z_column_index in pauli.z_bits().support() {
            result.set((row_index, qubit_count + z_column_index), true);
        }
    }
    result
}

pub fn paulis_qubit_count<PauliLike: Pauli>(pauli: &[PauliLike]) -> usize {
    pauli.iter().map(super::Pauli::qubit_count).max().unwrap_or(0)
}

pub fn are_the_same_group_up_to_phases<PauliLike1: Pauli, PauliLike2: Pauli>(
    paulis_left: &[PauliLike1],
    paulis_right: &[PauliLike2],
) -> bool {
    let qubit_count = paulis_qubit_count(paulis_left);
    if paulis_qubit_count(paulis_right) != qubit_count {
        return false;
    }
    let mut matrix_left = bitmatrix_from_paulis(paulis_left, qubit_count);
    let mut matrix_right = bitmatrix_from_paulis(paulis_right, qubit_count);
    let left_rank_profile = matrix_left.echelonize();
    let right_rank_profile = matrix_right.echelonize();
    let rank = left_rank_profile.len();
    if rank == right_rank_profile.len() {
        matrix_left
            .rows()
            .take(rank)
            .zip(matrix_right.rows().take(rank))
            .all(|(row_left, row_right)| row_left == row_right)
    } else {
        false
    }
}

pub fn are_mutually_commuting<PauliLike: Pauli>(paulis: &[PauliLike]) -> bool {
    for i in 0..paulis.len() {
        for j in 0..i {
            if anti_commutes_with(&paulis[i], &paulis[j]) {
                return false;
            }
        }
    }
    true
}

pub fn apply_pauli_exponent<PauliLike: Pauli + PauliBinaryOps>(target: &mut PauliLike, exponent: &PauliLike) {
    if anti_commutes_with(target, exponent) {
        target.mul_assign_left(exponent);
        target.add_assign_phase_exp(1);
    }
}

pub fn apply_root_x<PauliLike: Pauli + PauliBinaryOps>(target: &mut PauliLike, qubit_index: usize) {
    if target.z_bits().index(qubit_index) {
        target.mul_assign_left_x(qubit_index);
        target.add_assign_phase_exp(3);
    }
}

pub fn apply_root_y<PauliLike: Pauli + PauliBinaryOps>(target: &mut PauliLike, qubit_index: usize) {
    if !(target.z_bits().index(qubit_index) & target.x_bits().index(qubit_index)) {
        target.mul_assign_left_y(qubit_index);
        target.add_assign_phase_exp(3);
    }
}

pub fn apply_root_z<PauliLike: Pauli + PauliBinaryOps>(target: &mut PauliLike, qubit_index: usize) {
    if target.x_bits().index(qubit_index) {
        target.mul_assign_left_z(qubit_index);
        target.add_assign_phase_exp(3);
    }
}
