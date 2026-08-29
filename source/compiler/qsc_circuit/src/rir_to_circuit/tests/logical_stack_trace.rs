// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::builder::WireMapBuilder;
use crate::rir_to_circuit::build_operation_list;
use crate::{
    builder::{
        GateInputs, LogicalStack, LogicalStackWithSourceLookup, OperationReceiver, ScopeStack,
        WireMap,
    },
    rir_to_circuit::{VariableTracker, reconstruct_control_flow},
};
use expect_test::Expect;
use expect_test::expect;
use indoc::indoc;
use qsc_codegen::qir::fir_to_rir;
use qsc_data_structures::{
    functors::FunctorApp, index_map::IndexMap, language_features::LanguageFeatures,
    source::SourceMap, target::Profile,
};
use qsc_fir::fir::{self};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_partial_eval::{PartialEvalConfig, ProgramEntry};
use qsc_passes::{PackageType, run_core_passes, run_default_passes};
use qsc_rir::debug::DbgScope;

// A simple test receiver that records the formatted call stack and gate name
// for each received operation, one per line.
struct TestOperationReceiver<'a> {
    trace: String,
    source_lookup: &'a (&'a compile::PackageStore, &'a fir::PackageStore),
}

impl TestOperationReceiver<'_> {
    fn append_line(&mut self, stack: LogicalStack, line: &str) {
        let formatted = LogicalStackWithSourceLookup {
            trace: stack,
            source_lookup: self.source_lookup,
        }
        .to_string();

        self.trace.push_str(&formatted);
        self.trace.push_str(" -> ");
        self.trace.push_str(line);
        self.trace.push('\n');
    }
}

impl OperationReceiver for TestOperationReceiver<'_> {
    fn gate(
        &mut self,
        _wire_map: &WireMap,
        name: &str,
        is_adjoint: bool,
        inputs: &GateInputs,
        _args: Vec<String>,
        call_stack: LogicalStack,
    ) {
        let targets = inputs
            .targets
            .iter()
            .map(|q| format!("q_{q}"))
            .collect::<Vec<_>>()
            .join(", ");

        let controls = inputs
            .controls
            .iter()
            .map(|q| format!("q_{q}"))
            .collect::<Vec<_>>()
            .join(", ");

        self.append_line(
            call_stack,
            &format!(
                "gate({}{}{}, targets=({}), controls=({}))",
                name,
                if is_adjoint { "†" } else { "" },
                "",
                targets,
                controls,
            ),
        );
    }

    fn measurement(
        &mut self,
        _wire_map: &WireMap,
        name: &str,
        qubit: usize,
        result: usize,
        call_stack: LogicalStack,
    ) {
        self.append_line(
            call_stack,
            &format!("measure({name}, q_{qubit}, c_{result})"),
        );
    }

    fn reset(&mut self, _wire_map: &WireMap, qubit: usize, call_stack: LogicalStack) {
        self.append_line(call_stack, &format!("reset(q_{qubit})"));
    }
}

fn check_trace(file: &str, expr: &str, expect: &Expect) {
    let (trace, _, ()) = compile_and_trace(file, expr, None, |_, _, _| ());
    expect.assert_eq(&trace);
}

