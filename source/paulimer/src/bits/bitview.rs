// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::bitvec::WORD_COUNT_DEFAULT;
use super::standard_types::BitsPerBlock;
use super::BitVec;
use crate::bits::bitblock::Word;

#[derive(Eq, Hash)]
pub struct BitView<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    pub blocks: &'life [[Word; WORD_COUNT]],
}

// Should we use convention <TypeName>Mutable or Mutable<TypeName> ?
#[derive(Eq, Hash)]
pub struct MutableBitView<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    pub blocks: &'life mut [[Word; WORD_COUNT]],
}

impl<const WORD_COUNT: usize> BitView<'_, WORD_COUNT> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len() * <[Word; WORD_COUNT]>::BITS_PER_BLOCK
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.len() == 0
    }

    #[must_use]
    pub fn top(&self) -> u64 {
        self.blocks[0][0]
    }
}

impl<const WORD_COUNT: usize> MutableBitView<'_, WORD_COUNT> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len() * <[Word; WORD_COUNT]>::BITS_PER_BLOCK
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.len() == 0
    }

    #[must_use]
    pub fn top(&self) -> u64 {
        self.blocks[0][0]
    }

    pub fn top_mut(&mut self) -> &mut u64 {
        &mut self.blocks[0][0]
    }
}

impl<'life, const WORD_COUNT: usize> From<BitView<'life, WORD_COUNT>> for BitVec<WORD_COUNT> {
    fn from(value: BitView<'life, WORD_COUNT>) -> Self {
        Self::from_view(&value)
    }
}

impl<'life, const WORD_COUNT: usize> From<&'life BitVec<WORD_COUNT>> for BitVec<WORD_COUNT> {
    fn from(value: &'life BitVec<WORD_COUNT>) -> Self {
        value.clone()
    }
}

impl<'life, const WORD_COUNT: usize> From<MutableBitView<'life, WORD_COUNT>>
    for BitVec<WORD_COUNT>
{
    fn from(value: MutableBitView<'life, WORD_COUNT>) -> Self {
        Self::from_view_mut(&value)
    }
}
