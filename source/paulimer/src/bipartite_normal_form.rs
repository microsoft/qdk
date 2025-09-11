use itertools::enumerate;
use std::ops::Range;

use crate::{
    bits::{
        bitmatrix::{directly_summed, full_rank_row_completion_with_inv, row_stacked, BitMatrix, MutableRow},
        BitVec, Bitwise, BitwiseBinaryOps, IndexAssignable, WORD_COUNT_DEFAULT,
    },
    clifford::{
        generic_algos::{support_restricted_z_images, support_restricted_z_images_from_support_complement},
        Clifford, CliffordUnitary, PreimageViews,
    },
    pauli::{anti_commutes_with, commutes_with, DensePauli, Pauli, PauliBinaryOps, PauliUnitary, Phase},
    setwise::{complement, is_subset},
    NeutralElement,
};

/// # Panics
///
/// Will panic
pub fn css_clifford(
    computational_basis_action: &BitMatrix,
    computational_basis_action_inv_t: &BitMatrix,
) -> CliffordUnitary {
    assert_eq!(
        computational_basis_action.rowcount(),
        computational_basis_action.columncount()
    );
    assert_eq!(
        computational_basis_action_inv_t.rowcount(),
        computational_basis_action_inv_t.columncount()
    );

    CliffordUnitary::from_css_preimage_indicators(computational_basis_action_inv_t, computational_basis_action)
}

/// # Safety
/// Get a pair of mutable indexes without checking if they are distinct
pub unsafe fn get_pair_mut_unsafe<T>(v: &mut Vec<T>, i: usize, j: usize) -> (&mut T, &mut T) {
    let ptr = v as *mut Vec<T>;
    (&mut (&mut (*ptr))[i], &mut (&mut (*ptr))[j])
}

/// # Panics
///
/// Will panic
pub fn symplectic_basis_with_transforms<T: PauliBinaryOps>(mut paulies: Vec<T>) -> (Vec<T>, BitMatrix, BitMatrix) {
    assert!(paulies.len() % 2 == 0);
    let mut res = BitMatrix::identity(paulies.len());
    let mut res_inv_t = BitMatrix::identity(paulies.len());
    let m = paulies.len() / 2;
    for k in 0..m {
        // find pauli that anti-commutes with
        let mut j = 2 * k + 1;
        while j < paulies.len() && commutes_with(&paulies[2 * k], &paulies[j]) {
            j += 1;
        }
        assert!(
            j < paulies.len(),
            "`paulies` cannot be transformed into symplectic basis"
        );

        paulies.swap(2 * k + 1, j);
        res.swap_rows(2 * k + 1, j);
        res_inv_t.swap_rows(2 * k + 1, j);

        for i in 2 * k + 2..2 * m {
            if anti_commutes_with(&paulies[i], &paulies[2 * k]) {
                unsafe {
                    let (p_i, p_2k_1) = get_pair_mut_unsafe(&mut paulies, i, 2 * k + 1);
                    p_i.mul_assign_right(p_2k_1);
                }
                res.add_into_row(2 * k + 1, j);
                res_inv_t.add_into_row(j, 2 * k + 1);
            }

            if anti_commutes_with(&paulies[i], &paulies[2 * k + 1]) {
                unsafe {
                    let (p_i, p_2k) = get_pair_mut_unsafe(&mut paulies, i, 2 * k);
                    p_i.mul_assign_right(p_2k);
                }
                res.add_into_row(2 * k, j);
                res_inv_t.add_into_row(j, 2 * k);
            }
        }
    }
    (paulies, res, res_inv_t)
}

