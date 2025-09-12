use itertools::{enumerate, Itertools};
use paulimer::bits::{BitMatrix, BitVec, Bitwise, IndexSet};
use paulimer::clifford::generic_algos::{clifford_from_images, clifford_to_prepare_bell_states};
use paulimer::clifford::{
    apply_qubit_clifford_by_axis, group_encoding_clifford_of, prepare_all_plus, prepare_all_zero,
    random_clifford_via_operations_sampling, split_clifford_encoder_mod_pauli, split_phased_css,
    split_qubit_cliffords_and_css, split_qubit_tensor_product_encoder, Clifford, CliffordMutable,
    CliffordStringParsingError, MutablePreImages, PreimageViews, Swap, XOrZ,
};
type CliffordUnitary = paulimer::clifford::CliffordUnitary;
type CliffordUnitaryModPauli = paulimer::clifford::CliffordUnitaryModPauli;

use paulimer::pauli::{
    anti_commutes_with, apply_pauli_exponent, apply_root_x, apply_root_y, apply_root_z,
    pauli_random, pauli_random_order_two, DensePauli, DensePauliProjective, PauliMutable,
    SparsePauliProjective,
};
use paulimer::pauli::{commutes_with, Pauli, PauliBinaryOps, PauliUnitary, Phase, SparsePauli};

use paulimer::operations::{css_operations, diagonal_operations};
use paulimer::quantum_core::{x, y, z, PositionedPauliObservable};
use proptest::prelude::*;
use rand::prelude::*;
use std::borrow::Borrow;
use std::ops::Range;
use std::str::FromStr;

pub trait TestableClifford:
    Clifford<
        DensePauli: for<'life, 'life1> PartialEq<&'life [PositionedPauliObservable]>
                        + PartialEq
                        + std::fmt::Display
                        + std::fmt::Debug,
    > + CliffordMutable<PhaseExponentValue = <Self as Clifford>::PhaseExponentValue>
    + PartialEq
    + Eq
    + PreimageViews<PhaseExponentValue = <Self as Clifford>::PhaseExponentValue>
    + MutablePreImages<PhaseExponentValue = <Self as Clifford>::PhaseExponentValue>
    + FromStr<Err = CliffordStringParsingError>
    + std::fmt::Display
    + std::fmt::Debug
{
    type SparsePauli: Pauli<PhaseExponentValue = <Self as Clifford>::PhaseExponentValue>
        + for<'life> From<&'life [PositionedPauliObservable]>
        + std::fmt::Display;
    type DensePauli: Pauli<PhaseExponentValue = <Self as Clifford>::PhaseExponentValue>
        + for<'life> PartialEq<&'life [PositionedPauliObservable]>
        + std::fmt::Display;
}

impl TestableClifford for CliffordUnitary {
    type SparsePauli = SparsePauli;
    type DensePauli = DensePauli;
}

impl TestableClifford for CliffordUnitaryModPauli {
    type SparsePauli = SparsePauliProjective;
    type DensePauli = DensePauliProjective;
}

fn is_pauli_x_up_to_phase(pauli: &impl Pauli, qubit_id: usize) -> bool {
    pauli.z_bits().is_zero() && pauli.x_bits().weight() == 1 && pauli.x_bits().index(qubit_id)
}

fn is_pauli_z_up_to_phase(pauli: &impl Pauli, qubit_id: usize) -> bool {
    pauli.x_bits().is_zero() && pauli.z_bits().weight() == 1 && pauli.z_bits().index(qubit_id)
}

fn are_identity_preimages_up_to_phase(clifford: &impl Clifford) -> bool {
    for j in 0..clifford.num_qubits() {
        if !is_pauli_x_up_to_phase(&clifford.preimage_x(j), j) {
            return false;
        }
        if !is_pauli_z_up_to_phase(&clifford.preimage_z(j), j) {
            return false;
        }
    }
    true
}

fn identity_preimages_with_dimension(dimension: usize) {
    let id = CliffordUnitary::identity(dimension);
    assert!(id.is_identity());
    let id_mod_pauli = CliffordUnitaryModPauli::identity(dimension);
    assert!(are_identity_preimages_up_to_phase(&id_mod_pauli));
}

#[test]
fn identity_preimages() {
    for dimension in 1..4 {
        identity_preimages_with_dimension(dimension);
    }
}

