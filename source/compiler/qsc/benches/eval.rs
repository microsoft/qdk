// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

allocator::assign_global!();

use criterion::{Criterion, criterion_group, criterion_main};
use indoc::indoc;
use qsc::{PackageType, TargetCapabilityFlags, interpret::Interpreter};
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_data_structures::source::SourceMap;
use qsc_eval::output::GenericReceiver;

const TELEPORT: &str = include_str!("../../../../samples/algorithms/Teleportation.qs");
const DEUTSCHJOZSA: &str = include_str!("../../../../samples/algorithms/DeutschJozsa.qs");
const LARGE: &str = include_str!("./large.qs");
const ARRAY_LITERAL: &str = include_str!("./array_literal");
const BENCH5X5: &str = include_str!("./bench5x5.qs");

pub fn teleport(c: &mut Criterion) {
    c.bench_function("Teleport evaluation", |b| {
        let sources = SourceMap::new([("Teleportation.qs".into(), TELEPORT.into())], None);
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");
        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn deutsch_jozsa(c: &mut Criterion) {
    c.bench_function("Deutsch-Jozsa evaluation", |b| {
        let sources = SourceMap::new([("DeutschJozsa.qs".into(), DEUTSCHJOZSA.into())], None);
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");
        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn large_file(c: &mut Criterion) {
    c.bench_function("Large file parity evaluation", |b| {
        let sources = SourceMap::new([("large.qs".into(), LARGE.into())], None);
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn bench5x5(c: &mut Criterion) {
    c.bench_function("Bench 5x5 evaluation", |b| {
        let sources = SourceMap::new([("bench5x5.qs".into(), BENCH5X5.into())], None);
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn array_append(c: &mut Criterion) {
    c.bench_function("Array append evaluation", |b| {
        let sources = SourceMap::new(
            [("none".into(), "".into())],
            Some(
                indoc! {"{
            mutable arr = [];
            for i in 0..999 {
                set arr += [i];
            }
            arr
        }"}
                .into(),
            ),
        );
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn array_update(c: &mut Criterion) {
    c.bench_function("Array update evaluation", |b| {
        let sources = SourceMap::new(
            [("none".into(), "".into())],
            Some(
                indoc! {"{
            mutable arr = [0, size = 10000];
            for i in 0..999 {
                set arr w/= i <- i;
            }
            arr
        }"}
                .into(),
            ),
        );
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn array_literal(c: &mut Criterion) {
    c.bench_function("Array literal evaluation", |b| {
        let sources = SourceMap::new([("none".into(), "".into())], Some(ARRAY_LITERAL.into()));
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn large_nested_iteration(c: &mut Criterion) {
    c.bench_function("Large nested iteration", |b| {
        let sources = SourceMap::new(
            [("none".into(), "".into())],
            Some(
                indoc! {"{
                    import Std.Arrays.*;
                    mutable arr = [[0, size = 100], size = 1000];
                    for i in IndexRange(arr) {
                        mutable inner = arr[i];
                        for j in IndexRange(inner) {
                            set inner w/= j <- j;
                        }
                        set arr w/= i <- inner;
                    }
                    arr
                }"}
                .into(),
            ),
        );
        let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let mut evaluator = Interpreter::new(
            sources,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
            Default::default(),
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

fn interpreter_for(source: &str) -> Interpreter {
    let sources = SourceMap::new([("bench.qs".into(), source.into())], None);
    let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
    Interpreter::new(
        sources,
        PackageType::Exe,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("code should compile")
}

fn bench_entry(c: &mut Criterion, name: &str, source: &str) {
    c.bench_function(name, |b| {
        let mut evaluator = interpreter_for(source);
        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
}

pub fn recursive_fibonacci(c: &mut Criterion) {
    bench_entry(
        c,
        "Recursive Fibonacci evaluation",
        indoc! {"
            function Fib(n : Int) : Int {
                if n < 2 { n } else { Fib(n - 1) + Fib(n - 2) }
            }
            @EntryPoint()
            function Main() : Int { Fib(20) }
        "},
    );
}

pub fn nested_loop_arithmetic(c: &mut Criterion) {
    bench_entry(
        c,
        "Nested loop arithmetic evaluation",
        indoc! {"
            @EntryPoint()
            function Main() : Int {
                mutable sum = 0;
                for i in 0..299 {
                    for j in 0..299 {
                        set sum += (i * j) % 7;
                    }
                }
                sum
            }
        "},
    );
}

pub fn higher_order_functions(c: &mut Criterion) {
    bench_entry(
        c,
        "Higher-order functions evaluation",
        indoc! {"
            import Std.Arrays.*;
            @EntryPoint()
            function Main() : Int {
                let xs = SequenceI(0, 9999);
                let squares = Mapped(x -> x * x, xs);
                let evens = Filtered(x -> x % 2 == 0, squares);
                Fold((acc, x) -> acc + x % 1000, 0, evens)
            }
        "},
    );
}

pub fn quantum_fourier_transform(c: &mut Criterion) {
    bench_entry(
        c,
        "QFT evaluation",
        indoc! {"
            import Std.Canon.*;
            import Std.Arrays.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[10];
                for _ in 1..10 {
                    ApplyQFT(qs);
                    Adjoint ApplyQFT(qs);
                }
                MResetEachZ(qs)
            }
        "},
    );
}

criterion_group!(
    benches,
    teleport,
    deutsch_jozsa,
    large_file,
    bench5x5,
    array_append,
    array_update,
    array_literal,
    large_nested_iteration,
    recursive_fibonacci,
    nested_loop_arithmetic,
    higher_order_functions,
    quantum_fourier_transform,
);
criterion_main!(benches);
