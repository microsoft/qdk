use super::{
    Bitwise, BitwiseBinaryOps, BitwiseNeutralElement, BorrowAsBitIterator, Dot, IndexAssignable,
    OverlapWeight,
};
use crate::bits::bitvec::block_count;
use crate::NeutralElement;
use sorted_iter::{assume::AssumeSortedByItemExt, SortedIterator};

// Helper functions

// TODO: See of ToOwned and Borrow, BorrowMut traits can be used instead of RefsAndValues and RefsMut

pub fn support_iterator(iter: impl Iterator<Item = bool>) -> impl SortedIterator<Item = usize> {
    iter.enumerate()
        .filter(|pair| pair.1)
        .map(|pair| pair.0)
        .assume_sorted_by_item()
}

pub struct ExactZipIter<U: Iterator, V: Iterator> {
    iter1: U,
    iter2: V,
}

impl<U: Iterator, V: Iterator> Iterator for ExactZipIter<U, V> {
    type Item = (U::Item, V::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let i1 = self.iter1.next();
        let i2 = self.iter2.next();
        match (i1, i2) {
            (None, None) => None,
            (None, Some(_)) => panic!("Iterators of different length. The second one is longer."),
            (Some(_), None) => panic!("Iterators of different length. The first one is longer"),
            (Some(a), Some(b)) => Some((a, b)),
        }
    }
}

pub fn exact_zip<U: Iterator, V: Iterator>(iter1: U, iter2: V) -> ExactZipIter<U, V> {
    ExactZipIter { iter1, iter2 }
}

pub fn are_sorted_iter_equal(
    iterator1: impl SortedIterator<Item = usize>,
    iterator2: impl SortedIterator<Item = usize>,
) -> bool {
    let mut iter1 = iterator1;
    let mut iter2 = iterator2;
    let mut i1 = iter1.next();
    let mut i2 = iter2.next();
    loop {
        match (i1, i2) {
            (None, None) => return true,
            (None, Some(_)) | (Some(_), None) => return false,
            (Some(val1), Some(val2)) => {
                if val1 != val2 {
                    return false;
                }
            }
        }
        i1 = iter1.next();
        i2 = iter2.next();
    }
}

pub fn are_supports_equal(bitwise1: &impl Bitwise, bitwise2: &impl Bitwise) -> bool {
    are_sorted_iter_equal(bitwise1.support(), bitwise2.support())
}

// Uniform refs and values

pub trait RefsAndValues {
    type Ref<'life>
    where
        Self: 'life;
    type Value;

    fn value(&self) -> Self::Value;
    fn as_ref(&self) -> Self::Ref<'_>;
}

pub trait MutRefs {
    type MutRef<'life>
    where
        Self: 'life;
    fn as_ref_mut(&mut self) -> Self::MutRef<'_>;
}

pub trait BitsPerBlock {
    const BITS_PER_BLOCK: usize;
}

pub trait IntoBitUIntIterator
where
    Self: Sized,
{
    type BitUIntIterator: Iterator<Item = bool>;
    type BitUIntSliceIterator<'life>: Iterator<Item = bool>
    where
        Self: 'life;
    fn from_value(value: Self) -> Self::BitUIntIterator;
    fn from_slice(slice: &[Self]) -> Self::BitUIntSliceIterator<'_>;
}

pub trait UnsignedIntTag {}

pub trait UnsignedIntArrayTag {}

macro_rules! refs_and_values_body_val {
    ($type:ty) => {
        type Ref<'life> = &'life $type;
        type Value = $type;

        #[inline]
        fn value(&self) -> Self::Value {
            *self
        }

        #[inline]
        fn as_ref(&self) -> Self::Ref<'_> {
            self
        }
    };
}

macro_rules! refs_and_values_body_ref {
    ($type:ty) => {
        type Ref<'life>
            = &'life $type
        where
            Self: 'life;
        type Value = $type;

        #[inline]
        fn value(&self) -> Self::Value {
            **self
        }

        #[inline]
        fn as_ref(&self) -> Self::Ref<'_> {
            *self
        }
    };
}

macro_rules! mut_ref_body {
    ($type:ty) => {
        type MutRef<'life>
            = &'life mut $type
        where
            Self: 'life;

        #[inline]
        fn as_ref_mut(&mut self) -> Self::MutRef<'_> {
            self
        }
    };
}

macro_rules! bits_per_block_body_uint {
    ($type:ty) => {
        const BITS_PER_BLOCK: usize = <$type>::BITS as usize;
    };
}

macro_rules! bits_per_block_body_uint_arr {
    ($type:ty) => {
        const BITS_PER_BLOCK: usize = (<$type>::BITS as usize) * WORD_COUNT;
    };
}

macro_rules! refs_and_values {
    ($type:ty) => {
        impl RefsAndValues for $type {
            refs_and_values_body_val!($type);
        }
        impl RefsAndValues for &$type {
            refs_and_values_body_ref!($type);
        }
        impl RefsAndValues for &mut $type {
            refs_and_values_body_ref!($type);
        }

        impl UnsignedIntTag for $type {}

        impl MutRefs for $type {
            mut_ref_body!($type);
        }
        impl MutRefs for &mut $type {
            mut_ref_body!($type);
        }
        impl BitsPerBlock for $type {
            bits_per_block_body_uint!($type);
        }
        impl BitsPerBlock for &$type {
            bits_per_block_body_uint!($type);
        }
        impl BitsPerBlock for &mut $type {
            bits_per_block_body_uint!($type);
        }
    };
}

