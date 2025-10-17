// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use criterion::{Criterion, criterion_group, criterion_main};
use indoc::indoc;
use qsc::{
    LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags, interpret::Interpreter,
};

pub fn deep_nested_callable_generics(c: &mut Criterion) {
    c.bench_function("Deeply nested callable generics", |b| {
        b.iter(move || {
            let sources = SourceMap::new(
                [("none".into(), "".into())],
                Some(
                    indoc! {"{
                    function swap<'T1, 'T2>(op : ('T1, 'T2) -> ('T2, 'T1), input: ('T1, 'T2)) : ('T2, 'T1) {
                        op(input)
                    }
                    function nested_swaps() : Unit {
                        let a = swap(swap(swap(swap(swap(_)))));
                    }
            }"}
                    .into(),
                ),
            );
            let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());

            assert!(Interpreter::new(
                sources,
                PackageType::Exe,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .is_err(), "code should fail with type error");
        });
    });
}

criterion_group!(benches, deep_nested_callable_generics);
criterion_main!(benches);
