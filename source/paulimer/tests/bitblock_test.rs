// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use paulimer::bits::bitblock::BitBlock;
use paulimer::bits::WORD_COUNT_DEFAULT;
use proptest::prelude::*;

proptest! {
    #[test]
    fn from_array(bits in arbitrary_bool_array()) {
        let block = BitBlock::from_array(bits);
        for index in 0..bits.len() {
            assert_eq!(block[index], bits[index]);
        }
    }

    #[test]
    fn set(block in arbitrary_bitblock(), index in 0..BITS) {
        let mut clone = block.clone();
        for value in [true, false] {
            clone.set(index, value);
            assert_eq!(clone[index], value);
            for index2 in 0..BITS {
                if index != index2 {
                    assert_eq!(clone[index2], block[index2]);
                }
            }
        }
    }

    #[test]
    fn xor(left in arbitrary_bitblock(), right in arbitrary_bitblock()) {
        let xor = &left ^ &right;
        for index in 0..BITS {
            assert_eq!(xor[index], left[index] ^ right[index]);
        }
    }

    #[test]
    fn xor_assign(mut left in arbitrary_bitblock(), right in arbitrary_bitblock()) {
        let xor = &left ^ &right;
        left ^= &right;
        assert_eq!(left, xor);
    }

    #[test]
    fn and(left in arbitrary_bitblock(), right in arbitrary_bitblock()) {
        let and = &left & &right;
        for index in 0..BITS {
            assert_eq!(and[index], left[index] & right[index]);
        }
    }

    #[test]
    fn and_assign(mut left in arbitrary_bitblock(), right in arbitrary_bitblock()) {
        let and = &left & &right;
        left &= &right;
        assert_eq!(left, and);
    }

}

#[test]
fn zeros() {
    let block = BitBlock::zeros();
    for index in 0..BITS {
        assert!(!block[index], "{}", index);
    }
}

#[test]
fn ones() {
    let block = BitBlock::ones();
    for index in 0..BITS {
        assert!(block[index], "{}", index);
    }
}

#[test]
fn all() {
    for value in [true, false] {
        let block = BitBlock::all(value);
        for index in 0..BITS {
            assert_eq!(block[index], value, "{index}");
        }
    }
}

const BITS: usize = BitBlock::<WORD_COUNT_DEFAULT>::BITS;

fn arbitrary_bool_array() -> impl Strategy<Value = [bool; BITS]> {
    proptest::array::uniform::<proptest::bool::Any, BITS>(proptest::bool::ANY)
}

fn arbitrary_bitblock() -> impl Strategy<Value = BitBlock> {
    arbitrary_bool_array().prop_map(BitBlock::from_array)
}
