// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::bits::bitblock::{BitAccessor, BitBlock};
use crate::bits::{
    BitVec, BitView, Bitwise, BitwiseBinaryOps, Dot, IndexAssignable, MutableBitView,
};
use crate::NeutralElement;
// use bit_vec::BitVec;
use itertools::enumerate;
use itertools::iproduct;
use sorted_iter::assume::AssumeSortedByItemExt;
use sorted_iter::SortedIterator;
use std::cmp::PartialEq;
use std::hash::Hash;
use std::iter::FromIterator;
use std::mem::size_of;
use std::ops::{Add, AddAssign, BitAnd, BitAndAssign, BitXor, BitXorAssign, Mul};
use std::ops::{Index, Range};
use std::str::FromStr;

use super::bitvec::WORD_COUNT_DEFAULT;
use super::{BitwiseNeutralElement, OverlapWeight};

#[must_use]
#[derive(Debug, Eq)]
pub struct BitMatrix<const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    blocks: Vec<BitBlock<WORD_COUNT>>,
    rows: Vec<*mut BitBlock<WORD_COUNT>>,
    columncount: usize,
}

impl<const WORD_COUNT: usize> Hash for BitMatrix<WORD_COUNT> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.blocks.hash(state);
    }
}

unsafe impl Sync for BitMatrix {}

pub type Row<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> = BitView<'life, WORD_COUNT>; // should we use View in the name to indicate that it is a view and not a copy of a row ?
pub type MutableRow<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> =
    MutableBitView<'life, WORD_COUNT>; // should we use View in the name to indicate that it is a view and not a copy of a row ?

#[derive(Clone, Debug, Hash)]
pub struct Column<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    // TODO(VK) should we use View in the name to indicate that it is a view and not a copy of a column ?
    rows: &'life [*mut BitBlock<WORD_COUNT>],
    accessor: BitAccessor<WORD_COUNT>,
    block_index: usize,
}

impl<const WORD_COUNT: usize> BitMatrix<WORD_COUNT> {
    pub fn with_shape(rows: usize, columns: usize) -> Self {
        Self::zeros(rows, columns)
    }

    pub fn zeros(rows: usize, columns: usize) -> Self {
        Self::with_value(false, (rows, columns))
    }

    pub fn ones(rows: usize, columns: usize) -> Self {
        Self::with_value(true, (rows, columns))
    }

    pub fn identity(dimension: usize) -> Self {
        let mut res = Self::zeros(dimension, dimension);
        for index in 0..dimension {
            res.set((index, index), true);
        }
        res
    }

