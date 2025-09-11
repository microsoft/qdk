use itertools::{enumerate, Itertools};
use crate::quantum_core::{y, Axis};
use sorted_iter::assume::AssumeSortedByItemExt;
use sorted_iter::SortedIterator;

use super::generic_algos::support_restricted_z_images_from_support_complement;
use super::{
    Bitwise, Clifford, CliffordModPauliBatch, CliffordMutable, CliffordStringParsingError, CliffordUnitary,
    CliffordUnitaryModPauli, ControlledPauli, Hadamard, MutablePreImages, PauliExponent, PreimageViews, Swap, XOrZ,
};

use crate::bits::bitmatrix::{are_zero_rows, is_zero_padded_identity, is_zero_padded_symmetric, BitMatrix, Column};
use crate::bits::{BitVec, BitView, BitwiseBinaryOps, IndexAssignable, IndexSet, MutableBitView, WORD_COUNT_DEFAULT};
use crate::pauli::generic::PhaseExponent;
use crate::pauli::{
    apply_pauli_exponent, apply_root_x, are_mutually_commuting, dense_from, remapped_sparse, DensePauli,
    DensePauliProjective, Pauli, PauliBinaryOps, PauliBits, PauliMutable, PauliUnitary, PauliUnitaryProjective,
    SparsePauli, SparsePauliProjective,
};
use crate::{assert_1q_gate, assert_2q_gate, UnitaryOp};
use crate::{subscript_digits, NeutralElement, Tuple2x2, Tuple4, Tuple4x2, Tuple8};

use core::fmt;
use std::collections::BTreeSet;
use std::fmt::{Debug, Display};
use std::iter::{zip, IntoIterator};
use std::ops::Mul;
use std::str::FromStr;
use std::vec;

// Utils

fn concat2<T>(ab: Tuple2x2<T>) -> Tuple4<T> {
    (ab.0 .0, ab.0 .1, ab.1 .0, ab.1 .1)
}

fn split2<T>(ab: Tuple4<T>) -> Tuple2x2<T> {
    ((ab.0, ab.1), (ab.2, ab.3))
}

fn concat4<T>(a: Tuple4x2<T>) -> Tuple8<T> {
    (a.0 .0, a.0 .1, a.1 .0, a.1 .1, a.2 .0, a.2 .1, a.3 .0, a.3 .1)
}

fn split4<T>(abcd: Tuple8<T>) -> Tuple4x2<T> {
    ((abcd.0, abcd.1), (abcd.2, abcd.3), (abcd.4, abcd.5), (abcd.6, abcd.7))
}

/// Does not check if indices are distinct
unsafe fn tuple2_from_vec<T>(vec: &mut Vec<T>, index: (usize, usize)) -> (&mut T, &mut T) {
    let ptr = vec.as_mut_ptr();
    unsafe { (&mut *ptr.add(index.0), &mut *ptr.add(index.1)) }
}

/// Does not check if indices are distinct
unsafe fn tuple4_from_vec<T>(
    vec: &mut Vec<T>,
    index: (usize, usize, usize, usize),
) -> (&mut T, &mut T, &mut T, &mut T) {
    let ptr = vec.as_mut_ptr();
    unsafe {
        (
            &mut *ptr.add(index.0),
            &mut *ptr.add(index.1),
            &mut *ptr.add(index.2),
            &mut *ptr.add(index.3),
        )
    }
}

// Neutral element trait

fn set_identity_pre_images<PauliLike: Pauli, CliffordLike: Clifford + MutablePreImages>(clifford: &mut CliffordLike)
where
    for<'life> <CliffordLike as MutablePreImages>::PreImageViewMut<'life>: PauliBinaryOps<PauliLike>,
{
    for index in 0..clifford.num_qubits() {
        debug_assert!(clifford.preimage_x_view_mut(index).is_identity());
        clifford.preimage_x_view_mut(index).mul_assign_left_x(index);
        debug_assert!(clifford.preimage_z_view_mut(index).is_identity());
        clifford.preimage_z_view_mut(index).mul_assign_right_z(index);
    }
}

impl NeutralElement for CliffordUnitary {
    type NeutralElementType = CliffordUnitary;

    fn neutral_element(&self) -> Self::NeutralElementType {
        Self::identity(self.num_qubits())
    }

    fn default_size_neutral_element() -> Self::NeutralElementType {
        Self::identity(0)
    }

    fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
        Self::identity(size)
    }
}

impl NeutralElement for CliffordUnitaryModPauli {
    type NeutralElementType = CliffordUnitaryModPauli;

    fn neutral_element(&self) -> Self::NeutralElementType {
        Self::identity(self.num_qubits())
    }

    fn default_size_neutral_element() -> Self::NeutralElementType {
        Self::identity(0)
    }

    fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
        Self::identity(size)
    }
}

// Clifford trait

fn projective_image_at<const WORD_COUNT: usize>(
    bits: &BitMatrix<WORD_COUNT>,
    dimension: usize,
    qubit_index: usize,
    x_bits_start: usize,
    z_bits_start: usize,
) -> PauliUnitaryProjective<Column<WORD_COUNT>> {
    let column = bits.column(qubit_index);
    let x_bits = column.slice(x_bits_start..dimension + x_bits_start);
    let z_bits = column.slice(z_bits_start..dimension + z_bits_start);
    PauliUnitaryProjective::<Column<WORD_COUNT>>::from_bits(x_bits, z_bits)
}

fn projective_x_image_at<const WORD_COUNT: usize>(
    bits: &BitMatrix<WORD_COUNT>,
    dimension: usize,
    qubit_index: usize,
) -> PauliUnitaryProjective<Column<WORD_COUNT>> {
    projective_image_at(
        bits,
        dimension,
        qubit_index,
        z_of_preimage_z_offset(dimension),
        z_of_preimage_x_offset(dimension),
    )
}

fn projective_z_image_at<const WORD_COUNT: usize>(
    bits: &BitMatrix<WORD_COUNT>,
    dimension: usize,
    qubit_index: usize,
) -> PauliUnitaryProjective<Column<WORD_COUNT>> {
    projective_image_at(
        bits,
        dimension,
        qubit_index,
        x_of_preimage_z_offset(dimension),
        x_of_preimage_x_offset(dimension),
    )
}

/// index of `preimage_phase_exponents` that describes phase of preimage x_`index`
#[inline]
fn phase_of_preimage_x(index: usize) -> usize {
    2 * index
}

/// index of `preimage_phase_exponents` that describes phase of preimage z_`index`
#[inline]
fn phase_of_preimage_z(index: usize) -> usize {
    2 * index + 1
}

/// first row where x bits of preimage of x of a clifford unitary are stored
#[inline]
fn x_of_preimage_x_offset(_dimension: usize) -> usize {
    0
}

/// first row where z bits of preimage of x of a clifford unitary are stored
#[inline]
fn z_of_preimage_x_offset(dimension: usize) -> usize {
    dimension
}

/// first row where x bits of preimage of z of a clifford unitary are stored
#[inline]
fn x_of_preimage_z_offset(dimension: usize) -> usize {
    2 * dimension
}

/// first row where z bits of preimage of z of a clifford unitary are stored
#[inline]
fn z_of_preimage_z_offset(dimension: usize) -> usize {
    3 * dimension
}

/// index of row of bits that describes `z_bits` of preimage z_`index`
#[inline]
fn z_of_pz(dimension: usize, index: usize) -> usize {
    index + z_of_preimage_z_offset(dimension)
}

/// index of row of bits that describes `x_bits` of preimage z_`index`
#[inline]
fn x_of_pz(dimension: usize, index: usize) -> usize {
    index + x_of_preimage_z_offset(dimension)
}

/// index of row of bits that describes `z_bits` of preimage x_`index`
#[inline]
fn z_of_px(dimension: usize, index: usize) -> usize {
    index + z_of_preimage_x_offset(dimension)
}

/// index of row of bits that describes `x_bits` of preimage x_`index`
#[inline]
fn x_of_px(dimension: usize, index: usize) -> usize {
    index + x_of_preimage_x_offset(dimension)
}

#[inline]
fn x_preimage_rows_ids(dimension: usize, qubit_id: usize) -> (usize, usize) {
    (x_of_px(dimension, qubit_id), z_of_px(dimension, qubit_id))
}

#[inline]
fn z_preimage_rows_ids(dimension: usize, qubit_id: usize) -> (usize, usize) {
    (x_of_pz(dimension, qubit_id), z_of_pz(dimension, qubit_id))
}

#[inline]
fn xz_preimage_rows_ids(dimension: usize, qubit_id: usize) -> ((usize, usize), (usize, usize)) {
    (
        x_preimage_rows_ids(dimension, qubit_id),
        z_preimage_rows_ids(dimension, qubit_id),
    )
}

