pub mod bitblock;

pub mod standard_types;
pub use standard_types::are_supports_equal;

mod bitvec;
pub use bitvec::{BitVec, WORD_COUNT_DEFAULT};

mod bitview;
pub use bitview::{BitView, MutableBitView};

mod index_set;
pub use index_set::{remapped, IndexSet};

pub mod bitmatrix;
pub use bitmatrix::BitMatrix;

// Unary traits, involve only one type
use crate::NeutralElement;

pub trait BitwiseNeutralElement:
    Bitwise
    + NeutralElement<NeutralElementType: BitwiseBinaryOps<Self> + NeutralElement + IndexAssignable>
{
}

pub trait Bitwise {
    fn index(&self, index: usize) -> bool;
    fn support(&self) -> impl sorted_iter::SortedIterator<Item = usize>;
    fn weight(&self) -> usize {
        self.support().count()
    }
    fn parity(&self) -> bool {
        (self.weight() % 2) == 1
    }
    fn is_zero(&self) -> bool {
        self.weight() == 0
    }
    fn is_one_bit(&self, index: usize) -> bool {
        self.weight() == 1 && self.index(index)
    }
    fn max_bit_id(&self) -> Option<usize> {
        self.support().last()
    }
}

pub trait IndexAssignable: Bitwise {
    fn assign_index(&mut self, index: usize, to: bool);
    fn negate_index(&mut self, index: usize);
    fn clear_bits(&mut self);
    fn set_random(&mut self, num_bits: usize, random_number_generator: &mut impl rand::Rng) {
        for j in 0..num_bits {
            self.assign_index(j, random_number_generator.gen());
        }
    }
}

pub trait BorrowAsBitIterator {
    type BitIterator<'life>: ExactSizeIterator<Item = bool>
    where
        Self: 'life;
    fn borrow_as_bit_iterator(&self) -> Self::BitIterator<'_>;
}

// Binary traits, involve two types

pub trait BitwiseBinaryOps<Other: ?Sized + Bitwise = Self>: Bitwise + IndexAssignable {
    fn assign(&mut self, other: &Other);

    fn assign_with_offset(&mut self, other: &Other, start_bit: usize, num_bits: usize) {
        for bit_index in 0..num_bits {
            self.assign_index(bit_index + start_bit, other.index(bit_index));
        }
    }

    fn assign_from_interval(&mut self, other: &Other, start_bit: usize, num_bits: usize) {
        for bit_index in 0..num_bits {
            self.assign_index(bit_index, other.index(start_bit + bit_index));
        }
    }

    fn bitxor_assign(&mut self, other: &Other);
    fn bitand_assign(&mut self, other: &Other);
}

// TODO: should we eliminate Dot and move `dot` into OverlapWeight ?
pub trait Dot<Other: ?Sized = Self> {
    fn dot(&self, other: &Other) -> bool;
}

pub trait OverlapWeight<Other: ?Sized = Self> {
    fn and_weight(&self, other: &Other) -> usize;
    fn or_weight(&self, other: &Other) -> usize;
}

pub trait FromBits<Other> {
    fn from_bits(other: &Other) -> Self;
}

//TODO: Should BitVec, BitView, MutableBitView, BitBlock thoroughly implement standard traits such as : BitXor, BitAnd, BitXorAssign, BitAndAssign, Index, FromIterator<bool>, Display ?
pub mod bitchunk;
pub mod tiny_matrix;