proptest! {
    #[test]
    fn from_images(clifford in arbitrary_clifford(0..1)) {
        let images = images_of(&clifford);
        let from_images : CliffordUnitary = clifford_from_images(images.as_slice().iter());
        assert_eq!(images, images_of(&from_images));
    }

    #[test]
    fn format_string_roundtrip(clifford in arbitrary_clifford(1..10)) {
        format_string_roundtrip_generic_test(&clifford);
        format_string_roundtrip_generic_test::<CliffordUnitaryModPauli>(&clifford.into());
    }

    #[test]
    fn clone(clifford in arbitrary_clifford(0..10)) {
        let cloned = clifford.clone();
        assert_eq!(images_of(&cloned), images_of(&clifford));
    }

    #[test]
    fn pauli_exponent_multiply(clifford in arbitrary_clifford(0..10)) {
        let pauli = pauli_random_order_two::<<paulimer::clifford::CliffordUnitary as Clifford>::DensePauli>(clifford.num_qubits(),&mut thread_rng());
        let mut product = clifford.clone();
        product.left_mul_pauli_exp(&pauli);
        let ipauli = pauli.clone() * Phase::from_exponent(1u8);

        let assert_image = |indicator: PauliUnitary<Vec<bool>, u8>| {
            let power_image = product.image(&indicator);
            let clifford_image = clifford.image(&indicator);
            // println!("indicator={}, pauli={}, power_image={}, clifford_image={}", indicator, pauli, power_image, clifford_image);
            if commutes_with(&clifford_image,&pauli) {
                assert_eq!(clifford_image, power_image);
            } else {
                assert_eq!(ipauli.clone() * &clifford_image, power_image);
            }
        };

        for index in 0..clifford.num_qubits() {
            assert_image(x_at(index, clifford.num_qubits()));
            assert_image(z_at(index, clifford.num_qubits()));
        }
    }

    #[test]
    fn controlled_pauli_multiply(clifford in arbitrary_clifford(2..3)) {

        assert!(clifford.num_qubits() >= 2);
        let mut control : <paulimer::clifford::CliffordUnitary as Clifford>::DensePauli = pauli_random_order_two(clifford.num_qubits(),&mut thread_rng());
        while control.x_bits().is_zero() && control.z_bits().is_zero() {
            control = pauli_random_order_two(clifford.num_qubits(),&mut thread_rng());
        }
        let mut target : <paulimer::clifford::CliffordUnitary as Clifford>::DensePauli = pauli_random_order_two(clifford.num_qubits(),&mut thread_rng());
        while !commutes_with(&control,&target) || (control.x_bits().is_zero() && control.z_bits().is_zero()) {
            target = pauli_random_order_two(clifford.num_qubits(),&mut thread_rng());
        }

        let mut product = clifford.clone();
        assert!(control.is_order_two());
        assert!(target.is_order_two());
        product.left_mul_controlled_pauli(&control,&target);

        let images = images_of(&clifford);
        let product_images = images_of(&product);
        for (image, product_image) in images.iter().zip(product_images.iter()) {
            let mut expected_image = image.clone();
            if anti_commutes_with(image,&control) {
                expected_image.mul_assign_right(&target);
            }
            if anti_commutes_with(image, &target) {
                expected_image.mul_assign_left(&control);
            }
            // println!("Q={}, P1={}, P2={}, expected={}, actual={}", image, control, target, expected_image, *product_image);
            assert_eq!(expected_image, *product_image);
        }
    }

    #[test]
    fn pauli_multiply(clifford in arbitrary_clifford(0..1)) {
        let pauli = arbitrary_pauli_of_length(clifford.num_qubits());
        let product = &pauli * clifford.clone();

        let assert_image_sign = |indicator: PauliUnitary<Vec<bool>, u8>| {
            let product_image = product.image(&indicator);
            let clifford_image = clifford.image(&indicator);
            // println!("indicator={}, pauli={}, product_image={}, clifford_image={}", indicator, pauli, product_image, clifford_image);
            if commutes_with(&clifford_image, &pauli) {
                assert_eq!(clifford_image, product_image);
            } else {
                assert_eq!(-clifford_image, product_image);
            }
        };

        for index in 0..clifford.num_qubits() {
            assert_image_sign(x_at(index, clifford.num_qubits()));
            assert_image_sign(z_at(index, clifford.num_qubits()));
        }
    }

    #[test]
    fn composition((left, right) in composable_cliffords(0..10)) {
        let composed = left.multiply_with(&right);
        for index in 0..left.num_qubits() {
            let x = x_at(index, left.num_qubits());
            let z = z_at(index, left.num_qubits());
            assert_eq!(composed.preimage(&x), right.preimage(&left.preimage(&x)));
            assert_eq!(composed.preimage(&z), right.preimage(&left.preimage(&z)));
        }
    }

    #[test]
    fn right_swap(clifford in arbitrary_clifford(2..10), mut index0 in 0..10usize, mut index1 in 0..10usize) {
        index0 %= clifford.num_qubits();
        index1 %= clifford.num_qubits();
        let swapped = clifford.clone() * Swap(index0, index1);
        let clifford_images = images_of(&clifford);
        let swapped_images = images_of(&swapped);
        assert_eq!(swapped_images[2*index0], clifford_images[2*index1]);
        assert_eq!(swapped_images[2*index0+1], clifford_images[2*index1+1]);
        assert_eq!(swapped_images[2*index1], clifford_images[2*index0]);
        assert_eq!(swapped_images[2*index1+1], clifford_images[2*index0+1]);
        for index in 0..clifford.num_qubits() {
            if index != index0 && index != index1 {
                assert_eq!(swapped_images[2*index], clifford_images[2*index]);
                assert_eq!(swapped_images[2*index+1], clifford_images[2*index+1]);
            }
        }
    }

    #[test]
    fn left_swap(clifford in arbitrary_clifford(2..3), mut index0 in 0..10usize, mut index1 in 0..10usize) {
        index0 %= clifford.num_qubits();
        index1 %= clifford.num_qubits();
        let mut swapped = clifford.clone();
        swapped.left_mul_swap(index0, index1);
        let clifford_images = preimages_of(&clifford);
        let swapped_images = preimages_of(&swapped);
        assert_eq!(swapped_images[2*index0], clifford_images[2*index1]);
        assert_eq!(swapped_images[2*index0+1], clifford_images[2*index1+1]);
        assert_eq!(swapped_images[2*index1], clifford_images[2*index0]);
        assert_eq!(swapped_images[2*index1+1], clifford_images[2*index0+1]);
        for index in 0..clifford.num_qubits() {
            if index != index0 && index != index1 {
                assert_eq!(swapped_images[2*index], clifford_images[2*index]);
                assert_eq!(swapped_images[2*index+1], clifford_images[2*index+1]);
            }
        }
    }

    #[test]
    fn preimage_inverts_image(clifford in arbitrary_clifford(0..10)) {
        let clifford = CliffordUnitary::identity(clifford.num_qubits());
        for index in 0..clifford.num_qubits() {
            let x = &x_at(index, clifford.num_qubits());
            let z = &z_at(index, clifford.num_qubits());
            let x_image = clifford.image(x);
            let z_image = clifford.image(z);
            let x_image_preimage = clifford.preimage(&x_image);
            let z_image_preimage = clifford.preimage(&z_image);
            assert!( x == x_image_preimage);
            assert!( z == z_image_preimage);
        }
    }

    #[test]
    fn split_clifford(clifford1 in arbitrary_clifford(1..5), clifford2 in arbitrary_clifford(1..5)) {
        let c1 = CliffordUnitaryModPauli::from(clifford1);
        let c2 = CliffordUnitaryModPauli::from(clifford2);
        let qubit_count = c1.num_qubits() + c2.num_qubits();
        let mut c3 = random_diagonal_clifford::<CliffordUnitaryModPauli>(qubit_count).multiply_with(&random_css_clifford(qubit_count));

        let support = c1.qubits().collect_vec();
        let support_complement = (c1.num_qubits()..c3.num_qubits()).collect_vec();

        c3.left_mul_clifford(&c1, &support);
        c3.left_mul_clifford(&c2, &support_complement);

        if let Some((split_clifford1, split_clifford2)) = split_clifford_encoder_mod_pauli(&c3, &support, &support_complement) {
            assert!(split_clifford1.is_valid());
            assert!(split_clifford2.is_valid());

            assert_eq!(split_clifford1.num_qubits(), c1.num_qubits());
            assert_eq!(split_clifford2.num_qubits(), c2.num_qubits());

            for qubit_index in split_clifford1.qubits() {
                assert!(c3.preimage(&split_clifford1.image_z(qubit_index)).x_bits().is_zero());
            }

             // tensor product of split_clifford1, split_clifford2 encode the same state as c4
            let mut c4 = CliffordUnitaryModPauli::identity(qubit_count);
            c4.left_mul_clifford(&split_clifford1, &support);
            c4.left_mul_clifford(&split_clifford2, &support_complement);
            for qubit_index in c4.qubits() {
                assert!(c3.preimage(&c4.image_z(qubit_index)).x_bits().is_zero());
            }

        }
        else {
            panic!("Clifford should split")
        }
    }

    #[test]
    fn identity_dimension(dimension in 0..1000usize) {
        let identity = CliffordUnitary::identity(dimension);
        assert_eq!(dimension, identity.num_qubits());
    }

    // #[test]
    // fn identity_images(dimension in 0..100usize) {
    //     let identity = CliffordUnitary::<WORD_COUNT_DEFAULT>::identity(dimension);
    //     let mut identity_images = vec![];
    //     for index in 0..dimension {
    //         identity_images.push(x_at(index, dimension));
    //         identity_images.push(z_at(index, dimension));
    //     }
    //     assert_eq!(CliffordUnitary::from_images(identity_images), identity);
    // }

    #[test]
    fn identity_multiplication_is_trivial(clifford in arbitrary_clifford(0..10)) {
        let identity = CliffordUnitary::identity(clifford.num_qubits());
        assert_eq!(clifford, clifford.multiply_with(&identity));
        assert_eq!(clifford, identity.multiply_with(&clifford));
    }

    #[test]
    fn group_encoding_clifford_of_test(clifford in arbitrary_clifford(1..20), qubit_count in 1usize..20  ) {
        let num_images = qubit_count.min(clifford.num_qubits());
        let images = (0 .. num_images).map(|id| clifford.image_z(id) ).collect_vec();
        let encoding_clifford = group_encoding_clifford_of(&images,clifford.num_qubits());
        for image in images {
            let preimage = encoding_clifford.preimage(&image);
            assert!(preimage.x_bits().is_zero());
            assert!(preimage.z_bits().max_bit_id().unwrap() < num_images);
            assert_eq!(preimage.xz_phase_exponent(),0);
        }
    }



    #[test]
    fn left_mul_root_and_apply_root_are_consistent(qubit_count in 1..10usize) {
        check_left_mul_root_and_apply_root_are_consistent(qubit_count, <CliffordUnitary as CliffordMutable>::left_mul_root_x, apply_root_x::<DensePauli>);
        check_left_mul_root_and_apply_root_are_consistent(qubit_count, <CliffordUnitary as CliffordMutable>::left_mul_root_y, apply_root_y::<DensePauli>);
        check_left_mul_root_and_apply_root_are_consistent(qubit_count, <CliffordUnitary as CliffordMutable>::left_mul_root_z, apply_root_z::<DensePauli>);
    }

    #[test]
    fn left_mul_pauli_exp_and_apply_pauli_exp_are_consistent(clifford in arbitrary_clifford(1..20)) {
        let identity = CliffordUnitary::identity(clifford.num_qubits());
        let mut pauli_exp = CliffordUnitary::identity(clifford.num_qubits());
        let exp = clifford.image_z(0);
        pauli_exp.left_mul_pauli_exp(&exp);
        for qubit_index in 0 .. clifford.num_qubits() {
            let mut image_z = identity.image_z(qubit_index);
            apply_pauli_exponent(&mut image_z, &exp);
            assert_eq!(image_z, pauli_exp.image_z(qubit_index));

            let mut image_x = identity.image_x(qubit_index);
            apply_pauli_exponent(&mut image_x, &exp);
            assert_eq!(image_x, pauli_exp.image_x(qubit_index));
        }
    }

    #[test]
    fn inverse(clifford in arbitrary_clifford(1..2)) {
        let inverse = clifford.inverse();
        let identity = CliffordUnitary::identity(clifford.num_qubits());
        assert_eq!(identity, clifford.multiply_with(&inverse));
    }

    #[test]
    fn is_diagonal(clifford in arbitrary_diagonal_clifford(1..15usize)) {
        generic_diagonal_clifford_test::<CliffordUnitaryModPauli>(&clifford.clone().into());
        generic_diagonal_clifford_test::<CliffordUnitary>(&clifford);

    }

    #[test]
    fn diagonal_resource_state_encoder_test(qubit_count in 1..15usize) {
        assert!(prepare_all_plus(qubit_count).is_diagonal_resource_encoder(XOrZ::Z));
        assert!(prepare_all_zero(qubit_count).is_diagonal_resource_encoder(XOrZ::X));
        assert!(prepare_all_plus(qubit_count).unitary_from_diagonal_resource_state(XOrZ::Z).unwrap().is_identity());
        assert!(prepare_all_zero(qubit_count).unitary_from_diagonal_resource_state(XOrZ::X).unwrap().is_identity());
    }

    #[test]
    fn is_css(clifford in arbitrary_css_clifford(2..10usize)) {
        generic_is_css_clifford_test::<CliffordUnitary>(&clifford);
        let clifford_mod_pauli: CliffordUnitaryModPauli = clifford.into();
        generic_is_css_clifford_test::<CliffordUnitaryModPauli>(&clifford_mod_pauli);
        let qubit_count = clifford_mod_pauli.num_qubits();
        assert!(prepare_all_plus(qubit_count).is_diagonal_resource_encoder(XOrZ::Z));
        assert!(clifford_mod_pauli.multiply_with(&prepare_all_plus(qubit_count)).is_diagonal_resource_encoder(XOrZ::Z));
        assert!(clifford_mod_pauli.multiply_with(&prepare_all_zero(qubit_count)).is_diagonal_resource_encoder(XOrZ::X));
    }

    #[test]
    fn is_phased_css_test( (css,diagonal) in composable_css_diagonal_cliffords(2..10usize)) {
        let c1 : CliffordUnitaryModPauli = css.multiply_with(&diagonal).into();
        let c2 : CliffordUnitaryModPauli = diagonal.multiply_with(&css).into();
        assert!(split_phased_css(&css.clone().into()).is_some());
        assert!(split_phased_css(&diagonal.clone().into()).is_some());
        if let Some((diag,extracted_css)) = split_phased_css(&c2) {
            assert_eq!(CliffordUnitaryModPauli::from(diagonal),diag);
            assert_eq!(CliffordUnitaryModPauli::from(css),extracted_css);
        }
        assert!(split_phased_css(&c1).is_some());
    }

    #[test]
    fn is_qubit_css_test( (css,qubit) in composable_css_qubit_cliffords(2..10usize)) {
        let c2 : CliffordUnitaryModPauli = qubit.multiply_with(&css).into();
        assert!(split_qubit_cliffords_and_css(&css.clone().into()).is_some());
        assert!(split_qubit_cliffords_and_css(&qubit.clone().into()).is_some());
        if let Some((extracted_qubit,extracted_css)) = split_qubit_cliffords_and_css(&c2) {
            assert_eq!(CliffordUnitaryModPauli::from(qubit),extracted_qubit);
            assert_eq!(CliffordUnitaryModPauli::from(css),extracted_css);
        }
    }

    #[test]
    fn qubit_cliffords_recognition_test( qubit_cliffords in arbitrary_qubit_cliffords(1..10usize)) {
        let qubit_count = qubit_cliffords.num_qubits();
        let c : CliffordUnitaryModPauli = qubit_cliffords.into();
        let mut r = CliffordUnitaryModPauli::identity(qubit_count);
        let plus = prepare_all_plus(qubit_count);
        let zero = prepare_all_zero(qubit_count);
        let plus_axes = split_qubit_tensor_product_encoder(&c.multiply_with(&plus)).unwrap();
        let zero_axes = split_qubit_tensor_product_encoder(&c.multiply_with(&zero)).unwrap();
        for (qubit_index,(zero,plus)) in enumerate(std::iter::zip(zero_axes,plus_axes)) {
            apply_qubit_clifford_by_axis(&mut r, qubit_index, zero, plus);
        }
        assert_eq!(c,r);
    }

}