macro_rules! clifford_common_impl {
    () => {
        fn preimage_x_bits(&self, x_bits: &impl Bitwise) -> Self::DensePauli {
            let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
            super::generic_algos::mul_assign_right_clifford_preimage_x_bits(&mut res, self, x_bits);
            res
        }

        fn preimage_z_bits(&self, z_bits: &impl Bitwise) -> Self::DensePauli {
            let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
            super::generic_algos::mul_assign_right_clifford_preimage_z_bits(&mut res, self, z_bits);
            res
        }

        fn preimage<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
            &self,
            pauli: &PauliLike,
        ) -> Self::DensePauli {
            let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
            super::generic_algos::mul_assign_right_clifford_preimage(&mut res, self, pauli);
            res
        }

        fn num_qubits(&self) -> usize {
            self.bits.columncount()
        }

        fn is_valid(&self) -> bool {
            super::generic_algos::is_valid_clifford(self)
        }

        fn is_identity(&self) -> bool {
            super::generic_algos::clifford_is_identity(self)
        }

        fn multiply_with(&self, rhs: &Self) -> Self {
            super::generic_algos::clifford_multiply_with(self, &rhs)
        }

        fn from_preimages(preimages: &[Self::DensePauli]) -> Self {
            super::generic_algos::clifford_from_preimages(preimages.into_iter())
        }

        fn preimage_x(&self, qubit_index: usize) -> Self::DensePauli {
            self.preimage_x_view(qubit_index).into()
        }

        fn preimage_z(&self, qubit_index: usize) -> Self::DensePauli {
            self.preimage_z_view(qubit_index).into()
        }

        fn random(num_qubits: usize, random_number_generator: &mut impl rand::Rng) -> Self {
            let mut res = Self::identity(num_qubits);
            let mut random_pauli: Self::DensePauli = Self::DensePauli::neutral_element_of_size(num_qubits);
            for _ in 0..2 * num_qubits + 1 {
                random_pauli.set_random_order_two(num_qubits, random_number_generator);
                res.left_mul_pauli_exp(&random_pauli);
            }
            res
        }

        fn identity(num_qubits: usize) -> Self {
            let mut res = Self::zero(num_qubits);
            set_identity_pre_images::<Self::DensePauli, Self>(&mut res);
            res
        }

        fn from_css_preimage_indicators(x_indicators: &BitMatrix, z_indicators: &BitMatrix) -> Self {
            super::generic_algos::clifford_from_css_preimage_indicators(x_indicators, z_indicators)
        }

        fn tensor(&self, rhs: &Self) -> Self {
            super::generic_algos::clifford_tensored(self, rhs)
        }

        fn is_diagonal(&self, axis: XOrZ) -> bool {
            match axis {
                XOrZ::X => is_x_diagonal(self),
                XOrZ::Z => is_z_diagonal(self),
            }
        }

        fn is_diagonal_resource_encoder(&self, axis: XOrZ) -> bool {
            match axis {
                XOrZ::X => is_x_diagonal_resource_encoder(self).is_some(),
                XOrZ::Z => is_z_diagonal_resource_encoder(self).is_some(),
            }
        }

        fn is_css(&self) -> bool {
            is_css_clifford(self)
        }

        fn symplectic_matrix(&self) -> BitMatrix {
            let qubit_count = self.num_qubits();
            let mut res = BitMatrix::zeros(2 * qubit_count, 2 * qubit_count);
            for qubit in self.qubits() {
                let (x, z) = (self.preimage_x_view(qubit), self.preimage_z_view(qubit));
                res.row_mut(qubit).assign_with_offset(x.x_bits(), 0, qubit_count);
                res.row_mut(qubit)
                    .assign_with_offset(x.z_bits(), qubit_count, qubit_count);
                res.row_mut(qubit + qubit_count)
                    .assign_with_offset(z.x_bits(), 0, qubit_count);
                res.row_mut(qubit + qubit_count)
                    .assign_with_offset(z.z_bits(), qubit_count, qubit_count);
            }
            res
        }
    };
}

impl Clifford for CliffordUnitary {
    type PhaseExponentValue = u8;
    type DensePauli = PauliUnitary<BitVec<WORD_COUNT_DEFAULT>, u8>;

    fn image_x(&self, qubit_index: usize) -> Self::DensePauli {
        let mut image_up_to_phase = self.x_image_view_up_to_phase(0).neutral_element();
        image_up_to_phase.mul_assign_left(&self.x_image_view_up_to_phase(qubit_index));
        super::generic_algos::clifford_image_with_phase(self, image_up_to_phase.to_xz_bits())
    }

    fn image_z(&self, qubit_index: usize) -> Self::DensePauli {
        let mut image_up_to_phase = self.z_image_view_up_to_phase(0).neutral_element();
        image_up_to_phase.mul_assign_left(&self.z_image_view_up_to_phase(qubit_index));
        super::generic_algos::clifford_image_with_phase(self, image_up_to_phase.to_xz_bits())
    }

    fn image_x_bits(&self, x_bits: &impl Bitwise) -> Self::DensePauli {
        let mut image_up_to_phase = self.x_image_view_up_to_phase(0).neutral_element();
        super::generic_algos::mul_assign_right_clifford_image_x_bits_up_to_phase(&mut image_up_to_phase, self, x_bits);
        super::generic_algos::clifford_image_with_phase(self, image_up_to_phase.to_xz_bits())
    }

    fn image_z_bits(&self, z_bits: &impl Bitwise) -> Self::DensePauli {
        let mut image_up_to_phase = self.x_image_view_up_to_phase(0).neutral_element();
        super::generic_algos::mul_assign_right_clifford_image_z_bits_up_to_phase(&mut image_up_to_phase, self, z_bits);
        super::generic_algos::clifford_image_with_phase(self, image_up_to_phase.to_xz_bits())
    }

    fn image<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &self,
        pauli: &PauliLike,
    ) -> Self::DensePauli {
        let mut image_up_to_phase = self.x_image_view_up_to_phase(0).neutral_element();
        super::generic_algos::mul_assign_right_clifford_image_up_to_phase(&mut image_up_to_phase, self, pauli);
        let mut res = super::generic_algos::clifford_image_with_phase(self, image_up_to_phase.to_xz_bits());
        res.mul_assign_phase_from(pauli);
        res
    }

    fn unitary_from_diagonal_resource_state(&self, axis: XOrZ) -> Option<Self> {
        if let Some(mut res) = blocks_from_diagonal_resource_state(self, axis) {
            // make sure pre-images are hermitian
            for qubit_index in self.qubits() {
                if !res.preimage_z(qubit_index).is_order_two() {
                    res.preimage_z_view_mut(qubit_index).add_assign_phase_exp(1);
                }
                if !res.preimage_x(qubit_index).is_order_two() {
                    res.preimage_x_view_mut(qubit_index).add_assign_phase_exp(1);
                }
            }
            // make sure images signs match
            debug_assert!(res.is_valid());
            match axis {
                XOrZ::X => {
                    for qubit_index in self.qubits() {
                        let mut im_z = self.image_z(qubit_index);
                        im_z.mul_assign_left(&res.image_z(qubit_index));
                        if im_z.xz_phase_exponent() != 0 {
                            res.left_mul_pauli(&res.image_x(qubit_index));
                        }
                    }
                }
                XOrZ::Z => {
                    for qubit_index in self.qubits() {
                        let mut im_z = self.image_z(qubit_index);
                        im_z.mul_assign_left(&res.image_x(qubit_index));
                        if im_z.xz_phase_exponent() != 0 {
                            res.left_mul_pauli(&res.image_z(qubit_index));
                        }
                    }
                }
            }
            Some(res)
        } else {
            None
        }
    }

    fn zero(num_qubits: usize) -> Self {
        CliffordUnitary {
            bits: BitMatrix::<WORD_COUNT_DEFAULT>::zeros(num_qubits * 4, num_qubits),
            preimage_phase_exponents: vec![0u8; 2 * num_qubits],
        }
    }

    clifford_common_impl! {}

    fn inverse(&self) -> Self {
        inverse_with_signs(self)
    }
}

impl PreimageViews for CliffordUnitary {
    type PhaseExponentValue = u8;
    type PreImageView<'life> = PauliUnitary<BitView<'life, WORD_COUNT_DEFAULT>, &'life u8>;
    type ImageViewUpToPhase<'life> = PauliUnitaryProjective<Column<'life, WORD_COUNT_DEFAULT>>;

    fn preimage_x_view(&self, qubit_index: usize) -> Self::PreImageView<'_> {
        let xz_bits = self.bits.rows2(x_preimage_rows_ids(self.num_qubits(), qubit_index));
        Self::PreImageView::from_bits_tuple(
            xz_bits,
            &self.preimage_phase_exponents[phase_of_preimage_x(qubit_index)],
        )
    }

    fn preimage_z_view(&self, qubit_index: usize) -> Self::PreImageView<'_> {
        let xz_bits = self.bits.rows2(z_preimage_rows_ids(self.num_qubits(), qubit_index));
        Self::PreImageView::from_bits_tuple(
            xz_bits,
            &self.preimage_phase_exponents[phase_of_preimage_z(qubit_index)],
        )
    }

    fn x_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        projective_x_image_at(&self.bits, self.num_qubits(), qubit_index)
    }

    fn z_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        projective_z_image_at(&self.bits, self.num_qubits(), qubit_index)
    }
}

fn inverse_with_signs<CliffordLikeFrom: Clifford, CliffordLikeTo>(from: &CliffordLikeFrom) -> CliffordLikeTo
where
    CliffordLikeTo: Clifford + PreimageViews + MutablePreImages,
    for<'life> <CliffordLikeTo as MutablePreImages>::PreImageViewMut<'life>:
        PauliBinaryOps<CliffordLikeFrom::DensePauli>,
{
    let mut res = CliffordLikeTo::identity(from.num_qubits());
    for qubit_index in 0..from.num_qubits() {
        res.preimage_x_view_mut(qubit_index).assign(&from.image_x(qubit_index));
        res.preimage_z_view_mut(qubit_index).assign(&from.image_z(qubit_index));
    }
    res
}

impl Clifford for CliffordUnitaryModPauli {
    type PhaseExponentValue = ();
    type DensePauli = PauliUnitaryProjective<BitVec<WORD_COUNT_DEFAULT>>;
    // type SparsePauli = PauliUnitaryProjective<IndexSet>;

    fn image_x(&self, qubit_index: usize) -> Self::DensePauli {
        let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
        res.assign(&self.x_image_view_up_to_phase(qubit_index));
        res
    }

    fn image_z(&self, qubit_index: usize) -> Self::DensePauli {
        let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
        res.assign(&self.z_image_view_up_to_phase(qubit_index));
        res
    }