    pub fn from_row_iter<'life>(
        iter: impl ExactSizeIterator<Item = BitView<'life, WORD_COUNT>>,
        columns: usize,
    ) -> Self {
        let rows = iter.len();
        let mut matrix = Self::zeros(rows, columns);
        for (row_from, mut row_to) in std::iter::zip(iter, matrix.row_iterator_mut(0..rows)) {
            row_to.assign(&row_from);
        }
        matrix
    }

    pub fn from_iter<Row, Rows>(iter: Rows, columncount: usize) -> Self
    where
        Row: IntoIterator<Item = bool>,
        Rows: IntoIterator<Item = Row>,
    {
        // TODO(AEP): Expanding first into Vec<bool> is
        // inefficient. Instead, append to Vec<BitBlock::<WORD_COUNT_DEFAULT>> as necessary.
        let mut rows = Vec::<Vec<bool>>::new();
        let mut rowcount = 0;
        for row in iter {
            rows.push(row.into_iter().collect());
            rowcount += 1;
        }
        let mut matrix = BitMatrix::with_shape(rowcount, columncount);
        for (row_index, row) in rows.iter().enumerate() {
            for (column_index, value) in row.iter().take(columncount).enumerate() {
                matrix.set((row_index, column_index), *value);
            }
        }
        matrix
    }

    fn with_value(value: bool, shape: (usize, usize)) -> Self {
        let (rowcount, columncount) = shape;
        let rowstride = Self::rowstride_of(columncount);
        let buffer = vec![BitBlock::<WORD_COUNT>::all(value); rowcount * rowstride];
        Self::from_blocks(buffer, shape)
    }

    fn from_blocks(mut buffer: Vec<BitBlock<WORD_COUNT>>, shape: (usize, usize)) -> Self {
        let rows = Self::rows_of(buffer.as_mut_slice(), shape.0);
        Self::from_blocks_and_rows(buffer, shape, rows)
    }

    fn from_blocks_and_rows(
        buffer: Vec<BitBlock<WORD_COUNT>>,
        shape: (usize, usize),
        rows: Vec<*mut BitBlock<WORD_COUNT>>,
    ) -> Self {
        let matrix = Self {
            blocks: buffer,
            rows,
            columncount: shape.1,
        };
        debug_assert!(matrix.is_aligned());
        matrix
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        let zero = BitBlock::<WORD_COUNT>::zeros();
        for block in &self.blocks {
            if *block != zero {
                return false;
            }
        }
        true
    }

    fn is_aligned(&self) -> bool {
        let alignment = (self.blocks.as_ptr() as usize) % size_of::<BitBlock<WORD_COUNT>>();
        if alignment != 0 {
            return false;
        }
        for row in &self.rows {
            let alignment = (*row as usize) % size_of::<BitBlock<WORD_COUNT>>();
            if alignment != 0 {
                return false;
            }
        }
        true
    }

    fn rowstride(&self) -> usize {
        Self::rowstride_of(self.columncount)
    }

    fn rowstride_of(columncount: usize) -> usize {
        let rowstride = columncount / BitBlock::<WORD_COUNT>::BITS;
        let adjustment = !columncount.is_multiple_of(BitBlock::<WORD_COUNT>::BITS);
        rowstride + usize::from(adjustment)
    }

    fn rows_of(
        blocks: &mut [BitBlock<WORD_COUNT>],
        rowcount: usize,
    ) -> Vec<*mut BitBlock<WORD_COUNT>> {
        let mut rows = Vec::<*mut BitBlock<WORD_COUNT>>::new();
        let rowstride = if rowcount == 0 {
            0
        } else {
            blocks.len() / rowcount
        };
        if rowstride == 0 {
            rows = vec![blocks.as_mut_ptr(); rowcount];
        } else {
            for row in blocks.chunks_exact_mut(rowstride) {
                rows.push(row.as_mut_ptr());
            }
        }
        rows
    }

    #[must_use]
    pub fn rowcount(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn columncount(&self) -> usize {
        self.columncount
    }

    #[must_use]
    pub fn shape(&self) -> (usize, usize) {
        (self.rowcount(), self.columncount())
    }

    #[must_use]
    pub fn row(&self, index: usize) -> Row<'_, WORD_COUNT> {
        Row::<WORD_COUNT> {
            blocks: unsafe {
                std::slice::from_raw_parts((*self.rows[index]).array(), self.block_count())
            },
        }
    }

    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'_, WORD_COUNT>> {
        self.row_iterator(0..self.rowcount())
    }

    pub fn row_iterator(
        &self,
        index_iterator: impl ExactSizeIterator<Item = usize>,
    ) -> impl ExactSizeIterator<Item = Row<'_, WORD_COUNT>> {
        index_iterator.map(|index| self.row(index))
    }

    pub fn row_iterator_mut(
        &mut self,
        index_iterator: impl ExactSizeIterator<Item = usize>,
    ) -> impl ExactSizeIterator<Item = MutableRow<'_, WORD_COUNT>> {
        index_iterator.map(|index| self.build_mutable_row(index))
    }

    pub fn row_mut(&mut self, index: usize) -> MutableRow<'_, WORD_COUNT> {
        self.build_mutable_row(index)
    }

    #[inline]
    fn block_count(&self) -> usize {
        let mut block_count = self.columncount() / BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        if !self
            .columncount()
            .is_multiple_of(BitBlock::<WORD_COUNT_DEFAULT>::BITS)
        {
            block_count += 1;
        }
        block_count
    }

    fn build_mutable_row(&self, index: usize) -> MutableRow<'_, WORD_COUNT> {
        let ptr = self.rows[index];
        MutableRow::<WORD_COUNT> {
            blocks: unsafe {
                std::slice::from_raw_parts_mut((*ptr).array_mut(), self.block_count())
            },
        }
    }

    pub fn rows_mut(
        &mut self,
        index: usize,
        index2: usize,
    ) -> (MutableRow<'_, WORD_COUNT>, MutableRow<'_, WORD_COUNT>) {
        (
            self.build_mutable_row(index),
            self.build_mutable_row(index2),
        )
    }

    pub fn rows2_mut(
        &mut self,
        index: (usize, usize),
    ) -> (MutableRow<'_, WORD_COUNT>, MutableRow<'_, WORD_COUNT>) {
        (
            self.build_mutable_row(index.0),
            self.build_mutable_row(index.1),
        )
    }

    #[must_use]
    pub fn rows2(&self, index: (usize, usize)) -> (Row<'_, WORD_COUNT>, Row<'_, WORD_COUNT>) {
        (self.row(index.0), self.row(index.1))
    }

    /// # Safety
    /// Does not check if all indexes are distinct
    pub unsafe fn rows4_mut(
        &mut self,
        index: (usize, usize, usize, usize),
    ) -> (
        MutableRow<'_, WORD_COUNT>,
        MutableRow<'_, WORD_COUNT>,
        MutableRow<'_, WORD_COUNT>,
        MutableRow<'_, WORD_COUNT>,
    ) {
        (
            self.build_mutable_row(index.0),
            self.build_mutable_row(index.1),
            self.build_mutable_row(index.2),
            self.build_mutable_row(index.3),
        )
    }

    /// TODO(VK): Maybe use <https://doc.rust-lang.org/std/primitive.slice.html#method.get_many_mut> when it becomes stable
    /// # Safety
    /// Does not check if all indexes are distinct
    pub unsafe fn rows8_mut(
        &mut self,
        index: crate::Tuple8<usize>,
    ) -> crate::Tuple8<MutableRow<'_, WORD_COUNT>> {
        (
            self.build_mutable_row(index.0),
            self.build_mutable_row(index.1),
            self.build_mutable_row(index.2),
            self.build_mutable_row(index.3),
            self.build_mutable_row(index.4),
            self.build_mutable_row(index.5),
            self.build_mutable_row(index.6),
            self.build_mutable_row(index.7),
        )
    }

    #[must_use]
    pub fn column(&self, index: usize) -> Column<'_, WORD_COUNT> {
        let block_index = index / BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        let bit_index = index % BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        Column::<WORD_COUNT> {
            rows: &self.rows,
            accessor: BitAccessor::for_index(bit_index),
            block_index,
        }
    }

    #[must_use]
    pub fn columns(&self) -> impl ExactSizeIterator<Item = Column<'_, WORD_COUNT>> {
        let indexes = 0..self.columncount();
        indexes.map(|index| self.column(index))
    }

    /// # Panics
    ///
    /// Will panic if index out of range
    pub fn set(&mut self, index: (usize, usize), to: bool) {
        assert!(index.0 < self.rowcount() && index.1 < self.columncount());
        unsafe { self.set_unchecked(index, to) };
    }

    /// # Safety
    /// Dose not check if index is out of bounds
    pub unsafe fn set_unchecked(&mut self, index: (usize, usize), to: bool) {
        let (block, bit_index) = self.block_index_of_mut(index);
        block.set(bit_index, to);
    }

    /// # Panics
    ///
    /// Will panic if index out of range
    #[must_use]
    pub fn get(&self, index: (usize, usize)) -> bool {
        assert!(index.0 < self.rowcount() && index.1 < self.columncount());
        unsafe { self.get_unchecked(index) }
    }

    /// # Safety
    /// Does not check if index is out of bounds
    #[must_use]
    pub unsafe fn get_unchecked(&self, index: (usize, usize)) -> bool {
        let (block, bit_index) = self.block_index_of(index);
        block.get_unchecked(bit_index)
    }

    pub fn echelonize(&mut self) -> Vec<usize> {
        let mut pivot = pivot_of(self, (0, 0));
        let mut rank_profile = Vec::<usize>::with_capacity(self.columncount());

        for row_index in 0..self.rowcount() {
            if pivot.1 >= self.columncount() {
                break;
            }
            self.swap_rows(pivot.0, row_index);
            pivot.0 = row_index;
            rank_profile.push(pivot.1);
            reduce(self, pivot);
            pivot = pivot_of(self, (pivot.0 + 1, pivot.1 + 1));
        }
        rank_profile
    }

    #[must_use]
    pub fn rank(&self) -> usize {
        self.clone().echelonize().len()
    }

    pub fn transposed(&self) -> Self {
        let mut res = Self::with_shape(self.columncount(), self.rowcount());
        for i in 0..self.rowcount() {
            for j in 0..self.columncount() {
                res.set((j, i), self[(i, j)]);
            }
        }
        res
    }

    pub fn submatrix(&self, rows: &[usize], columns: &[usize]) -> Self {
        let mut res = Self::with_shape(rows.len(), columns.len());
        for (row_index, &row) in rows.iter().enumerate() {
            for (column_index, &column) in columns.iter().enumerate() {
                res.set((row_index, column_index), self[(row, column)]);
            }
        }
        res
    }

    /// # Panics
    ///
    /// Will panic if matrix is not invertible
    pub fn inverted(&self) -> BitMatrix<WORD_COUNT> {
        assert!(self.columncount() == self.rowcount());
        let (_, t, _, profile) = rref_with_transforms(self.clone());
        assert!(profile.len() == self.rowcount());
        debug_assert_eq!(
            self * &t,
            BitMatrix::<WORD_COUNT>::identity(self.rowcount())
        );
        t
    }

    pub fn swap_rows(&mut self, left_row_index: usize, right_row_index: usize) {
        self.rows.swap(left_row_index, right_row_index);
    }

    pub fn swap_columns(&mut self, left_column_index: usize, right_column_index: usize) {
        for row_index in 0..self.rowcount() {
            let left_bit = self.get((row_index, left_column_index));
            let right_bit = self.get((row_index, right_column_index));
            self.set((row_index, left_column_index), right_bit);
            self.set((row_index, right_column_index), left_bit);
        }
    }

    pub fn permute_rows(&mut self, permutation: &[usize]) {
        let old_rows = self.rows.clone();
        for index in 0..permutation.len() {
            self.rows[index] = old_rows[permutation[index]];
        }
    }

    pub fn add_into_row(&mut self, to_index: usize, from_index: usize) {
        let mut to_block = self.rows[to_index];
        let mut from_block = self.rows[from_index];
        for _ in 0..self.rowstride() {
            unsafe {
                BitwiseBinaryOps::bitxor_assign(&mut *to_block, &*from_block);
                to_block = to_block.add(1);
                from_block = from_block.add(1);
            }
        }
    }

    // TODO(VK): check if we also need dot_transposed
    /// # Panics
    ///
    /// Will panic if matrix dimensions are incompatible
    pub fn dot(&self, rhs: &BitMatrix<WORD_COUNT>) -> BitMatrix<WORD_COUNT> {
        assert_eq!(self.columncount(), rhs.rowcount());
        let mut rows = Vec::with_capacity(self.rowcount());
        for output_row in 0..self.rowcount() {
            let mut row = BitVec::<WORD_COUNT>::zeros(rhs.columncount());
            for column_index in 0..self.columncount() {
                if self[(output_row, column_index)] {
                    // TODO(AEP): This is needlessly slow. Make it fast.
                    for into_column_index in 0..rhs.columncount() {
                        row.assign_index(
                            into_column_index,
                            row.index(into_column_index)
                                ^ rhs.get((column_index, into_column_index)),
                        );
                    }
                }
            }
            rows.push(row);
        }
        Self::from_iter(rows.iter(), rhs.columncount())
    }

    fn block_index_of_mut(&mut self, index: (usize, usize)) -> (&mut BitBlock<WORD_COUNT>, usize) {
        let column_block = index.1 / BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        let column_remainder = index.1 % BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        let bit_index = column_remainder % BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        unsafe {
            let block = self.rows[index.0].add(column_block);
            (&mut *block, bit_index)
        }
    }

    fn block_index_of(&self, index: (usize, usize)) -> (&BitBlock<WORD_COUNT>, usize) {
        let column_block = index.1 / BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        let column_remainder = index.1 % BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        let bit_index = column_remainder % BitBlock::<WORD_COUNT_DEFAULT>::BITS;
        unsafe {
            let block = self.rows[index.0].add(column_block);
            (&*block, bit_index)
        }
    }
}