macro_rules! refs_and_values_arrays {
    ($type:ty) => {
        impl<const WORD_COUNT: usize> UnsignedIntArrayTag for [$type; WORD_COUNT] {}

        impl<const WORD_COUNT: usize> RefsAndValues for [$type; WORD_COUNT] {
            refs_and_values_body_val!([$type; WORD_COUNT]);
        }
        impl<const WORD_COUNT: usize> RefsAndValues for &[$type; WORD_COUNT] {
            refs_and_values_body_ref!([$type; WORD_COUNT]);
        }
        impl<const WORD_COUNT: usize> RefsAndValues for &mut [$type; WORD_COUNT] {
            refs_and_values_body_ref!([$type; WORD_COUNT]);
        }
        impl<const WORD_COUNT: usize> MutRefs for [$type; WORD_COUNT] {
            mut_ref_body!([$type; WORD_COUNT]);
        }
        impl<const WORD_COUNT: usize> MutRefs for &mut [$type; WORD_COUNT] {
            mut_ref_body!([$type; WORD_COUNT]);
        }
        impl<const WORD_COUNT: usize> BitsPerBlock for [$type; WORD_COUNT] {
            bits_per_block_body_uint_arr!($type);
        }
        impl<const WORD_COUNT: usize> BitsPerBlock for &[$type; WORD_COUNT] {
            bits_per_block_body_uint_arr!($type);
        }
        impl<const WORD_COUNT: usize> BitsPerBlock for &mut [$type; WORD_COUNT] {
            bits_per_block_body_uint_arr!($type);
        }
    };
}

refs_and_values!(u16);
refs_and_values!(u32);
refs_and_values!(u64);
refs_and_values!(u128);
refs_and_values_arrays!(u16);
refs_and_values_arrays!(u32);
refs_and_values_arrays!(u64);
refs_and_values_arrays!(u128);

// Bit traits for unsigned ints

macro_rules! bit_iterator_for_unsigned_int {
    ($name:ident, $word_type:ty) => {
        pub struct $name {
            word_mask: $word_type,
            word: $word_type,
        }

        impl $name {
            #[must_use]
            pub fn from_bits(word: &$word_type) -> $name {
                $name {
                    word_mask: 1 as $word_type,
                    word: *word,
                }
            }
        }

        impl Iterator for $name {
            type Item = bool;

            fn next(&mut self) -> Option<Self::Item> {
                if self.word_mask != 0 {
                    let value = self.word & self.word_mask == self.word_mask;
                    self.word_mask <<= 1;
                    Some(value)
                } else {
                    None
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                let size = <$word_type>::BITS as usize;
                (size, Some(size))
            }
        }

        impl ExactSizeIterator for $name {
            fn len(&self) -> usize {
                <$word_type>::BITS as usize
            }
        }
    };
}

macro_rules! bitwise_for_unsigned_int {
    ($word_type:ty) => {
        impl Bitwise for $word_type {
            #[inline]
            fn index(&self, index: usize) -> bool {
                assert!(index < (<Self as RefsAndValues>::Value::BITS as usize));
                let mask = (1 as <Self as RefsAndValues>::Value) << index;
                mask & *self.as_ref() == mask
            }

            #[inline]
            fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize> {
                support_iterator(self.borrow_as_bit_iterator())
            }

            #[inline]
            fn weight(&self) -> usize {
                self.as_ref().count_ones() as usize
            }

            #[inline]
            fn parity(&self) -> bool {
                self.as_ref().count_ones() & 1 == 1
            }

            #[inline]
            fn is_zero(&self) -> bool {
                *self.as_ref() == (0 as <Self as RefsAndValues>::Value)
            }
        }

        impl BorrowAsBitIterator for $word_type {
            type BitIterator<'life>
                = <<Self as RefsAndValues>::Value as IntoBitUIntIterator>::BitUIntIterator
            where
                Self: 'life;
            fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
                <<Self as RefsAndValues>::Value as IntoBitUIntIterator>::from_value((*self).value())
            }
        }
    };
}

// impl<const WORD_COUNT:usize> IntoBitIterator for &mut [u64;WORD_COUNT] {
//     type BitIterator<'life> = <u64 as IntoBitUIntIterator>::BitUIntSliceIterator<'life> where Self:'life;

//     fn into_bit_iterator(&self) -> Self::BitIterator<'_> {
//         <u64 as IntoBitUIntIterator>::from_slice(self.as_slice())
//     }
// }

macro_rules! dot_for_unsigned_int {
    ($word_type1:ty, $word_type2:ty) => {
        impl Dot<$word_type1> for $word_type2 {
            #[inline]
            fn dot(&self, other: &$word_type1) -> bool {
                (*self.as_ref() & *other.as_ref()).count_ones() & 1 == 1
            }
        }

        impl OverlapWeight<$word_type1> for $word_type2 {
            #[inline]
            fn and_weight(&self, other: &$word_type1) -> usize {
                (*self.as_ref() & *other.as_ref()).count_ones() as usize
            }

            fn or_weight(&self, other: &$word_type1) -> usize {
                (*self.as_ref() | *other.as_ref()).count_ones() as usize
            }
        }
    };
}

macro_rules! dot_for_unsigned_int_with_refs {
    ($word_type1:ty, $word_type2:ty) => {
        dot_for_unsigned_int!($word_type1, $word_type2);
        dot_for_unsigned_int!($word_type1, &$word_type2);
        dot_for_unsigned_int!($word_type1, &mut $word_type2);
    };
}

macro_rules! dot_variations_for_unsigned_int_with_refs {
    ($word_type1:ty) => {
        dot_for_unsigned_int_with_refs!($word_type1, $word_type1);
        dot_for_unsigned_int_with_refs!(&$word_type1, $word_type1);
        dot_for_unsigned_int_with_refs!(&mut $word_type1, $word_type1);
    };
}

