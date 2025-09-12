// Tiny bitmatrix with RREF, not heap allocation, compile-time known sizes

use super::BitMatrix;

#[must_use]
pub fn tiny_matrix_from_bitmatrix<const ROW_COUNT: usize>(matrix: &BitMatrix) -> [u64; ROW_COUNT] {
    let mut tiny_matrix = [0u64; ROW_COUNT];
    let column_count = matrix.columncount();
    for row_id in 0..ROW_COUNT {
        for column_id in 0..column_count {
            if matrix[(row_id, column_id)] {
                tiny_matrix[row_id] ^= 1 << column_id;
            }
        }
    }
    tiny_matrix
}

pub fn xor_tiny_column<const NUM_ROWS: usize>(
    tiny_matrix: &mut [u64; NUM_ROWS],
    column_index: usize,
    column: u64,
) {
    let mask = 1 << column_index;
    for (row_id, row) in tiny_matrix.iter_mut().enumerate() {
        if column & (1 << row_id) != 0 {
            *row ^= mask;
        }
    }
}

#[must_use]
pub fn get_tiny_column<const NUM_ROWS: usize>(
    tiny_matrix: &[u64; NUM_ROWS],
    column_index: usize,
) -> u64 {
    let mask = 1 << column_index;
    let mut column = 0u64;
    for (row_id, row) in tiny_matrix.iter().enumerate() {
        if *row & mask != 0 {
            column ^= 1u64 << row_id;
        }
    }
    column
}

fn tiny_pivot_row<const NUM_ROWS: usize>(
    tiny_matrix: &mut [u64; NUM_ROWS],
    column_id: usize,
    start_row: usize,
) -> usize {
    let column_mask = 1u64 << column_id;
    for (row_id, row) in tiny_matrix.iter().enumerate().skip(start_row) {
        if *row & column_mask != 0 {
            return row_id;
        }
    }
    NUM_ROWS
}

fn tiny_reduce_forward<const NUM_ROWS: usize>(
    tiny_matrix: &mut [u64; NUM_ROWS],
    column_id: usize,
    start_row: usize,
) -> usize {
    let column_mask = 1u64 << column_id;
    let reducing_row = tiny_matrix[start_row];
    for row in tiny_matrix.iter_mut().skip(start_row + 1) {
        if *row & column_mask != 0 {
            *row ^= reducing_row;
        }
    }
    NUM_ROWS
}

fn tiny_reduce_backward<const NUM_ROWS: usize>(
    tiny_matrix: &mut [u64; NUM_ROWS],
    column_id: usize,
    end_row: usize,
) -> usize {
    let column_mask = 1u64 << column_id;
    let reducing_row = tiny_matrix[end_row];
    for row in tiny_matrix.iter_mut().take(end_row) {
        if *row & column_mask != 0 {
            *row ^= reducing_row;
        }
    }
    NUM_ROWS
}

pub fn tiny_matrix_rref<const NUM_ROWS: usize, const NUM_COLUMNS: usize>(
    tiny_matrix: &mut [u64; NUM_ROWS],
) -> usize {
    let mut rank_profile = [0usize; 64];
    let mut column_id = 0;
    let mut current_row_id = 0;
    let mut rank = 0;
    while column_id < NUM_COLUMNS {
        let pivot_row = tiny_pivot_row(tiny_matrix, column_id, current_row_id);
        if pivot_row != NUM_ROWS {
            tiny_matrix.swap(current_row_id, pivot_row);
            tiny_reduce_forward(tiny_matrix, column_id, current_row_id);
            rank_profile[current_row_id] = column_id;
            current_row_id += 1;
            rank += 1;
        }
        column_id += 1;
    }
    for (row_id, column_id) in rank_profile.iter().enumerate().take(rank) {
        tiny_reduce_backward(tiny_matrix, *column_id, row_id);
    }
    rank
}
