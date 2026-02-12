// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir_to_circuit_2::build_operation_list;
use crate::{
    builder::{
        GateInputs, LogicalStack, LogicalStackWithSourceLookup, OperationReceiver, ScopeStack,
        WireMap,
    },
    rir_to_circuit_2::{FixedQubitRegisterMapBuilder, ProgramMap, reconstruct_control_flow},
};
use expect_test::Expect;
use expect_test::expect;
use indoc::indoc;
use qsc_codegen::qir::fir_to_rir;
use qsc_data_structures::{
    index_map::IndexMap, language_features::LanguageFeatures, source::SourceMap, target::Profile,
};
use qsc_fir::fir::{self};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_partial_eval::ProgramEntry;
use qsc_passes::{PackageType, PassContext, run_core_passes, run_default_passes};

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
    let capabilities = Profile::AdaptiveRIF.into();
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    let core_fir = fir_lowerer.lower_package(&core.package, &fir_store);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, capabilities);
    assert!(std.errors.is_empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std_fir = fir_lowerer.lower_package(&std.package, &fir_store);
    let std_id = store.insert(std);

    let sources = SourceMap::new([("A.qs".into(), file.into())], Some(expr.into()));
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
    let unit_fir = fir_lowerer.lower_package(&unit.package, &fir_store);
    let id = store.insert(unit);

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
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

    let compute_properties = PassContext::run_fir_passes_on_fir(&fir_store, id, capabilities)
        .expect("FIR passes should succeed");

    // TODO: can we pass none for compute_properties?
    let (_, rir) = fir_to_rir(
        &fir_store,
        capabilities,
        Some(compute_properties),
        &entry,
        true,
    )
    .expect("RIR lowering should succeed");

    let mut program_map = ProgramMap {
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
    if let Err(err) = build_operation_list(
        &mut program_map,
        &rir,
        &mut FixedQubitRegisterMapBuilder::new(
            rir.num_qubits.try_into().expect("num qubits fits in usize"),
        ),
        &mut builder,
        &structured_control_flow,
        &[],
        &ScopeStack::top(),
    ) {
        panic!("error building operation list: {err}");
    }

    expect.assert_eq(&builder.trace);
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
fn controlled_operation() {
    check_trace(
        indoc! {"
            operation Main() : Unit {
                use q = Qubit();
                use q1 = Qubit();
                Controlled Foo([q1], q);
            }

            operation Foo(q : Qubit) : Unit is Ctl {
                body (...) {
                    X(q);
                }

                controlled (cs, ...) {
                    CNOT(cs[0], q);
                }
            }
        "},
        "A.Main()",
        &expect![[r#"
            Main@A.qs:3:4 -> Foo@A.qs:12:8 -> CNOT@qsharp-library-source:Std/Intrinsic.qs:113:8 -> gate(X, targets=(q_0), controls=(q_1))
        "#]],
    );
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
            Test@A.qs:1:5 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
            Test@A.qs:1:12 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_1, c_1)
        "#]],
    );
}

#[test]
fn adjoint_operation_in_entry_expr() {
    // TODO: adjoints not showing up
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
            <lambda>@<entry>:2:10 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
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
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
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
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
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
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
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
            Main@A.qs:3:17 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
            Main@A.qs:4:4[true] -> if: c_0 = |0〉@A.qs:5:8 -> gate(G, targets=(q_0), controls=())
            Main@A.qs:4:4[false] -> if: c_0 = |1〉@A.qs:7:22 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_1)
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
            Main@A.qs:2:4 -> Foo@A.qs:8:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
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
fn weird_repro() {
    check_trace(
        indoc! {"
        import Std.Diagnostics.DumpMachine;
        import Std.Math.ArcCos;
        import Std.Convert.IntAsDouble;
        import Std.Arrays.Subarray;
        import Std.StatePreparation.PreparePureStateD;

        @EntryPoint(Adaptive_RIF)
        operation Main() : Double {
            Foo()
        }

        operation Foo() : Double {
            use q = Qubit();
            Bar(q);
            return 0.0;
        }

           operation RemoveMeAndWatchItAllCrumble() : Unit {}

        operation Bar(
            q : Qubit
        ) : Unit {
            within {} apply {
                Baz(q);
            }
        }

        operation Baz(q : Qubit) : Unit {
            H(q);
        }
        "},
        indoc! {"
        {
            use qs = Qubit[0];
            (A.Main)();
            let r: Result[] = [];
            r
        }
        "},
        &expect![[r#"
            Main@A.qs:8:4 -> Foo@A.qs:13:4 -> Bar@A.qs:23:8 -> Baz@A.qs:28:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
        "#]],
    );

    check_trace(
        indoc! {"
        import Std.Diagnostics.DumpMachine;
        import Std.Math.ArcCos;
        import Std.Convert.IntAsDouble;
        import Std.Arrays.Subarray;
        import Std.StatePreparation.PreparePureStateD;

        @EntryPoint(Adaptive_RIF)
        operation Main() : Double {
            Foo()
        }

        operation Foo() : Double {
            use q = Qubit();
            Bar(q);
            return 0.0;
        }

        // operation RemoveMeAndWatchItAllCrumble() : Unit {}

        operation Bar(
            q : Qubit
        ) : Unit {
            within {} apply {
                Baz(q);
            }
        }

        operation Baz(q : Qubit) : Unit {
            H(q);
        }
        "},
        indoc! {"
        {
            use qs = Qubit[0];
            (A.Main)();
            let r: Result[] = [];
            r
        }
        "},
        &expect![[r#"
            Main@A.qs:8:4 -> Foo@A.qs:13:4 -> Bar@A.qs:23:8 -> Baz@A.qs:28:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
        "#]],
    );
}

#[test]
fn dynamic_double_arg() {
    // TODO: I don't know about this trace
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
            Main@A.qs:4:12 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, c_0)
            Main@A.qs:14:4 -> Rx@qsharp-library-source:Std/Intrinsic.qs:510:8 -> using: c_0@qsharp-library-source:Std/Intrinsic.qs:510:8 -> gate(Rx, targets=(q_1), controls=())
            Main@A.qs:15:13 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_1, c_1)
        "#]],
    );
}