macro_rules! index_assignable_for_unsigned_int {
    ($word_type:ty) => {
        impl IndexAssignable for $word_type {
            #[inline]
            fn assign_index(&mut self, index: usize, to: bool) {
                assert!(index < (<$word_type as RefsAndValues>::Value::BITS as usize));
                let mask = (1 as <$word_type as RefsAndValues>::Value) << index;
                if to {
                    *self.as_ref_mut() |= mask;
                } else {
                    *self.as_ref_mut() &= !mask;
                }
            }

            #[inline]
            fn negate_index(&mut self, index: usize) {
                assert!(index < (<$word_type as RefsAndValues>::Value::BITS as usize));
                *self.as_ref_mut() ^= (1 as <$word_type as RefsAndValues>::Value) << index;
            }

            #[inline]
            fn clear_bits(&mut self) {
                *self.as_ref_mut() = (0 as <$word_type as RefsAndValues>::Value);
            }
        }
    };
}

macro_rules! neutral_element_for_unsigned_int {
    ($word_type:ty) => {
        impl NeutralElement for $word_type {
            type NeutralElementType = <Self as RefsAndValues>::Value;

            #[inline]
            fn neutral_element(&self) -> <Self as NeutralElement>::NeutralElementType {
                0 as Self::NeutralElementType
            }

            #[inline]
            fn default_size_neutral_element() -> <Self as NeutralElement>::NeutralElementType {
                0 as Self::NeutralElementType
            }

            fn neutral_element_of_size(
                size: usize,
            ) -> <Self as NeutralElement>::NeutralElementType {
                assert!(size <= Self::BITS_PER_BLOCK);
                Self::default_size_neutral_element()
            }
        }

        impl BitwiseNeutralElement for $word_type {}
    };
}

macro_rules! bitwise_binary_for_unsigned_int {
    ($word_type1:ty, $word_type2:ty) => {
        impl BitwiseBinaryOps<$word_type2> for $word_type1 {
            #[inline]
            fn assign(&mut self, other: &$word_type2) {
                *self.as_ref_mut() = *other.as_ref();
            }

            #[inline]
            fn bitxor_assign(&mut self, other: &$word_type2) {
                *self.as_ref_mut() ^= *other.as_ref();
            }

            #[inline]
            fn bitand_assign(&mut self, other: &$word_type2) {
                *self.as_ref_mut() &= *other.as_ref();
            }
        }
    };
}

macro_rules! bitwise_binary_for_unsigned_int_and_refs {
    ($word_type1:ty, $word_type2:ty) => {
        bitwise_binary_for_unsigned_int!($word_type1, $word_type2);
        bitwise_binary_for_unsigned_int!($word_type1, &$word_type2);
        bitwise_binary_for_unsigned_int!($word_type1, &mut $word_type2);
    };
}

macro_rules! bit_traits_for_uint {
    ($iterator_name:ident, $word_type:ty) => {
        bitwise_for_unsigned_int!($word_type);
        bitwise_for_unsigned_int!(&$word_type);
        bitwise_for_unsigned_int!(&mut $word_type);
        dot_variations_for_unsigned_int_with_refs!($word_type);
        index_assignable_for_unsigned_int!($word_type);
        index_assignable_for_unsigned_int!(&mut $word_type);
        neutral_element_for_unsigned_int!($word_type);
        neutral_element_for_unsigned_int!(&$word_type);
        neutral_element_for_unsigned_int!(&mut $word_type);
        bitwise_binary_for_unsigned_int_and_refs!($word_type, $word_type);
        bitwise_binary_for_unsigned_int_and_refs!(&mut $word_type, $word_type);
    };
}

bit_traits_for_uint!(BitIteratorU16, u16);
bit_traits_for_uint!(BitIteratorU32, u32);
bit_traits_for_uint!(BitIteratorU64, u64);
bit_traits_for_uint!(BitIteratorU128, u128);

// Bit traits for fixed sized arrays
// array helper functions

#[inline]
fn array_weight<const WORD_COUNT: usize, T: Bitwise>(array: &[T; WORD_COUNT]) -> usize {
    array.iter().map(super::Bitwise::weight).sum()
}

#[inline]
fn array_parity<const WORD_COUNT: usize, T: Bitwise>(array: &[T; WORD_COUNT]) -> bool {
    array.iter().fold(false, |parity, x| parity ^ x.parity())
}

#[inline]
fn array_is_zero<const WORD_COUNT: usize, T: Bitwise>(array: &[T; WORD_COUNT]) -> bool {
    array.iter().all(super::Bitwise::is_zero)
}

#[inline]
fn array_clear<const WORD_COUNT: usize, T: Bitwise + IndexAssignable>(array: &mut [T; WORD_COUNT]) {
    for element in array.iter_mut() {
        element.clear_bits();
    }
}

#[inline]
fn block_and_bit_index<T: BitsPerBlock>(index: usize) -> (usize, usize) {
    let block_index = index / T::BITS_PER_BLOCK;
    let bit_index = index % T::BITS_PER_BLOCK;
    (block_index, bit_index)
}

/// # Safety
/// Does not check if index is out of bounds
#[inline]
pub unsafe fn array_get_unchecked<const WORD_COUNT: usize, T: BitsPerBlock + Bitwise>(
    array: &[T; WORD_COUNT],
    index: usize,
) -> bool {
    let (block_index, bit_index) = block_and_bit_index::<T>(index);
    array.get_unchecked(block_index).index(bit_index)
}

/// # Safety
/// Does not check if index is out of bounds
#[inline]
pub unsafe fn array_set_unchecked<const WORD_COUNT: usize, T: BitsPerBlock + IndexAssignable>(
    array: &mut [T; WORD_COUNT],
    index: usize,
    to: bool,
) {
    let (block_index, bit_index) = block_and_bit_index::<T>(index);
    array
        .get_unchecked_mut(block_index)
        .assign_index(bit_index, to);
}

#[inline]
unsafe fn array_bitxor_unchecked<const WORD_COUNT: usize, T: BitsPerBlock + IndexAssignable>(
    array: &mut [T; WORD_COUNT],
    index: usize,
) {
    let (block_index, bit_index) = block_and_bit_index::<T>(index);
    array.get_unchecked_mut(block_index).negate_index(bit_index);
}

