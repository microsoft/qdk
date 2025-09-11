use paulimer::bits::BitMatrix;
use rand::prelude::*;

fn main() {
    let mut matrix = random_bitmatrix(10000, 10000);
    matrix.echelonize();
}

fn random_bitmatrix(rowcount: usize, columncount: usize) -> BitMatrix {
    let mut matrix = BitMatrix::with_shape(rowcount, columncount);
    let mut bits = std::iter::from_fn(move || Some(thread_rng().gen::<bool>()));
    for row_index in 0..rowcount {
        for column_index in 0..columncount {
            matrix.set((row_index, column_index), bits.next().expect("boom"));
        }
    }
    matrix
}
