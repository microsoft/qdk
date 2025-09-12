use itertools::iproduct;
use paulimer::bits::bitmatrix::{directly_summed, kernel_basis_matrix, rref_with_transforms};
use paulimer::bits::tiny_matrix::{tiny_matrix_from_bitmatrix, tiny_matrix_rref};
use paulimer::bits::{BitMatrix, BitVec, Bitwise, BitwiseBinaryOps, WORD_COUNT_DEFAULT};
use proptest::prelude::*;
use rand::prelude::*;
use rand::Rng;
use sorted_iter::assume::AssumeSortedByItemExt;
use sorted_iter::SortedIterator;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

proptest! {
    #[test]
    fn shape(rowcount in 0..100usize, columncount in 0..100usize) {
        let matrix = BitMatrix::<WORD_COUNT_DEFAULT>::with_shape(rowcount, columncount);
        assert_eq!(matrix.rowcount(), rowcount);
        assert_eq!(matrix.columncount(), columncount);
        assert_eq!(matrix.shape(), (rowcount, columncount));
    }

    #[test]
    fn zeros(rowcount in 0..100usize, columncount in 0..100usize) {
        let matrix = BitMatrix::<WORD_COUNT_DEFAULT>::zeros(rowcount, columncount);
        for index in iproduct!(0..matrix.rowcount(), 0..matrix.columncount()) {
            assert!(!matrix[index]);
        }
    }

    // #[test]
    // fn ones(rowcount in 0..100usize, columncount in 0..100usize) {
    //     let matrix = BitMatrix::ones(rowcount, columncount);
    //     for index in iproduct!(0..matrix.rowcount(), 0..matrix.columncount()) {
    //         assert_eq!(matrix[index], true);
    //     }
    // }

    #[test]
    fn indexing(matrix in arbitrary_bitmatrix(100)) {
        for index in iproduct!(0..matrix.rowcount(), 0..matrix.columncount()) {
            assert_eq!(matrix[index], matrix[[index.0, index.1]]);
        }
    }

    #[test]
    fn clone(matrix in arbitrary_bitmatrix(100)) {
        assert_eq!(matrix, matrix.clone());
    }

    #[test]
    fn swap_rows(matrix in nonempty_bitmatrix(100), raw_row_indexes in (0..100usize, 0..100usize)) {
        let row_indexes = [raw_row_indexes.0 % matrix.rowcount(), raw_row_indexes.1 % matrix.rowcount()];
        let mut swapped = matrix.clone();
        swapped.swap_rows(row_indexes[0], row_indexes[1]);
        for column_index in 0..matrix.columncount() {
            assert_eq!(matrix[[row_indexes[0], column_index]], swapped[[row_indexes[1], column_index]]);
        }
        for row_index in (0..matrix.rowcount()).collect::<HashSet<usize>>().difference(&HashSet::from(row_indexes)) {
            for column_index in 0..matrix.columncount() {
                assert_eq!(matrix[[*row_index, column_index]], swapped[[*row_index, column_index]]);
            }
        }
    }

    #[test]
    fn swap_columns(matrix in nonempty_bitmatrix(100), raw_column_indexes in (0..100usize, 0..100usize)) {
        let column_indexes = [raw_column_indexes.0 % matrix.columncount(), raw_column_indexes.1 % matrix.columncount()];
        let mut swapped = matrix.clone();
        swapped.swap_columns(column_indexes[0], column_indexes[1]);
        for row_index in 0..matrix.rowcount() {
            assert_eq!(matrix[[row_index, column_indexes[0]]], swapped[[row_index, column_indexes[1]]]);
        }
        for column_index in (0..matrix.columncount()).collect::<HashSet<usize>>().difference(&HashSet::from(column_indexes)) {
            for row_index in 0..matrix.rowcount() {
                assert_eq!(matrix[[row_index, *column_index]], swapped[[row_index, *column_index]]);
            }
        }
    }

    #[test]
    fn addition((left, right) in equal_shape_bitmatrices(100)) {
        let sum = &left + &right;
        for index in iproduct!(0..left.rowcount(), 0..right.columncount()) {
            assert_eq!(sum[index], left[index] ^ right[index]);
        }
        assert_eq!(sum, &right + &left);
    }

    #[test]
    fn addition_inplace((mut left, right) in equal_shape_bitmatrices(100)) {
        let sum = &left + &right;
        left += &right;
        assert_eq!(sum, left);
    }

    #[test]
    fn xor((left, right) in equal_shape_bitmatrices(100)) {
        assert_eq!(&left ^ &right, &left + &right);
    }

    #[test]
    fn xor_inplace((mut left, right) in equal_shape_bitmatrices(100)) {
        let xor = &left ^ &right;
        left ^= &right;
        assert_eq!(xor, left);
    }

    #[test]
    fn and((left, right) in equal_shape_bitmatrices(100)) {
        let and = &left & &right;
        for index in iproduct!(0..left.rowcount(), 0..left.columncount()) {
            assert_eq!(and[index], left[index] & right[index]);
        }
        assert_eq!(and, &right & &left);
    }


    #[test]
    fn and_inplace((mut left, right) in equal_shape_bitmatrices(100)) {
        let and = &left & &right;
        left &= &right;
        assert_eq!(and, left);
    }

    #[test]
    fn equality(left in arbitrary_bitmatrix(100), right in arbitrary_bitmatrix(100)) {
        let mut are_equal = left.shape() == right.shape();
        if are_equal {
            for index in iproduct!(0..left.rowcount(), 0..right.columncount()) {
                are_equal &= left[index] == right[index];
            }
        }
        assert_eq!(left == right, are_equal);
    }

    #[test]
    fn transpose(matrix in arbitrary_bitmatrix(100)) {
        let transposed = matrix.transposed();
        for row in 0..matrix.rowcount() {
            for column in 0..matrix.columncount() {
                assert_eq!(matrix[(row, column)], transposed[(column, row)]);
            }
        }
    }

    #[test]
    fn inverse(matrix in invertible_bitmatrix(100)) {
        let inverted = matrix.inverted();
        let identity = BitMatrix::identity(matrix.rowcount());
        assert_eq!(&matrix * &inverted, identity);
    }

    #[test]
    fn echelon_form(matrix in arbitrary_bitmatrix(100)) {
        let mut echeloned = matrix.clone();
        let profile = echeloned.echelonize();
        assert!(is_rref(&echeloned, &profile));
        assert!(preserves_rowspan_of(&matrix, &echeloned));
    }

    #[test]
    fn tiny_matrix_echelon_form(matrix in fixed_size_bitmatrix(32,60)) {
        let mut echeloned = matrix.clone();
        let _ = echeloned.echelonize();
        let mut tiny1 = tiny_matrix_from_bitmatrix::<32>(&matrix);
        tiny_matrix_rref::<32,60>(&mut tiny1);
        let tiny2 = tiny_matrix_from_bitmatrix::<32>(&echeloned);
        assert_eq!(tiny1,tiny2);
    }

    #[test]
    fn direct_sum(left in arbitrary_bitmatrix(100), right in arbitrary_bitmatrix(100)) {
        let summed = directly_summed([&left, &right]);
        let expected_shape = (left.rowcount() + right.rowcount(), left.columncount() + right.columncount());
        assert_eq!(expected_shape, summed.shape());
        for row_index in 0..left.rowcount() {
            for column_index in 0..left.columncount() {
                assert_eq!(left[(row_index, column_index)], summed[(row_index, column_index)]);
            }
            for column_index in left.columncount()..summed.columncount() {
                assert!(!summed[(row_index, column_index)]);
            }
        }
        for row_index in 0..right.rowcount() {
            for column_index in 0..right.columncount() {
                assert_eq!(right[(row_index, column_index)], summed[(left.rowcount() + row_index, left.columncount() + column_index)]);
            }
            for column_index in 0..left.columncount() {
                assert!(!summed[(left.rowcount() + row_index, column_index)]);
            }
        }
    }

}