prop_compose! {
   fn arbitrary_clifford(dimension_range: Range<usize>)(dimension in dimension_range) -> CliffordUnitary {
        arbitrary_clifford_of_dimension(dimension)
   }
}

prop_compose! {
    fn arbitrary_css_clifford(dimension_range: Range<usize>)(dimension in dimension_range) -> CliffordUnitary {
        let mut clifford: CliffordUnitary = random_css_clifford(dimension);
        let pauli = pauli_random_order_two::<<paulimer::clifford::CliffordUnitary as Clifford>::DensePauli>(clifford.num_qubits(),&mut thread_rng());
        clifford.left_mul_pauli(&pauli);
        clifford
    }
}

prop_compose! {
    fn arbitrary_diagonal_clifford(dimension_range: Range<usize>)(dimension in dimension_range) -> CliffordUnitary {
        let mut clifford: CliffordUnitary= random_diagonal_clifford(dimension);
        let pauli = pauli_random_order_two::<<paulimer::clifford::CliffordUnitary as Clifford>::DensePauli>(clifford.num_qubits(),&mut thread_rng());
        clifford.left_mul_pauli(&pauli);
        clifford
    }
}

prop_compose! {
   fn composable_cliffords(dimension_range: Range<usize>)(dimension in dimension_range) -> (CliffordUnitary, CliffordUnitary) {
        (arbitrary_clifford_of_dimension(dimension), arbitrary_clifford_of_dimension(dimension))
   }
}

