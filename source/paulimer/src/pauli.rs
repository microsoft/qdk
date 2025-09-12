// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod generic;
pub use generic::{
    add_assign_bits, pauli_random, pauli_random_order_two, PauliUnitary, PauliUnitaryProjective,
};

pub mod operators;
pub use operators::Phase;

mod dense;
pub use dense::{dense_from, DensePauli, DensePauliProjective};
use sorted_iter::SortedIterator;

mod sparse;
pub use sparse::{
    as_sparse, as_sparse_projective, remapped_sparse, SparsePauli, SparsePauliProjective,
};

mod algorithms;
pub use algorithms::{
    apply_pauli_exponent, apply_root_x, apply_root_y, apply_root_z, are_mutually_commuting,
    are_the_same_group_up_to_phases, complete_to_full_pauli_basis, paulis_qubit_count,
};

use crate::bits::{Bitwise, BitwiseBinaryOps, Dot, IndexAssignable, OverlapWeight};
use crate::NeutralElement;

pub trait PauliBits: Bitwise + Dot + OverlapWeight + PartialEq + std::hash::Hash {}
impl<T: Bitwise + Dot + OverlapWeight + PartialEq + std::hash::Hash> PauliBits for T {}

// Generalizes Pauli unitaries and Projective Pauli unitaries
// TODO: add Pauli: + for<'a> PartialEq<&'a [quantum_core::PositionedPauliObservable]>
pub trait Pauli: PartialEq {
    type Bits: PauliBits;
    type PhaseExponentValue;

    fn xz_phase_exponent(&self) -> Self::PhaseExponentValue;
    fn x_bits(&self) -> &Self::Bits;
    fn z_bits(&self) -> &Self::Bits;

    fn support(&self) -> impl SortedIterator<Item = usize> {
        self.x_bits().support().union(self.z_bits().support())
    }

    fn y_parity(&self) -> bool {
        self.x_bits().dot(self.z_bits())
    }

    fn weight(&self) -> usize {
        self.x_bits().or_weight(self.z_bits())
    }

    fn is_order_two(&self) -> bool;
    fn is_identity(&self) -> bool;
    fn is_pauli_x(&self, qubit: usize) -> bool;
    fn is_pauli_z(&self, qubit: usize) -> bool;
    fn is_pauli_y(&self, qubit: usize) -> bool;
    fn equals_to(&self, rhs: &Self) -> bool;
    fn to_xz_bits(self) -> (Self::Bits, Self::Bits);
    fn max_qubit_id(&self) -> Option<usize> {
        match (self.x_bits().max_bit_id(), self.z_bits().max_bit_id()) {
            (None, None) => None,
            (None, Some(id)) | (Some(id), None) => Some(id),
            (Some(id1), Some(id2)) => Some(std::cmp::max(id1, id2)),
        }
    }
    fn qubit_count(&self) -> usize {
        self.max_qubit_id().map_or(0, |max_id| max_id + 1)
    }

    #[must_use]
    fn x(qubit_index: usize, qubit_count: usize) -> Self
    where
        Self: Sized,
        Self: NeutralElement<NeutralElementType = Self>,
        Self: PauliMutable,
    {
        let mut result = Self::neutral_element_of_size(qubit_count);
        result.mul_assign_left_x(qubit_index);
        result
    }

    #[must_use]
    fn y(qubit_index: usize, qubit_count: usize) -> Self
    where
        Self: Sized,
        Self: NeutralElement<NeutralElementType = Self>,
        Self: PauliMutable,
    {
        let mut result = Self::neutral_element_of_size(qubit_count);
        result.mul_assign_left_y(qubit_index);
        result
    }

    #[must_use]
    fn z(qubit_index: usize, qubit_count: usize) -> Self
    where
        Self: Sized,
        Self: NeutralElement<NeutralElementType = Self>,
        Self: PauliMutable,
    {
        let mut result = Self::neutral_element_of_size(qubit_count);
        result.mul_assign_left_z(qubit_index);
        result
    }
}

pub trait PauliMutable: Pauli<Bits: IndexAssignable> {
    fn assign_phase_exp(&mut self, rhs: u8);
    fn add_assign_phase_exp(&mut self, rhs: u8);
    fn complex_conjugate(&mut self);
    fn invert(&mut self);
    fn negate(&mut self);
    fn assign_phase_from<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &mut self,
        other: &PauliLike,
    );
    fn mul_assign_phase_from<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &mut self,
        other: &PauliLike,
    );

    fn mul_assign_left_x(&mut self, qubit_id: usize);
    fn mul_assign_right_x(&mut self, qubit_id: usize);
    fn mul_assign_left_z(&mut self, qubit_id: usize);
    fn mul_assign_right_z(&mut self, qubit_id: usize);
    fn set_identity(&mut self);
    fn set_random(&mut self, num_qubits: usize, random_number_generator: &mut impl rand::Rng);
    fn set_random_order_two(
        &mut self,
        num_qubits: usize,
        random_number_generator: &mut impl rand::Rng,
    );

    fn mul_assign_left_y(&mut self, qubit_id: usize) {
        self.mul_assign_left_z(qubit_id);
        self.mul_assign_left_x(qubit_id);
        self.add_assign_phase_exp(1u8);
    }

    fn mul_assign_right_y(&mut self, qubit_id: usize) {
        self.mul_assign_right_x(qubit_id);
        self.mul_assign_right_z(qubit_id);
        self.add_assign_phase_exp(1u8);
    }
}

pub fn anti_commutes_with<LeftPauli: Pauli, RightPauli: Pauli>(
    left: &LeftPauli,
    right: &RightPauli,
) -> bool
where
    LeftPauli::Bits: Dot<RightPauli::Bits>,
{
    left.x_bits().dot(right.z_bits()) ^ left.z_bits().dot(right.x_bits())
}

pub fn commutes_with<LeftPauli: Pauli, RightPauli: Pauli>(
    left: &LeftPauli,
    right: &RightPauli,
) -> bool
where
    LeftPauli::Bits: Dot<RightPauli::Bits>,
{
    !anti_commutes_with(left, right)
}

pub trait PauliMutableBits<Bits: Bitwise>: PauliMutable {
    type BitsMutable: BitwiseBinaryOps<Bits>;
    fn x_bits_mut(&mut self) -> &mut Self::BitsMutable;
    fn z_bits_mut(&mut self) -> &mut Self::BitsMutable;
}

pub trait PauliBinaryOps<Other: ?Sized + Pauli = Self>: PauliMutable {
    fn assign(&mut self, rhs: &Other);
    fn assign_with_offset(&mut self, rhs: &Other, start_qubit_index: usize, num_qubits: usize);
    fn mul_assign_right(&mut self, rhs: &Other);
    fn mul_assign_left(&mut self, lhs: &Other);
}

pub trait PauliNeutralElement:
    Pauli + NeutralElement<NeutralElementType: PauliBinaryOps<Self>>
{
}