#[allow(clippy::too_many_lines)]
fn compile_and_trace<T>(
    file: &str,
    expr: &str,
    dependency: Option<(&str, &str, Option<&str>)>,
    mutate_fir: impl FnOnce(&mut fir::PackageStore, fir::PackageId, Option<fir::PackageId>) -> T,
) -> (String, qsc_rir::rir::Program, T) {
    let capabilities = Profile::AdaptiveRIF.into();
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    let core_fir =
        fir_lowerer.lower_package(&core.package, &fir_store, qsc_fir::fir::PackageId::CORE);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, capabilities);
    assert!(std.errors.is_empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std_id = store.insert(std);
    let std_fir = fir_lowerer.lower_package(
        &store.get(std_id).expect("package should exist").package,
        &fir_store,
        map_hir_package_to_fir(std_id),
    );

    let mut dependencies = vec![(std_id, None)];
    let dependency_fir = dependency.map(|(source_name, source, entry)| {
        let sources = SourceMap::new([(source_name.into(), source.into())], entry.map(Into::into));
        let mut unit = compile(
            &store,
            &[(std_id, None)],
            sources,
            capabilities,
            LanguageFeatures::default(),
        );
        assert!(unit.errors.is_empty(), "{:?}", unit.errors);
        let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
        assert!(pass_errors.is_empty(), "{pass_errors:?}");
        let dependency_id = store.insert(unit);
        let dependency_fir_id = map_hir_package_to_fir(dependency_id);
        let dependency_fir = qsc_lowerer::Lowerer::new().lower_package(
            &store
                .get(dependency_id)
                .expect("dependency package should exist")
                .package,
            &fir_store,
            dependency_fir_id,
        );
        dependencies.push((dependency_id, None));
        (dependency_fir_id, dependency_fir)
    });

    let sources = SourceMap::new([("A.qs".into(), file.into())], Some(expr.into()));
    let mut unit = compile(
        &store,
        &dependencies,
        sources,
        capabilities,
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let id = store.insert(unit);
    let unit_fir = qsc_lowerer::Lowerer::new().lower_package(
        &store.get(id).expect("package should exist").package,
        &fir_store,
        map_hir_package_to_fir(id),
    );

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    let dependency_id = dependency_fir
        .as_ref()
        .map(|(dependency_id, _)| *dependency_id);
    if let Some((dependency_id, dependency_fir)) = dependency_fir {
        fir_store.insert(dependency_id, dependency_fir);
    }
    let id = map_hir_package_to_fir(id);
    fir_store.insert(id, unit_fir);

    let package = fir_store.get(id);
    let entry = ProgramEntry {
        exec_graph: package.entry_exec_graph.clone(),
        expr: (
            id,
            package
                .entry
                .expect("package must have an entry expression"),
        )
            .into(),
    };
    let fixture = mutate_fir(&mut fir_store, id, dependency_id);
    let compute_properties =
        qsc_passes::PassContext::run_fir_passes_on_fir(&fir_store, id, capabilities)
            .expect("FIR passes should succeed");

    let (_, rir) = fir_to_rir(
        &fir_store,
        capabilities,
        &compute_properties,
        &entry,
        PartialEvalConfig {
            generate_debug_metadata: true,
        },
    )
    .expect("RIR lowering should succeed");

    let mut program_map = VariableTracker {
        variables: IndexMap::default(),
        blocks_to_control_results: IndexMap::default(),
    };

    let entry_block_id = rir
        .callables
        .get(rir.entry)
        .expect("entry callable should exist")
        .body
        .expect("entry callable should have a body");
    let structured_control_flow = reconstruct_control_flow(&rir.blocks, entry_block_id);

    let mut builder = TestOperationReceiver {
        trace: String::new(),
        source_lookup: &(&store, &fir_store),
    };
    let num_qubits = rir.num_qubits.try_into().expect("num qubits fits in usize");
    let mut wire_map_builder = WireMapBuilder::default();
    for id in 0..num_qubits {
        wire_map_builder.map_qubit(id, None);
    }

    if let Err(err) = build_operation_list(
        &mut program_map,
        &rir,
        &mut wire_map_builder,
        &mut builder,
        &structured_control_flow,
        &[],
        &ScopeStack::top(),
        &(&store, &fir_store),
    ) {
        panic!("error building operation list: {err}");
    }

    (builder.trace, rir, fixture)
}

#[test]
fn no_sim_calls() {
    check_trace(
        indoc! {r#"
        operation Main() : Unit {
            for i in 0..2 {
                Message("Hello");
            }
        }
        "#},
        "A.Main()",
        &expect![""],
    );
}

#[test]
fn gate() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            X(q);
            X(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn toffoli() {
    check_trace(
        indoc! {"
            operation Main() : Unit {
                use q = Qubit[3];
                CCNOT(q[0], q[1], q[2]);
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> CCNOT@qsharp-library-source:Std/Intrinsic.qs:75:8 -> gate(X, targets=(q_2), controls=(q_0, q_1))
        "#]],
    );
}

#[test]
fn multi_qubit_alloc() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit[3];
        }
        "},
        "A.Main()",
        &expect![""],
    );
}

#[test]
fn qubit_alloc_in_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            for i in 1..2 {
                use q = Qubit();
                H(q);
            }
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[1] -> (1)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[2] -> (2)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn nested_callables() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            Foo(q);
            MResetZ(q);
        }

        operation Foo(q: Qubit) : Unit {
            H(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> Foo@A.qs:7:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:3:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_0)
        "#]],
    );
}

#[test]
fn for_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            for i in 0..2 {
                H(q);
            }
            MResetZ(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:5:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_0)
        "#]],
    );
}