prop_compose! {
   fn composable_css_diagonal_cliffords(dimension_range: Range<usize>)(dimension in dimension_range) -> (CliffordUnitary, CliffordUnitary) {
        (arbitrary_css_clifford_of_dimension(dimension), arbitrary_diagonal_clifford_of_dimension(dimension))
   }
}

prop_compose! {
   fn composable_css_qubit_cliffords(dimension_range: Range<usize>)(dimension in dimension_range) -> (CliffordUnitary, CliffordUnitary) {
        (arbitrary_css_clifford_of_dimension(dimension), arbitrary_qubit_cliffords_of_dimension(dimension))
   }
}

prop_compose! {
    fn arbitrary_qubit_cliffords(dimension_range: Range<usize>)(dimension in dimension_range) -> CliffordUnitary {
        arbitrary_qubit_cliffords_of_dimension(dimension)
    }
}

prop_compose! {
   fn arbitrary_images(max_dimension: usize)(dimension in 0..=max_dimension) -> Vec<PauliUnitary<BitVec, u8>> {
        let images: Vec<PauliUnitary<BitVec, u8>> = std::iter::from_fn(|| Some(arbitrary_pauli_of_length(dimension))).take(dimension*2).collect();
        images
   }
}

prop_compose! {
   fn arbitrary_pauli(max_dimension: usize)(dimension in 0..=max_dimension) -> PauliUnitary<BitVec, u8> {
        arbitrary_pauli_of_length(dimension)
   }
}

fn arbitrary_clifford_of_dimension(dimension: usize) -> CliffordUnitary {
    CliffordUnitary::random(dimension, &mut thread_rng())
}

fn arbitrary_css_clifford_of_dimension(dimension: usize) -> CliffordUnitary {
    let mut clifford: CliffordUnitary = random_css_clifford(dimension);
    let pauli = pauli_random_order_two::<
        <paulimer::clifford::CliffordUnitary as Clifford>::DensePauli,
    >(clifford.num_qubits(), &mut thread_rng());
    clifford.left_mul_pauli(&pauli);
    clifford
}

fn arbitrary_qubit_cliffords_of_dimension(dimension: usize) -> CliffordUnitary {
    let mut clifford: CliffordUnitary = CliffordUnitary::identity(dimension);
    for qubit_index in clifford.qubits() {
        let qubit_random_clifford = arbitrary_clifford_of_dimension(1);
        clifford.left_mul_clifford(&qubit_random_clifford, &[qubit_index]);
    }
    clifford
}

fn arbitrary_diagonal_clifford_of_dimension(dimension: usize) -> CliffordUnitary {
    let mut clifford: CliffordUnitary = random_diagonal_clifford(dimension);
    let pauli = pauli_random_order_two::<
        <paulimer::clifford::CliffordUnitary as Clifford>::DensePauli,
    >(clifford.num_qubits(), &mut thread_rng());
    clifford.left_mul_pauli(&pauli);
    clifford
}

fn arbitrary_pauli_of_length(length: usize) -> PauliUnitary<BitVec, u8> {
    pauli_random(length, &mut thread_rng())
}

fn images_of<CliffordLike: Clifford>(clifford: &CliffordLike) -> Vec<CliffordLike::DensePauli> {
    let mut images = vec![];
    for qubit_index in clifford.qubits() {
        images.push(clifford.image_x(qubit_index));
        images.push(clifford.image_z(qubit_index));
    }
    images
}

fn preimages_of<CliffordLike: Clifford>(clifford: &CliffordLike) -> Vec<CliffordLike::DensePauli> {
    let mut preimages = vec![];
    for qubit_index in clifford.qubits() {
        preimages.push(clifford.preimage_x(qubit_index));
        preimages.push(clifford.preimage_z(qubit_index));
    }
    preimages
}

fn x_at(index: usize, length: usize) -> PauliUnitary<Vec<bool>, u8> {
    let zeros = vec![false; length];
    let mut bits = zeros.clone();
    bits[index] = true;
    PauliUnitary::from_bits(bits, zeros, 0u8)
}

fn z_at(index: usize, length: usize) -> PauliUnitary<Vec<bool>, u8> {
    let zeros = vec![false; length];
    let mut bits = zeros.clone();
    bits[index] = true;
    PauliUnitary::from_bits(zeros, bits, 0u8)
}

/// One and two-qubit Clifford gates tests
macro_rules! generic_qubit_unitary_test_macro {
    ($func:ident, $image_func:expr) => {
        for num_qubits in 0..6 {
            for qubit_index in 0..num_qubits {
                generic_qubit_unitary_test(
                    num_qubits,
                    qubit_index,
                    CliffordUnitary::$func,
                    $image_func,
                );
                generic_qubit_unitary_test(
                    num_qubits,
                    qubit_index,
                    CliffordUnitaryModPauli::$func,
                    $image_func,
                );
            }
        }
    };
}

