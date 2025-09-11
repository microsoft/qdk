use core::fmt;
use std::str::FromStr;

use paulimer::{
    bits::MutableBitView,
    pauli::{
        commutes_with, generic::PhaseExponent, DensePauli, DensePauliProjective, Pauli, PauliBinaryOps, PauliUnitary,
        Phase, SparsePauli, SparsePauliProjective,
    },
};
use proptest::prelude::*;
use paulimer::quantum_core::{x, y, z};
use rand::thread_rng;

proptest! {
    #[test]
    fn from_bits(pauli in arbitrary_pauli(1000)) {
        let from_bits = PauliUnitary::from_bits(pauli.x_bits().clone(), pauli.z_bits().clone(), pauli.xz_phase_exponent());
        assert_eq!(pauli.x_bits(), from_bits.x_bits());
        assert_eq!(pauli.z_bits(), from_bits.z_bits());
        assert_eq!(pauli.xz_phase_exponent(), from_bits.xz_phase_exponent());
        assert_eq!(pauli, from_bits);
    }

    #[test]
    fn from_references(pauli in arbitrary_pauli(1000)) {
        let from_reference = PauliUnitary::from_bits(pauli.x_bits().as_slice(), pauli.z_bits().as_slice(), pauli.xz_phase_exponent());
        assert_eq!(pauli.x_bits(), from_reference.x_bits());
        assert_eq!(pauli.z_bits(), from_reference.z_bits());
        assert_eq!(pauli.xz_phase_exponent(), from_reference.xz_phase_exponent());
        assert_eq!(pauli, from_reference);
    }

    #[test]
    fn is_hermitian(pauli in arbitrary_pauli(1000)) {
        let square = pauli.clone() * &pauli;
        let expect_hermitian = square.xz_phase_exponent().value() == 0;
        assert_eq!(pauli.is_order_two(), expect_hermitian);
    }

    #[test]
    fn commutes_with_matches_explicit_commutator((left, right) in equal_length_paulis(1)) {
        let leftright = left.clone() * &right;
        let rightleft = right.clone() * &left;
        let expect_commutes_with = leftright == rightleft;
        assert_eq!(commutes_with(&left,&right), expect_commutes_with);
    }

    #[test]
    fn cayley_table(length in 1..1000usize, mut index in 0..999usize) {
        index %= length;
        let zeros = vec![false; length];
        let mut bits = zeros.clone();
        bits[index] = true;
        let identity = PauliUnitary::from_bits(zeros.clone(), zeros.clone(), 0u8);
        let i = &PauliUnitary::from_bits(zeros.clone(), zeros.clone(), 1u8);
        let neg = &PauliUnitary::from_bits(zeros.clone(), zeros.clone(), 2u8);
        let x = &PauliUnitary::from_bits(bits.clone(), zeros.clone(), 0u8);
        let z = &PauliUnitary::from_bits(zeros.clone(), bits.clone(), 0u8);
        let y = &PauliUnitary::from_bits(bits.clone(), bits.clone(), 1u8);

        assert_eq!(x.clone() * x, identity);
        assert_eq!(y.clone() * y, identity);
        assert_eq!(z.clone() * z, identity);

        assert_eq!(x * x, identity);
        assert_eq!(y * y, identity);
        assert_eq!(z * z, identity);

        assert_eq!(z.clone() * x, y.clone() * i);
        assert_eq!(x.clone() * z, y.clone() * i * neg);
        assert_eq!(y.clone() * z, x.clone() * i);
        assert_eq!(z.clone() * y, x.clone() * i * neg);
        assert_eq!(x.clone() * y, z.clone() * i);
        assert_eq!(y.clone() * x, z.clone() * i * neg);

        assert_eq!(z * x, i * y );
        assert_eq!(x * z, -i * y );
        assert_eq!(y * z, i * x);
        assert_eq!(z * y, -i * x);
        assert_eq!(x * y, i * z );
        assert_eq!(y * x, -i * z);

        assert_eq!(i * x * z, y);
    }

    #[test]
    fn unsigned_int_paulis(index in 0..32usize) {
        let one_bit = 1u32 << index;
        let zero_bit = 0u32;
        let x = &PauliUnitary::from_bits(one_bit, zero_bit, 0u8);
        let z = &PauliUnitary::from_bits(zero_bit, one_bit, 0u8);
        let y = &PauliUnitary::from_bits(one_bit, one_bit, 1u8);
        let i = &PauliUnitary::from_bits(zero_bit, zero_bit, 1u8);
        assert_eq!(i * x * z, y);
    }

    #[test]
    fn mul_assign((mut left, right) in equal_length_paulis(1000)) {
        let product = left.clone() * &right;
        left *= &right;
        assert_eq!(left, product);
    }

    #[test]
    fn mul_assign_phase(mut pauli in arbitrary_pauli(1000), raw_exponent in 0..4u8) {
        let original = pauli.clone();
        let exponent = raw_exponent;
        pauli *= Phase::from_exponent(exponent);
        assert_eq!(original.x_bits(), pauli.x_bits());
        assert_eq!(original.z_bits(), pauli.z_bits());
        assert_eq!(
            original.xz_phase_exponent().raw_value().wrapping_add(exponent) % 4u8,
            pauli.xz_phase_exponent().value()
        );
    }

    #[test]
    fn format_round_trip(pauli in arbitrary_pauli(50)) {
        let str_sparse = format!("{pauli}");
        let str_dense = format!("{pauli:#}");
        test_round_trip::<SparsePauli>(&str_sparse, &str_dense, true);
        test_round_trip::<DensePauli>(&str_sparse, &str_dense, true);
        test_round_trip::<SparsePauliProjective>(&str_sparse, &str_dense, false);
        test_round_trip::<DensePauliProjective>(&str_sparse, &str_dense, false);
    }

    #[test]
    fn left_multiply((left, right) in equal_length_paulis(1000)) {
        let right_product = left.clone() * &right;
        let left_product = &left * right;
        assert_eq!(right_product, left_product);
    }


}

