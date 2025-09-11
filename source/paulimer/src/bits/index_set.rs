use super::{BitVec, BitwiseNeutralElement, BorrowAsBitIterator, FromBits, OverlapWeight};
use crate::bits::{are_supports_equal, Bitwise, BitwiseBinaryOps, Dot, IndexAssignable};
use crate::NeutralElement;
use itertools::{equal, sorted, Itertools};
use sorted_iter::{assume::AssumeSortedByItemExt, SortedIterator};
use sorted_vec::SortedSet;

#[must_use]
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct IndexSet {
    indexes: SortedSet<usize>,
}

impl IndexSet {
    pub fn new() -> IndexSet {
        IndexSet {
            indexes: SortedSet::new(),
        }
    }

    pub fn singleton(value: usize) -> Self {
        IndexSet {
            indexes: unsafe { SortedSet::from_sorted(vec![value]) },
        }
    }
}

impl Default for IndexSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<usize> for IndexSet {
    fn from_iter<Iterator: IntoIterator<Item = usize>>(iterator: Iterator) -> Self {
        let indexes = SortedSet::from_unsorted(iterator.into_iter().collect());
        IndexSet { indexes }
    }
}

impl IntoIterator for IndexSet {
    type Item = usize;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.indexes.into_vec().into_iter()
    }
}

impl Bitwise for IndexSet {
    fn index(&self, index: usize) -> bool {
        self.indexes.contains(&index)
    }

    fn weight(&self) -> usize {
        self.indexes.len()
    }

    fn support(&self) -> impl SortedIterator<Item = usize> {
        self.indexes.iter().copied().assume_sorted_by_item()
    }
}

impl<Bits: Bitwise> BitwiseBinaryOps<Bits> for IndexSet {
    fn assign(&mut self, other: &Bits) {
        let indexes: Vec<usize> = other.support().collect();
        self.indexes = indexes.into();
    }

    fn bitxor_assign(&mut self, other: &Bits) {
        for index in other.support() {
            let found = self.indexes.find_or_insert(index);
            if found.is_found() {
                self.indexes.remove_index(found.index());
            }
        }
    }

    fn bitand_assign(&mut self, other: &Bits) {
        let self_support = self.indexes.iter().copied().assume_sorted_by_item();
        let other_support = other.support();
        let indexes: Vec<usize> = self_support.intersection(other_support).collect();
        self.indexes = indexes.into();
    }
}

impl IndexAssignable for IndexSet {
    fn assign_index(&mut self, index: usize, to: bool) {
        if to {
            self.indexes.push(index);
        } else {
            self.indexes.remove_item(&index);
        }
    }

    fn negate_index(&mut self, index: usize) {
        if self.indexes.contains(&index) {
            self.indexes.remove_item(&index);
        } else {
            self.indexes.push(index);
        }
    }

    fn clear_bits(&mut self) {
        self.indexes.clear();
    }
}

impl<Bits: Bitwise> Dot<Bits> for IndexSet {
    fn dot(&self, other: &Bits) -> bool {
        debug_assert!(is_sorted(self.support()));
        debug_assert!(is_sorted(other.support()));
        let mut res = false;
        for index in self.support() {
            res ^= other.index(index);
        }
        res
    }
}

impl<T: IndexAssignable + BorrowAsBitIterator> Dot<IndexSet> for T {
    fn dot(&self, other: &IndexSet) -> bool {
        other.dot(self)
    }
}

impl<Bits: Bitwise> OverlapWeight<Bits> for IndexSet {
    fn and_weight(&self, other: &Bits) -> usize {
        let mut res = 0usize;
        for index in self.support() {
            if other.index(index) {
                res += 1;
            }
        }
        res
    }

    fn or_weight(&self, other: &Bits) -> usize {
        self.weight() + other.weight() - self.and_weight(other)
    }
}

impl<T: IndexAssignable + BorrowAsBitIterator> OverlapWeight<IndexSet> for T {
    fn and_weight(&self, other: &IndexSet) -> usize {
        other.and_weight(self)
    }

    fn or_weight(&self, other: &IndexSet) -> usize {
        other.or_weight(self)
    }
}

fn is_sorted<Items: Iterator<Item = usize>>(items: Items) -> bool {
    let vec_items: Vec<usize> = items.collect();
    equal(sorted(vec_items.iter()), vec_items.iter())
}

impl<T: IndexAssignable + BorrowAsBitIterator> BitwiseBinaryOps<IndexSet> for T {
    #[allow(clippy::explicit_iter_loop)]
    fn assign(&mut self, other: &IndexSet) {
        self.clear_bits();
        for index in other.indexes.iter() {
            self.assign_index(*index, true);
        }
    }

    fn bitxor_assign(&mut self, other: &IndexSet) {
        for index in other.support() {
            self.negate_index(index);
        }
    }

    fn bitand_assign(&mut self, other: &IndexSet) {
        let intersection: Vec<usize> = self.support().intersection(other.support()).collect();
        self.clear_bits();
        for k in intersection {
            self.assign_index(k, true);
        }
    }
}

impl<T: IndexAssignable + BorrowAsBitIterator> PartialEq<T> for IndexSet {
    fn eq(&self, other: &T) -> bool {
        are_supports_equal(self, other)
    }
}

impl<Other: Bitwise> FromBits<Other> for IndexSet {
    fn from_bits(other: &Other) -> Self {
        IndexSet {
            indexes: other.support().collect::<Vec<usize>>().into(),
        }
    }
}

impl NeutralElement for IndexSet {
    type NeutralElementType = IndexSet;

    fn neutral_element(&self) -> Self::NeutralElementType {
        IndexSet::new()
    }

    fn default_size_neutral_element() -> Self::NeutralElementType {
        IndexSet::new()
    }

    fn neutral_element_of_size(_size: usize) -> Self::NeutralElementType {
        IndexSet::new()
    }
}

impl BitwiseNeutralElement for IndexSet {}

impl<'life, const WORD_COUNT: usize> From<&'life BitVec<WORD_COUNT>> for IndexSet {
    fn from(value: &'life BitVec<WORD_COUNT>) -> Self {
        unsafe {
            IndexSet {
                indexes: SortedSet::from_sorted(value.support().collect_vec()),
            }
        }
    }
}

pub fn remapped(bits: &IndexSet, support: &[usize]) -> IndexSet {
    bits.support().map(|id| support[id]).collect()
}

impl<T> From<T> for IndexSet
where
    T: SortedIterator<Item = usize>,
{
    fn from(value: T) -> Self {
        unsafe {
            IndexSet {
                indexes: SortedSet::from_sorted(value.collect_vec()),
            }
        }
    }
}