macro_rules! generic_two_qubit_unitary_test_macro {
    ($func:ident, $image_func:expr) => {
        for num_qubits in 0..6 {
            for qubit_index1 in 0..num_qubits {
                for qubit_index2 in 0..num_qubits {
                    if qubit_index1 != qubit_index2 {
                        generic_two_qubit_unitary_test(
                            num_qubits,
                            qubit_index1,
                            qubit_index2,
                            CliffordUnitary::$func,
                            $image_func,
                        );
                        generic_two_qubit_unitary_test(
                            num_qubits,
                            qubit_index1,
                            qubit_index2,
                            CliffordUnitaryModPauli::$func,
                            $image_func,
                        );
                    }
                }
            }
        }
    };
}

#[test]
fn hadamard_test() {
    generic_qubit_unitary_test_macro!(left_mul_hadamard, h_images);
}

#[test]
fn root_x_test() {
    generic_qubit_unitary_test_macro!(left_mul_root_x, root_x_images);
    generic_qubit_unitary_test_macro!(left_mul_root_x_inverse, root_x_inv_images);
}

#[test]
fn root_z_test() {
    generic_qubit_unitary_test_macro!(left_mul_root_z, root_z_images);
    generic_qubit_unitary_test_macro!(left_mul_root_z_inverse, root_z_inv_images);
}

#[test]
fn root_y_test() {
    generic_qubit_unitary_test_macro!(left_mul_root_y, root_y_images);
    generic_qubit_unitary_test_macro!(left_mul_root_y_inverse, root_y_inv_images);
}

#[test]
fn xyz_test() {
    generic_qubit_unitary_test_macro!(left_mul_x, x_images);
    generic_qubit_unitary_test_macro!(left_mul_y, y_images);
    generic_qubit_unitary_test_macro!(left_mul_z, z_images);
}

#[test]
fn cx_test() {
    generic_two_qubit_unitary_test_macro!(left_mul_cx, cx_images);
}

#[test]
fn cz_test() {
    generic_two_qubit_unitary_test_macro!(left_mul_cz, cz_images);
}

#[test]
fn swap_test() {
    generic_two_qubit_unitary_test_macro!(left_mul_swap, swap_images);
}

#[test]
fn prepare_bell_test() {
    generic_two_qubit_unitary_test_macro!(left_mul_prepare_bell, prepare_bell_images);
}

type ImageTable = Vec<(
    Vec<PositionedPauliObservable>,
    Vec<PositionedPauliObservable>,
)>;

fn cx_images(c: usize, t: usize) -> ImageTable {
    vec![
        (vec![x(c)], vec![x(c), x(t)]),
        (vec![z(c)], vec![z(c)]),
        (vec![x(t)], vec![x(t)]),
        (vec![z(t)], vec![z(c), z(t)]),
        (vec![y(t)], vec![z(c), y(t)]),
        (vec![y(c)], vec![y(c), x(t)]),
    ]
}

fn cz_images(c: usize, t: usize) -> ImageTable {
    vec![
        (vec![x(c)], vec![x(c), z(t)]),
        (vec![z(c)], vec![z(c)]),
        (vec![x(t)], vec![z(c), x(t)]),
        (vec![z(t)], vec![z(t)]),
        (vec![y(t)], vec![z(c), y(t)]),
        (vec![y(c)], vec![y(c), z(t)]),
    ]
}

fn swap_images(q1: usize, q2: usize) -> ImageTable {
    vec![
        (vec![x(q1)], vec![x(q2)]),
        (vec![z(q1)], vec![z(q2)]),
        (vec![y(q1)], vec![y(q2)]),
        (vec![-x(q1), z(q2)], vec![-x(q2), z(q1)]),
    ]
}

fn prepare_bell_images(q1: usize, q2: usize) -> ImageTable {
    vec![
        (vec![z(q1)], vec![x(q1), x(q2)]),
        (vec![x(q1)], vec![z(q1)]),
        (vec![z(q2)], vec![z(q1), z(q2)]),
        (vec![x(q2)], vec![x(q2)]),
        (vec![z(q1), z(q2)], vec![-y(q1), y(q2)]),
    ]
}

fn x_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![-z(q)]),
        (vec![x(q)], vec![x(q)]),
        (vec![y(q)], vec![-y(q)]),
    ]
}

fn y_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![-z(q)]),
        (vec![x(q)], vec![-x(q)]),
        (vec![y(q)], vec![y(q)]),
    ]
}

fn z_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![z(q)]),
        (vec![x(q)], vec![-x(q)]),
        (vec![y(q)], vec![-y(q)]),
    ]
}

fn h_images(q: usize) -> ImageTable {
    vec![
        (vec![x(q)], vec![z(q)]),
        (vec![z(q)], vec![x(q)]),
        (vec![y(q)], vec![-y(q)]),
    ]
}

fn root_z_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![z(q)]),
        (vec![x(q)], vec![y(q)]),
        (vec![y(q)], vec![-x(q)]),
    ]
}

fn root_z_inv_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![z(q)]),
        (vec![x(q)], vec![-y(q)]),
        (vec![y(q)], vec![x(q)]),
    ]
}

fn root_x_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![-y(q)]),
        (vec![x(q)], vec![x(q)]),
        (vec![y(q)], vec![z(q)]),
    ]
}

fn root_x_inv_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![y(q)]),
        (vec![x(q)], vec![x(q)]),
        (vec![y(q)], vec![-z(q)]),
    ]
}

fn root_y_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![x(q)]),
        (vec![x(q)], vec![-z(q)]),
        (vec![y(q)], vec![y(q)]),
    ]
}

fn root_y_inv_images(q: usize) -> ImageTable {
    vec![
        (vec![z(q)], vec![-x(q)]),
        (vec![x(q)], vec![z(q)]),
        (vec![y(q)], vec![y(q)]),
    ]
}

fn check_images<CliffordLike: TestableClifford>(c: &CliffordLike, image_table: &ImageTable) {
    let sparse = sparse::<CliffordLike>;
    for (p, im_p) in image_table {
        assert!(c.image(&sparse(p)) == im_p.as_slice());
        assert!(c.preimage(&sparse(im_p)) == p.as_slice());
    }
}

fn generic_qubit_unitary_test<CliffordLike: TestableClifford>(
    num_qubits: usize,
    qubit_index: usize,
    apply_transformation: impl FnOnce(&mut CliffordLike, usize),
    images: impl Fn(usize) -> ImageTable,
) {
    let mut c = CliffordLike::identity(num_qubits);
    apply_transformation(&mut c, qubit_index);
    assert!(c.is_valid());
    check_images(&c, &images(qubit_index));
    for j in c.qubits() {
        if j != qubit_index {
            assert_identity_on(&c, j);
        }
    }
}