#[test]
fn nested_callables_and_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            for i in 0..2 {
                Foo(q);
            }
            MResetZ(q);
        }

        operation Foo(q: Qubit) : Unit {
            H(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:5:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_0)
        "#]],
    );
}

#[test]
fn while_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            mutable i = 0;
            while (i < 2) {
                Foo(q);
                set i += 1;
            }
        }

        operation Foo(q: Qubit) : Unit {
            Y(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> loop: i < 2@A.qs:3:18[1] -> (1)@A.qs:4:8 -> Foo@A.qs:10:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 2@A.qs:3:18[2] -> (2)@A.qs:4:8 -> Foo@A.qs:10:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn while_loop_different_iterations() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();
            mutable i = 0;
            while (i < 7) {
                if (i % 3 == 0) {
                    set i += 2;
                } else {
                    set i += 1;
                }

                if (i % 2 == 0) {
                    Foo(q);
                } else {
                    X(q);
                }
            }
        }

        operation Foo(q: Qubit) : Unit {
            Y(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[1] -> (1)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[2] -> (2)@A.qs:13:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[3] -> (3)@A.qs:13:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[4] -> (4)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[5] -> (5)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn nested_for_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use qs = Qubit[2];
            for j in 0..2 {
                for i in 0..1 {
                    Foo(qs[i]);
                }
            }
        }

        operation Foo(q: Qubit) : Unit {
            X(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
        "#]],
    );
}

#[test]
fn qubit_reuse() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            {
                use q1 = Qubit();
                X(q1);
                MResetZ(q1);
            }
            {
                use q2 = Qubit();
                Y(q2);
                MResetZ(q2);
            }
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:4:8 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_0)
            Main@A.qs:8:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:9:8 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_1)
        "#]],
    );
}