#[inline]
pub fn array_dot<const WORD_COUNT: usize, T: Dot>(
    array1: &[T; WORD_COUNT],
    array2: &[T; WORD_COUNT],
) -> bool {
    let mut parity = false;
    for j in 0..WORD_COUNT {
        parity ^= unsafe { array1.get_unchecked(j).dot(array2.get_unchecked(j)) };
    }
    parity
}

#[inline]
pub fn array_and_weight<const WORD_COUNT: usize, T: OverlapWeight>(
    array1: &[T; WORD_COUNT],
    array2: &[T; WORD_COUNT],
) -> usize {
    let mut weight = 0usize;
    for j in 0..WORD_COUNT {
        weight += unsafe { array1.get_unchecked(j).and_weight(array2.get_unchecked(j)) };
    }
    weight
}

#[inline]
pub fn array_or_weight<const WORD_COUNT: usize, T: OverlapWeight>(
    array1: &[T; WORD_COUNT],
    array2: &[T; WORD_COUNT],
) -> usize {
    let mut weight = 0usize;
    for j in 0..WORD_COUNT {
        weight += unsafe { array1.get_unchecked(j).or_weight(array2.get_unchecked(j)) };
    }
    weight
}

// Bitwise, IndexAssignable, Dot and BitwiseBinaryOps for [Word;WORD_COUNT]

macro_rules! dot_for_unsigned_int_array {
    ($arr_type1:ty, $arr_type2:ty) => {
        impl<const WORD_COUNT: usize> Dot<$arr_type2> for $arr_type1 {
            fn dot(&self, other: &$arr_type2) -> bool {
                array_dot(RefsAndValues::as_ref(self), RefsAndValues::as_ref(other))
            }
        }

        impl<const WORD_COUNT: usize> OverlapWeight<$arr_type2> for $arr_type1 {
            fn and_weight(&self, other: &$arr_type2) -> usize {
                array_and_weight(RefsAndValues::as_ref(self), RefsAndValues::as_ref(other))
            }

            fn or_weight(&self, other: &$arr_type2) -> usize {
                array_or_weight(RefsAndValues::as_ref(self), RefsAndValues::as_ref(other))
            }
        }
    };
}

macro_rules! dot_for_unsigned_int_arr_with_refs {
    ($arr_type1:ty, $arr_type2:ty) => {
        dot_for_unsigned_int_array!($arr_type1, $arr_type2);
        dot_for_unsigned_int_array!($arr_type1, &$arr_type2);
        dot_for_unsigned_int_array!($arr_type1, &mut $arr_type2);
    };
}

macro_rules! dot_variations_for_unsigned_int_arr_with_refs {
    ($arr_type1:ty) => {
        dot_for_unsigned_int_arr_with_refs!($arr_type1, $arr_type1);
        dot_for_unsigned_int_arr_with_refs!(&$arr_type1, $arr_type1);
        dot_for_unsigned_int_arr_with_refs!(&mut $arr_type1, $arr_type1);
    };
}

macro_rules! bitwise_for_unsigned_int_arr {
    ($arr_type1:ty,$word_type:ty) => {
        impl<const WORD_COUNT: usize> Bitwise for $arr_type1 {
            #[inline]
            fn index(&self, index: usize) -> bool {
                assert!(index < <Self as BitsPerBlock>::BITS_PER_BLOCK);
                unsafe { array_get_unchecked((RefsAndValues::as_ref(self)), index) }
            }

            #[inline]
            fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize> {
                support_iterator(self.borrow_as_bit_iterator())
            }

            #[inline]
            fn weight(&self) -> usize {
                array_weight(RefsAndValues::as_ref(self))
            }

            #[inline]
            fn parity(&self) -> bool {
                array_parity(RefsAndValues::as_ref(self))
            }

            #[inline]
            fn is_zero(&self) -> bool {
                array_is_zero(RefsAndValues::as_ref(self))
            }
        }

        impl<const WORD_COUNT: usize> BorrowAsBitIterator for $arr_type1 {
            type BitIterator<'life>
                = <$word_type as IntoBitUIntIterator>::BitUIntSliceIterator<'life>
            where
                Self: 'life;

            fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
                <$word_type as IntoBitUIntIterator>::from_slice(self.as_slice())
            }
        }
    };
}

macro_rules! index_assignable_for_unsigned_int_arr {
    ($arr_type1:ty) => {
        impl<const WORD_COUNT: usize> IndexAssignable for $arr_type1 {
            #[inline]
            fn assign_index(&mut self, index: usize, to: bool) {
                assert!(index < <Self as BitsPerBlock>::BITS_PER_BLOCK);
                unsafe { array_set_unchecked(MutRefs::as_ref_mut(self), index, to) }
            }

            #[inline]
            fn negate_index(&mut self, index: usize) {
                assert!(index < <Self as BitsPerBlock>::BITS_PER_BLOCK);
                unsafe { array_bitxor_unchecked(MutRefs::as_ref_mut(self), index) }
            }

            #[inline]
            fn clear_bits(&mut self) {
                array_clear(MutRefs::as_ref_mut(self));
            }
        }
    };
}

macro_rules! bitwise_binary_ops_for_unsigned_int_array {
    ($arr_type1:ty, $arr_type2:ty) => {
        impl<const WORD_COUNT: usize> BitwiseBinaryOps<$arr_type2> for $arr_type1 {
            #[inline]
            fn assign(&mut self, other: &$arr_type2) {
                for j in 0..WORD_COUNT {
                    unsafe {
                        *MutRefs::as_ref_mut(self).get_unchecked_mut(j) =
                            *RefsAndValues::as_ref(other).get_unchecked(j)
                    }
                }
            }

            #[inline]
            fn bitxor_assign(&mut self, other: &$arr_type2) {
                for j in 0..WORD_COUNT {
                    unsafe {
                        *MutRefs::as_ref_mut(self).get_unchecked_mut(j) ^=
                            *RefsAndValues::as_ref(other).get_unchecked(j)
                    }
                }
            }

            #[inline]
            fn bitand_assign(&mut self, other: &$arr_type2) {
                for j in 0..WORD_COUNT {
                    unsafe {
                        *MutRefs::as_ref_mut(self).get_unchecked_mut(j) &=
                            *RefsAndValues::as_ref(other).get_unchecked(j)
                    }
                }
            }
        }
    };
}