fn generic_two_qubit_unitary_test<CliffordLike: TestableClifford>(
    num_qubits: usize,
    qubit_index1: usize,
    qubit_index2: usize,
    apply_transformation: impl FnOnce(&mut CliffordLike, usize, usize),
    images: impl Fn(usize, usize) -> ImageTable,
) {
    let mut c = CliffordLike::identity(num_qubits);
    apply_transformation(&mut c, qubit_index1, qubit_index2);
    assert!(c.is_valid());
    check_images(&c, &images(qubit_index1, qubit_index2));
    for j in c.qubits() {
        if j != qubit_index1 && j != qubit_index2 {
            assert_identity_on(&c, j);
        }
    }
}

fn assert_identity_on(c: &impl Clifford, qubit_index: usize) {
    assert!(c.image_x(qubit_index).is_pauli_x(qubit_index));
    assert!(c.image_z(qubit_index).is_pauli_z(qubit_index));
    assert!(c.preimage_x(qubit_index).is_pauli_x(qubit_index));
    assert!(c.preimage_z(qubit_index).is_pauli_z(qubit_index));
}

fn generic_prepare_bell_states_test<CliffordLike: TestableClifford>() {
    let c = clifford_to_prepare_bell_states::<CliffordLike>(1);
    assert!(c.is_valid());
    assert!(c.image_z(0) == [x(0), x(1)].borrow());
    assert!(c.image_z(1) == [z(0), z(1)].borrow());
}

#[test]
fn clifford_to_prepare_bell_states_test() {
    generic_prepare_bell_states_test::<CliffordUnitaryModPauli>();
    generic_prepare_bell_states_test::<CliffordUnitary>();
}

fn is_identity_by_images(clifford: &impl Clifford) -> bool {
    for qubit_id in 0..clifford.num_qubits() {
        if !clifford.image_z(qubit_id).is_pauli_z(qubit_id) {
            return false;
        }
        if !clifford.image_x(qubit_id).is_pauli_x(qubit_id) {
            return false;
        }
    }
    true
}

fn generic_clifford_identity_test<CliffordLike: Clifford>(max_qubits: usize) {
    for j in 0..max_qubits {
        let id = CliffordLike::identity(j);
        assert!(id.is_valid());
        assert!(id.is_identity());
        assert!(is_identity_by_images(&id));
    }
}

#[test]
fn clifford_identity_test() {
    let max_qubits = 10usize;
    generic_clifford_identity_test::<CliffordUnitaryModPauli>(max_qubits);
    generic_clifford_identity_test::<CliffordUnitary>(max_qubits);
}

fn two_qubit_clifford<CliffordLike: TestableClifford>(
    transformation: impl FnOnce(&mut CliffordLike, usize, usize),
) -> CliffordLike {
    let mut res = CliffordLike::identity(2);
    transformation(&mut res, 0, 1);
    res
}

fn one_qubit_clifford<CliffordLike: TestableClifford>(
    transformation: impl FnOnce(&mut CliffordLike, usize),
) -> CliffordLike {
    let mut res = CliffordLike::identity(1);
    transformation(&mut res, 0);
    res
}

fn clifford_examples<CliffordLike: TestableClifford>() -> Vec<CliffordLike> {
    vec![
        one_qubit_clifford(CliffordLike::left_mul_x),
        one_qubit_clifford(CliffordLike::left_mul_y),
        one_qubit_clifford(CliffordLike::left_mul_z),
        one_qubit_clifford(CliffordLike::left_mul_hadamard),
        one_qubit_clifford(CliffordLike::left_mul_root_x),
        one_qubit_clifford(CliffordLike::left_mul_root_y),
        one_qubit_clifford(CliffordLike::left_mul_root_z),
        one_qubit_clifford(CliffordLike::left_mul_root_x_inverse),
        one_qubit_clifford(CliffordLike::left_mul_root_y_inverse),
        one_qubit_clifford(CliffordLike::left_mul_root_z_inverse),
        two_qubit_clifford(CliffordLike::left_mul_cx),
        two_qubit_clifford(CliffordLike::left_mul_cz),
        two_qubit_clifford(CliffordLike::left_mul_swap),
        two_qubit_clifford(CliffordLike::left_mul_prepare_bell),
    ]
}

fn clifford_order2_examples<CliffordLike: TestableClifford>() -> Vec<CliffordLike> {
    vec![
        one_qubit_clifford(CliffordLike::left_mul_x),
        one_qubit_clifford(CliffordLike::left_mul_y),
        one_qubit_clifford(CliffordLike::left_mul_z),
        one_qubit_clifford(CliffordLike::left_mul_hadamard),
        two_qubit_clifford(CliffordLike::left_mul_cx),
        two_qubit_clifford(CliffordLike::left_mul_cz),
        two_qubit_clifford(CliffordLike::left_mul_swap),
    ]
}

fn assert_images_consistent<CliffordLike: TestableClifford>(clifford: &CliffordLike) {
    let sparse = sparse::<CliffordLike>;
    for qubit_index in clifford.qubits() {
        let im_x = clifford.image_x(qubit_index);
        let im_z = clifford.image_z(qubit_index);
        assert!(clifford.image_x_bits(&IndexSet::singleton(qubit_index)) == im_x);
        assert!(clifford.image_z_bits(&IndexSet::singleton(qubit_index)) == im_z);
        assert!(clifford.image(&sparse(&[x(qubit_index)])) == im_x);
        assert!(clifford.image(&sparse(&[z(qubit_index)])) == im_z);
    }
}

fn assert_preimages_consistent<CliffordLike: TestableClifford>(clifford: &CliffordLike) {
    let sparse = sparse::<CliffordLike>;
    for qubit_index in clifford.qubits() {
        let pre_im_x = clifford.preimage_x(qubit_index);
        let pre_im_z = clifford.preimage_z(qubit_index);
        assert!(clifford.preimage_x_bits(&IndexSet::singleton(qubit_index)) == pre_im_x);
        assert!(clifford.preimage_z_bits(&IndexSet::singleton(qubit_index)) == pre_im_z);
        assert!(clifford.preimage(&sparse(&[x(qubit_index)])) == pre_im_x);
        assert!(clifford.preimage(&sparse(&[z(qubit_index)])) == pre_im_z);
    }
}

fn assert_inverse_and_multiply_are_consistent<CliffordLike: TestableClifford>(
    clifford: &CliffordLike,
) {
    let inv = clifford.inverse();
    assert!(inv.multiply_with(clifford).is_identity());
    assert!(clifford.multiply_with(&inv).is_identity());
}

fn generic_consistency_test<CliffordLike: TestableClifford>() {
    let test_cases = clifford_examples::<CliffordLike>();
    for c in test_cases {
        assert!(c.is_valid());
        assert_images_consistent(&c);
        assert_preimages_consistent(&c);
        assert_inverse_and_multiply_are_consistent(&c);
    }
}

