// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use paulimer::bits::BitMatrix;
use rand::prelude::*;

struct Parameters((f64, usize));

pub fn echelon_form_benchmark(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Bitmatrix::echelon_form");
    for sparsity in [0.5, 0.1, 0.01, 0.001] {
        for size in [100usize, 1000usize, 10000usize] {
            group.sample_size(10);
            let parameters = Parameters((sparsity, size));
            group.bench_with_input(
                BenchmarkId::from_parameter(&parameters),
                &parameters,
                |bencher, parameters| {
                    let (sparsity, size) = parameters.0;
                    bencher.iter_batched(
                        || random_bitmatrix(size, size, sparsity),
                        |mut matrix| matrix.echelonize(),
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}

impl std::fmt::Display for Parameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)?;
        Ok(())
    }
}

criterion_group!(benches, echelon_form_benchmark);
criterion_main!(benches);

fn random_bitmatrix(rowcount: usize, columncount: usize, sparsity: f64) -> BitMatrix {
    let mut matrix = BitMatrix::with_shape(rowcount, columncount);
    let mut bits = std::iter::from_fn(move || Some(thread_rng().gen_bool(sparsity)));
    for row_index in 0..rowcount {
        for column_index in 0..columncount {
            matrix.set((row_index, column_index), bits.next().expect("boom"));
        }
    }
    matrix
}
