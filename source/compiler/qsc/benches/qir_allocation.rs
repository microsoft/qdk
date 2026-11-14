// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

allocator::assign_global!();

use criterion::{Criterion, criterion_group, criterion_main};
use qsc::{PackageType, TargetCapabilityFlags, compile, interpret::Interpreter};
use qsc_data_structures::{language_features::LanguageFeatures, source::SourceMap};
use qsc_eval::output::GenericReceiver;
use std::hint::black_box;

const SOURCE: &str = "
operation Allocate(n : Int) : Unit {
    use qs = Qubit[n];
}
";
const EXPRESSION: &str = "Allocate(20000)";

/// Measures QIR generation without timing initialization or source loading.
fn qubit_array_allocation(c: &mut Criterion) {
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = compile::package_store_with_stdlib(capabilities);
    let mut interpreter = Interpreter::new(
        SourceMap::default(),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .expect("interpreter should initialize");
    interpreter
        .eval_fragments(&mut GenericReceiver::new(&mut std::io::sink()), SOURCE)
        .expect("allocation source should compile");
    let qir = interpreter
        .qirgen(EXPRESSION)
        .expect("allocation should generate QIR");
    assert!(qir.warnings.is_empty(), "{:?}", qir.warnings);
    assert!(qir.qir.contains("\"required_num_qubits\"=\"20000\""));

    c.bench_function("qir/qubit_array_allocation/20000", |b| {
        b.iter(|| {
            black_box(
                interpreter
                    .qirgen(black_box(EXPRESSION))
                    .expect("allocation should generate QIR"),
            )
        });
    });
}

criterion_group!(benches, qubit_array_allocation);
criterion_main!(benches);