pub fn bipartite_normal_form2<CliffordLike>(
    clifford: &CliffordLike,
    sorted_support: &[usize],
) -> (
    CliffordLike::NeutralElementType,
    CliffordLike::NeutralElementType,
    BitMatrix,
    BitMatrix,
)
where
    CliffordLike: Clifford + PreimageViews + NeutralElement,
    for<'a> MutableRow<'a>: BitwiseBinaryOps<<CliffordLike::PreImageView<'a> as Pauli>::Bits>,
{
    let num_qubits = clifford.num_qubits();
    let support_complement = complement(sorted_support, num_qubits);

    let right_clifford = <CliffordLike as NeutralElement>::neutral_element_of_size(sorted_support.len());
    let left_clifford = <CliffordLike as NeutralElement>::neutral_element_of_size(support_complement.len());
    let transform = BitMatrix::zeros(num_qubits, num_qubits);
    let transform_inv = BitMatrix::zeros(num_qubits, num_qubits);

    let transform_1 =
        support_restricted_z_images_from_support_complement::<CliffordLike>(clifford, &support_complement);
    let transform_2 = support_restricted_z_images_from_support_complement::<CliffordLike>(clifford, sorted_support);
    let (_restricting_transform, _restricting_transform_inv) =
        full_rank_row_completion_with_inv(&row_stacked([&transform_1, &transform_2]));

    (right_clifford, left_clifford, transform, transform_inv)
}

#[allow(clippy::similar_names)]
fn ensure_commutation(xs: &mut [DensePauli], zs: &mut [DensePauli], k: usize) {
    assert_eq!(xs.len(), zs.len());
    assert!(k <= xs.len());

    let (first_xs, second_xs) = xs.split_at_mut(k);
    let (_, second_zs) = zs.split_at_mut(k);
    for pauli in first_xs {
        for j in 0..second_xs.len() {
            if !commutes_with(pauli, &second_xs[j]) {
                *pauli *= &second_zs[j];
            }
            if !commutes_with(pauli, &second_zs[j]) {
                *pauli *= &second_xs[j];
            }
        }
    }
}