    fn image_x_bits(&self, x_bits: &impl Bitwise) -> Self::DensePauli {
        let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
        super::generic_algos::mul_assign_right_clifford_image_x_bits_up_to_phase(&mut res, self, x_bits);
        res
    }

    fn image_z_bits(&self, z_bits: &impl Bitwise) -> Self::DensePauli {
        let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
        super::generic_algos::mul_assign_right_clifford_image_z_bits_up_to_phase(&mut res, self, z_bits);
        res
    }

    fn image<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
        &self,
        pauli: &PauliLike,
    ) -> Self::DensePauli {
        let mut res = Self::DensePauli::neutral_element_of_size(self.num_qubits());
        super::generic_algos::mul_assign_right_clifford_image_up_to_phase(&mut res, self, pauli);
        res
    }

    fn zero(num_qubits: usize) -> Self {
        CliffordUnitaryModPauli {
            bits: BitMatrix::<WORD_COUNT_DEFAULT>::zeros(num_qubits * 4, num_qubits),
        }
    }

    clifford_common_impl! {}

    fn inverse(&self) -> Self {
        super::generic_algos::clifford_inverse_up_to_signs(self)
    }

    fn unitary_from_diagonal_resource_state(&self, axis: XOrZ) -> Option<Self> {
        blocks_from_diagonal_resource_state(self, axis)
    }
}

impl PreimageViews for CliffordUnitaryModPauli {
    type PreImageView<'life> = PauliUnitaryProjective<BitView<'life, WORD_COUNT_DEFAULT>>;
    type ImageViewUpToPhase<'life> = PauliUnitaryProjective<Column<'life, WORD_COUNT_DEFAULT>>;

    fn preimage_x_view(&self, qubit_index: usize) -> Self::PreImageView<'_> {
        let xz_bits = self.bits.rows2(x_preimage_rows_ids(self.num_qubits(), qubit_index));
        Self::PreImageView::from_bits_tuple(xz_bits)
    }

    fn preimage_z_view(&self, qubit_index: usize) -> Self::PreImageView<'_> {
        let xz_bits = self.bits.rows2(z_preimage_rows_ids(self.num_qubits(), qubit_index));
        Self::PreImageView::from_bits_tuple(xz_bits)
    }

    fn x_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        projective_x_image_at(&self.bits, self.num_qubits(), qubit_index)
    }

    fn z_image_view_up_to_phase(&self, qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        projective_z_image_at(&self.bits, self.num_qubits(), qubit_index)
    }

    type PhaseExponentValue = ();
}

// CliffordMutable trait

fn swap_clifford_bits<const WORD_COUNT: usize>(
    dimension: usize,
    qubit1_id: usize,
    qubit2_id: usize,
    bits: &mut BitMatrix<WORD_COUNT>,
) {
    let ((a1, b1), (c1, d1)) = xz_preimage_rows_ids(dimension, qubit1_id);
    let ((a2, b2), (c2, d2)) = xz_preimage_rows_ids(dimension, qubit2_id);
    bits.swap_rows(a1, a2);
    bits.swap_rows(b1, b2);
    bits.swap_rows(c1, c2);
    bits.swap_rows(d1, d2);
}

fn hadamard_clifford_bits<const WORD_COUNT: usize>(
    dimension: usize,
    qubit_id: usize,
    bits: &mut BitMatrix<WORD_COUNT>,
) {
    let (x1, x2) = x_preimage_rows_ids(dimension, qubit_id);
    let (z1, z2) = z_preimage_rows_ids(dimension, qubit_id);
    bits.swap_rows(x1, z1);
    bits.swap_rows(x2, z2);
}

impl MutablePreImages for CliffordUnitaryModPauli
where
    for<'life> PauliUnitaryProjective<MutableBitView<'life, WORD_COUNT_DEFAULT>>:
        PauliBinaryOps + Pauli<PhaseExponentValue = ()>,
{
    type PhaseExponentValue = ();
    type PreImageViewMut<'life> = PauliUnitaryProjective<MutableBitView<'life, WORD_COUNT_DEFAULT>>;

    fn preimage_x_view_mut(&mut self, index: usize) -> Self::PreImageViewMut<'_> {
        let xz_bits = self.bits.rows2_mut(x_preimage_rows_ids(self.num_qubits(), index));
        Self::PreImageViewMut::from_bits_tuple(xz_bits)
    }

    fn preimage_z_view_mut(&mut self, index: usize) -> Self::PreImageViewMut<'_> {
        let xz_bits = self.bits.rows2_mut(z_preimage_rows_ids(self.num_qubits(), index));
        Self::PreImageViewMut::from_bits_tuple(xz_bits)
    }

    fn preimage_xz_views_mut(&mut self, index: usize) -> (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>) {
        unsafe {
            let xz_ids = xz_preimage_rows_ids(self.num_qubits(), index);
            let (xz_of_x, xz_of_z) = split2(self.bits.rows4_mut(concat2(xz_ids)));
            (
                Self::PreImageViewMut::from_bits_tuple(xz_of_x),
                Self::PreImageViewMut::from_bits_tuple(xz_of_z),
            )
        }
    }

    #[allow(clippy::similar_names)]
    fn preimage_xz_views_mut_distinct(
        &mut self,
        index: (usize, usize),
    ) -> (
        (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>),
        (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>),
    ) {
        assert_ne!(index.0, index.1);
        let (xz_of_x0_ids, xz_of_z0_ids) = xz_preimage_rows_ids(self.num_qubits(), index.0);
        let (xz_of_x1_ids, xz_of_z1_ids) = xz_preimage_rows_ids(self.num_qubits(), index.1);
        unsafe {
            let (xz_of_x0, xz_of_z0, xz_of_x1, xz_of_z1) =
                split4(
                    self.bits
                        .rows8_mut(concat4((xz_of_x0_ids, xz_of_z0_ids, xz_of_x1_ids, xz_of_z1_ids))),
                );
            (
                (
                    Self::PreImageViewMut::from_bits_tuple(xz_of_x0),
                    Self::PreImageViewMut::from_bits_tuple(xz_of_z0),
                ),
                (
                    Self::PreImageViewMut::from_bits_tuple(xz_of_x1),
                    Self::PreImageViewMut::from_bits_tuple(xz_of_z1),
                ),
            )
        }
    }
}

macro_rules! clifford_mutable_common_impl {
    () => {
        fn left_mul_root_z(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_z(self, qubit_id)
        }

        fn left_mul_root_z_inverse(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_z_inverse(self, qubit_id)
        }

        fn left_mul_root_x(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_x(self, qubit_id)
        }

        fn left_mul_root_x_inverse(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_x_inverse(self, qubit_id)
        }

        fn left_mul_root_y(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_y(self, qubit_id)
        }

        fn left_mul_root_y_inverse(&mut self, qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_root_y_inverse(self, qubit_id)
        }

        fn left_mul_cx(&mut self, control_qubit_id: usize, target_qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_cnot(self, control_qubit_id, target_qubit_id)
        }

        fn left_mul_cz(&mut self, control_qubit_id: usize, target_qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_cz(self, control_qubit_id, target_qubit_id)
        }

        fn left_mul_prepare_bell(&mut self, control_qubit_id: usize, target_qubit_id: usize) {
            super::generic_algos::clifford_left_mul_eq_prepare_bell(self, control_qubit_id, target_qubit_id)
        }

        fn left_mul(&mut self, unitary_op: UnitaryOp, support: &[usize]) {
            use crate::UnitaryOp::*;
            match unitary_op {
                I => {}
                X => {
                    assert_1q_gate!(support);
                    self.left_mul_x(support[0]);
                }
                Y => {
                    assert_1q_gate!(support);
                    self.left_mul_y(support[0]);
                }
                Z => {
                    assert_1q_gate!(support);
                    self.left_mul_z(support[0]);
                }
                SqrtX => {
                    assert_1q_gate!(support);
                    self.left_mul_root_x(support[0]);
                }
                SqrtXInv => {
                    assert_1q_gate!(support);
                    self.left_mul_root_x_inverse(support[0]);
                }
                SqrtY => {
                    assert_1q_gate!(support);
                    self.left_mul_root_y(support[0]);
                }
                SqrtYInv => {
                    assert_1q_gate!(support);
                    self.left_mul_root_y_inverse(support[0]);
                }
                SqrtZ => {
                    assert_1q_gate!(support);
                    self.left_mul_root_z(support[0]);
                }
                SqrtZInv => {
                    assert_1q_gate!(support);
                    self.left_mul_root_z_inverse(support[0]);
                }
                Hadamard => {
                    assert_1q_gate!(support);
                    self.left_mul_hadamard(support[0]);
                }
                Swap => {
                    assert_2q_gate!(support);
                    self.left_mul_swap(support[0], support[1]);
                }
                ControlledX => {
                    assert_2q_gate!(support);
                    self.left_mul_cx(support[0], support[1]);
                }
                ControlledZ => {
                    assert_2q_gate!(support);
                    self.left_mul_cz(support[0], support[1]);
                }
                PrepareBell => {
                    assert_2q_gate!(support);
                    self.left_mul_prepare_bell(support[0], support[1]);
                }
            }
        }
    };
}