macro_rules! bitmatrix{
    ($($t:tt)+) => {
        $crate::BitMatrix::<{paulimer::bits::WORD_COUNT_DEFAULT}>::from_str(stringify!($($t)+)).unwrap()
    };
}

prop_compose! {
   fn arbitrary_bitmatrix(max_dimension: usize)(shape in (0..=max_dimension, 0..=max_dimension)) -> BitMatrix {
       random_bitmatrix(shape.0, shape.1)
   }
}

prop_compose! {
   fn fixed_size_bitmatrix(row_count: usize, column_count: usize)(_ in 0..column_count) -> BitMatrix {
       random_bitmatrix(row_count, column_count)
   }
}

prop_compose! {
   fn invertible_bitmatrix(max_dimension: usize)(dimension in 1..=max_dimension) -> BitMatrix {
       let mut matrix = BitMatrix::identity(dimension);
       for _ in 0..dimension^2 {
            let from_index = thread_rng().gen_range(0..dimension);
            let to_index = thread_rng().gen_range(0..dimension);
            if from_index != to_index {
                matrix.add_into_row(to_index, from_index);
            }
       }
       for _ in 0..dimension.pow(2) {
            let from_index = thread_rng().gen_range(0..dimension);
            let to_index = thread_rng().gen_range(0..dimension);
            matrix.swap_rows(from_index, to_index);
       }
       matrix
   }
}

