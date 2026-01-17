use crate::bits::bitblock::{BitBlock, Word};
use crate::bits::bitview::{BitView, MutableBitView};
use crate::bits::index_set::IndexSet;
use crate::bits::{Bitwise, BitwiseBinaryOps, Dot, IndexAssignable, OverlapWeight};
use crate::NeutralElement;

use super::standard_types::{BitIterator, BitsPerBlock};
use super::{are_supports_equal, BorrowAsBitIterator};
use super::{BitwiseNeutralElement, FromBits};

pub const WORD_COUNT_DEFAULT: usize = 8usize;

pub fn block_count(length: usize, bits_per_block: usize) -> usize {
    let mut block_count = length / bits_per_block;
    if !length.is_multiple_of(bits_per_block) {
        block_count += 1;
    }
    block_count
}

#[must_use]
#[derive(Eq, Clone, Debug)]
pub struct BitVec<const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    pub(crate) blocks: Vec<[Word; WORD_COUNT]>,
}

impl<const WORD_COUNT: usize> std::hash::Hash for BitVec<WORD_COUNT> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.blocks.hash(state);
    }
}

impl<const WORD_COUNT: usize> BitVec<WORD_COUNT> {
    const fn bits_per_block() -> usize {
        <[Word; WORD_COUNT] as BitsPerBlock>::BITS_PER_BLOCK
    }

    pub fn of_length(length: usize) -> BitVec<WORD_COUNT> {
        Self::zeros(length)
    }

    pub fn zeros(length: usize) -> BitVec<WORD_COUNT> {
        BitVec {
            blocks: vec![[0; WORD_COUNT]; block_count(length, Self::bits_per_block())],
        }
    }

    #[must_use]
    pub fn top(&self) -> u64 {
        self.blocks[0][0]
    }

    pub fn top_mut(&mut self) -> &mut u64 {
        &mut self.blocks[0][0]
    }

    pub fn from_view<const WORD_COUNT_IN: usize>(
        view: &BitView<WORD_COUNT_IN>,
    ) -> BitVec<WORD_COUNT_IN> {
        BitVec::<WORD_COUNT_IN> {
            blocks: view.blocks.to_vec(),
        }
    }

    pub fn from_view_mut<const WORD_COUNT_IN: usize>(
        view: &MutableBitView<WORD_COUNT_IN>,
    ) -> BitVec<WORD_COUNT_IN> {
        BitVec::<WORD_COUNT_IN> {
            blocks: view.blocks.to_vec(),
        }
    }

    pub fn selected_from<'life, Iterable, const WORD_COUNT_IN: usize>(
        view: &'life BitView<WORD_COUNT_IN>,
        support: Iterable,
    ) -> BitVec<WORD_COUNT_IN>
    where
        Iterable: IntoIterator<Item = &'life usize>,
        Iterable::IntoIter: ExactSizeIterator<Item = &'life usize>,
    {
        let support_iterator = support.into_iter();
        let mut bits = BitVec::<WORD_COUNT_IN>::of_length(support_iterator.len());
        for index in support_iterator {
            bits.assign_index(*index, view.index(*index));
        }
        bits
    }

    #[must_use]
    pub fn as_view(&self) -> BitView<'_, WORD_COUNT> {
        BitView {
            blocks: &self.blocks,
        }
    }

    pub fn as_view_mut(&mut self) -> MutableBitView<'_, WORD_COUNT> {
        MutableBitView {
            blocks: &mut self.blocks,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len() * <[Word; WORD_COUNT] as BitsPerBlock>::BITS_PER_BLOCK
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.len() == 0
    }
}

// Bit traits : Unary

macro_rules! blocks_neutral_element_body {
    () => {
        type NeutralElementType = BitVec<WORD_COUNT>;

        fn neutral_element(&self) -> <Self as NeutralElement>::NeutralElementType {
            BitVec::<WORD_COUNT> {
                blocks: self.blocks.neutral_element(),
            }
        }

        fn default_size_neutral_element() -> <Self as NeutralElement>::NeutralElementType {
            BitVec::<WORD_COUNT>::zeros(0)
        }

        fn neutral_element_of_size(size: usize) -> <Self as NeutralElement>::NeutralElementType {
            BitVec::<WORD_COUNT>::zeros(size)
        }
    };
}

impl<const WORD_COUNT: usize> NeutralElement for BitVec<WORD_COUNT> {
    blocks_neutral_element_body!();
}