macro_rules! clifford_mutable_common_multi_qubit_impl {
    ($DensePauli:ty) => {
        fn left_mul_pauli_exp<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
            &mut self,
            pauli: &PauliLike,
        ) {
            if self.num_qubits() > 0 {
                super::generic_algos::clifford_left_mul_eq_pauli_exp(self, pauli);
            }
        }

        fn left_mul_pauli<PauliLike: Pauli>(&mut self, pauli: &PauliLike) {
            for qubit_x_index in pauli.x_bits().support() {
                self.left_mul_x(qubit_x_index)
            }
            for qubit_z_index in pauli.z_bits().support() {
                self.left_mul_z(qubit_z_index)
            }
        }

        fn left_mul_controlled_pauli<PauliLike: Pauli<PhaseExponentValue = Self::PhaseExponentValue>>(
            &mut self,
            control: &PauliLike,
            target: &PauliLike,
        ) {
            if self.num_qubits() > 0 {
                super::generic_algos::clifford_left_mul_eq_controlled_pauli(self, control, target);
            }
        }

        fn left_mul_permutation(&mut self, permutation: &[usize], support: &[usize]) {
            assert! {is_permutation(permutation)};
            assert! {has_no_duplicates(support)};
            assert_eq! {permutation.len(), support.len()};
            let mut new_preimages = Vec::<($DensePauli, $DensePauli)>::with_capacity(support.len());
            for elt_index in 0..support.len() {
                new_preimages.push((
                    self.preimage_x_view(support[permutation[elt_index]]).into(),
                    self.preimage_z_view(support[permutation[elt_index]]).into(),
                ));
            }
            for (elt_index, elt) in enumerate(support) {
                <Self as MutablePreImages>::preimage_x_view_mut(self, *elt).assign(&new_preimages[elt_index].0);
                <Self as MutablePreImages>::preimage_z_view_mut(self, *elt).assign(&new_preimages[elt_index].1);
            }
        }
    };
}

fn reindexed_support(new_index: &[usize], bit_support: impl sorted_iter::SortedIterator<Item = usize>) -> IndexSet {
    bit_support.map(|bit| new_index[bit]).collect()
}

fn sparse_projective_pauli_on_support(pauli: &impl Pauli, support: &[usize]) -> PauliUnitaryProjective<IndexSet> {
    PauliUnitaryProjective::<IndexSet>::from_bits(
        reindexed_support(support, pauli.x_bits().support()),
        reindexed_support(support, pauli.z_bits().support()),
    )
}

fn sparse_pauli_on_support<PauliLike: Pauli>(pauli: &PauliLike, support: &[usize]) -> SparsePauli
where
    SparsePauli: Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
{
    let mut res = SparsePauli::from_bits(
        reindexed_support(support, pauli.x_bits().support()),
        reindexed_support(support, pauli.z_bits().support()),
        0,
    );
    res.assign_phase_from(pauli);
    res
}

fn is_permutation(sequence: &[usize]) -> bool {
    let mut seq = sequence.to_vec();
    seq.sort_unstable();
    if seq[0] != 0 {
        return false;
    }
    for j in 0..seq.len() - 1 {
        if seq[j] + 1 != seq[j + 1] {
            return false;
        }
    }
    true
}

fn has_no_duplicates(sequence: &[usize]) -> bool {
    let mut seq = sequence.to_vec();
    seq.sort_unstable();
    for j in 0..seq.len() - 1 {
        if seq[j] == seq[j + 1] {
            return false;
        }
    }
    true
}

impl CliffordMutable for CliffordUnitaryModPauli {
    clifford_mutable_common_impl!();
    clifford_mutable_common_multi_qubit_impl!(<Self as Clifford>::DensePauli);

    fn left_mul_hadamard(&mut self, qubit_id: usize) {
        hadamard_clifford_bits(self.num_qubits(), qubit_id, &mut self.bits);
    }

    fn left_mul_swap(&mut self, qubit1_id: usize, qubit2_id: usize) {
        swap_clifford_bits(self.num_qubits(), qubit1_id, qubit2_id, &mut self.bits);
    }

    fn left_mul_x(&mut self, _qubit_index: usize) {}

    fn left_mul_y(&mut self, _qubit_index: usize) {}

    fn left_mul_z(&mut self, _qubit_index: usize) {}

    #[allow(clippy::similar_names)]
    fn left_mul_clifford<CliffordLike: Clifford + PreimageViews>(
        &mut self,
        clifford: &CliffordLike,
        support: &[usize],
    ) {
        assert_eq! {support.len(),clifford.num_qubits()};
        assert!(has_no_duplicates(support));

        let mut new_preimages =
            Vec::<(<Self as Clifford>::DensePauli, <Self as Clifford>::DensePauli)>::with_capacity(support.len());
        for elt_index in 0..support.len() {
            let px_on_support = sparse_projective_pauli_on_support(&clifford.preimage_x_view(elt_index), support);
            let pz_on_support = sparse_projective_pauli_on_support(&clifford.preimage_z_view(elt_index), support);
            new_preimages.push((self.preimage(&px_on_support), self.preimage(&pz_on_support)));
        }

        for (elt_index, elt) in enumerate(support) {
            self.preimage_x_view_mut(*elt).assign(&new_preimages[elt_index].0);
            self.preimage_z_view_mut(*elt).assign(&new_preimages[elt_index].1);
        }
    }

    type PhaseExponentValue = ();
}

impl MutablePreImages for CliffordUnitary
where
    for<'life> PauliUnitary<MutableBitView<'life, WORD_COUNT_DEFAULT>, &'life mut u8>:
        PauliBinaryOps + Pauli<PhaseExponentValue = u8>,
{
    type PreImageViewMut<'life> = PauliUnitary<MutableBitView<'life, WORD_COUNT_DEFAULT>, &'life mut u8>;

    fn preimage_x_view_mut(&mut self, index: usize) -> Self::PreImageViewMut<'_> {
        let xz_bits = self.bits.rows2_mut(x_preimage_rows_ids(self.num_qubits(), index));
        Self::PreImageViewMut::from_bits_tuple(xz_bits, &mut self.preimage_phase_exponents[phase_of_preimage_x(index)])
    }

    fn preimage_z_view_mut(&mut self, index: usize) -> Self::PreImageViewMut<'_> {
        let xz_bits = self.bits.rows2_mut(z_preimage_rows_ids(self.num_qubits(), index));
        Self::PreImageViewMut::from_bits_tuple(xz_bits, &mut self.preimage_phase_exponents[phase_of_preimage_z(index)])
    }

    fn preimage_xz_views_mut(&mut self, index: usize) -> (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>) {
        unsafe {
            let (xz_of_x, xz_of_z) = split2(
                self.bits
                    .rows4_mut(concat2(xz_preimage_rows_ids(self.num_qubits(), index))),
            );
            let (px, pz) = tuple2_from_vec(
                &mut self.preimage_phase_exponents,
                (phase_of_preimage_x(index), phase_of_preimage_z(index)),
            );
            (
                Self::PreImageViewMut::from_bits_tuple(xz_of_x, px),
                Self::PreImageViewMut::from_bits_tuple(xz_of_z, pz),
            )
        }
    }

    #[allow(clippy::similar_names)]
    fn preimage_xz_views_mut_distinct(
        &mut self,
        index: (usize, usize),
    ) -> (
        (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>),
        (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>),
    ) {
        let (xz_of_x0_ids, xz_of_z0_ids) = xz_preimage_rows_ids(self.num_qubits(), index.0);
        let (xz_of_x1_ids, xz_of_z1_ids) = xz_preimage_rows_ids(self.num_qubits(), index.1);
        unsafe {
            let (xz_of_x0, xz_of_z0, xz_of_x1, xz_of_z1) =
                split4(
                    self.bits
                        .rows8_mut(concat4((xz_of_x0_ids, xz_of_z0_ids, xz_of_x1_ids, xz_of_z1_ids))),
                );
            let (px0, pz0, px1, pz1) = tuple4_from_vec(
                &mut self.preimage_phase_exponents,
                (
                    phase_of_preimage_x(index.0),
                    phase_of_preimage_z(index.0),
                    phase_of_preimage_x(index.1),
                    phase_of_preimage_z(index.1),
                ),
            );
            (
                (
                    Self::PreImageViewMut::from_bits_tuple(xz_of_x0, px0),
                    Self::PreImageViewMut::from_bits_tuple(xz_of_z0, pz0),
                ),
                (
                    Self::PreImageViewMut::from_bits_tuple(xz_of_x1, px1),
                    Self::PreImageViewMut::from_bits_tuple(xz_of_z1, pz1),
                ),
            )
        }
    }

    type PhaseExponentValue = u8;
}

impl CliffordMutable for CliffordUnitary {
    fn left_mul_hadamard(&mut self, qubit_id: usize) {
        hadamard_clifford_bits(self.num_qubits(), qubit_id, &mut self.bits);
        self.preimage_phase_exponents
            .swap(phase_of_preimage_x(qubit_id), phase_of_preimage_z(qubit_id));
    }

    fn left_mul_swap(&mut self, qubit1_id: usize, qubit2_id: usize) {
        swap_clifford_bits(self.num_qubits(), qubit1_id, qubit2_id, &mut self.bits);
        self.preimage_phase_exponents
            .swap(phase_of_preimage_x(qubit1_id), phase_of_preimage_x(qubit2_id));
        self.preimage_phase_exponents
            .swap(phase_of_preimage_z(qubit1_id), phase_of_preimage_z(qubit2_id));
    }

    fn left_mul_x(&mut self, qubit_id: usize) {
        super::generic_algos::clifford_left_mul_eq_x(self, qubit_id);
    }

    fn left_mul_y(&mut self, qubit_id: usize) {
        super::generic_algos::clifford_left_mul_eq_y(self, qubit_id);
    }

    fn left_mul_z(&mut self, qubit_id: usize) {
        super::generic_algos::clifford_left_mul_eq_z(self, qubit_id);
    }

    clifford_mutable_common_impl!();
    clifford_mutable_common_multi_qubit_impl!(<Self as Clifford>::DensePauli);