macro_rules! neutral_element_for_unsigned_int_array {
    ($array_type:ty) => {
        impl<const WORD_COUNT: usize> NeutralElement for $array_type {
            type NeutralElementType = <Self as RefsAndValues>::Value;

            fn neutral_element(&self) -> Self::NeutralElementType {
                Self::default_size_neutral_element()
            }

            fn default_size_neutral_element() -> Self::NeutralElementType {
                [0; WORD_COUNT]
            }

            fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
                assert!(size <= Self::BITS_PER_BLOCK);
                Self::default_size_neutral_element()
            }
        }

        impl<const WORD_COUNT: usize> BitwiseNeutralElement for $array_type {}
    };
}

macro_rules! bitwise_binary_ops_for_unsigned_int_array_with_refs {
    ($arr_type1:ty, $arr_type2:ty) => {
        bitwise_binary_ops_for_unsigned_int_array!($arr_type1, $arr_type2);
        bitwise_binary_ops_for_unsigned_int_array!($arr_type1, &$arr_type2);
        bitwise_binary_ops_for_unsigned_int_array!($arr_type1, &mut $arr_type2);
    };
}

macro_rules! bit_traits_for_arrays_of_unsigned_ints {
    ($word_type:ty) => {
        dot_variations_for_unsigned_int_arr_with_refs!([$word_type; WORD_COUNT]);
        bitwise_for_unsigned_int_arr!([$word_type; WORD_COUNT], $word_type);
        bitwise_for_unsigned_int_arr!(&[$word_type; WORD_COUNT], $word_type);
        bitwise_for_unsigned_int_arr!(&mut [$word_type; WORD_COUNT], $word_type);
        index_assignable_for_unsigned_int_arr!([$word_type; WORD_COUNT]);
        index_assignable_for_unsigned_int_arr!(&mut [$word_type; WORD_COUNT]);
        bitwise_binary_ops_for_unsigned_int_array_with_refs!(
            [$word_type; WORD_COUNT],
            [$word_type; WORD_COUNT]
        );
        bitwise_binary_ops_for_unsigned_int_array_with_refs!(
            &mut [$word_type; WORD_COUNT],
            [$word_type; WORD_COUNT]
        );
        neutral_element_for_unsigned_int_array!([$word_type; WORD_COUNT]);
        neutral_element_for_unsigned_int_array!(&[$word_type; WORD_COUNT]);
        neutral_element_for_unsigned_int_array!(&mut [$word_type; WORD_COUNT]);
    };
}

bit_traits_for_arrays_of_unsigned_ints!(u16);
bit_traits_for_arrays_of_unsigned_ints!(u32);
bit_traits_for_arrays_of_unsigned_ints!(u64);
bit_traits_for_arrays_of_unsigned_ints!(u128);

// BitIterator for slices of unsigned ints

macro_rules! bit_iterator_for_unsigned_int_slice {
    ($name:ident, $word_type:ty) => {
        pub struct $name<'life> {
            word_index: usize,
            word_mask: $word_type,
            bits: &'life [$word_type],
        }

        impl<'life> $name<'life> {
            #[must_use]
            pub fn from_bits(bits: &'life [$word_type]) -> $name<'life> {
                $name {
                    word_index: 0,
                    word_mask: 1,
                    bits,
                }
            }
        }

        impl<'life> Iterator for $name<'life> {
            type Item = bool;

            fn next(&mut self) -> Option<Self::Item> {
                const LAST_BIT_MASK: $word_type = (1 as $word_type) << (<$word_type>::BITS - 1);
                if self.word_index < self.bits.len() {
                    let value = (*unsafe { self.bits.get_unchecked(self.word_index) })
                        & self.word_mask
                        == self.word_mask;
                    if self.word_mask == LAST_BIT_MASK {
                        self.word_mask = 1;
                        self.word_index += 1;
                    } else {
                        self.word_mask <<= 1;
                    }
                    return Some(value);
                }
                None
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                let size = (<$word_type>::BITS as usize) * self.bits.len();
                (size, Some(size))
            }
        }

        impl<'life> ExactSizeIterator for $name<'life> {
            fn len(&self) -> usize {
                (<$word_type>::BITS as usize) * self.bits.len()
            }
        }
    };
}

macro_rules! into_bit_uint_iterator {
    ($word_type:ty, $bit_iterator_name:ident, $bit_slice_iterator_name:ident) => {
        bit_iterator_for_unsigned_int!($bit_iterator_name, $word_type);
        bit_iterator_for_unsigned_int_slice!($bit_slice_iterator_name, $word_type);

        impl IntoBitUIntIterator for $word_type {
            type BitUIntIterator = $bit_iterator_name;
            type BitUIntSliceIterator<'life> = $bit_slice_iterator_name<'life>;

            fn from_value(value: Self) -> Self::BitUIntIterator {
                Self::BitUIntIterator::from_bits(&value)
            }

            fn from_slice(slice: &[Self]) -> Self::BitUIntSliceIterator<'_> {
                Self::BitUIntSliceIterator::from_bits(&slice)
            }
        }
    };
}

into_bit_uint_iterator!(u16, BitIteratorU16, BitIteratorArrU16);
into_bit_uint_iterator!(u32, BitIteratorU32, BitIteratorArrU32);
into_bit_uint_iterator!(u64, BitIteratorU64, BitIterator);
into_bit_uint_iterator!(u128, BitIteratorU128, BitIteratorArrU128);

