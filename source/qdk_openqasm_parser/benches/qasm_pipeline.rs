// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use qdk_openqasm_parser::{analyze_source, parse_source, semantic::lower_parse_result};

mod corpus;

use corpus::{Corpus, broadcast_gate, flat_gate, include_heavy};

fn assert_parse_success(corpus: &Corpus, result: &qdk_openqasm_parser::parser::QasmParseResult) {
    assert!(
        !result.has_errors(),
        "{} parse corpus produced {} errors",
        corpus.name,
        result.all_errors().len()
    );
}

fn assert_semantic_success(
    corpus: &Corpus,
    result: &qdk_openqasm_parser::semantic::QasmSemanticParseResult,
) {
    assert!(
        !result.has_errors(),
        "{} semantic corpus produced {} errors",
        corpus.name,
        result.all_errors().len()
    );
}

fn parse(corpus: &Corpus) -> qdk_openqasm_parser::parser::QasmParseResult {
    let mut resolver = corpus.resolver();
    let result = parse_source(
        corpus.source.clone(),
        corpus.path.clone(),
        Some(&mut resolver),
    );
    assert_parse_success(corpus, &result);
    result
}

fn analyze(corpus: &Corpus) -> qdk_openqasm_parser::semantic::QasmSemanticParseResult {
    let mut resolver = corpus.resolver();
    let result = analyze_source(
        corpus.source.clone(),
        corpus.path.clone(),
        Some(&mut resolver),
    );
    assert_semantic_success(corpus, &result);
    result
}

fn bench_corpus(c: &mut Criterion, corpus: &Corpus) {
    let mut group = c.benchmark_group(corpus.name);
    group.throughput(criterion::Throughput::Elements(
        corpus.statement_count.try_into().unwrap_or(u64::MAX),
    ));

    group.bench_function("parse", |b| {
        b.iter(|| black_box(parse(black_box(corpus))));
    });

    group.bench_function("semantic_lower", |b| {
        b.iter_batched(
            || parse(corpus),
            |parse_result| {
                let result = lower_parse_result(parse_result);
                assert_semantic_success(corpus, &result);
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("analyze", |b| {
        b.iter(|| black_box(analyze(black_box(corpus))));
    });

    group.finish();
}

pub fn qasm_pipeline(c: &mut Criterion) {
    for corpus in [
        flat_gate(1_024),
        broadcast_gate(256, 32),
        include_heavy(64, 8),
    ] {
        bench_corpus(c, &corpus);
    }
}

criterion_group!(benches, qasm_pipeline);
criterion_main!(benches);
