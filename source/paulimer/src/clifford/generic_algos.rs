use itertools::Itertools;

use super::{Clifford, CliffordMutable, MutablePreImages, PreimageViews};
use crate::pauli::{PauliMutable, PauliMutableBits, PauliNeutralElement};
use crate::{
    bits::{
        bitmatrix::{kernel_basis_matrix, MutableRow},
        BitMatrix, Bitwise, BitwiseBinaryOps,
    },
    pauli::{anti_commutes_with, commutes_with, Pauli, PauliBinaryOps, PauliBits},
    setwise::complement,
    NeutralElement,
};

pub fn mul_assign_right_clifford_preimage_x_bits<'life, Target, PreImageUnder: PreimageViews>(
    target: &mut Target,
    clifford: &'life PreImageUnder,
    bits: &impl Bitwise,
) where
    Target: PauliBinaryOps<PreImageUnder::PreImageView<'life>>,
{
    for id in bits.support() {
        target.mul_assign_right(&clifford.preimage_x_view(id));
    }
}

pub fn mul_assign_right_clifford_preimage_z_bits<'life, Target, PreImageUnder: PreimageViews>(
    target: &mut Target,
    clifford: &'life PreImageUnder,
    bits: &impl Bitwise,
) where
    Target: PauliBinaryOps<PreImageUnder::PreImageView<'life>>,
{
    for id in bits.support() {
        target.mul_assign_right(&clifford.preimage_z_view(id));
    }
}

pub fn mul_assign_right_clifford_preimage<
    'life,
    Target,
    PreImageUnder: PreimageViews,
    PreImageOf: Pauli,
>(
    target: &mut Target,
    clifford: &'life PreImageUnder,
    pauli: &PreImageOf,
) where
    Target: PauliBinaryOps<PreImageUnder::PreImageView<'life>>
        + Pauli<PhaseExponentValue = PreImageOf::PhaseExponentValue>,
{
    mul_assign_right_clifford_preimage_x_bits(target, clifford, pauli.x_bits());
    mul_assign_right_clifford_preimage_z_bits(target, clifford, pauli.z_bits());
    target.mul_assign_phase_from(pauli);
}

pub fn mul_assign_right_clifford_image_x_bits_up_to_phase<
    'life,
    Target,
    ImageUnder: Clifford + PreimageViews,
>(
    target: &mut Target,
    clifford: &'life ImageUnder,
    bits: &impl Bitwise,
) where
    Target: PauliBinaryOps<ImageUnder::ImageViewUpToPhase<'life>>,
{
    for id in bits.support() {
        target.mul_assign_right(&clifford.x_image_view_up_to_phase(id));
    }
}

pub fn mul_assign_right_clifford_image_z_bits_up_to_phase<
    'life,
    Target,
    ImageUnder: Clifford + PreimageViews,
>(
    target: &mut Target,
    clifford: &'life ImageUnder,
    bits: &impl Bitwise,
) where
    Target: PauliBinaryOps<ImageUnder::ImageViewUpToPhase<'life>>,
{
    for id in bits.support() {
        target.mul_assign_right(&clifford.z_image_view_up_to_phase(id));
    }
}

pub fn mul_assign_right_clifford_image_up_to_phase<
    'life,
    Target,
    ImageUnder: Clifford + PreimageViews,
    ImageOf: Pauli,
>(
    target: &mut Target,
    clifford: &'life ImageUnder,
    pauli: &ImageOf,
) where
    Target: PauliBinaryOps<ImageUnder::ImageViewUpToPhase<'life>>,
{
    mul_assign_right_clifford_image_x_bits_up_to_phase(target, clifford, pauli.x_bits());
    mul_assign_right_clifford_image_z_bits_up_to_phase(target, clifford, pauli.z_bits());
}