prop_compose! {
   fn nonempty_bitmatrix(max_dimension: usize)(shape in (1..=max_dimension, 1..=max_dimension)) -> BitMatrix {
       random_bitmatrix(shape.0, shape.1)
   }
}

prop_compose! {
   fn equal_shape_bitmatrices(max_dimension: usize)(shape in (1..=max_dimension, 1..=max_dimension)) -> (BitMatrix, BitMatrix) {
       (random_bitmatrix(shape.0, shape.1), random_bitmatrix(shape.0, shape.1))
   }
}

// #[test]
// fn reduce() {
//     for _ in 0..100 {
//         let array = random_bitmatrix(100, 100);
//         let reduced = rref(array);
//         assert!(is_rref(&reduced));
//     }

//     for _ in 0..100 {
//         let array = random_bitmatrix(50, 100);
//         let (reduced, profile) = rref_with_rank_profile(array);
//         assert_eq!(profile.len(), reduced.rowcount());
//         assert!(is_rref(&reduced));
//     }

//     {
//         let matrix = bitmatrix!(
//             |10 011 01|
//             |.. 111 01|
//             |.. ... 10|);
//         assert!(is_rref(&matrix));
//         let (reduced, profile) = rref_with_rank_profile(matrix);
//         assert!(is_rref(&reduced));
//         assert_eq!(profile, vec![0, 2, 5]);
//     }
// }

#[test]
fn reduce_with_transforms() {
    for _ in 0..100 {
        check_rref_with_transforms_on_random_matrix(100, 100);
    }
    for _ in 0..100 {
        check_rref_with_transforms_on_random_matrix(50, 100);
    }
}

fn check_rref_with_transforms_on_random_matrix(nrows: usize, ncols: usize) {
    let array = random_bitmatrix(nrows, ncols);
    let (reduced, t, t_inv_t, profile) = rref_with_transforms(array.clone());
    assert!(is_rref(&reduced, &profile));
    assert_eq!(t.dot(&array), reduced);
    assert_eq!(
        t.dot(&t_inv_t.transposed()),
        BitMatrix::identity(array.rowcount())
    );
}