/// # Panics
///
/// Will panic
#[allow(clippy::similar_names)]
pub fn bipartite_normal_form(
    clifford: &CliffordUnitary,
    support_range: usize,
) -> (CliffordUnitary, CliffordUnitary, BitMatrix, BitMatrix) {
    let support: Vec<usize> = (0..support_range).collect();
    let num_qubits = clifford.num_qubits();
    let support_complement: Vec<usize> = (support_range..num_qubits).collect();
    let right_clifford = CliffordUnitary::identity(support.len());
    let left_clifford = CliffordUnitary::identity(support_complement.len());

    let transform_1 = support_restricted_z_images::<CliffordUnitary>(clifford, &support);
    let transform_2 = support_restricted_z_images::<CliffordUnitary>(clifford, &support_complement);
    let restricting_transform = full_rank_row_completion(row_stacked([&transform_1, &transform_2]));
    let restricting_transform_inv = restricting_transform.inverted();
    let clifford_with_restricted_images =
        clifford * &css_clifford(&restricting_transform_inv, &restricting_transform.transposed());

    let k1 = transform_1.rowcount();
    let k2 = transform_2.rowcount();

    check_restricted_clifford(&clifford_with_restricted_images, &support, &support_complement, k1, k2);

    let offset = k1 + k2;
    let num_remaining_images = num_qubits - offset;
    let mut remaining_z_images = Vec::<DensePauli>::with_capacity(num_remaining_images);
    for k in 0..num_remaining_images {
        remaining_z_images.push(image_z_restriction_up_to_sign(
            &clifford_with_restricted_images,
            k + offset,
            &support,
        ));
    }

    let (symplectic_basis, symplectic_transform, symplectic_transform_inv_t) =
        symplectic_basis_with_transforms(remaining_z_images);

    check_symplectic_basis_results(
        &symplectic_transform,
        &symplectic_transform_inv_t,
        num_remaining_images,
        &symplectic_basis,
    );

    let permuted_transform =
        directly_summed([&BitMatrix::identity(k1 + k2), &symplectic_transform]).dot(&restricting_transform);
    let row_permutation: Vec<usize> = (0..k1)
        .chain((offset..num_qubits).step_by(2))
        .chain(k1..offset)
        .chain((offset + 1..num_qubits).step_by(2))
        .collect();

    let mut tmp = permuted_transform.clone();
    tmp.permute_rows(&row_permutation);
    let transform = tmp.inverted();
    let transform_inv_t = tmp.transposed();
    let block_clifford = clifford * &css_clifford(&transform, &transform_inv_t);
    let k = (num_qubits - (k1 + k2)) / 2;

    check_block_clifford_properties(
        support_range,
        k,
        &block_clifford,
        &support,
        &support_complement,
        num_qubits,
    );

    let mut left_clifford_x_images = Vec::<DensePauli>::with_capacity(support_range);
    let mut left_clifford_z_images = Vec::<DensePauli>::with_capacity(support_range);

    let mut right_clifford_x_images = Vec::<DensePauli>::with_capacity(num_qubits - support_range);
    let mut right_clifford_z_images = Vec::<DensePauli>::with_capacity(num_qubits - support_range);

    for qubit_id in 0..k1 {
        let (before, _) = image_z_split(&block_clifford, qubit_id, support_range);
        right_clifford_z_images.push(before);
    }

    for qubit_id in 0..k1 {
        let (before, _) = image_x_split(&block_clifford, qubit_id, support_range);
        right_clifford_x_images.push(before);
    }

    for qubit_id in support_range..support_range + k2 {
        let (_, after) = image_z_split(&block_clifford, qubit_id, support_range);
        left_clifford_z_images.push(after);
    }

    for qubit_id in support_range..support_range + k2 {
        let (_, after) = image_x_split(&block_clifford, qubit_id, support_range);
        left_clifford_x_images.push(after);
    }

    assert_eq!(support_range - k1, k);
    for qubit_id in k1..support_range {
        let (before, after) = image_z_split(&block_clifford, qubit_id, support_range);
        right_clifford_z_images.push(before);
        left_clifford_z_images.push(after);
    }

    assert_eq!(num_qubits - (support_range + k2), k);
    for qubit_id in support_range + k2..num_qubits {
        let (before, after) = image_z_split(&block_clifford, qubit_id, support_range);
        right_clifford_x_images.push(before);
        left_clifford_x_images.push(after);
    }

    assert_eq!(right_clifford_x_images.len(), support_range);
    assert_eq!(right_clifford_z_images.len(), support_range);
    assert_eq!(left_clifford_x_images.len(), num_qubits - support_range);
    assert_eq!(left_clifford_z_images.len(), num_qubits - support_range);

    ensure_commutation(&mut right_clifford_x_images, &mut right_clifford_z_images, k1);
    ensure_commutation(&mut left_clifford_x_images, &mut left_clifford_z_images, k2);

    check_left_and_right_clifford_images(
        &right_clifford_x_images,
        &right_clifford_z_images,
        &left_clifford_x_images,
        &left_clifford_z_images,
    );

    (left_clifford, right_clifford, transform, transform_inv_t)
}

#[allow(clippy::similar_names)]
fn check_left_and_right_clifford_images(
    right_clifford_x_images: &[DensePauli],
    right_clifford_z_images: &[DensePauli],
    left_clifford_x_images: &[DensePauli],
    left_clifford_z_images: &[DensePauli],
) {
    fn interleaved<T>(a: &[T], b: &[T]) -> Vec<T>
    where
        T: Clone,
    {
        assert_eq!(a.len(), b.len());
        let mut res = Vec::<T>::with_capacity(a.len() + b.len());
        for k in 0..a.len() {
            res.push(a[k].clone());
            res.push(b[k].clone());
        }
        res
    }
    assert!(is_symplectic_basis(&interleaved(
        right_clifford_x_images,
        right_clifford_z_images
    )));
    assert!(is_symplectic_basis(&interleaved(
        left_clifford_x_images,
        left_clifford_z_images
    )));
}