impl<const WORD_COUNT: usize> NeutralElement for MutableBitView<'_, WORD_COUNT> {
    blocks_neutral_element_body!();
}

impl<const WORD_COUNT: usize> NeutralElement for BitView<'_, WORD_COUNT> {
    blocks_neutral_element_body!();
}

impl<const WORD_COUNT: usize> BitwiseNeutralElement for BitVec<WORD_COUNT> {}
impl<const WORD_COUNT: usize> BitwiseNeutralElement for MutableBitView<'_, WORD_COUNT> {}
impl<const WORD_COUNT: usize> BitwiseNeutralElement for BitView<'_, WORD_COUNT> {}

macro_rules! blocks_bitwise_body {
    () => {
        fn index(&self, index: usize) -> bool {
            self.blocks.index(index)
        }

        fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize> {
            self.blocks.support()
        }

        fn weight(&self) -> usize {
            self.blocks.weight()
        }

        fn parity(&self) -> bool {
            self.blocks.parity()
        }

        fn is_zero(&self) -> bool {
            self.blocks.is_zero()
        }
    };
}

impl<const WORD_COUNT: usize> Bitwise for BitVec<WORD_COUNT> {
    blocks_bitwise_body!();
}

impl<const WORD_COUNT: usize> Bitwise for MutableBitView<'_, WORD_COUNT> {
    blocks_bitwise_body!();
}

impl<const WORD_COUNT: usize> Bitwise for BitView<'_, WORD_COUNT> {
    blocks_bitwise_body!();
}

impl<const WORD_COUNT: usize> Bitwise for BitBlock<WORD_COUNT> {
    blocks_bitwise_body!();
}

impl<const WORD_COUNT: usize> Bitwise for &BitBlock<WORD_COUNT> {
    blocks_bitwise_body!();
}

impl<const WORD_COUNT: usize> Bitwise for &mut BitBlock<WORD_COUNT> {
    blocks_bitwise_body!();
}

macro_rules! blocks_index_assignable_body {
    () => {
        fn assign_index(&mut self, index: usize, to: bool) {
            self.blocks.assign_index(index, to)
        }

        fn negate_index(&mut self, index: usize) {
            self.blocks.negate_index(index)
        }

        fn clear_bits(&mut self) {
            self.blocks.clear_bits()
        }
    };
}

impl<const WORD_COUNT: usize> IndexAssignable for BitVec<WORD_COUNT> {
    blocks_index_assignable_body!();
}

impl<const WORD_COUNT: usize> IndexAssignable for MutableBitView<'_, WORD_COUNT> {
    blocks_index_assignable_body!();
}

impl<const WORD_COUNT: usize> IndexAssignable for &mut BitBlock<WORD_COUNT> {
    blocks_index_assignable_body!();
}

impl<const WORD_COUNT: usize> IndexAssignable for BitBlock<WORD_COUNT> {
    blocks_index_assignable_body!();
}

macro_rules! borrow_as_bit_iterator_body {
    () => {
        type BitIterator<'life1>
            = BitIterator<'life1>
        where
            Self: 'life1;
        fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
            self.blocks.borrow_as_bit_iterator()
        }
    };
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for BitVec<WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for BitBlock<WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for &BitBlock<WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for &mut BitBlock<WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for MutableBitView<'_, WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

impl<const WORD_COUNT: usize> BorrowAsBitIterator for BitView<'_, WORD_COUNT> {
    borrow_as_bit_iterator_body!();
}

// Bit traits : Binary

macro_rules! blocks_bitwise_binary_ops_body {
    ($other_type:ty) => {
        fn assign(&mut self, other: &$other_type) {
            self.blocks.assign(&other.blocks)
        }

        fn bitxor_assign(&mut self, other: &$other_type) {
            self.blocks.bitxor_assign(&other.blocks)
        }

        fn bitand_assign(&mut self, other: &$other_type) {
            self.blocks.bitand_assign(&other.blocks)
        }
    };
}

macro_rules! blocks_bitwise_binary_ops_refs {
    ($other_type:ty,$vec_type:ty) => {
        impl<const WORD_COUNT: usize> BitwiseBinaryOps<$other_type> for $vec_type {
            blocks_bitwise_binary_ops_body!($other_type);
        }
    };
}