#[allow(clippy::similar_names)]
pub fn is_valid_clifford<CliffordLike: Clifford + PreimageViews>(candidate: &CliffordLike) -> bool {
    for index in 0..candidate.num_qubits() {
        let x_preimage = candidate.preimage_x_view(index);
        let z_preimage = candidate.preimage_z_view(index);
        if commutes_with(&x_preimage, &z_preimage) {
            println!("commutes_with failed for:{index}");
            return false;
        }
        for other_index in index + 1..candidate.num_qubits() {
            let other_x_preimage = candidate.preimage_x_view(other_index);
            let other_z_preimage = candidate.preimage_z_view(other_index);
            let xx = anti_commutes_with(&x_preimage, &other_x_preimage);
            let xz = anti_commutes_with(&x_preimage, &other_z_preimage);
            let zx = anti_commutes_with(&z_preimage, &other_x_preimage);
            let zz = anti_commutes_with(&z_preimage, &other_z_preimage);
            if xx || xz || zx || zz {
                println!(
                    "anti_commutes_with failed for:{index}, {other_index}, {xx} {xz} {zx} {zz}"
                );
                return false;
            }
        }
    }
    true
}

pub fn clifford_left_mul_eq_cnot(
    clifford: &mut impl MutablePreImages,
    control_id: usize,
    target_id: usize,
) {
    let ((mut xc, zc), (xt, mut zt)) =
        clifford.preimage_xz_views_mut_distinct((control_id, target_id));
    xc.mul_assign_left(&xt);
    zt.mul_assign_left(&zc);
}

pub fn clifford_left_mul_eq_cz(
    clifford: &mut impl MutablePreImages,
    control_id: usize,
    target_id: usize,
) {
    let ((mut xc, zc), (mut xt, zt)) =
        clifford.preimage_xz_views_mut_distinct((control_id, target_id));
    xc.mul_assign_left(&zt);
    xt.mul_assign_left(&zc);
}

pub fn clifford_left_mul_eq_root_z(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (mut x, z) = clifford.preimage_xz_views_mut(qubit_id);
    x.mul_assign_left(&z);
    x.add_assign_phase_exp(1);
}

pub fn clifford_left_mul_eq_root_z_inverse(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (mut x, z) = clifford.preimage_xz_views_mut(qubit_id);
    x.mul_assign_left(&z);
    x.add_assign_phase_exp(3);
}

pub fn clifford_left_mul_eq_root_x(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (x, mut z) = clifford.preimage_xz_views_mut(qubit_id);
    z.mul_assign_left(&x);
    z.add_assign_phase_exp(1);
}

pub fn clifford_left_mul_eq_root_y(clifford: &mut impl CliffordMutable, qubit_id: usize) {
    clifford.left_mul_z(qubit_id);
    clifford.left_mul_hadamard(qubit_id);
}

pub fn clifford_left_mul_eq_root_y_inverse(clifford: &mut impl CliffordMutable, qubit_id: usize) {
    clifford.left_mul_hadamard(qubit_id);
    clifford.left_mul_z(qubit_id);
}

pub fn clifford_left_mul_eq_x(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (_, mut z) = clifford.preimage_xz_views_mut(qubit_id);
    z.add_assign_phase_exp(2);
}

pub fn clifford_left_mul_eq_z(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (mut x, _) = clifford.preimage_xz_views_mut(qubit_id);
    x.add_assign_phase_exp(2);
}

pub fn clifford_left_mul_eq_y(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (mut x, mut z) = clifford.preimage_xz_views_mut(qubit_id);
    x.add_assign_phase_exp(2);
    z.add_assign_phase_exp(2);
}

pub fn clifford_left_mul_eq_root_x_inverse(clifford: &mut impl MutablePreImages, qubit_id: usize) {
    let (x, mut z) = clifford.preimage_xz_views_mut(qubit_id);
    z.mul_assign_left(&x);
    z.add_assign_phase_exp(3);
}

