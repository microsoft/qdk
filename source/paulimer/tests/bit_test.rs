use itertools::Itertools;
use paulimer::{
    bits::{bitblock::BitBlock, BitVec, Bitwise, BitwiseBinaryOps, BitwiseNeutralElement, IndexAssignable, IndexSet},
    NeutralElement,
};
use std::any::type_name;

/// # Panics
///
/// Will panic
pub fn test_one_bit_index<T: IndexAssignable + Bitwise + BitwiseNeutralElement>(mut bits: T, index: usize) -> T {
    bits.clear_bits();
    assert!(bits.is_zero());
    bits.assign_index(index, true);
    assert!(bits.index(index));
    assert!(bits.support().contains(&index));
    assert_eq!(bits.support().count(), 1);
    assert_eq!(bits.weight(), 1);
    assert!(bits.parity());
    bits.negate_index(index);
    assert!(!bits.index(index));
    assert_eq!(bits.support().count(), 0);
    assert_eq!(bits.weight(), 0);
    assert!(!bits.parity());

    let mut other_bits = bits.neutral_element();
    bits.negate_index(index);
    assert!(other_bits.is_zero());
    other_bits.neutral_element();
    other_bits.bitxor_assign(&bits);
    assert_eq!(other_bits.support().count(), 1);
    assert_eq!(other_bits.weight(), 1);
    assert!(other_bits.parity());
    bits.clear_bits();

    other_bits.negate_index(index + 1);
    other_bits.bitand_assign(&bits);
    assert_eq!(bits.weight(), 0);
    assert!(!bits.parity());

    bits
}

/// # Panics
///
/// Will panic
pub fn test_unary_bit_traits<T: BitwiseNeutralElement>(size: usize, index: usize)
where
    T::NeutralElementType: Bitwise + IndexAssignable + BitwiseNeutralElement,
{
    assert!(index + 1 < size);
    println!(
        "Testing: test_unary_bit_traits::<{}> size:{} index:{}",
        type_name::<T>(),
        size,
        index
    );
    let item = T::neutral_element_of_size(size);
    test_one_bit_index(item, index);
}

macro_rules! call_test_per_uint {
    ($function:ident,$uint:ty,$first:expr $(, $rest:expr)*) => {
        $function::<$uint>($first,$($rest),*);
        $function::<[$uint;ARRAY_SIZE]>($first,$($rest),*);
        $function::<Vec<$uint>>($first,$($rest),*);
        $function::<Vec<[$uint;ARRAY_SIZE]>>($first,$($rest),*);
    };
}

macro_rules! call_test {
    ($function:ident,$first:expr $(, $rest:expr)*) => {
        call_test_per_uint!($function,u16,$first,$($rest),*);
        call_test_per_uint!($function,u32,$first,$($rest),*);
        call_test_per_uint!($function,u64,$first,$($rest),*);
        call_test_per_uint!($function,u128,$first,$($rest),*);
        $function::<IndexSet>($first,$($rest),*);
        $function::<BitBlock>($first,$($rest),*);
        $function::<BitVec>($first,$($rest),*);
    };
}

#[test]
fn bit_test() {
    const ARRAY_SIZE: usize = 4;
    let size = 10;
    let index = 7;
    call_test!(test_unary_bit_traits, size, index);
}