macro_rules! blocks_bitwise_binary_ops {
    ($vec_type:ty) => {
        blocks_bitwise_binary_ops_refs!(BitVec<WORD_COUNT>, $vec_type);
        blocks_bitwise_binary_ops_refs!(BitView<'_, WORD_COUNT>, $vec_type);
        blocks_bitwise_binary_ops_refs!(MutableBitView<'_, WORD_COUNT>, $vec_type);
    };
}

blocks_bitwise_binary_ops!(BitVec<WORD_COUNT>);
blocks_bitwise_binary_ops!(MutableBitView<'_, WORD_COUNT>);

macro_rules! bitblock_bitwise_binary_ops {
    ($bitblock_type:ty) => {
        blocks_bitwise_binary_ops_refs!(BitBlock<WORD_COUNT>, $bitblock_type);
        blocks_bitwise_binary_ops_refs!(&BitBlock<WORD_COUNT>, $bitblock_type);
        blocks_bitwise_binary_ops_refs!(&mut BitBlock<WORD_COUNT>, $bitblock_type);
    };
}

bitblock_bitwise_binary_ops!(BitBlock<WORD_COUNT>);
bitblock_bitwise_binary_ops!(&mut BitBlock<WORD_COUNT>);

macro_rules! blocks_dot_body {
    ($other_type:ty) => {
        fn dot(&self, other: &$other_type) -> bool {
            self.blocks.dot(&other.blocks)
        }
    };
}

macro_rules! blocks_overlap_weight_body {
    ($other_type:ty) => {
        fn and_weight(&self, other: &$other_type) -> usize {
            self.blocks.and_weight(&other.blocks)
        }

        fn or_weight(&self, other: &$other_type) -> usize {
            self.blocks.or_weight(&other.blocks)
        }
    };
}

macro_rules! bitblock_dot_refs {
    ($other_type:ty,$bitblock_type:ty) => {
        impl<const WORD_COUNT: usize> Dot<$other_type> for $bitblock_type {
            blocks_dot_body!($other_type);
        }

        impl<const WORD_COUNT: usize> OverlapWeight<$other_type> for $bitblock_type {
            blocks_overlap_weight_body!($other_type);
        }
    };
}

macro_rules! blocks_dot {
    ($vec_type:ty) => {
        bitblock_dot_refs!(BitVec<WORD_COUNT>, $vec_type);
        bitblock_dot_refs!(BitView<'_, WORD_COUNT>, $vec_type);
        bitblock_dot_refs!(MutableBitView<'_, WORD_COUNT>, $vec_type);
    };
}

blocks_dot!(BitVec<WORD_COUNT>);
blocks_dot!(BitView<'_, WORD_COUNT>);
blocks_dot!(MutableBitView<'_, WORD_COUNT>);

macro_rules! bitblock_dot {
    ($bitblock_type:ty) => {
        bitblock_dot_refs!(BitBlock<WORD_COUNT>, $bitblock_type);
        bitblock_dot_refs!(&BitBlock<WORD_COUNT>, $bitblock_type);
        bitblock_dot_refs!(&mut BitBlock<WORD_COUNT>, $bitblock_type);
    };
}

bitblock_dot!(BitBlock<WORD_COUNT>);
bitblock_dot!(&BitBlock<WORD_COUNT>);
bitblock_dot!(&mut BitBlock<WORD_COUNT>);

// Standard traits

impl<const WORD_COUNT: usize> FromIterator<bool> for BitVec<WORD_COUNT> {
    fn from_iter<Iterator: IntoIterator<Item = bool>>(iterator: Iterator) -> Self {
        let mut blocks = vec![];
        let mut iterator = iterator.into_iter();

        // Note: once `Iterator::array_chunks` is stabilized, we can use that instead.
        loop {
            let mut block = [0 as Word; WORD_COUNT];
            for index in 0..Self::bits_per_block() {
                match iterator.next() {
                    Some(bit) => block.assign_index(index, bit),
                    None if index == 0 => return BitVec { blocks },
                    None => {
                        blocks.push(block);
                        return BitVec { blocks };
                    }
                }
            }
            blocks.push(block);
        }
    }
}

macro_rules! blocks_partial_eq_body {
    ($other_type:ty) => {
        fn eq(&self, other: &$other_type) -> bool {
            self.blocks == other.blocks
        }
    };
}

macro_rules! blocks_partial_eq_body_bool {
    ($other_type:ty) => {
        fn eq(&self, other: &$other_type) -> bool {
            are_supports_equal(self, other)
        }
    };
}

macro_rules! blocks_partial_eq_vec_bool {
    ($vec_bool:ty,$vec_type:ty) => {
        impl<const WORD_COUNT: usize> PartialEq<$vec_bool> for $vec_type {
            blocks_partial_eq_body_bool!($vec_bool);
        }

        impl<const WORD_COUNT: usize> PartialEq<$vec_type> for $vec_bool {
            blocks_partial_eq_body_bool!($vec_type);
        }
    };
}

macro_rules! blocks_partial_eq {
    ($vec_type:ty) => {
        impl<const WORD_COUNT: usize> PartialEq<BitVec<WORD_COUNT>> for $vec_type {
            blocks_partial_eq_body!(BitVec<WORD_COUNT>);
        }

        impl<const WORD_COUNT: usize> PartialEq<MutableBitView<'_, WORD_COUNT>> for $vec_type {
            blocks_partial_eq_body!(MutableBitView<'_, WORD_COUNT>);
        }

        impl<const WORD_COUNT: usize> PartialEq<BitView<'_, WORD_COUNT>> for $vec_type {
            blocks_partial_eq_body!(BitView<'_, WORD_COUNT>);
        }

        impl<const WORD_COUNT: usize> PartialEq<IndexSet> for $vec_type {
            blocks_partial_eq_body_bool!(IndexSet);
        }

        blocks_partial_eq_vec_bool!(Vec<bool>, $vec_type);
        blocks_partial_eq_vec_bool!(&[bool], $vec_type);
        blocks_partial_eq_vec_bool!(&mut [bool], $vec_type);
    };
}

blocks_partial_eq!(BitVec<WORD_COUNT>);
blocks_partial_eq!(MutableBitView<'_, WORD_COUNT>);
blocks_partial_eq!(BitView<'_, WORD_COUNT>);

macro_rules! bitblocks_partial_eq {
    ($vec_type:ty) => {
        impl<const WORD_COUNT: usize> PartialEq<IndexSet> for $vec_type {
            blocks_partial_eq_body_bool!(IndexSet);
        }

        blocks_partial_eq_vec_bool!(Vec<bool>, $vec_type);
        blocks_partial_eq_vec_bool!(&[bool], $vec_type);
        blocks_partial_eq_vec_bool!(&mut [bool], $vec_type);
    };
}

bitblocks_partial_eq!(BitBlock<WORD_COUNT>);
bitblocks_partial_eq!(&mut BitBlock<WORD_COUNT>);
bitblocks_partial_eq!(&BitBlock<WORD_COUNT>);

macro_rules! blocks_into_iterator_body {
    () => {
        type Item = bool;
        type IntoIter = BitIterator<'life>;
        fn into_iter(self) -> Self::IntoIter {
            self.blocks.borrow_as_bit_iterator()
        }
    };
}

impl<'life, const WORD_COUNT: usize> IntoIterator for &'life BitVec<WORD_COUNT> {
    blocks_into_iterator_body!();
}

impl<'life, const WORD_COUNT: usize> IntoIterator for &'life MutableBitView<'_, WORD_COUNT> {
    blocks_into_iterator_body!();
}

impl<'life, const WORD_COUNT: usize> IntoIterator for &'life BitView<'_, WORD_COUNT> {
    blocks_into_iterator_body!();
}

impl<const WORD_COUNT: usize> BitVec<WORD_COUNT> {
    #[must_use]
    pub fn iter(&self) -> BitIterator<'_> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<const WORD_COUNT: usize> BitView<'_, WORD_COUNT> {
    #[must_use]
    pub fn iter(&self) -> BitIterator<'_> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<const WORD_COUNT: usize> MutableBitView<'_, WORD_COUNT> {
    #[must_use]
    pub fn iter(&self) -> BitIterator<'_> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<
        Other: BorrowAsBitIterator + Bitwise,
        T: NeutralElement<NeutralElementType = Self> + IndexAssignable + BorrowAsBitIterator,
    > FromBits<Other> for T
{
    fn from_bits(other: &Other) -> Self {
        let iter = other.borrow_as_bit_iterator();
        let mut res = Self::neutral_element_of_size(iter.len());
        for index in other.support() {
            res.assign_index(index, true);
        }
        res
    }
}