fn slice_weight<T: Bitwise>(slice: &[T]) -> usize {
    let mut res = 0usize;
    for elt in slice {
        res += elt.weight();
    }
    res
}

fn slice_parity<T: Bitwise>(slice: &[T]) -> bool {
    let mut res = false;
    for elt in slice {
        res ^= elt.parity();
    }
    res
}

fn slice_is_zero<T: Bitwise>(slice: &[T]) -> bool {
    for elt in slice {
        if !elt.is_zero() {
            return false;
        }
    }
    true
}

macro_rules! bitwise_body_for_vec {
    ($elt_type:ty) => {
        fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize> {
            support_iterator(self.borrow_as_bit_iterator())
        }

        fn index(&self, index: usize) -> bool {
            let (block_index, bit_index) = block_and_bit_index::<$elt_type>(index);
            self[block_index].index(bit_index)
        }

        fn weight(&self) -> usize {
            slice_weight(self)
        }

        fn parity(&self) -> bool {
            slice_parity(self)
        }

        fn is_zero(&self) -> bool {
            slice_is_zero(self)
        }
    };
}

macro_rules! bitwise_for_vec {
    ($vec_type:ty, $vec_arr_type:ty, $word_type:ty, $arr_type:ty) => {
        impl<const WORD_COUNT: usize> Bitwise for $vec_arr_type {
            bitwise_body_for_vec!($arr_type);
        }

        impl Bitwise for $vec_type {
            bitwise_body_for_vec!($word_type);
        }

        impl BorrowAsBitIterator for $vec_type {
            type BitIterator<'life>
                = <$word_type as IntoBitUIntIterator>::BitUIntSliceIterator<'life>
            where
                Self: 'life;

            fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
                <$word_type as IntoBitUIntIterator>::from_slice(self)
            }
        }

        impl<const WORD_COUNT: usize> BorrowAsBitIterator for $vec_arr_type {
            type BitIterator<'life>
                = <$word_type as IntoBitUIntIterator>::BitUIntSliceIterator<'life>
            where
                Self: 'life;

            fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
                <$word_type as IntoBitUIntIterator>::from_slice(self.as_flattened())
            }
        }
    };
}

// impl IntoBitIterator for Vec<u64> {
//     type BitIterator<'life> = <u64 as IntoBitUIntIterator>::BitUIntSliceIterator<'life>  where Self:'life;

//     fn into_bit_iterator(&self) -> Self::BitIterator<'_> {
//         <u64 as IntoBitUIntIterator>::from_slice(self)
//     }
// }

macro_rules! bitwise_for_vec_variation {
    ($word_type:ty) => {
        bitwise_for_vec!(
            Vec<$word_type>,
            Vec<[$word_type; WORD_COUNT]>,
            $word_type,
            [$word_type; WORD_COUNT]
        );
        bitwise_for_vec!(
            &[$word_type],
            &[[$word_type; WORD_COUNT]],
            $word_type,
            [$word_type; WORD_COUNT]
        );
        bitwise_for_vec!(
            &mut [$word_type],
            &mut [[$word_type; WORD_COUNT]],
            $word_type,
            [$word_type; WORD_COUNT]
        );
    };
}

bitwise_for_vec_variation!(u16);
bitwise_for_vec_variation!(u32);
bitwise_for_vec_variation!(u64);
bitwise_for_vec_variation!(u128);

macro_rules! dot_for_vec {
    ($vec_type1:ty, $vec_type2:ty) => {
        impl<T: Dot, U: Dot> Dot<$vec_type2> for $vec_type1
        where
            U: Dot<T>,
        {
            fn dot(&self, other: &$vec_type2) -> bool {
                assert!(self.len() == other.len());
                let mut res = false;
                for (a, b) in exact_zip(self.iter(), other.iter()) {
                    res ^= a.dot(b)
                }
                res
            }
        }

        impl<T: OverlapWeight, U: OverlapWeight> OverlapWeight<$vec_type2> for $vec_type1
        where
            U: OverlapWeight<T>,
        {
            fn and_weight(&self, other: &$vec_type2) -> usize {
                assert!(self.len() == other.len());
                let mut res = 0usize;
                for (a, b) in exact_zip(self.iter(), other.iter()) {
                    res += a.and_weight(b)
                }
                res
            }

            fn or_weight(&self, other: &$vec_type2) -> usize {
                assert!(self.len() == other.len());
                let mut res = 0usize;
                for (a, b) in exact_zip(self.iter(), other.iter()) {
                    res += a.or_weight(b)
                }
                res
            }
        }
    };
}

macro_rules! dot_for_vec_refs {
    ($vec_type1:ty) => {
        dot_for_vec!($vec_type1, Vec<T>);
        dot_for_vec!($vec_type1, &[T]);
        dot_for_vec!($vec_type1, &mut [T]);
    };
}

dot_for_vec_refs!(Vec<U>);
dot_for_vec_refs!(&[U]);
dot_for_vec_refs!(&mut [U]);

macro_rules! index_assignable_for_vec_body {
    ($word:ty) => {
        fn assign_index(&mut self, index: usize, to: bool) {
            let (block_index, bit_index) = block_and_bit_index::<$word>(index);
            self[block_index].assign_index(bit_index, to)
        }

        fn negate_index(&mut self, index: usize) {
            let (block_index, bit_index) = block_and_bit_index::<$word>(index);
            self[block_index].negate_index(bit_index)
        }

        fn clear_bits(&mut self) {
            for val in self.iter_mut() {
                val.clear_bits();
            }
        }
    };
}

macro_rules! index_assignable_for_vec {
    ($vec:ty, $vec_arr:ty, $word:ty) => {
        impl IndexAssignable for $vec {
            index_assignable_for_vec_body!($word);
        }
        impl<const WORD_COUNT: usize> IndexAssignable for $vec_arr {
            index_assignable_for_vec_body!([$word; WORD_COUNT]);
        }
    };
}