unsafe impl<const WORD_COUNT: usize> Send for BitMatrix<WORD_COUNT> {}

impl<const WORD_COUNT: usize> Clone for BitMatrix<WORD_COUNT> {
    fn clone(&self) -> Self {
        let mut blocks = self.blocks.clone();
        let mut rows = Vec::<*mut BitBlock<WORD_COUNT>>::new();
        let offset = unsafe { blocks.as_mut_ptr().offset_from(self.blocks.as_ptr()) };
        for row in &self.rows {
            rows.push(unsafe { row.offset(offset) });
        }
        BitMatrix::from_blocks_and_rows(blocks, self.shape(), rows)
    }
}

impl<const WORD_COUNT: usize> Index<[usize; 2]> for BitMatrix<WORD_COUNT> {
    type Output = bool;

    fn index(&self, index: [usize; 2]) -> &Self::Output {
        &self[(index[0], index[1])]
    }
}

impl<const WORD_COUNT: usize> Index<(usize, usize)> for BitMatrix<WORD_COUNT> {
    type Output = bool;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        if self.get(index) {
            return &true;
        }
        &false
    }
}

impl<const WORD_COUNT: usize> PartialEq for BitMatrix<WORD_COUNT> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape() != other.shape() {
            return false;
        }
        for index in iproduct!(0..self.rowcount(), 0..self.columncount()) {
            if self[index] != other[index] {
                return false;
            }
        }
        true
    }
}

