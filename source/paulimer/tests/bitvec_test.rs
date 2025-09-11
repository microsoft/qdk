use paulimer::bits::BitVec;
use paulimer::bits::Bitwise;
use paulimer::bits::BitwiseBinaryOps;
use paulimer::bits::IndexAssignable;
use paulimer::bits::WORD_COUNT_DEFAULT;
use proptest::prelude::*;

proptest! {
    #[test]
    fn from_iter(bits in prop::collection::vec(any::<bool>(), 0..2000)) {
        let bitvec = BitVec::<WORD_COUNT_DEFAULT>::from_iter(bits.clone());
        let actual: Vec<bool> = bitvec.into_iter().collect();
        assert_eq!(bits.support().collect::<Vec<usize>>(), actual.support().collect::<Vec<usize>>());
    }

    #[test]
    fn index(bits in prop::collection::vec(any::<bool>(), 0..2000)) {
        let bitvec = BitVec::<WORD_COUNT_DEFAULT>::from_iter(bits.clone());
        for (index, expected) in bits.iter().enumerate() {
            assert_eq!(bitvec.index(index), *expected);
        }
    }

    #[test]
    fn weight(bits in arbitrary_bitvec(2000)) {
        let ones = bits.into_iter().filter(|bit| *bit);
        assert_eq!(ones.count(), bits.weight());
    }

    #[test]
    fn support(bits in arbitrary_bitvec(2000)) {
        let support: Vec<usize> = bits.support().collect();
        assert_eq!(support.len(), bits.weight());
        for index in support {
            assert!(bits.index(index));
        }
    }

    #[test]
    fn assign(bits in prop::collection::vec(any::<bool>(), 0..10)) {
        let mut bitvec = BitVec::<WORD_COUNT_DEFAULT>::of_length(bits.len());
        for (index, bit) in bits.iter().enumerate() {
            bitvec.assign_index(index, *bit);
        }
        let actual: Vec<bool> = bitvec.into_iter().take(bits.len()).collect();
        assert_eq!(bits, actual);
    }

    #[test]
    fn bitxor_assign((left, right) in equal_length_bitvecs(2000)) {
        let mut xored = left.clone();
        xored.bitxor_assign(&right);
        for (index, result) in xored.into_iter().enumerate() {
            assert_eq!(result, left.index(index) ^ right.index(index));
        }
    }

}

fn arbitrary_bitvec(max_length: usize) -> impl Strategy<Value = BitVec<WORD_COUNT_DEFAULT>> {
    prop::collection::vec(any::<bool>(), 0..max_length).prop_map(BitVec::<WORD_COUNT_DEFAULT>::from_iter)
}

fn equal_length_bitvecs(max_length: usize) -> impl Strategy<Value = (BitVec, BitVec)> {
    (0..max_length).prop_flat_map(|length| {
        (
            prop::collection::vec(any::<bool>(), length).prop_map(BitVec::from_iter),
            prop::collection::vec(any::<bool>(), length).prop_map(BitVec::from_iter),
        )
    })
}
