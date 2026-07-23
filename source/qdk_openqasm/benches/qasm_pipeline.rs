// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use qdk_openqasm::{
    analyze_source, parse_source, semantic::lower_parse_result, tokens::tokenize, unparse::unparse,
};

#[allow(dead_code, unused_imports)]
mod corpus;

use corpus::{
    Corpus, ExactSize, broadcast_gate, directive_heavy, exact_size, flat_gate, include_heavy,
};

fn assert_parse_success(corpus: &Corpus, result: &qdk_openqasm::parser::ParseResult) {
    assert!(
        !result.has_errors(),
        "{} parse corpus produced {} errors",
        corpus.name,
        result.all_errors().len()
    );
}

fn assert_semantic_success(corpus: &Corpus, result: &qdk_openqasm::semantic::AnalysisResult) {
    assert!(
        !result.has_errors(),
        "{} semantic corpus produced {} errors",
        corpus.name,
        result.all_errors().len()
    );
}

fn parse(corpus: &Corpus) -> qdk_openqasm::parser::ParseResult {
    let mut resolver = corpus.resolver();
    let result = parse_source(
        corpus.source.clone(),
        corpus.path.clone(),
        Some(&mut resolver),
    );
    assert_parse_success(corpus, &result);
    result
}

fn analyze(corpus: &Corpus) -> qdk_openqasm::semantic::AnalysisResult {
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

    group.bench_function("qdk_dumps", |b| {
        b.iter_batched(
            || parse(corpus),
            |parse_result| {
                let program = parse_result
                    .source
                    .program()
                    .expect("successful syntax parse should retain its program");
                black_box(unparse(program).expect("valid corpus should serialize"));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("qdk_tokenize", |b| {
        b.iter(|| black_box(tokenize(black_box(&corpus.source))));
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
        directive_heavy(1_024),
    ] {
        bench_corpus(c, &corpus);
    }
}

pub fn exact_size_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("exact_size");

    for size in ExactSize::ALL {
        let corpus = exact_size(size);
        group.throughput(Throughput::Bytes(
            corpus.source_bytes().try_into().unwrap_or(u64::MAX),
        ));

        group.bench_with_input(
            BenchmarkId::new("parse_source", size.label()),
            &corpus,
            |b, corpus| b.iter(|| black_box(parse(black_box(corpus)))),
        );
        group.bench_with_input(
            BenchmarkId::new("analyze_source", size.label()),
            &corpus,
            |b, corpus| b.iter(|| black_box(analyze(black_box(corpus)))),
        );
    }

    group.finish();
}

criterion_group!(benches, qasm_pipeline, exact_size_pipeline);
criterion_main!(benches);