impl<const WORD_COUNT: usize> AddAssign<&BitMatrix<WORD_COUNT>> for BitMatrix<WORD_COUNT> {
    fn add_assign(&mut self, other: &BitMatrix<WORD_COUNT>) {
        assert_eq!(self.shape(), other.shape());
        for index in 0..self.rowcount() {
            self.row_mut(index).bitxor_assign(&other.row(index));
        }
    }
}

impl<const WORD_COUNT: usize> Add for &BitMatrix<WORD_COUNT> {
    type Output = BitMatrix<WORD_COUNT>;

    fn add(self, other: Self) -> Self::Output {
        let mut clone = (*self).clone();
        clone += other;
        clone
    }
}

impl<const WORD_COUNT: usize> BitXor for &BitMatrix<WORD_COUNT> {
    type Output = BitMatrix<WORD_COUNT>;

    fn bitxor(self, other: Self) -> Self::Output {
        self.add(other)
    }
}

impl<const WORD_COUNT: usize> BitXorAssign<&BitMatrix<WORD_COUNT>> for BitMatrix<WORD_COUNT> {
    fn bitxor_assign(&mut self, other: &BitMatrix<WORD_COUNT>) {
        self.add_assign(other);
    }
}

impl<const WORD_COUNT: usize> BitAndAssign<&BitMatrix<WORD_COUNT>> for BitMatrix<WORD_COUNT> {
    fn bitand_assign(&mut self, other: &Self) {
        assert_eq!(self.shape(), other.shape());
        for index in 0..self.rowcount() {
            self.row_mut(index).bitand_assign(&other.row(index));
        }
    }
}