#[test]
fn test_dot() {
    println!("0");
    let x = bitmatrix!(
        |01|
        |10|);
    let id = bitmatrix!(
        |10|
        |01|);
    println!("1");
    assert_eq!(x.dot(&x), id);
    assert_eq!(x.dot(&id), x);
    assert_eq!(id.dot(&x), x);

    // multiplication is associative
    println!("2");
    for _ in 0..100 {
        let a = random_bitmatrix(10, 10);
        let b = random_bitmatrix(10, 10);
        let c = random_bitmatrix(10, 10);
        assert_eq!((a.dot(&b)).dot(&c), a.dot(&b.dot(&c)));
    }

    println!("3");
    // multiplication by zero is zero
    for _ in 0..100 {
        let a = random_bitmatrix(10, 10);
        let z = BitMatrix::zeros(10, 10);
        assert_eq!(a.dot(&z), z);
    }

    // multiplication by id
    for _ in 0..100 {
        let a = random_bitmatrix(3, 3);
        let id = BitMatrix::identity(3);
        assert_eq!(a.dot(&id), a);
    }
}

#[test]
fn test_kernel_basis() {
    let num_cols = 100;
    for _ in 0..100 {
        let mut matrix = random_bitmatrix(50, 100);
        let rrp = matrix.echelonize();
        let mut kernel_basis_matrix = kernel_basis_matrix(&matrix);
        let prod = matrix.dot(&kernel_basis_matrix.transposed());
        assert!(prod.is_zero());
        let rrpc = kernel_basis_matrix.echelonize();
        assert_eq!(rrp.len() + rrpc.len(), num_cols);
    }
}

fn preserves_rowspan_of(matrix: &BitMatrix, rref_matrix: &BitMatrix) -> bool {
    let profile = fast_profile_of(rref_matrix);
    let mut profile_rows = BTreeMap::new();
    for (row_index, column_index) in profile.iter().enumerate() {
        profile_rows.insert(column_index, row_index);
    }
    for row in matrix.rows() {
        let mut reduced = BitVec::<WORD_COUNT_DEFAULT>::from_view(&row);
        let support = row
            .support()
            .assume_sorted_by_item()
            .intersection(profile.iter().copied().assume_sorted_by_item());

        for column_index in support {
            let row_index = profile_rows[&column_index];
            let rref_row = BitVec::<WORD_COUNT_DEFAULT>::from_view(&rref_matrix.row(row_index));
            reduced.bitxor_assign(&rref_row);
        }
        if reduced.weight() > 0 {
            return false;
        }
    }
    true
}

fn is_rref(matrix: &BitMatrix, with_profile: &[usize]) -> bool {
    let expected_profile = fast_profile_of(matrix);
    (expected_profile == with_profile) && columns_are_pivots_of(matrix, with_profile)
}

fn columns_are_pivots_of(matrix: &BitMatrix, column_indexes: &[usize]) -> bool {
    for &column_index in column_indexes {
        let column = matrix.column(column_index);
        if column.weight() != 1 {
            return false;
        }
    }
    true
}

fn fast_profile_of(matrix: &BitMatrix) -> Vec<usize> {
    let mut profile = vec![];
    for row_index in 0..matrix.rowcount() {
        let row = matrix.row(row_index);
        let pivot = row.into_iter().position(|bit| bit);
        if pivot.is_none() {
            break;
        }
        profile.push(pivot.unwrap());
    }
    profile
}

fn random_bitmatrix(rowcount: usize, columncount: usize) -> BitMatrix {
    let mut matrix = BitMatrix::with_shape(rowcount, columncount);
    let mut bits = std::iter::from_fn(move || Some(thread_rng().gen::<bool>()));
    for row_index in 0..rowcount {
        for column_index in 0..columncount {
            matrix.set((row_index, column_index), bits.next().expect("boom"));
        }
    }
    for _ in 0..rowcount {
        let from_index = thread_rng().gen_range(0..rowcount);
        let to_index = thread_rng().gen_range(0..rowcount);
        matrix.swap_rows(from_index, to_index);
    }
    matrix
}
