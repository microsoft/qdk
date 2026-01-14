// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use qsc_openqasm_compiler::{
    CompilerConfig, OutputSemantics, ProgramType, QasmCompileUnit, QubitSemantics,
    compiler::parse_and_compile_to_qsharp_ast_with_config,
};
use qsc_openqasm_parser::io::InMemorySourceResolver;

fn rgqft_multiplier<S: Into<Arc<str>>>(source: S) -> QasmCompileUnit {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    parse_and_compile_to_qsharp_ast_with_config(
        source,
        "",
        None::<&mut InMemorySourceResolver>,
        config,
    )
}

pub fn rgqft_multiplier_1q(c: &mut Criterion) {
    const SOURCE: &str = include_str!("./rgqft_multiplier_1q.qasm");

    c.bench_function("rgqft_multiplier_1q sample compilation", |b| {
        b.iter(move || black_box(rgqft_multiplier(SOURCE)));
    });
}

pub fn rgqft_multiplier_4q(c: &mut Criterion) {
    const SOURCE: &str = include_str!("./rgqft_multiplier_4q.qasm");

    c.bench_function("rgqft_multiplier_4q sample compilation", |b| {
        b.iter(move || black_box(rgqft_multiplier(SOURCE)));
    });
}

criterion_group!(benches, rgqft_multiplier_1q, rgqft_multiplier_4q);
criterion_main!(benches);