fn check_restricted_clifford(
    clifford_with_restricted_images: &CliffordUnitary,
    support: &[usize],
    support_complement: &[usize],
    k1: usize,
    k2: usize,
) {
    // check that images of z operators of clifford_with_restricted_images supported on support and its complement
    assert!(z_images_are_restricted_to(
        support,
        clifford_with_restricted_images,
        0..k1
    ));

    assert!(z_images_are_restricted_to(
        support_complement,
        clifford_with_restricted_images,
        k1..k1 + k2
    ));
}

fn check_symplectic_basis_results(
    symplectic_transform: &BitMatrix,
    symplectic_transform_inv_t: &BitMatrix,
    num_remaining_images: usize,
    symplectic_basis: &[DensePauli],
) {
    assert_eq!(
        symplectic_transform.dot(&symplectic_transform_inv_t.transposed()),
        BitMatrix::identity(num_remaining_images)
    );
    assert!(is_symplectic_basis(symplectic_basis));
}

fn z_images_are_restricted_to(support: &[usize], clifford: &CliffordUnitary, mut indexes: Range<usize>) -> bool {
    indexes.all(|index| {
        is_subset(
            &Iterator::collect::<Vec<usize>>(clifford.image_z(index).support()),
            support,
        )
    })
}

fn check_block_clifford_properties(
    support_range: usize,
    k: usize,
    block_clifford: &CliffordUnitary,
    support: &[usize],
    support_complement: &[usize],
    num_qubits: usize,
) {
    // some of the images have a supported on `support` or `support_complement`
    assert!(z_images_are_restricted_to(
        support,
        block_clifford,
        0..support_range - k
    ));
    assert!(z_images_are_restricted_to(
        support_complement,
        block_clifford,
        support_range..num_qubits - k
    ));

    // some of images restrictions commute
    for qubit_id1 in support_range - k..support_range {
        for qubit_id2 in support_range - k..support_range {
            assert!(commutes_with(
                &image_z_restriction_up_to_sign(block_clifford, qubit_id1, support),
                &image_z_restriction_up_to_sign(block_clifford, qubit_id2, support)
            ));

            assert!(commutes_with(
                &image_z_restriction_up_to_sign(block_clifford, qubit_id1, support_complement),
                &image_z_restriction_up_to_sign(block_clifford, qubit_id2, support_complement)
            ));
        }
    }

    for qubit_id1 in num_qubits - k..num_qubits {
        for qubit_id2 in num_qubits - k..num_qubits {
            assert!(commutes_with(
                &image_z_restriction_up_to_sign(block_clifford, qubit_id1, support),
                &image_z_restriction_up_to_sign(block_clifford, qubit_id2, support)
            ));

            assert!(commutes_with(
                &image_z_restriction_up_to_sign(block_clifford, qubit_id1, support_complement),
                &image_z_restriction_up_to_sign(block_clifford, qubit_id2, support_complement)
            ));
        }
    }

    // some of the image restrictions form symplectic basis
    for qubit_id1 in 0..k {
        for qubit_id2 in 0..k {
            assert_eq!(
                commutes_with(
                    &image_z_restriction_up_to_sign(block_clifford, support_range - k + qubit_id1, support),
                    &image_z_restriction_up_to_sign(block_clifford, num_qubits - k + qubit_id2, support)
                ),
                qubit_id1 != qubit_id2
            );

            assert_eq!(
                commutes_with(
                    &image_z_restriction_up_to_sign(block_clifford, support_range - k + qubit_id1, support_complement),
                    &image_z_restriction_up_to_sign(block_clifford, num_qubits - k + qubit_id2, support_complement)
                ),
                qubit_id1 != qubit_id2
            );
        }
    }

    for qubit_id1 in 0..support_range - k {
        for qubit_id2 in 0..support_range - k {
            assert_eq!(
                commutes_with(
                    &image_z_restriction_up_to_sign(block_clifford, qubit_id1, support),
                    &image_x_restriction_up_to_sign(block_clifford, qubit_id2, support)
                ),
                qubit_id1 != qubit_id2
            );
        }
    }

    for qubit_id1 in support_range..num_qubits - k {
        for qubit_id2 in support_range..num_qubits - k {
            let z_image = image_z_restriction_up_to_sign(block_clifford, qubit_id1, support_complement);
            let x_image = image_x_restriction_up_to_sign(block_clifford, qubit_id1, support_complement);
            assert_eq!(commutes_with(&z_image, &x_image), qubit_id1 != qubit_id2);
        }
    }
}