#[test]
pub fn clifford_consistency_test() {
    generic_consistency_test::<CliffordUnitary>();
    generic_consistency_test::<CliffordUnitaryModPauli>();
}

fn generic_multiply_test<CliffordLike: TestableClifford>() {
    let examples = clifford_order2_examples::<CliffordLike>();
    for clifford in examples {
        let r = clifford.multiply_with(&clifford);
        assert!(r.is_valid());
        assert!(r.is_identity());
    }
}

#[test]
pub fn clifford_multiply_test() {
    generic_multiply_test::<CliffordUnitary>();
    generic_multiply_test::<CliffordUnitaryModPauli>();
}

fn compare_clifford_transformations<CliffordLike: TestableClifford>(
    num_qubits: usize,
    apply_transformation1: impl FnOnce(&mut CliffordLike),
    apply_transformation2: impl FnOnce(&mut CliffordLike),
) {
    let mut c1 = CliffordLike::identity(num_qubits);
    let mut c2 = CliffordLike::identity(num_qubits);
    apply_transformation1(&mut c1);
    apply_transformation2(&mut c2);
    assert!(c1.is_valid());
    assert!(c2.is_valid());
    assert!(c1 == c2);
}

fn sparse<CliffordLike: TestableClifford>(
    observable: &[PositionedPauliObservable],
) -> <CliffordLike as TestableClifford>::SparsePauli {
    CliffordLike::SparsePauli::from(observable)
}

fn apply_exp_zz<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    clifford.left_mul_pauli_exp(&sparse::<CliffordLike>(&[z(0), z(1)]));
}

fn apply_exp_xx<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    clifford.left_mul_pauli_exp(&sparse::<CliffordLike>(&[x(0), x(1)]));
}

fn apply_cz<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    clifford.left_mul_cz(0, 1);
}

fn apply_cz2<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    let z = |j| CliffordLike::SparsePauli::from(&[z(j)]);
    clifford.left_mul_controlled_pauli(&z(0), &z(1));
}

fn apply_cz3<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    let sparse = sparse::<CliffordLike>;
    clifford.left_mul_pauli_exp(&sparse(&[-z(0)]));
    clifford.left_mul_pauli_exp(&sparse(&[z(0), z(1)]));
    clifford.left_mul_pauli_exp(&sparse(&[-z(1)]));
}

fn apply_exp_zz_via_cx(clifford: &mut impl TestableClifford) {
    clifford.left_mul_cx(1, 0);
    clifford.left_mul_root_z_inverse(0);
    clifford.left_mul_cx(1, 0);
}

fn apply_exp_xx_via_cx(clifford: &mut impl TestableClifford) {
    clifford.left_mul_cx(1, 0);
    clifford.left_mul_root_x_inverse(1);
    clifford.left_mul_cx(1, 0);
}

fn root_xyz_identities<CliffordLike: TestableClifford>() {
    let mut clifford = CliffordLike::identity(3);
    let sparse = sparse::<CliffordLike>;
    clifford.left_mul_pauli_exp(&sparse(&[-z(0)]));
    clifford.left_mul_root_z_inverse(0);
    assert!(clifford.is_identity());
    clifford.left_mul_pauli_exp(&sparse(&[-x(1)]));
    clifford.left_mul_root_x_inverse(1);
    assert!(clifford.is_identity());
    clifford.left_mul_pauli_exp(&sparse(&[-y(2)]));
    clifford.left_mul_root_y_inverse(2);
    assert!(clifford.is_identity());
    clifford.left_mul_hadamard(2);
    clifford.left_mul_pauli_exp(&sparse(&[-z(2)]));
    clifford.left_mul_hadamard(2);
    clifford.left_mul_root_x_inverse(2);
    assert!(clifford.is_identity());
}

fn generic_clifford_identities_test<CliffordLike: TestableClifford>() {
    compare_clifford_transformations::<CliffordLike>(2, apply_exp_zz_via_cx, apply_exp_zz);
    compare_clifford_transformations::<CliffordLike>(2, apply_exp_xx_via_cx, apply_exp_xx);
    compare_clifford_transformations::<CliffordLike>(2, apply_cz, apply_cz2);
    compare_clifford_transformations::<CliffordLike>(2, apply_cz, apply_cz3);
    root_xyz_identities::<CliffordLike>();
}

fn controlled_pauli_via_pauli_exp_test(
    control: &[PositionedPauliObservable],
    target: &[PositionedPauliObservable],
) {
    let mut control_sparse: SparsePauli = control.into();
    let mut target_sparse: SparsePauli = target.into();

    let mut p1p2 = control_sparse.clone();
    p1p2.mul_assign_right(&target_sparse);

    let num_qubits =
        std::cmp::max(control_sparse.max_qubit_id(), target_sparse.max_qubit_id()).unwrap() + 1;
    let mut clifford1 = CliffordUnitary::identity(num_qubits);
    let mut clifford2 = CliffordUnitary::identity(num_qubits);
    clifford1.left_mul_controlled_pauli(&control_sparse, &target_sparse);
    clifford2.left_mul_pauli_exp(&p1p2);
    control_sparse.add_assign_phase_exp(2);
    clifford2.left_mul_pauli_exp(&control_sparse);
    target_sparse.add_assign_phase_exp(2);
    clifford2.left_mul_pauli_exp(&target_sparse);
    assert_eq!(clifford1, clifford2);
}

#[test]
fn clifford_identities_test() {
    controlled_pauli_via_pauli_exp_test(&[z(0)], &[z(1)]);
    controlled_pauli_via_pauli_exp_test(&[z(0)], &[x(1)]);
    controlled_pauli_via_pauli_exp_test(&[y(0), x(1)], &[z(0), z(1)]);
    controlled_pauli_via_pauli_exp_test(&[x(0), x(1)], &[z(0), z(1)]);
    generic_clifford_identities_test::<CliffordUnitaryModPauli>();
    generic_clifford_identities_test::<CliffordUnitary>();
}

fn generic_random_tensor_test<CliffordLike: TestableClifford>(
    num_qubits1: usize,
    num_qubits2: usize,
) {
    let id1 = CliffordLike::identity(num_qubits1);
    let id2 = CliffordLike::identity(num_qubits2);
    let r1 = CliffordLike::random(num_qubits1, &mut thread_rng());
    let r2 = CliffordLike::random(num_qubits2, &mut thread_rng());
    assert!((r1.tensor(&id2)).multiply_with(&(id1.tensor(&r2))) == r1.tensor(&r2));
}

fn generic_tensor_test<CliffordLike: TestableClifford>() {
    let mut c1 = CliffordLike::identity(2);
    c1.left_mul_cx(0, 1);
    let mut c2 = CliffordLike::identity(2);
    c2.left_mul_cz(0, 1);
    let mut c1xc2 = CliffordLike::identity(4);
    c1xc2.left_mul_cx(0, 1);
    c1xc2.left_mul_cz(2, 3);
    assert!(c1xc2 == c1.tensor(&c2));

    for _ in 0..10 {
        generic_random_tensor_test::<CliffordLike>(5, 10);
    }
}

