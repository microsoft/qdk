use crate::bits::{BitMatrix, Bitwise, WORD_COUNT_DEFAULT};
use crate::pauli::{Pauli, PauliBinaryOps, PauliBits, PauliMutable, PauliUnitary};
use crate::quantum_core;
use crate::UnitaryOp;
use num_derive::{FromPrimitive, ToPrimitive};

#[derive(FromPrimitive, ToPrimitive, Clone, Copy, Debug, PartialEq, Eq)]
pub enum XOrZ {
    X = Axis::X as isize,
    Z = Axis::Z as isize,
}

pub trait Clifford {
    type PhaseExponentValue;
    type DensePauli: PauliMutable + Pauli<PhaseExponentValue = Self::PhaseExponentValue>;
    fn num_qubits(&self) -> usize;
    fn qubits(&self) -> std::ops::Range<usize> {
        0..self.num_qubits()
    }

    fn is_valid(&self) -> bool;
    fn is_identity(&self) -> bool;

    #[must_use]
    fn identity(num_qubits: usize) -> Self;
    #[must_use]
    fn zero(num_qubits: usize) -> Self;
    #[must_use]
    fn random(num_qubits: usize, random_number_generator: &mut impl rand::Rng) -> Self;

    fn preimage_x(&self, qubit_index: usize) -> Self::DensePauli;
    fn preimage_z(&self, qubit_index: usize) -> Self::DensePauli;
    fn preimage_x_bits(&self, x_bits: &impl Bitwise) -> Self::DensePauli;
    fn preimage_z_bits(&self, z_bits: &impl Bitwise) -> Self::DensePauli;
    fn preimage<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &self,
        pauli: &PauliLike,
    ) -> Self::DensePauli;

    fn image_x(&self, qubit_index: usize) -> Self::DensePauli;
    fn image_z(&self, qubit_index: usize) -> Self::DensePauli;
    fn image_x_bits(&self, x_bits: &impl Bitwise) -> Self::DensePauli;
    fn image_z_bits(&self, z_bits: &impl Bitwise) -> Self::DensePauli;
    fn image<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &self,
        pauli: &PauliLike,
    ) -> Self::DensePauli;

    #[must_use]
    fn multiply_with(&self, rhs: &Self) -> Self;
    #[must_use]
    fn tensor(&self, rhs: &Self) -> Self;
    #[must_use]
    fn inverse(&self) -> Self;
    #[must_use]
    fn from_preimages(preimages: &[Self::DensePauli]) -> Self;
    #[must_use]
    fn from_css_preimage_indicators(x_indicators: &BitMatrix, z_indicators: &BitMatrix) -> Self;

    fn is_diagonal(&self, axis: XOrZ) -> bool;
    fn is_diagonal_resource_encoder(&self, axis: XOrZ) -> bool;
    fn is_css(&self) -> bool;
    fn unitary_from_diagonal_resource_state(&self, axis: XOrZ) -> Option<Self>
    where
        Self: Sized;

    fn symplectic_matrix(&self) -> BitMatrix;
}

pub trait CliffordMutable {
    type PhaseExponentValue;

    fn left_mul_x(&mut self, qubit_index: usize);
    fn left_mul_y(&mut self, qubit_index: usize);
    fn left_mul_z(&mut self, qubit_index: usize);

    fn left_mul_hadamard(&mut self, qubit_index: usize);
    fn left_mul_root_z(&mut self, qubit_index: usize);
    fn left_mul_root_z_inverse(&mut self, qubit_index: usize);
    fn left_mul_root_x(&mut self, qubit_index: usize);
    fn left_mul_root_x_inverse(&mut self, qubit_index: usize);
    fn left_mul_root_y(&mut self, qubit_index: usize);
    fn left_mul_root_y_inverse(&mut self, qubit_index: usize);

    fn left_mul_cx(&mut self, control_qubit_index: usize, target_qubit_index: usize);
    fn left_mul_cz(&mut self, qubit1_index: usize, qubit2_index: usize);
    fn left_mul_prepare_bell(&mut self, qubit1_index: usize, qubit2_index: usize);
    fn left_mul_swap(&mut self, qubit_index1: usize, qubit_index2: usize);