/// # Panics
/// Panics if matrix does not have full row rank
fn full_rank_row_completion(mut matrix: BitMatrix) -> BitMatrix {
    let ncols = matrix.columncount();
    let nrows = matrix.rowcount();
    assert!(ncols >= nrows);
    let mut res = row_stacked([&matrix, &BitMatrix::zeros(ncols - matrix.rowcount(), ncols)]);
    let profile = matrix.echelonize();

    assert_eq!(profile.len(), nrows); // we only complete full row rank matrices

    let profile_complement = complement(&profile, ncols);
    for (i, j) in enumerate(profile_complement) {
        res.set((nrows + i, j), true);
    }
    res
}

fn image_z_split(clifford: &CliffordUnitary, qubit_id: usize, split_position: usize) -> (DensePauli, DensePauli) {
    split_at(split_position, &clifford.image_z(qubit_id))
}

fn image_x_split(clifford: &CliffordUnitary, qubit_id: usize, split_position: usize) -> (DensePauli, DensePauli) {
    split_at(split_position, &clifford.image_x(qubit_id))
}

fn split_at(index: usize, image: &DensePauli) -> (DensePauli, DensePauli) {
    let num_qubits = image.x_bits().len();
    assert!(index <= num_qubits);
    let rem = num_qubits - index;
    let mut before_x = BitVec::of_length(index);
    let mut before_z = BitVec::of_length(index);
    let mut after_x = BitVec::of_length(rem);
    let mut after_z = BitVec::of_length(rem);

    for support in image.x_bits().support() {
        if support < index {
            before_x.assign_index(support, true);
        } else {
            after_x.assign_index(support - index, true);
        }
    }
    for support in image.z_bits().support() {
        if support < index {
            before_z.assign_index(support, true);
        } else {
            after_z.assign_index(support - index, true);
        }
    }
    let mut before = PauliUnitary::from_bits(before_x, before_z, image.xz_phase_exponent());
    let mut after = PauliUnitary::from_bits(after_x, after_z, 0u8);
    if !before.is_order_two() {
        before *= Phase::from_exponent(1u8);
        after *= Phase::from_exponent(3u8);
    }
    (before, after)
}

pub fn image_z_restriction_up_to_sign(clifford: &CliffordUnitary, qubit_id: usize, support: &[usize]) -> DensePauli {
    let image = clifford.image_z(qubit_id);
    DensePauli::from_bits(
        BitVec::<WORD_COUNT_DEFAULT>::selected_from(&image.x_bits().as_view(), support),
        BitVec::<WORD_COUNT_DEFAULT>::selected_from(&image.z_bits().as_view(), support),
        0,
    )
}

pub fn image_x_restriction_up_to_sign(clifford: &CliffordUnitary, qubit_id: usize, support: &[usize]) -> DensePauli {
    let image = clifford.image_x(qubit_id);
    DensePauli::from_bits(
        BitVec::<WORD_COUNT_DEFAULT>::selected_from(&image.x_bits().as_view(), support),
        BitVec::<WORD_COUNT_DEFAULT>::selected_from(&image.z_bits().as_view(), support),
        0,
    )
}

/// # Panics
///
/// Will panic
#[must_use]
pub fn is_symplectic_basis(paulis: &[DensePauli]) -> bool {
    assert_eq!(paulis.len() % 2, 0);
    let m = paulis.len() / 2;
    for k in 0..2 * m {
        for j in k + 1..2 * m {
            let comm = commutes_with(&paulis[k], &paulis[j]);
            if k / 2 == j / 2 {
                if comm {
                    return false;
                }
            } else if !comm {
                return false;
            }
        }
    }
    true
}