prop_compose! {
   fn arbitrary_pauli(max_dimension: usize)(dimension in 0..max_dimension) -> PauliUnitary<Vec<bool>, u8>{
        arbitrary_pauli_of_length(dimension)
   }
}
prop_compose! {
   fn equal_length_paulis(max_dimension: usize)(dimension in 0..max_dimension) -> (PauliUnitary<Vec<bool>, u8>, PauliUnitary<Vec<bool>, u8>){
        (arbitrary_pauli_of_length(dimension), arbitrary_pauli_of_length(dimension))
   }
}

fn arbitrary_pauli_of_length(length: usize) -> PauliUnitary<Vec<bool>, u8> {
    paulimer::pauli::pauli_random(length, &mut thread_rng())
}

#[test]
fn pauli_product_test() {
    let target: DensePauli = [z(0), z(1)].into();
    let preimage: DensePauli = [x(0)].into();
    let mut phase = preimage.xz_phase_exponent().raw_value();

    let (mut x_bits, mut z_bits) = preimage.to_xz_bits();
    let mut preimage_view =
        PauliUnitary::<MutableBitView, &mut u8>::from_bits(x_bits.as_view_mut(), z_bits.as_view_mut(), &mut phase);

    let control: DensePauli = [y(0), x(1)].into();
    let preimage_r: DensePauli = [-y(1)].into();
    preimage_view.mul_assign_right(&target);
    println!("{preimage_view}");
    preimage_view.mul_assign_left(&control);
    assert!(preimage_view == preimage_r);
}

fn test_round_trip<PauliLike: Pauli + FromStr<Err: fmt::Debug> + Eq + fmt::Debug + fmt::Display>(
    str_sparse: &str,
    str_dense: &str,
    with_phases: bool,
) {
    let pauli1 = str_sparse.parse::<PauliLike>().expect(str_sparse);
    let pauli2 = str_dense.parse::<PauliLike>().expect(str_dense);
    assert_eq!(pauli1, pauli2);

    let str_sparse_1 = format!("{pauli1}");
    let str_dense_1 = format!("{pauli1:#}");

    let str_sparse_2 = format!("{pauli2}");
    let str_dense_2 = format!("{pauli2:#}");

    assert_eq!(str_dense_2, str_dense_1);
    assert_eq!(str_sparse_2, str_sparse_1);

    if with_phases {
        assert_eq!(str_dense, str_dense_2);
        assert_eq!(str_sparse, str_sparse_2);
    }

    let pauli3 = str_sparse_2.parse::<PauliLike>().expect(str_sparse);
    let pauli4 = str_dense_2.parse::<PauliLike>().expect(str_dense);
    assert_eq!(pauli1, pauli3);
    assert_eq!(pauli1, pauli4);
}
