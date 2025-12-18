// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

allocator::assign_global!();

use criterion::{Criterion, criterion_group, criterion_main};
use qsc::{
    TargetCapabilityFlags,
    compile::{self, compile},
};
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_data_structures::source::SourceMap;
use qsc_frontend::compile::PackageStore;
use qsc_passes::PackageType;

const INPUT: &str = include_str!("./large.qs");

pub fn large_file(c: &mut Criterion) {
    c.bench_function("Large input file compilation", |b| {
        let mut store = PackageStore::new(compile::core());
        let std_id = store.new_package_id();
        store.insert(
            std_id,
            compile::std(std_id, &store, TargetCapabilityFlags::all()),
        );
        b.iter(|| {
            let sources = SourceMap::new([("large.qs".into(), INPUT.into())], None);
            let (_, reports) = compile(
                &store,
                &[(std_id, None)],
                sources,
                PackageType::Exe,
                store.peek_next_package_id(),
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
            );
            assert!(reports.is_empty());
        });
    });
}

pub fn large_file_interpreter(c: &mut Criterion) {
    c.bench_function("Large input file compilation (interpreter)", |b| {
        b.iter(|| {
            let sources = SourceMap::new([("large.qs".into(), INPUT.into())], None);
            let (std_id, store) =
                qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());

            let _evaluator = qsc::interpret::Interpreter::new(
                sources,
                PackageType::Exe,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("code should compile");
        });
    });
}

criterion_group!(benches, large_file, large_file_interpreter);
criterion_main!(benches);
