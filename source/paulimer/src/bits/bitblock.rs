// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ops::{BitAnd, BitAndAssign, BitXor, BitXorAssign, Index};

use super::standard_types::{array_get_unchecked, array_set_unchecked, BitIterator};
use super::{Bitwise, BitwiseNeutralElement, IndexAssignable, WORD_COUNT_DEFAULT};
use crate::NeutralElement;

pub type Word = u64;

#[repr(C, align(64))]
#[derive(Eq, Clone, Debug, Hash, PartialEq)]
pub struct BitBlock<const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    pub blocks: [Word; WORD_COUNT],
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub struct BitAccessor<const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    word_index: usize,
    bitmask: Word,
}

impl<const WORD_COUNT: usize> BitBlock<WORD_COUNT> {
    pub const BITS: usize = WORD_COUNT * (Word::BITS as usize);

    #[must_use]
    pub fn zeros() -> Self {
        Self {
            blocks: [0; WORD_COUNT],
        }
    }

    #[must_use]
    pub fn ones() -> Self {
        Self {
            blocks: [Word::MAX; WORD_COUNT],
        }
    }

    #[must_use]
    pub const fn bits() -> usize {
        Self::BITS
    }

    #[must_use]
    pub fn array(&self) -> &[Word; WORD_COUNT] {
        &self.blocks
    }

    pub fn array_mut(&mut self) -> &mut [Word; WORD_COUNT] {
        &mut self.blocks
    }

    #[must_use]
    pub fn all(value: bool) -> Self {
        if value {
            Self::ones()
        } else {
            Self::zeros()
        }
    }

    /// # Panics
    ///
    /// Will panic  when array is too big
    #[must_use]
    pub fn from_array<const ARRAY_SIZE: usize>(bits: [bool; ARRAY_SIZE]) -> Self {
        assert!(ARRAY_SIZE <= Self::bits());
        let mut block = Self::zeros();
        for (index, bit) in bits.iter().enumerate() {
            block.set(index, *bit);
        }
        block
    }

    /// # Panics
    ///
    /// Will panic if index out of range
    #[must_use]
    pub fn get(&self, index: usize) -> bool {
        assert!(index < Self::BITS);
        unsafe { self.get_unchecked(index) }
    }

    /// # Safety
    /// Does not check if index is out of bounds
    #[must_use]
    pub unsafe fn get_unchecked(&self, index: usize) -> bool {
        array_get_unchecked(self.array(), index)
    }

    /// # Panics
    ///
    /// Will panic if index out of range
    pub fn set(&mut self, index: usize, to: bool) {
        assert!(index < Self::BITS);
        unsafe { self.set_unchecked(index, to) };
    }

    /// # Safety
    /// Does not check if index is out of bounds
    pub unsafe fn set_unchecked(&mut self, index: usize, to: bool) {
        array_set_unchecked(self.array_mut(), index, to);
    }

    #[must_use]
    pub fn parity(&self) -> bool {
        self.blocks.parity()
    }

    #[must_use]
    pub fn weight(&self) -> usize {
        self.blocks.weight()
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.blocks.is_zero()
    }

    #[must_use]
    pub fn iter(&self) -> BitIterator<'_> {
        BitIterator::from_bits(&self.blocks)
    }
}

impl<'life, const WORD_COUNT: usize> IntoIterator for &'life BitBlock<WORD_COUNT> {
    type Item = bool;
    type IntoIter = BitIterator<'life>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<const WORD_COUNT: usize> NeutralElement for BitBlock<WORD_COUNT> {
    type NeutralElementType = BitBlock<WORD_COUNT>;

    fn neutral_element(&self) -> <Self as NeutralElement>::NeutralElementType {
        Self::zeros()
    }

    fn default_size_neutral_element() -> <Self as NeutralElement>::NeutralElementType {
        Self::zeros()
    }

    fn neutral_element_of_size(size: usize) -> <Self as NeutralElement>::NeutralElementType {
        assert!(size <= BitBlock::<WORD_COUNT>::BITS);
        Self::default_size_neutral_element()
    }
}

impl<const WORD_COUNT: usize> BitwiseNeutralElement for BitBlock<WORD_COUNT> {}

impl Index<usize> for BitBlock {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if Bitwise::index(&self.blocks, index) {
            &true
        } else {
            &false
        }
    }
}

impl<const WORDCOUNT: usize> BitXorAssign<&BitBlock<WORDCOUNT>> for BitBlock<WORDCOUNT> {
    fn bitxor_assign(&mut self, other: &Self) {
        for index in 0..WORDCOUNT {
            self.blocks[index] ^= other.blocks[index];
        }
    }
}

impl<const WORDCOUNT: usize> BitXor for &BitBlock<WORDCOUNT> {
    type Output = BitBlock<WORDCOUNT>;

    fn bitxor(self, other: Self) -> Self::Output {
        let mut clone = (*self).clone();
        clone ^= other;
        clone
    }
}

impl<const WORDCOUNT: usize> BitAndAssign<&BitBlock<WORDCOUNT>> for BitBlock<WORDCOUNT> {
    fn bitand_assign(&mut self, other: &Self) {
        for index in 0..WORDCOUNT {
            self.blocks[index] &= other.blocks[index];
        }
    }
}

impl<const WORDCOUNT: usize> BitAnd for &BitBlock<WORDCOUNT> {
    type Output = BitBlock<WORDCOUNT>;

    fn bitand(self, other: Self) -> Self::Output {
        let mut clone = (*self).clone();
        clone &= other;
        clone
    }
}

impl<const WORD_COUNT: usize> FromIterator<bool> for BitBlock<WORD_COUNT> {
    fn from_iter<Iterator: IntoIterator<Item = bool>>(iterator: Iterator) -> Self {
        let mut res: BitBlock<WORD_COUNT> = BitBlock::<WORD_COUNT>::default_size_neutral_element();
        for (index, bit) in iterator.into_iter().enumerate() {
            res.assign_index(index, bit);
        }
        res
    }
}

// Bit accessor for [Word; WORDCOUNT]

impl<const WORDCOUNT: usize> BitAccessor<WORDCOUNT> {
    /// # Panics
    ///
    /// Will panic index is out of range
    #[must_use]
    pub fn for_index(index: usize) -> Self {
        assert!(index < BitBlock::<WORDCOUNT>::BITS);
        unsafe { Self::for_index_unchecked(index) }
    }

    /// # Safety
    /// Does not check if index is out of bounds
    #[must_use]
    pub unsafe fn for_index_unchecked(index: usize) -> Self {
        let word_index = index / (Word::BITS as usize);
        let bit_index = index % (Word::BITS as usize);
        Self {
            word_index,
            bitmask: 1 << bit_index,
        }
    }

    #[must_use]
    pub fn array_value_of(&self, block: &[Word; WORDCOUNT]) -> bool {
        let word = unsafe { block.get_unchecked(self.word_index) };
        (*word & self.bitmask) != 0
    }

    pub fn array_bitxor(&self, block: &mut [Word; WORDCOUNT]) {
        let word: &mut Word = unsafe { block.get_unchecked_mut(self.word_index) };
        *word ^= self.bitmask;
    }

    pub fn array_set_value_of(&self, block: &mut [Word; WORDCOUNT], to: bool) {
        let word = unsafe { block.get_unchecked_mut(self.word_index) };
        // let mask = !self.bitmask;
        // let bit_index = mask.trailing_ones();
        // let bit_value = (to as Word) << bit_index;
        // *word &= mask;
        // *word |= bit_value;
        if to {
            *word |= self.bitmask;
        } else {
            *word &= !self.bitmask;
        }
    }
}