#[test]
fn tensor_test() {
    generic_tensor_test::<CliffordUnitary>();
    generic_tensor_test::<CliffordUnitaryModPauli>();
}

fn are_bits_equal_to_col(bitstring: &impl Bitwise, matrix: &BitMatrix, col: usize) -> bool {
    for j in 0..matrix.columncount() {
        if matrix[(j, col)] != bitstring.index(j) {
            return false;
        }
    }
    true
}

/// # Panics
///
/// Will panic
pub fn random_bitmatrix(rowcount: usize, columncount: usize) -> BitMatrix {
    let mut matrix = BitMatrix::with_shape(rowcount, columncount);
    let mut bits = std::iter::from_fn(move || Some(rand::Rng::gen::<bool>(&mut thread_rng())));
    for row_index in 0..rowcount {
        for column_index in 0..columncount {
            matrix.set((row_index, column_index), bits.next().expect("boom"));
        }
    }
    matrix
}

#[test]
fn css_clifford_test() {
    let mut num_tests = 0;
    while num_tests < 100 {
        let num_qubits = 10;
        let a = random_bitmatrix(num_qubits, num_qubits);
        if a.rank() == a.rowcount() {
            num_tests += 1;
            let a_inv_t = a.inverted().transposed();
            let c = CliffordUnitary::from_css_preimage_indicators(&a, &a_inv_t);
            assert!(c.is_valid());
            for k in c.qubits() {
                assert!(c.preimage_z(k).x_bits().is_zero());
                assert!(c.preimage_z(k).z_bits() == &a_inv_t.row(k));
                assert!(c.image_z(k).x_bits().is_zero());
                assert!(are_bits_equal_to_col(c.image_z(k).z_bits(), &a, k));

                assert!(c.preimage_x(k).z_bits().is_zero());
                assert!(c.preimage_x(k).x_bits() == &a.row(k));
                assert!(c.image_x(k).z_bits().is_zero());
                assert!(are_bits_equal_to_col(c.image_x(k).x_bits(), &a_inv_t, k));
            }
        }
    }
}

fn left_mul_clifford_generic_test<CliffordLike: TestableClifford>() {
    let mut clifford1 = CliffordLike::identity(4);
    let mut clifford2 = CliffordLike::identity(2);
    clifford1.left_mul_cx(1, 2);
    clifford2.left_mul_cx(0, 1);
    clifford1.left_mul_clifford(&clifford2, &[1, 2]);
    assert! {clifford1.is_identity()}
    clifford1.left_mul_cx(2, 3);
    clifford1.left_mul_clifford(&clifford2, &[2, 3]);
}

#[test]
fn left_mul_clifford_test() {
    left_mul_clifford_generic_test::<CliffordUnitary>();
    left_mul_clifford_generic_test::<CliffordUnitaryModPauli>();
}

fn left_mul_permutation_generic_test<CliffordLike: TestableClifford>() {
    let mut clifford1 = CliffordLike::identity(3);
    clifford1.left_mul_swap(0, 1);
    clifford1.left_mul_swap(1, 2);
    clifford1.left_mul_permutation(&[2, 0, 1], &[0, 1, 2]);
    assert!(clifford1.is_identity());
}

#[test]
fn left_mul_permutation_test() {
    left_mul_permutation_generic_test::<CliffordUnitary>();
    left_mul_permutation_generic_test::<CliffordUnitaryModPauli>();
}

fn format_string_roundtrip_generic_test<CliffordLike: TestableClifford>(clifford: &CliffordLike) {
    let sparse_str = format!("{clifford}");
    let dense_str = format!("{clifford:#}");
    let clifford1 = sparse_str.parse::<CliffordLike>().expect(&sparse_str);
    let clifford2 = dense_str.parse::<CliffordLike>().expect(&dense_str);
    assert_eq!(clifford, &clifford1);
    assert_eq!(clifford, &clifford2);
}

fn random_diagonal_clifford<CliffordLike: TestableClifford>(qubit_count: usize) -> CliffordLike {
    let generators = diagonal_operations(qubit_count);
    random_clifford_via_operations_sampling(qubit_count, qubit_count * qubit_count, &generators)
}

fn random_css_clifford<CliffordLike: TestableClifford>(qubit_count: usize) -> CliffordLike {
    let generators = css_operations(qubit_count);
    random_clifford_via_operations_sampling(qubit_count, qubit_count * qubit_count, &generators)
}

fn generic_diagonal_clifford_test<CliffordLike: TestableClifford>(c: &CliffordLike) {
    use XOrZ::{X, Z};
    assert!(c.is_diagonal(Z));
    assert!(c.inverse().is_diagonal(Z));

    let qubit_count = c.num_qubits();
    let mut c2 = CliffordLike::identity(qubit_count);
    transverse_h(&mut c2);
    c2.left_mul_clifford(c, &c.qubits().collect_vec());
    assert!(c2.is_diagonal_resource_encoder(Z));
    let c3 = c2.unitary_from_diagonal_resource_state(Z).unwrap();
    assert!(c3.is_valid());
    assert!(c3.is_diagonal(Z));
    for qubit_index in c3.qubits() {
        assert_eq!(c3.image_x(qubit_index), c2.image_z(qubit_index));
    }

    transverse_h(&mut c2);
    assert!(c2.is_diagonal(X));
    assert!(c2.inverse().is_diagonal(X));
    assert!(c2.is_diagonal_resource_encoder(X));
    let c4 = c2.unitary_from_diagonal_resource_state(X).unwrap();
    assert!(c4.is_valid());
    assert!(c4.is_diagonal(X));
    for qubit_index in c4.qubits() {
        assert_eq!(c4.image_z(qubit_index), c2.image_z(qubit_index));
    }
}

fn generic_is_css_clifford_test<CliffordLike: TestableClifford>(c: &CliffordLike) {
    assert!(c.is_css());
}

fn transverse_h<CliffordLike: TestableClifford>(clifford: &mut CliffordLike) {
    for qubit_index in clifford.qubits() {
        clifford.left_mul_hadamard(qubit_index);
    }
}

fn check_left_mul_root_and_apply_root_are_consistent(
    qubit_count: usize,
    left_mul_root: fn(&mut CliffordUnitary, usize),
    apply_root: fn(&mut DensePauli, usize),
) {
    let mut clifford = CliffordUnitary::identity(qubit_count);
    for target_qubit in 0..qubit_count {
        let mut image = clifford.image_z(target_qubit);
        left_mul_root(&mut clifford, target_qubit);
        apply_root(&mut image, target_qubit);
        assert_eq!(image, clifford.image_z(target_qubit));
    }
}