impl<const WORD_COUNT: usize> BitAnd for &BitMatrix<WORD_COUNT> {
    type Output = BitMatrix<WORD_COUNT>;

    fn bitand(self, other: Self) -> Self::Output {
        let mut clone = (*self).clone();
        clone &= other;
        clone
    }
}

impl<const WORD_COUNT: usize> Mul for &BitMatrix<WORD_COUNT> {
    type Output = BitMatrix<WORD_COUNT>;

    fn mul(self, other: Self) -> Self::Output {
        assert_eq!(self.columncount(), other.rowcount());

        let mut result = BitMatrix::<WORD_COUNT>::with_shape(self.rowcount(), other.columncount());

        for i in 0..self.rowcount() {
            for j in 0..other.columncount() {
                let mut val = false;
                for k in 0..self.columncount() {
                    val ^= self[[i, k]] && other[[k, j]];
                }
                result.set((i, j), val);
            }
        }

        result
    }
}

impl<const WORD_COUNT: usize> std::fmt::Display for BitMatrix<WORD_COUNT> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for row_index in 0..self.rowcount() {
            for column_index in 0..self.columncount() {
                let value = i32::from(self.get((row_index, column_index)));
                write!(f, "{value:?}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl<const WORD_COUNT: usize> FromStr for BitMatrix<WORD_COUNT> {
    type Err = usize;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut rows = Vec::<BitVec>::new();
        let mut column_count = 0;
        for row_string in s.split(&['|', '[', ']', '(', ')', ';', '\n']) {
            if !row_string.is_empty() {
                let mut res = Vec::<bool>::new();
                for char in row_string.chars() {
                    match char {
                        '0' | '.' | '▫' | '□' => res.push(false),
                        '1' | '▪' | '■' => res.push(true),
                        ' ' | '-' | ',' => {}
                        _ => return Err(0),
                    }
                }
                if !res.is_empty() {
                    column_count = column_count.max(res.len());
                    rows.push(res.into_iter().collect());
                }
            }
        }
        Ok(BitMatrix::from_iter(rows.iter(), column_count))
    }
}

/// # Panics
///
/// Should not panic. TODO: refactor so clippy does not complain
pub fn row_stacked<'t, Matrices, const WORD_COUNT: usize>(
    matrices: Matrices,
) -> BitMatrix<WORD_COUNT>
where
    Matrices: IntoIterator<Item = &'t BitMatrix<WORD_COUNT>>,
{
    let mut buffer = Vec::<BitBlock<WORD_COUNT>>::new();
    let mut columncount: Option<usize> = None;
    let mut rowcount = 0;
    for matrix in matrices {
        debug_assert!(columncount.is_none() || columncount.unwrap() == matrix.columncount());
        buffer.append(&mut matrix.blocks.clone());
        columncount = Some(matrix.columncount());
        rowcount += matrix.rowcount();
    }
    BitMatrix::<WORD_COUNT>::from_blocks(buffer, (rowcount, *columncount.get_or_insert(0)))
}

pub fn directly_summed<'t, Matrices>(matrices: Matrices) -> BitMatrix
where
    Matrices: IntoIterator<Item = &'t BitMatrix>,
{
    let mut rowcount = 0;
    let mut columncount = 0;
    let vec_matrices = Vec::from_iter(matrices);
    for matrix in &vec_matrices {
        rowcount += matrix.rowcount();
        columncount += matrix.columncount();
    }
    let mut sum = BitMatrix::zeros(rowcount, columncount);
    let mut sum_row_offset = 0;
    let mut sum_column_offset = 0;
    for matrix in &vec_matrices {
        for row_index in 0..matrix.rowcount() {
            for column_index in 0..matrix.columncount() {
                sum.set(
                    (row_index + sum_row_offset, column_index + sum_column_offset),
                    matrix[(row_index, column_index)],
                );
            }
        }
        sum_row_offset += matrix.rowcount();
        sum_column_offset += matrix.columncount();
    }
    sum
}