    #[allow(clippy::similar_names)]
    fn left_mul_clifford<
        CliffordLike: Clifford<PhaseExponentValue = Self::PhaseExponentValue>
            + PreimageViews<PhaseExponentValue = Self::PhaseExponentValue>,
    >(
        &mut self,
        clifford: &CliffordLike,
        support: &[usize],
    ) {
        assert_eq! {support.len(),clifford.num_qubits()};
        assert! {has_no_duplicates(support)};

        let mut new_preimages =
            Vec::<(<Self as Clifford>::DensePauli, <Self as Clifford>::DensePauli)>::with_capacity(support.len());
        for elt_index in 0..support.len() {
            let px_on_support = sparse_pauli_on_support(&clifford.preimage_x_view(elt_index), support);
            let pz_on_support = sparse_pauli_on_support(&clifford.preimage_z_view(elt_index), support);
            new_preimages.push((self.preimage(&px_on_support), self.preimage(&pz_on_support)));
        }

        for (elt_index, elt) in enumerate(support) {
            self.preimage_x_view_mut(*elt).assign(&new_preimages[elt_index].0);
            self.preimage_z_view_mut(*elt).assign(&new_preimages[elt_index].1);
        }
    }

    type PhaseExponentValue = u8;
}

fn clifford_display_fmt<'life, CliffordLike: Clifford + PreimageViews>(
    clifford: &'life CliffordLike,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result
where
    CliffordLike::PreImageView<'life>: fmt::Display,
    CliffordLike::DensePauli: fmt::Display,
{
    if f.alternate() {
        for index in 0..clifford.num_qubits() {
            let index_str = subscript_digits(index);
            write!(f, "Z{}→{:#}, ", index_str, clifford.image_z(index))?;
        }
        for index in 0..clifford.num_qubits() {
            let index_str = subscript_digits(index);
            write!(f, "X{}→{:#}, ", index_str, clifford.image_x(index))?;
        }
        Ok(())
    } else {
        for index in 0..clifford.num_qubits() {
            let index_str = subscript_digits(index);
            write!(f, "Z{}→{}, ", index_str, clifford.image_z(index))?;
        }
        for index in 0..clifford.num_qubits() {
            let index_str = subscript_digits(index);
            write!(f, "X{}→{}, ", index_str, clifford.image_x(index))?;
        }
        Ok(())
    }
}

impl Display for CliffordUnitary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        clifford_display_fmt(self, f)
    }
}

impl Display for CliffordUnitaryModPauli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        clifford_display_fmt(self, f)
    }
}

impl Debug for CliffordUnitary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        clifford_display_fmt(self, f)
    }
}

impl Debug for CliffordUnitaryModPauli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        clifford_display_fmt(self, f)
    }
}

fn clifford_from_str<DensePauliLike, SparsePauliLike, CliffordLike>(
    s: &str,
) -> Result<CliffordLike, CliffordStringParsingError>
where
    DensePauliLike: Pauli
        + NeutralElement<NeutralElementType = DensePauliLike>
        + Clone
        + PauliBinaryOps<SparsePauliLike>
        + fmt::Display,
    SparsePauliLike: Pauli + std::str::FromStr,
    CliffordLike: Clifford<DensePauli = DensePauliLike>,
{
    let trimmed = s.trim().trim_end_matches(',');
    let pauli_images = trimmed.split(['\n', ',']);
    let mut image_pairs = Vec::new();
    for pauli_image in pauli_images {
        let image_parts = pauli_image.split([':', '→']).collect_vec();
        if image_parts.len() == 2 {
            let from = image_parts[0].parse::<SparsePauliLike>();
            let to = image_parts[1].parse::<SparsePauliLike>();
            if let (Ok(pauli_from), Ok(pauli_to)) = (from, to) {
                image_pairs.push((pauli_from, pauli_to));
            } else {
                return Err(CliffordStringParsingError);
            }
        } else {
            return Err(CliffordStringParsingError);
        }
    }
    if image_pairs.len() % 2 == 0 {
        let qubit_count = image_pairs.len() / 2;
        let mut preimages = vec![DensePauliLike::neutral_element_of_size(qubit_count); 2 * qubit_count];
        for (pauli_from, pauli_to) in image_pairs {
            if pauli_from.weight() == 1 {
                if let Some(qubit_id) = pauli_from.support().next() {
                    if pauli_from.is_pauli_x(qubit_id) {
                        preimages[2 * qubit_id].assign(&pauli_to);
                    } else if pauli_from.is_pauli_z(qubit_id) {
                        preimages[2 * qubit_id + 1].assign(&pauli_to);
                    } else {
                        return Err(CliffordStringParsingError);
                    }
                } else {
                    return Err(CliffordStringParsingError);
                }
            } else {
                return Err(CliffordStringParsingError);
            }
        }
        let clifford = CliffordLike::from_preimages(&preimages);
        if !clifford.is_valid() {
            return Err(CliffordStringParsingError);
        }
        Ok(clifford.inverse())
    } else {
        Err(CliffordStringParsingError)
    }
}

impl FromStr for CliffordUnitary {
    type Err = CliffordStringParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        clifford_from_str::<DensePauli, SparsePauli, CliffordUnitary>(s)
    }
}

impl FromStr for CliffordUnitaryModPauli {
    type Err = CliffordStringParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        clifford_from_str::<DensePauliProjective, SparsePauliProjective, CliffordUnitaryModPauli>(s)
    }
}

impl<T: Clifford<PhaseExponentValue = ()>> From<T> for CliffordUnitary
where
    <CliffordUnitary as Clifford>::DensePauli: From<T::DensePauli>,
{
    fn from(value: T) -> Self {
        let mut preimages = Vec::new();
        for j in value.qubits() {
            preimages.push(value.preimage_x(j).into());
            preimages.push(value.preimage_z(j).into());
        }
        Self::from_preimages(&preimages)
    }
}

impl<T: Clifford<PhaseExponentValue = u8>> From<T> for CliffordUnitaryModPauli
where
    <CliffordUnitaryModPauli as Clifford>::DensePauli: From<T::DensePauli>,
{
    fn from(value: T) -> Self {
        let mut preimages = Vec::new();
        for j in value.qubits() {
            preimages.push(value.preimage_x(j).into());
            preimages.push(value.preimage_z(j).into());
        }
        Self::from_preimages(&preimages)
    }
}

/// Multiplication traits
impl Mul for &CliffordUnitary {
    type Output = CliffordUnitary;

    fn mul(self, other: Self) -> CliffordUnitary {
        self.multiply_with(other)
    }
}

impl<'life, Bits: PauliBits, _Phase: PhaseExponent> Mul<&'life mut CliffordUnitary> for &PauliUnitary<Bits, _Phase> {
    type Output = ();

    fn mul(self, clifford: &'life mut CliffordUnitary) -> Self::Output {
        clifford.left_mul_pauli(self);
        // for qubit_index in self.x_bits().support() {
        //     let mut clifford_preimage = clifford.z_preimage_at_mut(qubit_index);
        //     clifford_preimage *= Phase::from_exponent(2u8);
        // }
        // for qubit_index in self.z_bits().support() {
        //     let mut clifford_preimage = clifford.x_preimage_at_mut(qubit_index);
        //     clifford_preimage *= Phase::from_exponent(2u8);
        // }
    }
}

impl<Bits: PauliBits, Phase: PhaseExponent> Mul<&mut CliffordUnitary> for &ControlledPauli<Bits, Phase> {
    type Output = ();

    fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
        clifford.left_mul_controlled_pauli(&self.0, &self.1);
    }
}

impl<Bits: PauliBits, _Phase: PhaseExponent> Mul<&mut CliffordUnitary> for &PauliExponent<Bits, _Phase> {
    type Output = ();

    fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
        clifford.left_mul_pauli_exp(&self.0);
    }
}

impl Mul<&Swap> for CliffordUnitary {
    type Output = CliffordUnitary;

    fn mul(mut self, swap: &Swap) -> CliffordUnitary {
        self.bits.swap_columns(swap.0, swap.1);
        self
    }
}

impl Mul<Swap> for CliffordUnitary {
    type Output = CliffordUnitary;

    fn mul(self, swap: Swap) -> CliffordUnitary {
        self * &swap
    }
}

impl Mul<&mut CliffordUnitary> for &Swap {
    type Output = ();

    fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
        clifford.left_mul_swap(self.0, self.1);
    }
}

impl Mul<&mut CliffordUnitary> for &Hadamard {
    type Output = ();

    fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
        clifford.left_mul_hadamard(self.0);
    }
}

impl<const WORD_COUNT: usize, const QUBIT_COUNT: usize> CliffordModPauliBatch<WORD_COUNT, QUBIT_COUNT> {
    #[must_use]
    pub fn num_qubits(&self) -> usize {
        QUBIT_COUNT
    }

    pub fn preimage_bits_mut(
        &mut self,
        qubit_index: usize,
        axis_index: usize,
        preimage_index: usize,
    ) -> &mut [u64; WORD_COUNT] {
        &mut self.preimages[2 * preimage_index + axis_index][qubit_index]
    }

    #[must_use]
    pub fn preimage_bits(&self, qubit_index: usize, axis_index: usize, preimage_index: usize) -> &[u64; WORD_COUNT] {
        &self.preimages[2 * preimage_index + axis_index][qubit_index]
    }

    fn preimage<PauliLike: Pauli<PhaseExponentValue = ()>>(
        &self,
        pauli: &PauliLike,
    ) -> PauliUnitaryProjective<[u64; WORD_COUNT]> {
        let mut res = PauliUnitaryProjective::<[u64; WORD_COUNT]>::neutral_element_of_size(self.num_qubits());
        super::generic_algos::mul_assign_right_clifford_preimage(&mut res, self, pauli);
        res
    }

    pub fn clear(&mut self) {
        unsafe {
            std::ptr::write_bytes(self.preimages.as_mut_ptr(), 0, 4);
        }
    }
}

impl<const WORD_COUNT: usize, const QUBIT_COUNT: usize> Default for CliffordModPauliBatch<WORD_COUNT, QUBIT_COUNT> {
    fn default() -> Self {
        Self {
            preimages: [[[0u64; WORD_COUNT]; QUBIT_COUNT]; 4],
        }
    }
}