pub fn support_restricted_z_images_from_support_complement<CliffordLike>(
    clifford: &CliffordLike,
    support_complement: &[usize],
) -> BitMatrix
where
    CliffordLike: Clifford + PreimageViews,
    for<'a> MutableRow<'a>: BitwiseBinaryOps<<CliffordLike::PreImageView<'a> as Pauli>::Bits>,
{
    let num_qubits = clifford.num_qubits();
    let mut a = BitMatrix::zeros(2 * support_complement.len(), num_qubits);
    let complement_size = support_complement.len();

    for (j, qubit_index) in itertools::enumerate(support_complement) {
        let pre_img_x = clifford.preimage_x_view(*qubit_index);
        a.row_mut(j).assign(pre_img_x.x_bits());
    }

    for (j, qubit_index) in itertools::enumerate(support_complement) {
        let pre_img_z = clifford.preimage_z_view(*qubit_index);
        a.row_mut(j + complement_size).assign(pre_img_z.x_bits());
    }

    // for (j, qubit_index) in enumerate(support_complement) {
    //     let pre_img_z = clifford.z_preimage_view(*qubit_index);
    //     let pre_img_x = clifford.x_preimage_view(*qubit_index);
    //     for i in 0..num_qubits {
    //         a.set((j, i), pre_img_x.x_bits().index(i));
    //         a.set((j + complement_size, i), pre_img_z.x_bits().index(i))
    //     }
    // }

    kernel_basis_matrix(&a)
}

// pub fn z_clifford_image<'life, CliffordLike, ZBits>(
//     clifford: &'life CliffordLike,
//     z_bits: ZBits,
// ) -> <CliffordLike as Clifford<'life>>::ImageUpToPhase
// where
//     CliffordLike: Clifford<'life>,
//     ZBits: IntoIterator<Item = bool>,
// {
//     if clifford.num_qubits() == 0 {
//         <CliffordLike::ImageViewUpToPhase<'life> as NeutralElement>::default_size_neutral_element()
//     } else {
//         let mut res = clifford.z_image_view_up_to_phase(0).neutral_element();
//         for (index, val) in enumerate(z_bits.into_iter()) {
//             if val {
//                 res.mul_assign_left(&clifford.z_image_view_up_to_phase(index));
//             }
//         }
//         res
//     }
// }

// pub fn x_clifford_image<'life, CliffordLike, ZBits>(
//     clifford: &'life CliffordLike,
//     x_bits: ZBits,
// ) -> <CliffordLike as CliffordWithNeutrals<'life>>::ImageUpToPhase
// where
//     CliffordLike: CliffordWithNeutrals<'life>,
//     ZBits: IntoIterator<Item = bool>,
// {
//     if clifford.num_qubits() == 0 {
//         <CliffordLike::ImageViewUpToPhase<'life> as NeutralElement>::default_size_neutral_element()
//     } else {
//         let mut res = clifford.x_image_view_up_to_phase(0).neutral_element();
//         for (index, val) in enumerate(x_bits.into_iter()) {
//             if val {
//                 res.mul_assign_left(&clifford.x_image_view_up_to_phase(index));
//             }
//         }
//         res
//     }
// }

/// Each row of result a is such that image of Z^a is supported on `supported_on_qubits`.
/// The result has full row rank; the rank is maximal possible.
pub fn support_restricted_z_images<CliffordLike>(
    clifford: &CliffordLike,
    sorted_support: &[usize],
) -> BitMatrix
where
    CliffordLike: Clifford + PreimageViews,
    for<'a> MutableRow<'a>: BitwiseBinaryOps<<CliffordLike::PreImageView<'a> as Pauli>::Bits>,
{
    let num_qubits = clifford.num_qubits();
    let support_complement = complement(sorted_support, num_qubits);
    support_restricted_z_images_from_support_complement::<CliffordLike>(
        clifford,
        &support_complement,
    )
}