    fn left_mul(&mut self, unitary_op: UnitaryOp, support: &[usize]);

    fn left_mul_pauli<PauliLike: Pauli>(&mut self, pauli: &PauliLike);
    fn left_mul_pauli_exp<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &mut self,
        pauli: &PauliLike,
    );
    fn left_mul_controlled_pauli<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &mut self,
        control: &PauliLike,
        target: &PauliLike,
    );

    fn left_mul_permutation(&mut self, permutation: &[usize], support: &[usize]);
    fn left_mul_clifford<CliffordLike>(&mut self, clifford: &CliffordLike, support: &[usize])
    where
        CliffordLike: Clifford<PhaseExponentValue = Self::PhaseExponentValue>
            + PreimageViews<PhaseExponentValue = Self::PhaseExponentValue>;
}

pub trait PreimageViews {
    type PhaseExponentValue;
    type PreImageView<'life>: Pauli<PhaseExponentValue = Self::PhaseExponentValue>
    where
        Self: 'life;
    type ImageViewUpToPhase<'life>: Pauli<PhaseExponentValue = ()>
    where
        Self: 'life;

    fn preimage_x_view(&self, index: usize) -> Self::PreImageView<'_>;
    fn preimage_z_view(&self, index: usize) -> Self::PreImageView<'_>;
    fn x_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_>;
    fn z_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_>;
}

pub trait MutablePreImages {
    type PhaseExponentValue;
    type PreImageViewMut<'life>: PauliBinaryOps + Pauli<PhaseExponentValue = Self::PhaseExponentValue>
    where
        Self: 'life;
    fn preimage_x_view_mut(&mut self, qubit_index: usize) -> Self::PreImageViewMut<'_>;
    fn preimage_z_view_mut(&mut self, qubit_index: usize) -> Self::PreImageViewMut<'_>;
    fn preimage_xz_views_mut(&mut self, index: usize) -> (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>);
    fn preimage_xz_views_mut_distinct(&mut self, index: (usize, usize)) -> crate::Tuple2x2<Self::PreImageViewMut<'_>>;
}

pub mod generic_algos;

#[must_use]
#[derive(Eq, Clone)]
pub struct CliffordUnitary {
    bits: BitMatrix<WORD_COUNT_DEFAULT>,
    preimage_phase_exponents: Vec<u8>,
}

#[must_use]
#[derive(Eq, Clone)]
pub struct CliffordUnitaryModPauli {
    bits: BitMatrix<WORD_COUNT_DEFAULT>,
}

#[must_use]
#[repr(C, align(64))]
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct CliffordModPauliBatch<const WORD_COUNT: usize, const QUBIT_COUNT: usize> {
    pub preimages: [[[u64; WORD_COUNT]; QUBIT_COUNT]; 4],
}

pub struct PauliExponent<Bits: PauliBits, Phase: crate::pauli::generic::PhaseExponent>(PauliUnitary<Bits, Phase>);
pub struct ControlledPauli<Bits: PauliBits, Phase: crate::pauli::generic::PhaseExponent>(
    PauliUnitary<Bits, Phase>,
    PauliUnitary<Bits, Phase>,
);
pub struct Swap(pub usize, pub usize);
pub struct Hadamard(pub usize);

mod clifford_impl;
pub use clifford_impl::{
    apply_qubit_clifford_by_axis, group_encoding_clifford_of, prepare_all_plus, prepare_all_zero,
    random_clifford_via_operations_sampling, recover_z_images_phases, split_clifford_encoder,
    split_clifford_encoder_mod_pauli, split_clifford_mod_pauli_with_transforms, split_phased_css,
    split_qubit_cliffords_and_css, split_qubit_tensor_product_encoder,
};
use quantum_core::Axis;
mod special_clifford;

#[derive(Debug, PartialEq, Eq, Default)]
pub struct CliffordStringParsingError;
