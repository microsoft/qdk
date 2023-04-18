// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use criterion::{criterion_group, criterion_main, Criterion};
use qsc_frontend::compile::{self, compile, PackageStore};
use qsc_passes::run_default_passes;

const INPUT: &str = include_str!("./nqueens.qs");

pub fn nqueens(c: &mut Criterion) {
    c.bench_function("NQueens large input file", |b| {
        b.iter(|| {
            let mut std = compile::std();
            assert!(std.context.errors().is_empty());
            assert!(run_default_passes(&mut std).is_empty());
            let mut store = PackageStore::new();
            let stdlib = store.insert(std);
            let mut unit = compile(&store, [stdlib], [INPUT], "");
            assert!(unit.context.errors().is_empty());
            assert!(run_default_passes(&mut unit).is_empty());
        })
    });
}

criterion_group!(benches, nqueens);
criterion_main!(benches);