unsafe fn get_pair_mut_unsafe<T>(v: &mut [T; 4], i: usize) -> (&mut T, &mut T) {
    let ptr = v as *mut [T; 4];
    (&mut (*ptr)[i], &mut (*ptr)[i + 1])
}

unsafe fn get_quad_mut_unsafe<T>(v: &mut [T; 4]) -> (&mut T, &mut T, &mut T, &mut T) {
    let ptr = v as *mut [T; 4];
    (&mut (*ptr)[0], &mut (*ptr)[1], &mut (*ptr)[2], &mut (*ptr)[3])
}

unsafe fn get_tuple_mut_unsafe<T, const SIZE: usize>(v: &mut [T; SIZE], i: (usize, usize)) -> (&mut T, &mut T) {
    let ptr = v as *mut [T; SIZE];
    (&mut (*ptr)[i.0], &mut (*ptr)[i.1])
}

impl<const WORD_COUNT: usize, const QUBIT_COUNT: usize> MutablePreImages
    for CliffordModPauliBatch<WORD_COUNT, QUBIT_COUNT>
{
    type PreImageViewMut<'life> = PauliUnitaryProjective<&'life mut [u64; WORD_COUNT]>;

    fn preimage_x_view_mut(&mut self, qubit_index: usize) -> Self::PreImageViewMut<'_> {
        unsafe {
            let (x, z) = get_pair_mut_unsafe(&mut self.preimages, 0);
            PauliUnitaryProjective::from_bits(&mut x[qubit_index], &mut z[qubit_index])
        }
    }

    fn preimage_z_view_mut(&mut self, qubit_index: usize) -> Self::PreImageViewMut<'_> {
        unsafe {
            let (x, z) = get_pair_mut_unsafe(&mut self.preimages, 2);
            PauliUnitaryProjective::from_bits(&mut x[qubit_index], &mut z[qubit_index])
        }
    }

    fn preimage_xz_views_mut(&mut self, index: usize) -> (Self::PreImageViewMut<'_>, Self::PreImageViewMut<'_>) {
        unsafe {
            let (xx, xz, zx, zz) = get_quad_mut_unsafe(&mut self.preimages);
            (
                PauliUnitaryProjective::from_bits(&mut xx[index], &mut xz[index]),
                PauliUnitaryProjective::from_bits(&mut zx[index], &mut zz[index]),
            )
        }
    }

    #[allow(clippy::similar_names)]
    fn preimage_xz_views_mut_distinct(&mut self, index: (usize, usize)) -> crate::Tuple2x2<Self::PreImageViewMut<'_>> {
        debug_assert!(index.0 != index.1);
        unsafe {
            let (xx, xz, zx, zz) = get_quad_mut_unsafe(&mut self.preimages);
            let (xx0, xx1) = get_tuple_mut_unsafe(xx, index);
            let (xz0, xz1) = get_tuple_mut_unsafe(xz, index);
            let (zx0, zx1) = get_tuple_mut_unsafe(zx, index);
            let (zz0, zz1) = get_tuple_mut_unsafe(zz, index);
            (
                (
                    PauliUnitaryProjective::from_bits(xx0, xz0),
                    PauliUnitaryProjective::from_bits(zx0, zz0),
                ),
                (
                    PauliUnitaryProjective::from_bits(xx1, xz1),
                    PauliUnitaryProjective::from_bits(zx1, zz1),
                ),
            )
        }
    }

    type PhaseExponentValue = ();
}

impl<const WORD_COUNT: usize, const QUBIT_COUNT: usize> PreimageViews
    for CliffordModPauliBatch<WORD_COUNT, QUBIT_COUNT>
{
    type PreImageView<'life> = PauliUnitaryProjective<&'life [u64; WORD_COUNT]>;
    type ImageViewUpToPhase<'life> = PauliUnitaryProjective<&'life [u64; WORD_COUNT]>;

    fn preimage_x_view(&self, index: usize) -> Self::PreImageView<'_> {
        PauliUnitaryProjective::from_bits(&self.preimages[0][index], &self.preimages[1][index])
    }

    fn preimage_z_view(&self, index: usize) -> Self::PreImageView<'_> {
        PauliUnitaryProjective::from_bits(&self.preimages[2][index], &self.preimages[3][index])
    }

    fn x_image_view_up_to_phase(&self, _qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        todo!()
    }

    fn z_image_view_up_to_phase(&self, _qubit_index: usize) -> Self::ImageViewUpToPhase<'_> {
        todo!()
    }

    type PhaseExponentValue = ();
}

impl<const WORD_COUNT: usize, const QUBIT_COUNT: usize> CliffordMutable
    for CliffordModPauliBatch<WORD_COUNT, QUBIT_COUNT>
{
    clifford_mutable_common_impl!();

    fn left_mul_x(&mut self, _qubit_index: usize) {}

    fn left_mul_y(&mut self, _qubit_index: usize) {}

    fn left_mul_z(&mut self, _qubit_index: usize) {}

    #[allow(clippy::similar_names)]
    fn left_mul_hadamard(&mut self, qubit_index: usize) {
        unsafe {
            let (xx, xz, zx, zz) = get_quad_mut_unsafe(&mut self.preimages);
            std::mem::swap(&mut xx[qubit_index], &mut zx[qubit_index]);
            std::mem::swap(&mut xz[qubit_index], &mut zz[qubit_index]);
        }
    }

    #[allow(clippy::similar_names)]
    fn left_mul_swap(&mut self, qubit_index1: usize, qubit_index2: usize) {
        unsafe {
            let (xx, xz, zx, zz) = get_quad_mut_unsafe(&mut self.preimages);
            let index = (qubit_index1, qubit_index2);
            let (xx0, xx1) = get_tuple_mut_unsafe(xx, index);
            std::mem::swap(xx0, xx1);
            let (xz0, xz1) = get_tuple_mut_unsafe(xz, index);
            std::mem::swap(xz0, xz1);
            let (zx0, zx1) = get_tuple_mut_unsafe(zx, index);
            std::mem::swap(zx0, zx1);
            let (zz0, zz1) = get_tuple_mut_unsafe(zz, index);
            std::mem::swap(zz0, zz1);
        }
    }

    #[allow(clippy::similar_names)]
    fn left_mul_clifford<CliffordLike: Clifford + PreimageViews>(
        &mut self,
        clifford: &CliffordLike,
        support: &[usize],
    ) {
        assert_eq! {support.len(),clifford.num_qubits()};
        assert!(has_no_duplicates(support));

        let mut new_preimages = Vec::<(
            PauliUnitaryProjective<[u64; WORD_COUNT]>,
            PauliUnitaryProjective<[u64; WORD_COUNT]>,
        )>::with_capacity(support.len());
        for elt_index in 0..support.len() {
            let px_on_support = sparse_projective_pauli_on_support(&clifford.preimage_x_view(elt_index), support);
            let pz_on_support = sparse_projective_pauli_on_support(&clifford.preimage_z_view(elt_index), support);
            new_preimages.push((self.preimage(&px_on_support), self.preimage(&pz_on_support)));
        }

        for (elt_index, elt) in enumerate(support) {
            self.preimage_x_view_mut(*elt).assign(&new_preimages[elt_index].0);
            self.preimage_z_view_mut(*elt).assign(&new_preimages[elt_index].1);
        }
    }

    clifford_mutable_common_multi_qubit_impl!(PauliUnitaryProjective<[u64; WORD_COUNT]>);

    type PhaseExponentValue = ();
}

fn image_block_range(qubit_count: usize, bits: XOrZ, image: XOrZ) -> std::ops::Range<usize> {
    let offset = block_offset(qubit_count, bits, image);
    offset..offset + qubit_count
}

fn image_block_iterator(
    qubit_count: usize,
    bits: XOrZ,
    image: XOrZ,
    iter: impl ExactSizeIterator<Item = usize>,
) -> impl ExactSizeIterator<Item = usize> {
    let offset: usize = block_offset(qubit_count, bits, image);
    iter.map(move |x| x + offset)
}

fn block_offset(qubit_count: usize, bits: XOrZ, image: XOrZ) -> usize {
    use XOrZ::{X, Z};
    match (bits, image) {
        (X, X) => z_of_preimage_z_offset(qubit_count),
        (X, Z) => x_of_preimage_z_offset(qubit_count),
        (Z, X) => z_of_preimage_x_offset(qubit_count),
        (Z, Z) => x_of_preimage_x_offset(qubit_count),
    }
}

trait CliffordBitBlocks {
    type Column<'life>: Bitwise
    where
        Self: 'life;
    type ColumnMutable<'life>: Bitwise + BitwiseBinaryOps<Column<'life>>
    where
        Self: 'life;
    fn block(&self, bits: XOrZ, image: XOrZ) -> impl ExactSizeIterator<Item = Self::Column<'_>>;
    fn block_restriction(
        &self,
        bits: XOrZ,
        image: XOrZ,
        iter: impl ExactSizeIterator<Item = usize>,
    ) -> impl ExactSizeIterator<Item = Self::Column<'_>>;
    fn block_mut(&mut self, bits: XOrZ, image: XOrZ) -> impl ExactSizeIterator<Item = Self::ColumnMutable<'_>>;
    // fn block_restriction_mut(
    //     &mut self,
    //     bits: XOrZ,
    //     image: XOrZ,
    //     iter: impl ExactSizeIterator<Item = usize>,
    // ) -> impl ExactSizeIterator<Item = Self::ColumnMutable<'_>>;
}