pub fn clifford_left_mul_eq_prepare_bell<CliffordLike>(
    clifford: &mut CliffordLike,
    qubit_index1: usize,
    qubit_index2: usize,
) where
    CliffordLike: CliffordMutable,
{
    clifford.left_mul_hadamard(qubit_index1);
    clifford.left_mul_cx(qubit_index1, qubit_index2);
}

// Prepares state |0>^{k_1} \otimes |Bell_k> \otimes |0>^{k_2}
// Where |Bell_k> are k Bell states between qubits j and j + k for j in [k]
#[must_use]
pub fn clifford_to_prepare_bell_states<CliffordLike>(num_bell_pairs: usize) -> CliffordLike
where
    CliffordLike: CliffordMutable + Clifford,
{
    let mut res = CliffordLike::identity(num_bell_pairs * 2);
    for q in 0..num_bell_pairs {
        clifford_left_mul_eq_prepare_bell(&mut res, q, q + num_bell_pairs);
    }
    res
}

/// # Panics
///
/// Will panic if Clifford unitaries act on different number of qubits
pub fn clifford_multiply_with<
    PhaseType,
    CliffordLike: Clifford<PhaseExponentValue = PhaseType>
        + PreimageViews<PhaseExponentValue = PhaseType>
        + MutablePreImages<PhaseExponentValue = PhaseType>,
>(
    left: &CliffordLike,
    right: &CliffordLike,
) -> CliffordLike
where
    for<'life> <CliffordLike as MutablePreImages>::PreImageViewMut<'life>:
        PauliBinaryOps<<CliffordLike as Clifford>::DensePauli>,
{
    assert!(left.num_qubits() == right.num_qubits());
    let mut result = CliffordLike::identity(left.num_qubits());
    for qubit_index in 0..left.num_qubits() {
        result
            .preimage_x_view_mut(qubit_index)
            .assign(&right.preimage(&left.preimage_x_view(qubit_index)));
        result
            .preimage_z_view_mut(qubit_index)
            .assign(&right.preimage(&left.preimage_z_view(qubit_index)));
    }
    result
}

pub fn clifford_is_identity<CliffordLike: Clifford + PreimageViews>(
    clifford: &CliffordLike,
) -> bool {
    for qubit_id in 0..clifford.num_qubits() {
        if !clifford.preimage_x_view(qubit_id).is_pauli_x(qubit_id) {
            return false;
        }
        if !clifford.preimage_z_view(qubit_id).is_pauli_z(qubit_id) {
            return false;
        }
    }
    true
}

/// # Panics
///
/// Will panic if preimages do not correspond to a Clifford unitary
pub fn clifford_from_preimages<'life, Paulis, PauliLike: Pauli + 'life, CliffordLike>(
    preimages: Paulis,
) -> CliffordLike
where
    CliffordLike: Clifford + MutablePreImages + PreimageViews,
    for<'life1> <CliffordLike as MutablePreImages>::PreImageViewMut<'life1>:
        PauliBinaryOps<PauliLike>,
    Paulis: ExactSizeIterator<Item = &'life PauliLike>,
{
    let preimages_iter = preimages.into_iter();
    assert!(preimages_iter.len() % 2 == 0);
    let mut res = CliffordLike::zero(preimages_iter.len() / 2);
    for (qubit_index, xz_preimage) in preimages_iter.tuples().enumerate() {
        let (x_preimage, z_preimage) = xz_preimage;
        res.preimage_x_view_mut(qubit_index).assign(x_preimage);
        res.preimage_z_view_mut(qubit_index).assign(z_preimage);
    }
    assert!(is_valid_clifford(&res));
    res
}

pub fn clifford_from_images<'life, Paulis, PauliLike: Pauli + 'life, CliffordLike>(
    images: Paulis,
) -> CliffordLike
where
    CliffordLike: Clifford + MutablePreImages + PreimageViews,
    for<'life1> <CliffordLike as MutablePreImages>::PreImageViewMut<'life1>:
        PauliBinaryOps<PauliLike>,
    Paulis: ExactSizeIterator<Item = &'life PauliLike>,
{
    clifford_from_preimages::<Paulis, PauliLike, CliffordLike>(images).inverse()
}