macro_rules! index_assignable_for_vec_with_refs {
    ($word:ty) => {
        index_assignable_for_vec!(Vec<$word>, Vec<[$word; WORD_COUNT]>, $word);
        index_assignable_for_vec!(&mut [$word], &mut [[$word; WORD_COUNT]], $word);
    };
}

index_assignable_for_vec_with_refs!(u16);
index_assignable_for_vec_with_refs!(u32);
index_assignable_for_vec_with_refs!(u64);
index_assignable_for_vec_with_refs!(u128);

macro_rules! bitwise_binary_ops_for_vec_body {
    ($vec:ty) => {
        fn assign(&mut self, other: &$vec) {
            assert!(self.len() == other.len());
            for (a, b) in exact_zip(self.iter_mut(), other.iter()) {
                a.assign(b);
            }
        }

        fn bitxor_assign(&mut self, other: &$vec) {
            assert!(self.len() == other.len());
            for (a, b) in exact_zip(self.iter_mut(), other.iter()) {
                a.bitxor_assign(b);
            }
        }

        fn bitand_assign(&mut self, other: &$vec) {
            assert!(self.len() == other.len());
            for (a, b) in exact_zip(self.iter_mut(), other.iter()) {
                a.bitand_assign(b);
            }
        }
    };
}

macro_rules! bitwise_binary_ops_for_vec {
    ($mut_vec:ty, $vec:ty) => {
        impl BitwiseBinaryOps<$vec> for $mut_vec {
            bitwise_binary_ops_for_vec_body!($vec);
        }
    };
}

macro_rules! bitwise_binary_ops_for_vec_arr {
    ($mut_vec:ty, $vec:ty) => {
        impl<const WORD_COUNT: usize> BitwiseBinaryOps<$vec> for $mut_vec {
            bitwise_binary_ops_for_vec_body!($vec);
        }
    };
}

macro_rules! bitwise_binary_ops_for_vec_refs {
    ($vec:ty,$vec_arr:ty,$inner_type:ty) => {
        bitwise_binary_ops_for_vec!($vec, Vec<$inner_type>);
        bitwise_binary_ops_for_vec!($vec, &[$inner_type]);
        bitwise_binary_ops_for_vec!($vec, &mut [$inner_type]);
        bitwise_binary_ops_for_vec_arr!($vec_arr, Vec<[$inner_type; WORD_COUNT]>);
        bitwise_binary_ops_for_vec_arr!($vec_arr, &[[$inner_type; WORD_COUNT]]);
        bitwise_binary_ops_for_vec_arr!($vec_arr, &mut [[$inner_type; WORD_COUNT]]);
    };
}

macro_rules! bitwise_binary_ops_for_vec_variations {
    ($word_type:ty) => {
        bitwise_binary_ops_for_vec_refs!(
            Vec<$word_type>,
            Vec<[$word_type; WORD_COUNT]>,
            $word_type
        );
        bitwise_binary_ops_for_vec_refs!(
            &mut [$word_type],
            &mut [[$word_type; WORD_COUNT]],
            $word_type
        );
    };
}

bitwise_binary_ops_for_vec_variations!(u16);
bitwise_binary_ops_for_vec_variations!(u32);
bitwise_binary_ops_for_vec_variations!(u64);
bitwise_binary_ops_for_vec_variations!(u128);

macro_rules! neutral_element_for_vec_body {
    ($inner:ty) => {
        type NeutralElementType = Vec<<$inner as NeutralElement>::NeutralElementType>;

        fn neutral_element(&self) -> Self::NeutralElementType {
            vec![<$inner as NeutralElement>::default_size_neutral_element(); self.len()]
        }

        fn default_size_neutral_element() -> Self::NeutralElementType {
            Self::NeutralElementType::new()
        }

        fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
            let bits_per_block: usize =
                <$inner as NeutralElement>::NeutralElementType::BITS_PER_BLOCK;
            vec![
                <$inner as NeutralElement>::default_size_neutral_element();
                block_count(size, bits_per_block)
            ]
        }
    };
}

macro_rules! neutral_element_for_vec_refs {
    ($vec:ty, $vec_arr:ty, $inner:ty) => {
        impl NeutralElement for $vec {
            neutral_element_for_vec_body!($inner);
        }

        impl<const WORD_COUNT: usize> NeutralElement for $vec_arr {
            neutral_element_for_vec_body!([$inner; WORD_COUNT]);
        }

        impl BitwiseNeutralElement for $vec {}
        impl<const WORD_COUNT: usize> BitwiseNeutralElement for $vec_arr {}
    };
}

macro_rules! neutral_element_for_vec_variation {
    ($word:ty) => {
        neutral_element_for_vec_refs!(Vec<$word>, Vec<[$word; WORD_COUNT]>, $word);
        neutral_element_for_vec_refs!(&[$word], &[[$word; WORD_COUNT]], $word);
        neutral_element_for_vec_refs!(&mut [$word], &mut [[$word; WORD_COUNT]], $word);
    };
}

neutral_element_for_vec_variation!(u16);
neutral_element_for_vec_variation!(u32);
neutral_element_for_vec_variation!(u64);
neutral_element_for_vec_variation!(u128);

// Bit traits for Vec<bool>, &[bool], &mut[bool], [bool;SIZE]

macro_rules! implement_dot_array {
    () => {
        fn dot(&self, other: &T) -> bool {
            let mut res = false;
            for (a, b) in exact_zip(self.iter(), other.borrow_as_bit_iterator()) {
                res ^= a & b;
            }
            res
        }
    };
}