macro_rules! clifford_bit_blocks_common {
    () => {
        fn block(&self, bits: XOrZ, image: XOrZ) -> impl ExactSizeIterator<Item = Self::Column<'_>> {
            self.bits
                .row_iterator(image_block_range(self.num_qubits(), bits, image))
        }

        fn block_restriction(
            &self,
            bits: XOrZ,
            image: XOrZ,
            iter: impl ExactSizeIterator<Item = usize>,
        ) -> impl ExactSizeIterator<Item = Self::Column<'_>> {
            self.bits
                .row_iterator(image_block_iterator(self.num_qubits(), bits, image, iter))
        }

        fn block_mut(&mut self, bits: XOrZ, image: XOrZ) -> impl ExactSizeIterator<Item = Self::ColumnMutable<'_>> {
            self.bits
                .row_iterator_mut(image_block_range(self.num_qubits(), bits, image))
        }

        // fn block_restriction_mut(
        //     &mut self,
        //     bits: XOrZ,
        //     image: XOrZ,
        //     iter: impl ExactSizeIterator<Item = usize>,
        // ) -> impl ExactSizeIterator<Item = Self::ColumnMutable<'_>> {
        //     self.bits
        //         .row_iterator_mut(image_block_iterator(self.num_qubits(), bits, image, iter))
        // }
    };
}

impl CliffordBitBlocks for CliffordUnitaryModPauli {
    type Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>;
    type ColumnMutable<'life> = MutableBitView<'life, WORD_COUNT_DEFAULT>;
    clifford_bit_blocks_common!();
}

impl CliffordBitBlocks for CliffordUnitary {
    type Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>;
    type ColumnMutable<'life> = MutableBitView<'life, WORD_COUNT_DEFAULT>;
    clifford_bit_blocks_common!();
}

fn is_x_diagonal(clifford: &impl CliffordBitBlocks) -> bool {
    use XOrZ::{X, Z};
    is_zero_padded_identity(clifford.block(X, X))
        & is_zero_padded_identity(clifford.block(Z, Z))
        & are_zero_rows(&mut clifford.block(Z, X))
}

fn is_z_diagonal(clifford: &impl CliffordBitBlocks) -> bool {
    use XOrZ::{X, Z};
    is_zero_padded_identity(clifford.block(X, X))
        & is_zero_padded_identity(clifford.block(Z, Z))
        & are_zero_rows(&mut clifford.block(X, Z))
}

fn is_css_clifford(clifford: &impl CliffordBitBlocks) -> bool {
    use XOrZ::{X, Z};
    are_zero_rows(&mut clifford.block(X, Z)) & are_zero_rows(&mut clifford.block(Z, X))
}

// fn is_reduced_z_diagonal_resource_encoder<'life, CliffordLike>(clifford: &'life CliffordLike) -> bool
// where
//     CliffordLike: CliffordBitBlocks<Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>> + Clifford,
// {
//     use XOrZ::{X, Z};
//     is_zero_padded_identity(clifford.block(X, Z))
//         & is_zero_padded_symmetric(clifford.block(Z, Z), clifford.num_qubits())
// }

fn is_z_diagonal_resource_encoder<'life, CliffordLike>(clifford: &'life CliffordLike) -> Option<BitMatrix>
where
    CliffordLike: CliffordBitBlocks<Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>> + Clifford,
{
    use XOrZ::{X, Z};
    let qubit_count = clifford.num_qubits();
    let chain = clifford.block(X, Z).chain(clifford.block(Z, Z)).collect_vec();
    is_reduced_symmetric(qubit_count, chain)
}

fn is_reduced_symmetric(qubit_count: usize, chain: Vec<BitView<'_, WORD_COUNT_DEFAULT>>) -> Option<BitMatrix> {
    let mut matrix = BitMatrix::from_row_iter(chain.into_iter(), qubit_count).transposed();
    matrix.echelonize();
    let transposed_rref = matrix.transposed();
    let (top_block, bottom_block) = split_blocks(qubit_count, &transposed_rref);
    if is_zero_padded_identity(top_block) & is_zero_padded_symmetric(bottom_block, qubit_count) {
        Some(transposed_rref)
    } else {
        None
    }
}

fn split_blocks(
    qubit_count: usize,
    transposed_rref: &BitMatrix<WORD_COUNT_DEFAULT>,
) -> (
    impl ExactSizeIterator<Item = BitView<'_, WORD_COUNT_DEFAULT>>,
    impl ExactSizeIterator<Item = BitView<'_, WORD_COUNT_DEFAULT>>,
) {
    let top_block = transposed_rref.row_iterator(0..qubit_count);
    let bottom_block = transposed_rref.row_iterator(qubit_count..2 * qubit_count);
    (top_block, bottom_block)
}

fn is_x_diagonal_resource_encoder<'life, CliffordLike>(clifford: &'life CliffordLike) -> Option<BitMatrix>
where
    CliffordLike: CliffordBitBlocks<Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>> + Clifford,
{
    use XOrZ::{X, Z};
    let qubit_count = clifford.num_qubits();
    let chain = clifford.block(Z, Z).chain(clifford.block(X, Z)).collect_vec();
    is_reduced_symmetric(qubit_count, chain)
}

// fn is_reduced_x_diagonal_resource_encoder<'life, CliffordLike>(clifford: &'life CliffordLike) -> bool
// where
//     CliffordLike: CliffordBitBlocks<Column<'life> = BitView<'life, WORD_COUNT_DEFAULT>> + Clifford,
// {
//     use XOrZ::{X, Z};
//     is_zero_padded_identity(clifford.block(Z, Z))
//         & is_zero_padded_symmetric(clifford.block(X, Z), clifford.num_qubits())
// }

fn blocks_from_diagonal_resource_state<CliffordLike>(encoder: &CliffordLike, axis: XOrZ) -> Option<CliffordLike>
where
    for<'life1> CliffordLike: CliffordBitBlocks<Column<'life1> = BitView<'life1, 8>> + Clifford + 'life1,
    for<'life1, 'life2> <CliffordLike as CliffordBitBlocks>::ColumnMutable<'life1>:
        BitwiseBinaryOps<<CliffordLike as CliffordBitBlocks>::Column<'life2>>,
{
    use XOrZ::{X, Z};
    let some_blocks = match axis {
        X => is_x_diagonal_resource_encoder(encoder),
        Z => is_z_diagonal_resource_encoder(encoder),
    };

    if let Some(blocks) = some_blocks {
        let (_, symmetric_block) = split_blocks(encoder.num_qubits(), &blocks);
        let mut res = CliffordLike::identity(encoder.num_qubits());
        match axis {
            X => {
                for (mut row_to, row_from) in std::iter::zip(res.block_mut(X, Z), symmetric_block) {
                    row_to.assign(&row_from);
                }
            }
            Z => {
                for (mut row_to, row_from) in std::iter::zip(res.block_mut(Z, X), symmetric_block) {
                    row_to.assign(&row_from);
                }
            }
        }
        debug_assert!(res.is_diagonal(axis));
        Some(res)
    } else {
        None
    }
}

#[must_use]
pub fn split_clifford_mod_pauli_with_transforms(
    clifford: &CliffordUnitaryModPauli,
    support: &[usize],
    support_complement: &[usize],
) -> Option<(CliffordUnitaryModPauli, CliffordUnitaryModPauli, BitMatrix, BitMatrix)> {
    use XOrZ::{X, Z};

    let qubit_count = clifford.num_qubits();
    let restriction_transform =
        support_restricted_z_images_from_support_complement::<CliffordUnitaryModPauli>(clifford, support_complement);
    let restriction_transform_complement =
        support_restricted_z_images_from_support_complement::<CliffordUnitaryModPauli>(clifford, support);
    if restriction_transform.rowcount() + restriction_transform_complement.rowcount() != qubit_count {
        return None;
    }
    let stacked_rows = restriction_transform
        .rows()
        .chain(restriction_transform_complement.rows())
        .collect_vec();
    let stacked = BitMatrix::from_row_iter(stacked_rows.into_iter(), clifford.num_qubits());
    let stacked_inv_transpose = stacked.inverted().transposed();
    let split_transform =
        CliffordUnitaryModPauli::from_css_preimage_indicators(&stacked.transposed(), &stacked.inverted());
    let split_clifford = clifford.multiply_with(&split_transform);

    let size1 = support.len();
    let size2 = support_complement.len();
    let mut split_clifford1 = CliffordUnitaryModPauli::zero(size1);
    let mut split_clifford2 = CliffordUnitaryModPauli::zero(size2);
    for image_axis in [X, Z] {
        for bits_axis in [X, Z] {
            let block_from_1 = split_clifford.block_restriction(bits_axis, image_axis, support.iter().copied());
            let block_to_1 = split_clifford1.block_mut(bits_axis, image_axis);
            for (mut row_to, row_from) in zip(block_to_1, block_from_1) {
                row_to.assign_from_interval(&row_from, 0, size1);
            }

            let block_from_2 =
                split_clifford.block_restriction(bits_axis, image_axis, support_complement.iter().copied());
            let block_to_2 = split_clifford2.block_mut(bits_axis, image_axis);
            for (mut row_to, row_from) in zip(block_to_2, block_from_2) {
                row_to.assign_from_interval(&row_from, size1, size2);
            }
        }
    }
    Some((split_clifford1, split_clifford2, stacked, stacked_inv_transpose))
}

#[must_use]
pub fn split_clifford_encoder_mod_pauli(
    clifford: &CliffordUnitaryModPauli,
    support: &[usize],
    support_complement: &[usize],
) -> Option<(CliffordUnitaryModPauli, CliffordUnitaryModPauli)> {
    if let Some((clifford1, clifford2, _, _)) =
        split_clifford_mod_pauli_with_transforms(clifford, support, support_complement)
    {
        Some((clifford1, clifford2))
    } else {
        None
    }
}

