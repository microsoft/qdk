// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

allocator::assign_global!();

use criterion::{Criterion, criterion_group, criterion_main};
use indoc::indoc;
use qsc::{PackageType, TargetCapabilityFlags, interpret::Interpreter};
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_eval::output::GenericReceiver;
use qsc_frontend::compile::SourceMap;

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
        )
        .expect("code should compile");

        b.iter(move || {
            let mut out = Vec::new();
            let mut rec = GenericReceiver::new(&mut out);
            assert!(evaluator.eval_entry(&mut rec).is_ok());
        });
    });
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
);
criterion_main!(benches);