macro_rules! implement_overlap_weight_array {
    () => {
        fn and_weight(&self, other: &T) -> usize {
            let mut res = 0usize;
            for (a, b) in exact_zip(self.iter(), other.borrow_as_bit_iterator()) {
                if a & b {
                    res += 1;
                }
            }
            res
        }

        fn or_weight(&self, other: &T) -> usize {
            let mut res = 0usize;
            for (a, b) in exact_zip(self.iter(), other.borrow_as_bit_iterator()) {
                if a | b {
                    res += 1;
                }
            }
            res
        }
    };
}

macro_rules! implement_bitwise_array_body {
    () => {
        fn index(&self, index: usize) -> bool {
            self[index]
        }

        fn weight(&self) -> usize {
            self.iter().filter(|bit| **bit).count()
        }

        fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize> {
            sorted_iter::assume::AssumeSortedByItemExt::assume_sorted_by_item(
                self.iter()
                    .enumerate()
                    .filter(|pair| *pair.1)
                    .map(|pair| pair.0),
            )
        }
    };
}

macro_rules! borrow_as_bit_iterator_for_bool_body {
    () => {
        type BitIterator<'life>
            = std::iter::Copied<std::slice::Iter<'life, bool>>
        where
            Self: 'life;
        fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_> {
            self.iter().copied()
        }
    };
}

macro_rules! implement_bitwise_array_bool {
    ($array_type:ty) => {
        impl<const SIZE: usize> Bitwise for $array_type {
            implement_bitwise_array_body!();
        }

        impl<const SIZE: usize> BorrowAsBitIterator for $array_type {
            borrow_as_bit_iterator_for_bool_body!();
        }

        impl<const SIZE: usize> NeutralElement for $array_type {
            type NeutralElementType = [bool; SIZE];

            fn neutral_element(&self) -> Self::NeutralElementType {
                Self::default_size_neutral_element()
            }

            fn default_size_neutral_element() -> Self::NeutralElementType {
                [false; SIZE]
            }

            fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
                assert!(size <= SIZE);
                [false; SIZE]
            }
        }

        impl<const SIZE: usize> BitwiseNeutralElement for $array_type {}

        impl<T: BorrowAsBitIterator, const SIZE: usize> Dot<T> for $array_type {
            implement_dot_array!();
        }

        impl<T: BorrowAsBitIterator, const SIZE: usize> OverlapWeight<T> for $array_type {
            implement_overlap_weight_array!();
        }
    };
}

macro_rules! implement_bitwise_vec_bool {
    ($array_type:ty) => {
        impl Bitwise for $array_type {
            implement_bitwise_array_body!();
        }

        impl BorrowAsBitIterator for $array_type {
            borrow_as_bit_iterator_for_bool_body!();
        }

        impl NeutralElement for $array_type {
            type NeutralElementType = Vec<bool>;

            fn neutral_element(&self) -> Self::NeutralElementType {
                vec![false; self.len()]
            }

            fn default_size_neutral_element() -> Self::NeutralElementType {
                vec![]
            }

            fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
                vec![false; size]
            }
        }

        impl BitwiseNeutralElement for $array_type {}

        impl<T: BorrowAsBitIterator> Dot<T> for $array_type {
            implement_dot_array!();
        }

        impl<T: BorrowAsBitIterator> OverlapWeight<T> for $array_type {
            implement_overlap_weight_array!();
        }
    };
}

implement_bitwise_array_bool!([bool; SIZE]);
implement_bitwise_array_bool!(&[bool; SIZE]);
implement_bitwise_array_bool!(&mut [bool; SIZE]);
implement_bitwise_vec_bool!(Vec<bool>);
implement_bitwise_vec_bool!(&[bool]);
implement_bitwise_vec_bool!(&mut [bool]);

macro_rules! implement_index_assignable_array {
    ($right:ty) => {
        fn assign_index(&mut self, index: usize, to: bool) {
            self[index] = to;
        }

        fn negate_index(&mut self, index: usize) {
            self[index] ^= true;
        }

        fn clear_bits(&mut self) {
            for val in self.iter_mut() {
                *val = false;
            }
        }
    };
}

impl<const SIZE: usize> IndexAssignable for [bool; SIZE] {
    implement_index_assignable_array!(Self);
}

impl<const SIZE: usize> IndexAssignable for &mut [bool; SIZE] {
    implement_index_assignable_array!(Self);
}

impl IndexAssignable for Vec<bool> {
    implement_index_assignable_array!(Self);
}

impl IndexAssignable for &mut [bool] {
    implement_index_assignable_array!(Self);
}

macro_rules! bitwise_binary_ops_body_for_bool {
    () => {
        fn assign(&mut self, other: &T) {
            for (a, b) in exact_zip(self.iter_mut(), other.borrow_as_bit_iterator()) {
                *a = b
            }
        }

        fn bitxor_assign(&mut self, other: &T) {
            for (a, b) in exact_zip(self.iter_mut(), other.borrow_as_bit_iterator()) {
                *a ^= b
            }
        }

        fn bitand_assign(&mut self, other: &T) {
            for (a, b) in exact_zip(self.iter_mut(), other.borrow_as_bit_iterator()) {
                *a &= b
            }
        }
    };
}

macro_rules! bitwise_binary_ops_for_vec_bool {
    ($array_type:ty) => {
        impl<T: BorrowAsBitIterator + Bitwise> BitwiseBinaryOps<T> for $array_type {
            bitwise_binary_ops_body_for_bool!();
        }
    };
}

macro_rules! bitwise_binary_ops_for_arr_bool {
    ($array_type:ty) => {
        impl<T: BorrowAsBitIterator + Bitwise, const SIZE: usize> BitwiseBinaryOps<T>
            for $array_type
        {
            bitwise_binary_ops_body_for_bool!();
        }
    };
}

bitwise_binary_ops_for_vec_bool!(Vec<bool>);
bitwise_binary_ops_for_vec_bool!(&mut [bool]);
bitwise_binary_ops_for_arr_bool!([bool; SIZE]);
bitwise_binary_ops_for_arr_bool!(&mut [bool; SIZE]);