pub fn clifford_image_with_phase<CliffordLike: Clifford + PreimageViews, Bits: PauliBits>(
    clifford: &CliffordLike,
    xz_bits: (Bits, Bits),
) -> CliffordLike::DensePauli
where
    CliffordLike::DensePauli: PauliMutable,
    CliffordLike::DensePauli: From<(Bits, Bits)>,
    for<'life> <CliffordLike as PreimageViews>::PreImageView<'life>:
        PauliNeutralElement<NeutralElementType = CliffordLike::DensePauli>,
{
    let mut preimage = clifford.preimage_x_view(0).neutral_element();
    mul_assign_right_clifford_preimage_x_bits(&mut preimage, clifford, &xz_bits.0);
    mul_assign_right_clifford_preimage_z_bits(&mut preimage, clifford, &xz_bits.1);
    preimage.complex_conjugate();
    let mut res = CliffordLike::DensePauli::from(xz_bits);
    res.mul_assign_phase_from(&preimage);
    res
}

pub fn clifford_left_mul_eq_pauli_exp<
    PauliLike: Pauli,
    CliffordLike: PreimageViews<PhaseExponentValue = PauliLike::PhaseExponentValue>
        + MutablePreImages<PhaseExponentValue = PauliLike::PhaseExponentValue>,
>(
    clifford: &mut CliffordLike,
    pauli: &PauliLike,
) where
    for<'life> <CliffordLike as PreimageViews>::PreImageView<'life>:
        PauliNeutralElement + Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
    for<'life1, 'life2> <CliffordLike as MutablePreImages>::PreImageViewMut<'life1>: PauliBinaryOps<<<CliffordLike as PreimageViews>::PreImageView<'life2> as NeutralElement>::NeutralElementType>
        + Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
    for<'life> <<CliffordLike as PreimageViews>::PreImageView<'life> as NeutralElement>::NeutralElementType:
        Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
{
    let mut pauli_preimage = clifford.preimage_x_view(0).neutral_element();
    mul_assign_right_clifford_preimage(&mut pauli_preimage, clifford, pauli);
    pauli_preimage.add_assign_phase_exp(1u8);
    for index in pauli.x_bits().support() {
        clifford
            .preimage_z_view_mut(index)
            .mul_assign_right(&pauli_preimage);
    }
    for index in pauli.z_bits().support() {
        clifford
            .preimage_x_view_mut(index)
            .mul_assign_right(&pauli_preimage);
    }
}

pub fn clifford_left_mul_eq_controlled_pauli<CliffordLike: PreimageViews + MutablePreImages, PauliLike: Pauli>(
    clifford: &mut CliffordLike,
    control: &PauliLike,
    target: &PauliLike,
) where
    for<'life> <CliffordLike as PreimageViews>::PreImageView<'life>:
        PauliNeutralElement + Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
    for<'life> <CliffordLike as MutablePreImages>::PreImageViewMut<'life>:
        PauliBinaryOps<<<CliffordLike as PreimageViews>::PreImageView<'life> as NeutralElement>::NeutralElementType>,
    for<'life> <<CliffordLike as PreimageViews>::PreImageView<'life> as NeutralElement>::NeutralElementType:
        Pauli<PhaseExponentValue = PauliLike::PhaseExponentValue>,
{
    debug_assert!(commutes_with(control, target));
    let mut target_preimage = clifford.preimage_x_view(0).neutral_element();
    let mut control_preimage = clifford.preimage_x_view(0).neutral_element();
    mul_assign_right_clifford_preimage(&mut target_preimage, clifford, target);
    mul_assign_right_clifford_preimage(&mut control_preimage, clifford, control);
    for index in control.x_bits().support() {
        clifford
            .preimage_z_view_mut(index)
            .mul_assign_right(&target_preimage);
    }
    for index in control.z_bits().support() {
        clifford
            .preimage_x_view_mut(index)
            .mul_assign_right(&target_preimage);
    }
    for index in target.x_bits().support() {
        clifford
            .preimage_z_view_mut(index)
            .mul_assign_left(&control_preimage);
    }
    for index in target.z_bits().support() {
        clifford
            .preimage_x_view_mut(index)
            .mul_assign_left(&control_preimage);
    }
}