fn pivot_of<const WORD_COUNT: usize>(
    matrix: &BitMatrix<WORD_COUNT>,
    starting_at: (usize, usize),
) -> (usize, usize) {
    let (mut row_index, mut column_index) = starting_at;
    if row_index >= matrix.rowcount() || column_index >= matrix.columncount() {
        return (row_index, column_index);
    }
    while !matrix.get((row_index, column_index)) {
        row_index += 1;
        if row_index == matrix.rowcount() {
            column_index += 1;
            row_index = starting_at.0;
            if column_index == matrix.columncount() {
                break;
            }
        }
    }
    (row_index, column_index)
}

struct ReductionData<const WORD_COUNT: usize> {
    column_accessor: BitAccessor<WORD_COUNT>,
    blocks_per_row: usize,
    rowstride: usize,
    base_block: *const BitBlock<WORD_COUNT>,
    from_block: *mut BitBlock<WORD_COUNT>,
}

impl<const WORD_COUNT: usize> ReductionData<WORD_COUNT> {
    pub fn for_pivot(pivot: (usize, usize), within: &BitMatrix<WORD_COUNT>) -> Self {
        let start_block_offset = pivot.1 / BitBlock::<WORD_COUNT>::BITS;
        let bit_index = pivot.1 % BitBlock::<WORD_COUNT>::BITS;
        let from_block = unsafe { within.rows.get_unchecked(pivot.0).add(start_block_offset) };
        let base_block = unsafe { within.blocks.as_ptr().add(start_block_offset) };
        let rowstride = within.rowstride();

        ReductionData {
            column_accessor: BitAccessor::for_index(bit_index),
            blocks_per_row: rowstride - start_block_offset,
            rowstride,
            from_block,
            base_block,
        }
    }
}

fn reduce<const WORD_COUNT: usize>(matrix: &mut BitMatrix<WORD_COUNT>, from: (usize, usize)) {
    let data = ReductionData::for_pivot(from, matrix);
    let mut to_block = data.from_block;
    to_block = reduce_backward_until(data.base_block, to_block, &data);
    to_block = unsafe { to_block.add(data.rowstride * matrix.rowcount()) };
    let until_block = unsafe { data.from_block.add(data.rowstride) };
    reduce_backward_until(until_block, to_block, &data);
}

fn reduce_backward_until<const WORD_COUNT: usize>(
    until_block: *const BitBlock<WORD_COUNT>,
    mut to_block: *mut BitBlock<WORD_COUNT>,
    data: &ReductionData<WORD_COUNT>,
) -> *mut BitBlock<WORD_COUNT> {
    while until_block != to_block {
        to_block = unsafe { to_block.sub(data.rowstride) };
        let column_value = unsafe { data.column_accessor.array_value_of((*to_block).array()) };
        if column_value {
            add_into_block(to_block, data.from_block, data.blocks_per_row);
        }
    }
    to_block
}

fn add_into_block<const WORD_COUNT: usize>(
    mut to_block: *mut BitBlock<WORD_COUNT>,
    mut from_block: *const BitBlock<WORD_COUNT>,
    block_count: usize,
) {
    for _ in 0..block_count {
        unsafe {
            *to_block ^= &*from_block;
            to_block = to_block.add(1);
            from_block = from_block.add(1);
        }
    }
}

/// # Returns
/// Row reduced echelon form R of `matrix` , transformation matrix T and , inverse transpose of T,
/// and row rank profile.
/// T * `matrix` equals R
pub fn rref_with_transforms<const WORD_COUNT: usize>(
    mut matrix: BitMatrix<WORD_COUNT>,
) -> (
    BitMatrix<WORD_COUNT>,
    BitMatrix<WORD_COUNT>,
    BitMatrix<WORD_COUNT>,
    Vec<usize>,
) {
    let num_rows = matrix.rowcount();
    let mut transform = BitMatrix::identity(num_rows);
    let mut transform_inv_t = BitMatrix::identity(num_rows);
    let mut pivot = pivot_of(&matrix, (0, 0));
    let mut rank_profile = Vec::<usize>::with_capacity(matrix.columncount());

    for row_index in 0..matrix.rowcount() {
        if pivot.1 >= matrix.columncount() {
            break;
        }

        matrix.swap_rows(pivot.0, row_index);
        transform_inv_t.swap_rows(pivot.0, row_index);
        transform.swap_rows(pivot.0, row_index);

        pivot.0 = row_index;
        rank_profile.push(pivot.1);
        reduce_with_transforms(&mut matrix, &mut transform, &mut transform_inv_t, pivot);
        pivot = pivot_of(&matrix, (pivot.0 + 1, pivot.1 + 1));
    }
    (matrix, transform, transform_inv_t, rank_profile)
}