#[test]
fn custom_intrinsic() {
    check_trace(
        indoc! {"
        operation foo(n: Int, q: Qubit): Unit {
            body intrinsic;
        }

        operation Main() : Unit {
            use q = Qubit();
            X(q);
            foo(4, q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:6:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:7:4 -> gate(foo, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn adjoint_operation_implicit_specialization() {
    check_trace(
        indoc! {"
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
                Adjoint Foo(q);
            }

            operation Foo(q : Qubit) : Unit is Adj {
                body (...) {
                    X(q);
                    Y(q);
                }
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> Foo@A.qs:8:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> Foo@A.qs:9:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:9:8 -> Y†@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:8:8 -> X†@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn adjoint_operation_explicit_specialization() {
    check_trace(
        indoc! {"
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
                Adjoint Foo(q);
            }

            operation Foo(q : Qubit) : Unit is Adj {
                body (...) {
                    X(q);
                }

                adjoint (...) {
                    Y(q);
                }
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> Foo@A.qs:8:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:12:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn relocated_rir_scopes_preserve_source_and_storage_identity() {
    let dependency_source = indoc! {"
        namespace Dependency {
            operation Foreign(q : Qubit) : Unit {
                for index in 0..0 {
                    H(q);
                }
            }
            export Foreign;
        }
    "};
    let user_source = indoc! {"
        namespace A {
            operation Main(q : Qubit) : Unit {
                for index in 0..0 {
                    H(q);
                }
            }
        }
    "};
    let (trace, rir, (user_package, dependency_package)) = compile_and_trace(
        user_source,
        indoc! {"
            {
                use qs = Qubit[2];
                Dependency.Foreign(qs[0]);
                A.Main(qs[1]);
            }
        "},
        Some((
            "Dependency.qs",
            dependency_source,
            Some(indoc! {"
                {
                    use qs = Qubit[2];
                    Dependency.Foreign(qs[0]);
                    Dependency.Foreign(qs[1]);
                }
            "}),
        )),
        |fir_store, user_package, dependency_package| {
            let dependency_package = dependency_package.expect("dependency package should exist");
            let (
                dependency_callable_span,
                dependency_loop_id,
                dependency_loop_span,
                dependency_condition_span,
                dependency_body_span,
                dependency_gate_span,
            ) = {
                let package = fir_store.get(dependency_package);
                let callable_span = package
                    .items
                    .values()
                    .find_map(|item| match &item.kind {
                        fir::ItemKind::Callable(decl) if decl.name.name.as_ref() == "Foreign" => {
                            let fir::CallableImpl::Spec(spec_impl) = &decl.implementation else {
                                panic!("Foreign should have specializations");
                            };
                            Some(spec_impl.body.span)
                        }
                        _ => None,
                    })
                    .expect("Foreign callable should exist");
                let (loop_id, loop_expr, condition, body) = package
                    .exprs
                    .iter()
                    .find_map(|(expr_id, expr)| match expr.kind {
                        fir::ExprKind::While(condition, body) => {
                            Some((expr_id, expr, condition, body))
                        }
                        _ => None,
                    })
                    .expect("dependency loop should exist");
                let loop_span = loop_expr.span;
                let gate_span = package
                    .exprs
                    .values()
                    .find(|expr| {
                        matches!(expr.kind, fir::ExprKind::Call(..))
                            && expr.span.package == loop_span.package
                            && expr.span.lo >= loop_span.lo
                            && expr.span.hi <= loop_span.hi
                    })
                    .expect("dependency gate call should exist")
                    .span;
                (
                    callable_span,
                    loop_id,
                    loop_expr.span,
                    package
                        .exprs
                        .get(condition)
                        .expect("dependency condition should exist")
                        .span,
                    package
                        .blocks
                        .get(body)
                        .expect("dependency loop body should exist")
                        .span,
                    gate_span,
                )
            };
            let (user_loop_id, user_condition, user_body, user_gates) = {
                let package = fir_store.get(user_package);
                let (loop_id, loop_span, condition, body) = package
                    .exprs
                    .iter()
                    .find_map(|(expr_id, expr)| match expr.kind {
                        fir::ExprKind::While(condition, body) => {
                            Some((expr_id, expr.span, condition, body))
                        }
                        _ => None,
                    })
                    .expect("user loop should exist");
                let gates = package
                    .exprs
                    .iter()
                    .filter_map(|(expr_id, expr)| {
                        (matches!(expr.kind, fir::ExprKind::Call(..))
                            && expr.span.package == loop_span.package
                            && expr.span.lo >= loop_span.lo
                            && expr.span.hi <= loop_span.hi)
                            .then_some(expr_id)
                    })
                    .collect::<Vec<_>>();
                assert!(!gates.is_empty(), "user gate call should exist");
                (loop_id, condition, body, gates)
            };
            assert_eq!(dependency_loop_id, user_loop_id);

            let package = fir_store.get_mut(user_package);
            let main = package
                .items
                .values_mut()
                .find_map(|item| match &mut item.kind {
                    fir::ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => {
                        Some(decl)
                    }
                    _ => None,
                })
                .expect("Main callable should exist");
            let fir::CallableImpl::Spec(main_impl) = &mut main.implementation else {
                panic!("Main should have specializations");
            };
            main_impl.body.span = dependency_callable_span;
            package
                .exprs
                .get_mut(user_loop_id)
                .expect("user loop should exist")
                .span = dependency_loop_span;
            package
                .exprs
                .get_mut(user_condition)
                .expect("user condition should exist")
                .span = dependency_condition_span;
            package
                .blocks
                .get_mut(user_body)
                .expect("user loop body should exist")
                .span = dependency_body_span;
            for user_gate in user_gates {
                package
                    .exprs
                    .get_mut(user_gate)
                    .expect("user gate call should exist")
                    .span = dependency_gate_span;
            }

            (user_package, dependency_package)
        },
    );

    expect![[r#"
        Foreign@Dependency.qs:2:8 -> loop: 0..0@Dependency.qs:2:26[1] -> (1)@Dependency.qs:3:12 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
        Main@Dependency.qs:2:8 -> loop: 0..0@Dependency.qs:2:26[1] -> (1)@Dependency.qs:3:12 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_1), controls=())
    "#]]
    .assert_eq(&trace);

    let (main_id, main_location) = rir
        .dbg_info
        .dbg_scopes
        .values()
        .find_map(|(scope, _)| match scope {
            DbgScope::SubProgram {
                name,
                callable_id,
                location,
            } if name.as_ref() == "Main" => Some((*callable_id, *location)),
            _ => None,
        })
        .expect("Main debug scope should exist");
    let (foreign_id, foreign_location) = rir
        .dbg_info
        .dbg_scopes
        .values()
        .find_map(|(scope, _)| match scope {
            DbgScope::SubProgram {
                name,
                callable_id,
                location,
            } if name.as_ref() == "Foreign" => Some((*callable_id, *location)),
            _ => None,
        })
        .expect("Foreign debug scope should exist");
    assert_eq!(main_id.package_id, usize::from(user_package));
    assert_eq!(main_location.package_id, usize::from(dependency_package));
    assert_eq!(foreign_id.package_id, usize::from(dependency_package));
    assert_eq!(foreign_location.package_id, usize::from(dependency_package));
    assert_ne!(main_id.package_id, main_location.package_id);

    let loop_scopes = rir
        .dbg_info
        .dbg_scopes
        .iter()
        .filter_map(|(scope_id, (scope, _))| match scope {
            DbgScope::LexicalBlockFile {
                discriminator,
                loop_id,
                location,
            } => Some((scope_id, *loop_id, *location, *discriminator)),
            DbgScope::SubProgram { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(loop_scopes.len(), 2);
    assert_ne!(loop_scopes[0].0, loop_scopes[1].0);
    assert_eq!(loop_scopes[0].1.expr_id, loop_scopes[1].1.expr_id);
    assert_ne!(loop_scopes[0].1.package_id, loop_scopes[1].1.package_id);
    assert!(
        loop_scopes
            .iter()
            .any(|(_, loop_id, _, _)| loop_id.package_id == usize::from(user_package))
    );
    assert!(
        loop_scopes
            .iter()
            .any(|(_, loop_id, _, _)| loop_id.package_id == usize::from(dependency_package))
    );
    for (_, _, location, discriminator) in loop_scopes {
        assert_eq!(location.package_id, usize::from(dependency_package));
        assert_eq!(discriminator, 1);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn all_functor_app_scopes_use_distinct_identities() {
    let source = indoc! {"
            operation Main() : Unit {
                use q = Qubit();
                use control = Qubit();
                Foo(q);
                Adjoint Foo(q);
                Controlled Foo([control], q);
                Controlled Adjoint Foo([control], q);
            }

            operation Foo(q : Qubit) : Unit is Adj + Ctl {
                body (...) {
                    X(q);
                }

                adjoint (...) {
                    Y(q);
                }

                controlled (cs, ...) {
                    CNOT(cs[0], q);
                }

                controlled adjoint (cs, ...) {
                    CNOT(cs[0], q);
                }
            }
        "};
    let (trace, rir, (foo_id, expected_scopes)) =
        compile_and_trace(source, "A.Main()", None, |fir_store, user_package, _| {
            let package = fir_store.get(user_package);
            let (foo_id, foo) = package
                .items
                .iter()
                .find_map(|(item_id, item)| match &item.kind {
                    fir::ItemKind::Callable(decl) if decl.name.name.as_ref() == "Foo" => {
                        Some((item_id, decl))
                    }
                    _ => None,
                })
                .expect("Foo callable should exist");
            let fir::CallableImpl::Spec(spec_impl) = &foo.implementation else {
                panic!("Foo should have specializations");
            };
            (
                fir::StoreItemId {
                    package: user_package,
                    item: foo_id,
                },
                [
                    (FunctorApp::default(), spec_impl.body.span),
                    (
                        FunctorApp {
                            adjoint: true,
                            controlled: 0,
                        },
                        spec_impl.adj.as_ref().expect("adjoint should exist").span,
                    ),
                    (
                        FunctorApp {
                            adjoint: false,
                            controlled: 1,
                        },
                        spec_impl
                            .ctl
                            .as_ref()
                            .expect("controlled should exist")
                            .span,
                    ),
                    (
                        FunctorApp {
                            adjoint: true,
                            controlled: 1,
                        },
                        spec_impl
                            .ctl_adj
                            .as_ref()
                            .expect("controlled adjoint should exist")
                            .span,
                    ),
                ],
            )
        });

    expect![[r#"
        Main@A.qs:3:4 -> Foo@A.qs:11:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
        Main@A.qs:4:4 -> Foo†@A.qs:15:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        Main@A.qs:5:4 -> Foo@A.qs:19:8 -> CNOT@qsharp-library-source:Std/Intrinsic.qs:113:8 -> gate(X, targets=(q_0), controls=(q_1))
        Main@A.qs:6:4 -> Foo†@A.qs:23:8 -> CNOT@qsharp-library-source:Std/Intrinsic.qs:113:8 -> gate(X, targets=(q_0), controls=(q_1))
    "#]]
    .assert_eq(&trace);

    let foo_scopes = rir
        .dbg_info
        .dbg_scopes
        .values()
        .filter_map(|(scope, _)| match scope {
            DbgScope::SubProgram {
                callable_id,
                location,
                ..
            } if callable_id.package_id == usize::from(foo_id.package)
                && callable_id.item_id == usize::from(foo_id.item) =>
            {
                Some((*callable_id, *location))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(foo_scopes.len(), 4);
    for (index, (callable_id, _)) in foo_scopes.iter().enumerate() {
        assert!(!foo_scopes[..index].iter().any(|(id, _)| id == callable_id));
        assert_eq!(callable_id.package_id, foo_scopes[0].0.package_id);
        assert_eq!(callable_id.item_id, foo_scopes[0].0.item_id);
    }
    for (functor_app, expected_span) in expected_scopes {
        let (_, location) = foo_scopes
            .iter()
            .find(|(callable_id, _)| callable_id.functor_app == functor_app)
            .expect("expected functor scope should exist");
        assert_eq!(location.package_id, usize::from(expected_span.package));
        assert_eq!(location.offset, expected_span.lo);
    }
}

#[test]
fn entry_expr_allocates_qubits() {
    // mimics how entry expressions are created when generating
    // a circuit diagram for an operation.
    check_trace(
        indoc! {"
        operation Test(q1: Qubit, q2: Qubit) : Result[] {
            [M(q1), M(q2)]
        }
        "},
        indoc! {"
        {
            use qs = Qubit[2];
            (A.Test)(qs[0], qs[1]);
            let r: Result[] = [];
            r
        }"},
        &expect![[r#"
            Test@A.qs:1:5 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Test@A.qs:1:12 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_1, c_1)
        "#]],
    );
}

#[test]
fn adjoint_operation_in_entry_expr() {
    check_trace(
        indoc! {"
        operation Foo (q : Qubit) : Unit
            is Adj + Ctl {

            body (...) {
                X(q);
            }

            adjoint (...) {
                Y(q);
            }

            controlled (cs, ...) {
            }
        }
        "},
        indoc! {"
        {
            use qs = Qubit[1];
            (Adjoint A.Foo)(qs[0]);
            let r: Result[] = [];
            r
        }"},
        &expect![[r#"
            Foo†@A.qs:8:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn lambda_in_entry_expr() {
    check_trace(
        indoc! {"
        "},
        indoc! {"
        {
            use qs = Qubit[1];
            (q => H(q))(qs[0]);
            let r: Result[] = [];
            r
        }"},
        &expect![[r#"
            .lambda_1@<entry>:2:10 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn if_only() {
    check_trace(
        indoc! {"
        operation G(q: Qubit) : Unit { body intrinsic; }
        operation Main() : Unit {
            use q = Qubit();
            let result = M(q);
            if result == Zero {
                G(q);
            }
            G(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:4:4[true] -> if: c_0 = |0〉@A.qs:5:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:7:4 -> gate(G, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn if_else() {
    check_trace(
        indoc! {"
        operation G(q: Qubit) : Unit { body intrinsic; }
        operation Main() : Unit {
            use q = Qubit();
            let result = M(q);
            if result == Zero {
                G(q);
            } else {
                G(q);
            }
            G(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:4:4[true] -> if: c_0 = |0〉@A.qs:5:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:4:4[false] -> if: c_0 = |1〉@A.qs:7:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:9:4 -> gate(G, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn else_only() {
    check_trace(
        indoc! {"
        operation G(q: Qubit) : Unit { body intrinsic; }
        operation Main() : Unit {
            use q = Qubit();
            let result = M(q);
            if result == One {
                // empty true branch
            } else {
                G(q);
            }
            G(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:4:4[false] -> if: c_0 = |0〉@A.qs:7:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:9:4 -> gate(G, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn if_else_elseif() {
    check_trace(
        indoc! {"
        operation G(q: Qubit) : Unit { body intrinsic; }
        operation Main() : Unit {
            use q = Qubit();
            let result = M(q);
            if result == Zero {
                G(q);
            } else {
                let result2 = M(q);
                if result2 == Zero {
                    G(q);
                } else {
                    G(q);
                }
            }
            G(q);
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:4:4[true] -> if: c_0 = |0〉@A.qs:5:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:4:4[false] -> if: c_0 = |1〉@A.qs:7:22 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_1)
            Main@A.qs:4:4[false] -> if: c_0 = |1〉@A.qs:8:8[true] -> if: c_1 = |0〉@A.qs:9:12 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:4:4[false] -> if: c_0 = |1〉@A.qs:8:8[false] -> if: c_1 = |1〉@A.qs:11:12 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:14:4 -> gate(G, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn nested_callables_and_if() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use qs = Qubit[2];
            Foo(qs[0]);
            ResetAll(qs);
        }

        operation Foo(q: Qubit) : Result[] {
            H(q);
            let r1 = M(q);
            if (r1 == One) {
                X(q);
            }
            [r1]
        }

        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:4 -> Foo@A.qs:7:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> Foo@A.qs:8:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:2:4 -> Foo@A.qs:9:4[true] -> if: c_0 = |1〉@A.qs:10:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[1] -> (1)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_0)
            Main@A.qs:3:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[2] -> (2)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_1)
        "#]],
    );
}

#[test]
fn branch_in_for_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use qs = Qubit[2];
            let results = [MResetZ(qs[0]), MResetZ(qs[1])];

            for j in 0..1 {
                if results[j] == One {
                    X(qs[0]);
                }
            }
            ResetAll(qs);
        }

        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:2:19 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, c_0)
            Main@A.qs:2:35 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_1, c_1)
            Main@A.qs:4:4 -> loop: 0..1@A.qs:4:18[1] -> (1)@A.qs:5:8[true] -> if: c_0 = |1〉@A.qs:6:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:4:4 -> loop: 0..1@A.qs:4:18[2] -> (2)@A.qs:5:8[true] -> if: c_1 = |1〉@A.qs:6:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:9:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[1] -> (1)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_0)
            Main@A.qs:9:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[2] -> (2)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_1)
        "#]],
    );
}

#[test]
fn callable_in_for_loop() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit();

            for j in 0..1 {
                Baz(q);
            }
            Reset(q);
        }

        operation Baz(q : Qubit) : Unit {
            H(q);
        }

        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> loop: 0..1@A.qs:3:18[1] -> (1)@A.qs:4:8 -> Baz@A.qs:10:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: 0..1@A.qs:3:18[2] -> (2)@A.qs:4:8 -> Baz@A.qs:10:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:6:4 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_0)
        "#]],
    );
}

#[test]
fn nested_conditionals_in_callable() {
    check_trace(
        indoc! {"
        operation Main() : Unit {

            use qs = Qubit[3];
            NestedConditionalsInCallable(qs[0], qs[1], qs[2]);
            ResetAll(qs);
        }

        operation NestedConditionalsInCallable(q: Qubit, q0: Qubit, q1: Qubit) : Unit {
            let r0 = MResetZ(q0);
            let r1 = MResetZ(q1);
            Foo(q, r0, r1);
        }

        operation Foo(q : Qubit, r0 : Result, r1 : Result) : Unit {
            if r0 == One {
            } else {
                if r1 == One {
                    X(q);
                } else {
                    Z(q);
                }
            }
        }

        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> NestedConditionalsInCallable@A.qs:8:13 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_1, c_0)
            Main@A.qs:3:4 -> NestedConditionalsInCallable@A.qs:9:13 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_2, c_1)
            Main@A.qs:3:4 -> NestedConditionalsInCallable@A.qs:10:4 -> Foo@A.qs:14:4[false] -> if: c_0 = |0〉@A.qs:16:8[true] -> if: c_1 = |1〉@A.qs:17:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> NestedConditionalsInCallable@A.qs:10:4 -> Foo@A.qs:14:4[false] -> if: c_0 = |0〉@A.qs:16:8[false] -> if: c_1 = |0〉@A.qs:19:12 -> Z@qsharp-library-source:Std/Intrinsic.qs:1126:8 -> gate(Z, targets=(q_0), controls=())
            Main@A.qs:4:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[1] -> (1)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_0)
            Main@A.qs:4:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[2] -> (2)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_1)
            Main@A.qs:4:4 -> ResetAll@qsharp-library-source:Std/Intrinsic.qs:437:4 -> loop: qubits@qsharp-library-source:Std/Intrinsic.qs:437:20[3] -> (3)@qsharp-library-source:Std/Intrinsic.qs:438:8 -> Reset@qsharp-library-source:Std/Intrinsic.qs:426:4 -> reset(q_2)
        "#]],
    );
}

#[test]
fn dynamic_double_arg() {
    check_trace(
        indoc! {"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                mutable theta = 1.0;
                if r == One {
                    set theta = 2.0;
                };
                if theta > 1.5 {
                    set theta = 3.0;
                } else {
                    set theta = 4.0;
                }
                Rx(theta, q1);
                let r1 = M(q1);
                [r, r1]
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:4:12 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:14:4 -> Rx@qsharp-library-source:Std/Intrinsic.qs:510:8 -> using: c_0@qsharp-library-source:Std/Intrinsic.qs:510:8 -> gate(Rx, targets=(q_1), controls=())
            Main@A.qs:15:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_1, c_1)
        "#]],
    );
}

#[test]
fn binop_short_circuit() {
    check_trace(
        indoc! {"
            operation Main() : Unit {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                H(q1);
                let r = { M(q0) == Zero } and { M(q1) == Zero };
                let r1 = { M(q0) == Zero } or { M(q1) == Zero };
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:4:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_1), controls=())
            Main@A.qs:5:14 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:5:34[true] -> if: c_0 = |0〉@A.qs:5:36 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_1, c_1)
            Main@A.qs:6:15 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_2)
            Main@A.qs:6:34[false] -> if: c_2 = |1〉@A.qs:6:36 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_1, c_3)
        "#]],
    );
}

#[test]
fn result_to_result_comparison() {
    check_trace(
        indoc! {"
        operation G(q: Qubit) : Unit { body intrinsic; }
        operation Main() : Unit {
            use q0 = Qubit();
            use q1 = Qubit();
            let r0 = M(q0);
            let r1 = M(q1);
            if r0 == r1 {
                G(q0);
            }
            if r0 != r1 {
                G(q0);
            }
        }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:4:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_0, c_0)
            Main@A.qs:5:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> measure(M, q_1, c_1)
            Main@A.qs:6:4[true] -> if: c_0c_1 = |00〉 or c_0c_1 = |11〉@A.qs:7:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:9:4[true] -> if: c_0c_1 = |01〉 or c_0c_1 = |10〉@A.qs:10:8 -> gate(G, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn integer_comparison() {
    check_trace(
        indoc! {"
    operation Main() : Unit {
        use q = Qubit();
        use reg = Qubit[4];
        ApplyToEach(H, reg);
        let num = MeasureInteger(reg);
        if num < 8 {
            X(q);
        } else {
            Y(q);
        }
    }
    "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> ApplyToEach@qsharp-library-source:Std/Canon.qs:29:4 -> loop: register@qsharp-library-source:Std/Canon.qs:29:25[1] -> (1)@qsharp-library-source:Std/Canon.qs:30:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_1), controls=())
            Main@A.qs:3:4 -> ApplyToEach@qsharp-library-source:Std/Canon.qs:29:4 -> loop: register@qsharp-library-source:Std/Canon.qs:29:25[2] -> (2)@qsharp-library-source:Std/Canon.qs:30:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_2), controls=())
            Main@A.qs:3:4 -> ApplyToEach@qsharp-library-source:Std/Canon.qs:29:4 -> loop: register@qsharp-library-source:Std/Canon.qs:29:25[3] -> (3)@qsharp-library-source:Std/Canon.qs:30:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_3), controls=())
            Main@A.qs:3:4 -> ApplyToEach@qsharp-library-source:Std/Canon.qs:29:4 -> loop: register@qsharp-library-source:Std/Canon.qs:29:25[4] -> (4)@qsharp-library-source:Std/Canon.qs:30:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_4), controls=())
            Main@A.qs:4:14 -> MeasureInteger@qsharp-library-source:Std/Measurement.qs:155:4 -> loop: 0..nBits - 1@qsharp-library-source:Std/Measurement.qs:155:26[1] -> (1)@qsharp-library-source:Std/Measurement.qs:156:12 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_1, c_0)
            Main@A.qs:4:14 -> MeasureInteger@qsharp-library-source:Std/Measurement.qs:155:4 -> loop: 0..nBits - 1@qsharp-library-source:Std/Measurement.qs:155:26[2] -> (2)@qsharp-library-source:Std/Measurement.qs:156:12 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_2, c_1)
            Main@A.qs:4:14 -> MeasureInteger@qsharp-library-source:Std/Measurement.qs:155:4 -> loop: 0..nBits - 1@qsharp-library-source:Std/Measurement.qs:155:26[3] -> (3)@qsharp-library-source:Std/Measurement.qs:156:12 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_3, c_2)
            Main@A.qs:4:14 -> MeasureInteger@qsharp-library-source:Std/Measurement.qs:155:4 -> loop: 0..nBits - 1@qsharp-library-source:Std/Measurement.qs:155:26[4] -> (4)@qsharp-library-source:Std/Measurement.qs:156:12 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_4, c_3)
            Main@A.qs:5:4[true] -> if: (f(c_0, c_1, c_2, c_3)) < (8)@A.qs:6:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:5:4[false] -> if: (f(c_0, c_1, c_2, c_3)) >= (8)@A.qs:8:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
        "#]],
    );
}