#[must_use]
pub fn clifford_from_css_preimage_indicators<CliffordLike: Clifford + MutablePreImages>(
    x_indicators: &BitMatrix,
    z_indicators: &BitMatrix,
) -> CliffordLike
where
    for<'life> <CliffordLike as MutablePreImages>::PreImageViewMut<'life>:
        PauliMutableBits<crate::bits::BitView<'life>>,
{
    let num_qubits = x_indicators.columncount();
    let mut res = CliffordLike::zero(num_qubits);
    for qubit_index in res.qubits() {
        res.preimage_x_view_mut(qubit_index)
            .x_bits_mut()
            .assign(&x_indicators.row(qubit_index));
        res.preimage_z_view_mut(qubit_index)
            .z_bits_mut()
            .assign(&z_indicators.row(qubit_index));
    }
    debug_assert!(res.is_valid());
    res
}

pub fn clifford_tensored<CliffordLike>(left: &CliffordLike, right: &CliffordLike) -> CliffordLike
where
    CliffordLike: Clifford + MutablePreImages + PreimageViews,
    for<'life> <CliffordLike as MutablePreImages>::PreImageViewMut<'life>:
        PauliBinaryOps<<CliffordLike as PreimageViews>::PreImageView<'life>>,
{
    let lhs_num_qubits = left.num_qubits();
    let rhs_num_qubits = right.num_qubits();
    let num_output_qubits = lhs_num_qubits + rhs_num_qubits;
    let mut result = CliffordLike::zero(num_output_qubits);
    for qubit_id in 0..lhs_num_qubits {
        result.preimage_x_view_mut(qubit_id).assign_with_offset(
            &left.preimage_x_view(qubit_id),
            0,
            lhs_num_qubits,
        );
        result.preimage_z_view_mut(qubit_id).assign_with_offset(
            &left.preimage_z_view(qubit_id),
            0,
            lhs_num_qubits,
        );
    }
    for qubit_id in 0..rhs_num_qubits {
        result
            .preimage_x_view_mut(lhs_num_qubits + qubit_id)
            .assign_with_offset(
                &right.preimage_x_view(qubit_id),
                lhs_num_qubits,
                rhs_num_qubits,
            );
        result
            .preimage_z_view_mut(lhs_num_qubits + qubit_id)
            .assign_with_offset(
                &right.preimage_z_view(qubit_id),
                lhs_num_qubits,
                rhs_num_qubits,
            );
    }
    debug_assert!(result.is_valid());
    result
}

pub fn clifford_inverse_up_to_signs<CliffordLikeTo, CliffordLikeFrom: Clifford + PreimageViews>(
    from: &CliffordLikeFrom,
) -> CliffordLikeTo
where
    CliffordLikeTo: Clifford + MutablePreImages,
    for<'life1, 'life2> <CliffordLikeTo as MutablePreImages>::PreImageViewMut<'life1>:
        PauliBinaryOps<<CliffordLikeFrom as PreimageViews>::ImageViewUpToPhase<'life2>>,
{
    let mut res = CliffordLikeTo::identity(from.num_qubits());
    for qubit_index in 0..from.num_qubits() {
        res.preimage_x_view_mut(qubit_index)
            .assign(&from.x_image_view_up_to_phase(qubit_index));
        res.preimage_z_view_mut(qubit_index)
            .assign(&from.z_image_view_up_to_phase(qubit_index));
    }
    res
}