fn reduce_with_transforms<const WORD_COUNT: usize>(
    matrix: &mut BitMatrix<WORD_COUNT>,
    transform: &mut BitMatrix<WORD_COUNT>,
    transform_inv_t: &mut BitMatrix<WORD_COUNT>,
    from: (usize, usize),
) {
    let rowcount = matrix.rowcount();
    for row_index in 0..from.0 {
        xor_if_column_with_transforms(
            from.1,
            matrix,
            transform,
            transform_inv_t,
            row_index,
            from.0,
        );
    }
    for row_index in from.0 + 1..rowcount {
        xor_if_column_with_transforms(
            from.1,
            matrix,
            transform,
            transform_inv_t,
            row_index,
            from.0,
        );
    }
}

fn xor_if_column_with_transforms<const WORD_COUNT: usize>(
    column_index: usize,
    matrix: &mut BitMatrix<WORD_COUNT>,
    transform: &mut BitMatrix<WORD_COUNT>,
    transform_inv_t: &mut BitMatrix<WORD_COUNT>,
    row_index: usize,
    from_row_index: usize,
) {
    if matrix[(row_index, column_index)] {
        matrix.add_into_row(row_index, from_row_index);
        transform.add_into_row(row_index, from_row_index);
        transform_inv_t.add_into_row(from_row_index, row_index);
    }
}

pub fn kernel_basis_matrix<const WORD_COUNT: usize>(
    matrix: &BitMatrix<WORD_COUNT>,
) -> BitMatrix<WORD_COUNT> {
    let num_cols = matrix.columncount();
    let mut rr = matrix.clone();
    let rank_profile = rr.echelonize();
    let rank_profile_complement = crate::setwise::complement(&rank_profile, num_cols);
    let res_row_count = num_cols - rank_profile.len();
    let mut res = BitMatrix::zeros(res_row_count, num_cols);
    for (index, elt) in enumerate(rank_profile) {
        for (row_pos, col_src) in rank_profile_complement
            .iter()
            .enumerate()
            .take(res_row_count)
        {
            res.set((row_pos, elt), rr[(index, *col_src)]);
        }
    }
    for (index, position) in enumerate(rank_profile_complement) {
        res.set((index, position), true);
    }
    res
}

pub fn full_rank_row_completion_with_inv<const WORD_COUNT: usize>(
    _matrix: &BitMatrix<WORD_COUNT>,
) -> (BitMatrix<WORD_COUNT>, BitMatrix<WORD_COUNT>) {
    // let _num_cols = matrix.columncount();
    // let rr = matrix.clone();
    // let rr2 = matrix.clone();
    // (rr,rr2);
    todo!()
}

impl<const WORD_COUNT: usize> Bitwise for Column<'_, WORD_COUNT> {
    fn weight(&self) -> usize {
        self.into_iter().filter(|bit| *bit).count()
    }

    fn support(&self) -> impl SortedIterator<Item = usize> {
        Box::new(
            self.into_iter()
                .enumerate()
                .filter(|pair| pair.1)
                .map(|pair| pair.0),
        )
        .assume_sorted_by_item()
    }

    fn index(&self, index: usize) -> bool {
        let block = unsafe { &*self.rows[index].add(self.block_index) };
        self.accessor.array_value_of(block.array())
    }
}

impl<const WORD_COUNT: usize> Dot for Column<'_, WORD_COUNT> {
    fn dot(&self, other: &Self) -> bool {
        assert_eq!(self.rows.len(), other.rows.len());
        let mut result = false;
        for index in 0..self.rows.len() {
            result ^= self.index(index) & other.index(index);
        }
        result
    }
}

impl<const WORD_COUNT: usize> OverlapWeight for Column<'_, WORD_COUNT> {
    fn and_weight(&self, other: &Self) -> usize {
        assert_eq!(self.rows.len(), other.rows.len());
        let mut result = 0usize;
        for index in 0..self.rows.len() {
            if self.index(index) & other.index(index) {
                result += 1;
            }
        }
        result
    }

    fn or_weight(&self, other: &Self) -> usize {
        assert_eq!(self.rows.len(), other.rows.len());
        let mut result = 0usize;
        for index in 0..self.rows.len() {
            if self.index(index) | other.index(index) {
                result += 1;
            }
        }
        result
    }
}