pub fn recover_z_images_phases(
    clifford_up_to_phases: &mut CliffordUnitary,
    support: &[usize],
    reference_unitary: &CliffordUnitary,
) {
    for qubit_index in clifford_up_to_phases.qubits() {
        let stabilizer: SparsePauli = clifford_up_to_phases.image_z(qubit_index).into();
        let remapped_stabilizer = remapped_sparse(&stabilizer, support);
        let preimage = reference_unitary.preimage(&remapped_stabilizer);
        if preimage.xz_phase_exponent().wrapping_neg() != 0 {
            clifford_up_to_phases.left_mul_pauli(&clifford_up_to_phases.preimage_x(qubit_index));
        }
        debug_assert!(preimage.x_bits().is_zero());
    }
}

#[must_use]
pub fn split_clifford_encoder(
    first_part_qubit_count: usize,
    tensor_product_encoder: &CliffordUnitary,
) -> Option<(CliffordUnitary, CliffordUnitary)> {
    let first_part_qubits = (0..first_part_qubit_count).collect_vec();
    let second_part_qubits = (first_part_qubit_count..tensor_product_encoder.num_qubits()).collect_vec();
    if let Some((first_part_encoder_mod_pauli, second_part_encoder_mod_pauli)) = split_clifford_encoder_mod_pauli(
        &tensor_product_encoder.clone().into(),
        &first_part_qubits,
        &second_part_qubits,
    ) {
        let mut first_part_encoder: CliffordUnitary = first_part_encoder_mod_pauli.into();
        let mut second_part_encoder: CliffordUnitary = second_part_encoder_mod_pauli.into();
        recover_z_images_phases(&mut first_part_encoder, &first_part_qubits, tensor_product_encoder);
        recover_z_images_phases(&mut second_part_encoder, &second_part_qubits, tensor_product_encoder);
        Some((first_part_encoder, second_part_encoder))
    } else {
        None
    }
}

pub fn prepare_all_zero(qubit_count: usize) -> CliffordUnitaryModPauli {
    CliffordUnitaryModPauli::identity(qubit_count)
}

pub fn prepare_all_plus(qubit_count: usize) -> CliffordUnitaryModPauli {
    prepare_zero_plus(qubit_count, &(0..qubit_count).collect_vec())
}

pub fn prepare_zero_plus(qubit_count: usize, plus_indicies: &[usize]) -> CliffordUnitaryModPauli {
    let mut result = CliffordUnitaryModPauli::identity(qubit_count);
    for qubit_index in plus_indicies {
        result.left_mul_hadamard(*qubit_index);
    }
    result
}

#[must_use]
pub fn split_phased_css(
    clifford: &CliffordUnitaryModPauli,
) -> Option<(CliffordUnitaryModPauli, CliffordUnitaryModPauli)> {
    let qubit_count = clifford.num_qubits();
    let plus_resource = clifford.multiply_with(&prepare_all_plus(qubit_count));
    if let Some(diagonal_part) = plus_resource.unitary_from_diagonal_resource_state(XOrZ::Z) {
        // assert!(diagonal_part.multiply_with(&diagonal_part).is_identity());
        let css_remainder = diagonal_part.multiply_with(clifford);
        if css_remainder.is_css() {
            return Some((diagonal_part, css_remainder));
        }
        return None;
    }
    None
}

#[must_use]
pub fn split_qubit_tensor_product_encoder(clifford: &CliffordUnitaryModPauli) -> Option<Vec<Axis>> {
    let mut res = Vec::new();
    for qubit_index in clifford.qubits() {
        if clifford.preimage_x(qubit_index).x_bits().is_zero() {
            res.push(Axis::X);
        } else if clifford
            .preimage::<SparsePauliProjective>(&[y(qubit_index)].into())
            .x_bits()
            .is_zero()
        {
            res.push(Axis::Y);
        } else if clifford.preimage_z(qubit_index).x_bits().is_zero() {
            res.push(Axis::Z);
        } else {
            return None;
        }
    }
    Some(res)
}

#[must_use]
pub fn split_qubit_cliffords_and_css(
    clifford: &CliffordUnitaryModPauli,
) -> Option<(CliffordUnitaryModPauli, CliffordUnitaryModPauli)> {
    let qubit_count = clifford.num_qubits();
    let plus_resource = clifford.multiply_with(&prepare_all_plus(qubit_count));
    let zero_resource = clifford.multiply_with(&prepare_all_zero(qubit_count));
    if let (Some(plus_axes), Some(zero_axes)) = (
        split_qubit_tensor_product_encoder(&plus_resource),
        split_qubit_tensor_product_encoder(&zero_resource),
    ) {
        let mut qubit_product = CliffordUnitaryModPauli::identity(clifford.num_qubits());
        for (qubit_index, (zero_image, plus_image)) in enumerate(zip(zero_axes, plus_axes)) {
            apply_qubit_clifford_by_axis(&mut qubit_product, qubit_index, zero_image, plus_image)?;
        }
        let css_remainder = qubit_product.inverse().multiply_with(clifford);
        if css_remainder.is_css() {
            return Some((qubit_product, css_remainder));
        }
        return None;
    }
    None
}

pub fn apply_qubit_clifford_by_axis(
    qubit_product: &mut CliffordUnitaryModPauli,
    qubit_index: usize,
    zero_image: Axis,
    plus_image: Axis,
) -> Option<()> {
    match (zero_image, plus_image) {
        (Axis::Y, Axis::Y) | (Axis::Z, Axis::Z) | (Axis::X, Axis::X) => {
            return None;
        }
        (Axis::Z, Axis::X) => {}
        (Axis::Z, Axis::Y) => {
            qubit_product.left_mul_root_z(qubit_index);
        }
        (Axis::X, Axis::Z) => {
            qubit_product.left_mul_root_y(qubit_index);
        }
        (Axis::X, Axis::Y) => {
            qubit_product.left_mul_root_y(qubit_index);
            qubit_product.left_mul_root_x(qubit_index);
        }
        (Axis::Y, Axis::X) => {
            qubit_product.left_mul_root_x(qubit_index);
        }
        (Axis::Y, Axis::Z) => {
            qubit_product.left_mul_root_y(qubit_index);
            qubit_product.left_mul_root_z(qubit_index);
        }
    }
    Some(())
}

#[must_use]
pub fn random_clifford_via_operations_sampling<CliffordLike: Clifford + CliffordMutable>(
    qubit_count: usize,
    num_random_generators: usize,
    operations: &crate::operations::Operations,
) -> CliffordLike {
    let mut random_clifford = CliffordLike::identity(qubit_count);
    for _ in 0..num_random_generators {
        let (unitary_operation, support) = &operations[rand::random::<usize>() % operations.len()];
        random_clifford.left_mul(*unitary_operation, support);
    }
    random_clifford
}

pub fn dense_restriction_of(
    pauli: &impl Pauli<PhaseExponentValue = u8>,
    support: impl SortedIterator<Item = usize> + Clone,
    qubit_count: usize,
) -> DensePauli {
    let (mut x_bits, mut z_bits) = DensePauli::neutral_element_of_size(qubit_count).to_xz_bits();
    for index in pauli
        .x_bits()
        .support()
        .intersection(support.clone().assume_sorted_by_item())
    {
        x_bits.assign_index(index, true);
    }
    for index in pauli.z_bits().support().intersection(support.assume_sorted_by_item()) {
        z_bits.assign_index(index, true);
    }
    DensePauli::from_bits(x_bits, z_bits, pauli.xz_phase_exponent())
}

/// # Panics
/// If the generators are not mutually commuting.
pub fn group_encoding_clifford_of<PauliLike: Pauli<PhaseExponentValue = u8>>(
    generators: &[PauliLike],
    qubit_count: usize,
) -> CliffordUnitary
where
    DensePauli: PauliBinaryOps<PauliLike>,
{
    assert!(are_mutually_commuting(generators));
    let mut current_support = (0..qubit_count).collect::<BTreeSet<_>>();
    let mut current_images = generators
        .iter()
        .map(|sparse| dense_from(sparse, qubit_count))
        .collect_vec();
    let mut result = CliffordUnitary::identity(qubit_count);
    let mut pivots = Vec::new();
    for index in 0..current_images.len() {
        let mut remainder = dense_restriction_of(
            &current_images[index],
            current_support.iter().copied().assume_sorted_by_item(),
            qubit_count,
        );
        let support_first = remainder.support().next();
        if let Some(non_identity_index) = support_first {
            let x_bit = remainder.x_bits().index(non_identity_index);
            // ensure that x_bit is true
            if !x_bit {
                apply_root_x(&mut remainder, non_identity_index);
                result.left_mul_root_x(non_identity_index);
                for current_image in current_images.iter_mut().skip(index) {
                    apply_root_x(current_image, non_identity_index);
                }
            }

            remainder.mul_assign_left_z(non_identity_index);
            remainder.add_assign_phase_exp(1);

            result.left_mul_pauli_exp(&remainder);
            for current_image in current_images.iter_mut().skip(index) {
                apply_pauli_exponent(current_image, &remainder);
            }

            current_support.remove(&non_identity_index);
            pivots.push(non_identity_index);
        } else {
            panic!("Group generators are not independent")
        }
    }
    let new_order = pivots.iter().chain(current_support.iter()).copied().collect_vec();
    result.left_mul_permutation(&new_order, &(0..qubit_count).collect_vec());
    result.inverse()
}

// PartialEq trait

impl PartialEq for CliffordUnitaryModPauli {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}

impl PartialEq for CliffordUnitary {
    fn eq(&self, other: &Self) -> bool {
        zip(&self.preimage_phase_exponents, &other.preimage_phase_exponents)
            .all(|(x, y)| <u8 as PhaseExponent>::raw_eq(*x, *y))
            && (self.bits == other.bits)
    }
}

impl std::hash::Hash for CliffordUnitary {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits.hash(state);
        todo!("not implemented yet");
    }
}

impl std::hash::Hash for CliffordUnitaryModPauli {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits.hash(state);
    }
}