impl<const WORD_COUNT: usize> PartialEq for Column<'_, WORD_COUNT> {
    fn eq(&self, other: &Self) -> bool {
        if self.rows.len() != other.rows.len() {
            return false;
        }
        for index in 0..self.rows.len() {
            if self.index(index) != other.index(index) {
                return false;
            }
        }
        true
    }
}

impl<const WORD_COUNT: usize> Eq for Column<'_, WORD_COUNT> {}

impl<const WORD_COUNT: usize> Column<'_, WORD_COUNT> {
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Self {
        Column {
            rows: &self.rows[range],
            accessor: self.accessor.clone(),
            block_index: self.block_index,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// impl<'life,const WORD_COUNT: usize> IntoIterator for Column<'life,WORD_COUNT> {
//     type Item = bool;
//     type IntoIter = ColumnIterator<'life,WORD_COUNT>;
//     fn into_iter(self) -> Self::IntoIter {
//         ColumnIterator {
//             column: self,
//             row_index: 0,
//         }
//     }
// }

impl<'life, const WORD_COUNT: usize> IntoIterator for &'life Column<'_, WORD_COUNT> {
    type Item = bool;
    type IntoIter = ColumnIterator<'life, WORD_COUNT>;
    fn into_iter(self) -> Self::IntoIter {
        ColumnIterator {
            column: self,
            row_index: 0,
        }
    }
}

impl<const WORD_COUNT: usize> Column<'_, WORD_COUNT> {
    #[must_use]
    pub fn iter(&self) -> ColumnIterator<'_, WORD_COUNT> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

pub struct ColumnIterator<'life, const WORD_COUNT: usize = WORD_COUNT_DEFAULT> {
    column: &'life Column<'life, WORD_COUNT>,
    row_index: usize,
}

impl<const WORD_COUNT: usize> Iterator for ColumnIterator<'_, WORD_COUNT> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row_index >= self.column.rows.len() {
            return None;
        }
        let output = self.column.index(self.row_index);
        self.row_index += 1;
        Some(output)
    }
}

impl<const WORD_COUNT: usize> NeutralElement for Column<'_, WORD_COUNT> {
    type NeutralElementType = BitVec<WORD_COUNT>;

    fn neutral_element(&self) -> Self::NeutralElementType {
        BitVec::<WORD_COUNT>::zeros(self.rows.len())
    }

    fn default_size_neutral_element() -> Self::NeutralElementType {
        Self::NeutralElementType::default_size_neutral_element()
    }

    fn neutral_element_of_size(size: usize) -> Self::NeutralElementType {
        Self::NeutralElementType::neutral_element_of_size(size)
    }
}

impl<const WORD_COUNT: usize> BitwiseNeutralElement for Column<'_, WORD_COUNT> {}

impl<'life, const WORD_COUNT: usize> BitwiseBinaryOps<Column<'life, WORD_COUNT>>
    for BitVec<WORD_COUNT>
{
    fn assign(&mut self, other: &Column<'life, WORD_COUNT>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            self.assign_index(index, val);
        }
    }

    fn bitxor_assign(&mut self, other: &Column<'life, WORD_COUNT>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            if val {
                self.negate_index(index);
            }
        }
    }

    fn bitand_assign(&mut self, other: &Column<'life, WORD_COUNT>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            if !val {
                self.assign_index(index, false);
            }
        }
    }
}

impl<'life1, const WORD_COUNT_1: usize, const WORD_COUNT_2: usize>
    BitwiseBinaryOps<Column<'life1, WORD_COUNT_1>> for MutableBitView<'_, WORD_COUNT_2>
{
    fn assign(&mut self, other: &Column<'life1, WORD_COUNT_1>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            self.assign_index(index, val);
        }
    }

    fn bitxor_assign(&mut self, other: &Column<'life1, WORD_COUNT_1>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            if val {
                self.negate_index(index);
            }
        }
    }

    fn bitand_assign(&mut self, other: &Column<'life1, WORD_COUNT_1>) {
        for (index, val) in itertools::enumerate(other.into_iter()) {
            if !val {
                self.assign_index(index, false);
            }
        }
    }
}

pub fn is_zero_padded_identity(row_iterator: impl ExactSizeIterator<Item: Bitwise>) -> bool {
    enumerate(row_iterator).all(|(row_index, row)| row.is_one_bit(row_index))
}

pub fn is_zero_padded_symmetric<'life, const WORD_COUNT: usize>(
    row_iterator: impl ExactSizeIterator<Item = BitView<'life, WORD_COUNT>>,
    column_count: usize,
) -> bool {
    let matrix = BitMatrix::from_row_iter(row_iterator, column_count);
    matrix == matrix.transposed()
}

pub fn are_zero_rows(mut row_iterator: impl Iterator<Item: Bitwise>) -> bool {
    row_iterator.all(|row| row.is_zero())
}
