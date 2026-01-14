// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_eval::{
    Env, ErrorBehavior, State, StepAction,
    backend::{SparseSim, Tracer, TracingBackend},
    output::GenericReceiver,
    val::{self},
};
use qsc_fir::fir::{self, ExecGraphConfig};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_passes::{PackageType, run_core_passes, run_default_passes};

use crate::builder::{LogicalStack, LogicalStackWithSourceLookup};

struct TestTracer<'a> {
    trace: String,
    is_stack_tracing_enabled: bool,
    source_lookup: &'a (&'a compile::PackageStore, &'a fir::PackageStore),
}

impl Tracer for TestTracer<'_> {
    fn qubit_allocate(&mut self, stack: &qsc_eval::StackTrace, q: usize) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "qubit_allocate(q_{q})");
    }

    fn qubit_release(&mut self, stack: &qsc_eval::StackTrace, q: usize) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "qubit_release(q_{q})");
    }

    fn qubit_swap_id(&mut self, stack: &qsc_eval::StackTrace, q0: usize, q1: usize) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "qubit_swap_id(q_{q0}, q_{q1})");
    }

    fn gate(
        &mut self,
        stack: &qsc_eval::StackTrace,
        name: &str,
        is_adjoint: bool,
        targets: &[usize],
        controls: &[usize],
        theta: Option<f64>,
    ) {
        self.write_stack(stack);
        let _ = writeln!(
            self.trace,
            "gate({}{}{}, targets=({}), controls=({}))",
            name,
            if is_adjoint { "†" } else { "" },
            theta.map(|t| format!("({t:.4})")).unwrap_or_default(),
            targets
                .iter()
                .map(|q| format!("q_{q}"))
                .collect::<Vec<_>>()
                .join(", "),
            controls
                .iter()
                .map(|q| format!("q_{q}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    fn measure(&mut self, stack: &qsc_eval::StackTrace, name: &str, q: usize, r: &val::Result) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "measure({name}, q_{q}, {r:?})");
    }

    fn reset(&mut self, stack: &qsc_eval::StackTrace, q: usize) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "reset(q_{q})");
    }

    fn custom_intrinsic(&mut self, stack: &qsc_eval::StackTrace, name: &str, arg: val::Value) {
        self.write_stack(stack);
        let _ = writeln!(self.trace, "intrinsic({name}, {arg})");
    }

    fn is_stack_tracing_enabled(&self) -> bool {
        self.is_stack_tracing_enabled
    }
}

impl TestTracer<'_> {
    fn write_stack(&mut self, stack: &qsc_eval::StackTrace) {
        let trace = LogicalStack::from_evaluator_trace(stack);
        let display = LogicalStackWithSourceLookup {
            trace,
            source_lookup: self.source_lookup,
        };
        if display.trace.0.is_empty() {
            let _ = write!(self.trace, "[no stack] ");
        } else {
            let _ = write!(self.trace, "{display} -> ");
        }
    }
}

fn check_trace(file: &str, expr: &str, exec_graph_config: ExecGraphConfig, expect: &Expect) {
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    let core_fir = fir_lowerer.lower_package(&core.package, &fir_store);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, TargetCapabilityFlags::all());
    assert!(std.errors.is_empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std_fir = fir_lowerer.lower_package(&std.package, &fir_store);
    let std_id = store.insert(std);

    let sources = SourceMap::new([("A.qs".into(), file.into())], Some(expr.into()));
    let mut unit = compile(
        &store,
        &[(std_id, None)],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let unit_fir = fir_lowerer.lower_package(&unit.package, &fir_store);
    let entry = unit_fir.entry_exec_graph.clone();
    let id = store.insert(unit);

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    fir_store.insert(map_hir_package_to_fir(id), unit_fir);

    let mut out = Vec::new();
    let mut state = State::new(
        map_hir_package_to_fir(id),
        entry,
        exec_graph_config,
        None,
        ErrorBehavior::FailOnError,
    );

    let mut tracer = TestTracer {
        trace: String::new(),
        is_stack_tracing_enabled: true,
        source_lookup: &(&store, &fir_store),
    };
    let mut tracing_backend = TracingBackend::<SparseSim>::no_backend(&mut tracer);
    let _ = state.eval(
        &fir_store,
        &mut Env::default(),
        &mut tracing_backend,
        &mut GenericReceiver::new(&mut out),
        &[],
        StepAction::Continue,
    );
    expect.assert_eq(&tracer.trace);
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
        ExecGraphConfig::Debug,
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
        ExecGraphConfig::NoDebug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::NoDebug,
        &expect![[r#"
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_1)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_2)
            Main@A.qs:2:4 -> CCNOT@qsharp-library-source:Std/Intrinsic.qs:75:8 -> gate(X, targets=(q_2), controls=(q_0, q_1))
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_1)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_2)
        "#]],
    );
}

#[test]
fn multi_qubit_alloc_debug() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit[3];
        }
        "},
        "A.Main()",
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[1] -> (1)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[2] -> (2)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_1)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[3] -> (3)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_2)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[1] -> (1)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[2] -> (2)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_1)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[3] -> (3)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_2)
        "#]],
    );
}

#[test]
fn multi_qubit_alloc_no_debug() {
    check_trace(
        indoc! {"
        operation Main() : Unit {
            use q = Qubit[3];
        }
        "},
        "A.Main()",
        ExecGraphConfig::NoDebug,
        &expect![[r#"
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_1)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_2)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_1)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_2)
        "#]],
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[1] -> (1)@A.qs:2:8 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[1] -> (1)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[1] -> (1)@A.qs:2:8 -> qubit_release(q_0)
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[2] -> (2)@A.qs:2:8 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[2] -> (2)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:1:4 -> loop: 1..2@A.qs:1:18[2] -> (2)@A.qs:2:8 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> Foo@A.qs:7:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:3:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(0))
            Main@A.qs:1:4 -> qubit_release(q_0)
        "#]],
    );
}

#[test]
fn for_loop_debug() {
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:5:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(0))
            Main@A.qs:1:4 -> qubit_release(q_0)
        "#]],
    );
}

#[test]
fn for_loop_no_debug() {
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
        ExecGraphConfig::NoDebug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:3:8 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:5:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(0))
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> Foo@A.qs:9:4 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            Main@A.qs:5:4 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(0))
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:3:4 -> loop: i < 2@A.qs:3:18[1] -> (1)@A.qs:4:8 -> Foo@A.qs:10:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 2@A.qs:3:18[2] -> (2)@A.qs:4:8 -> Foo@A.qs:10:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[1] -> (1)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[2] -> (2)@A.qs:13:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[3] -> (3)@A.qs:13:12 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[4] -> (4)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> loop: i < 7@A.qs:3:18[5] -> (5)@A.qs:11:12 -> Foo@A.qs:19:4 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[1] -> (1)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            Main@A.qs:1:4 -> AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[2] -> (2)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_1)
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[1] -> (1)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[2] -> (2)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[1] -> (1)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> loop: 0..2@A.qs:2:18[3] -> (3)@A.qs:3:8 -> loop: 0..1@A.qs:3:22[2] -> (2)@A.qs:4:12 -> Foo@A.qs:10:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_1), controls=())
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[1] -> (1)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
            Main@A.qs:1:4 -> ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[2] -> (2)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_1)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:2:8 -> qubit_allocate(q_0)
            Main@A.qs:3:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:4:8 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(0))
            Main@A.qs:2:8 -> qubit_release(q_0)
            Main@A.qs:7:8 -> qubit_allocate(q_0)
            Main@A.qs:8:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:9:8 -> MResetZ@qsharp-library-source:Std/Measurement.qs:135:4 -> measure(MResetZ, q_0, Id(1))
            Main@A.qs:7:8 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:5:4 -> qubit_allocate(q_0)
            Main@A.qs:6:4 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:7:4 -> intrinsic(foo, (4, Qubit0))
            Main@A.qs:5:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> Foo@A.qs:8:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:2:4 -> Foo@A.qs:9:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:9:8 -> Y†@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:8:8 -> X†@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> Foo@A.qs:8:8 -> X@qsharp-library-source:Std/Intrinsic.qs:1038:8 -> gate(X, targets=(q_0), controls=())
            Main@A.qs:3:4 -> Foo†@A.qs:12:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            Main@A.qs:1:4 -> qubit_allocate(q_0)
            Main@A.qs:2:4 -> qubit_allocate(q_1)
            Main@A.qs:3:4 -> Foo@A.qs:12:8 -> CNOT@qsharp-library-source:Std/Intrinsic.qs:113:8 -> gate(X, targets=(q_0), controls=(q_1))
            Main@A.qs:2:4 -> qubit_release(q_1)
            Main@A.qs:1:4 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[1] -> (1)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[2] -> (2)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_1)
            Test@A.qs:1:5 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_0, Id(0))
            Test@A.qs:1:12 -> M@qsharp-library-source:Std/Intrinsic.qs:268:4 -> Measure@qsharp-library-source:Std/Intrinsic.qs:304:12 -> measure(M, q_1, Id(1))
            ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[1] -> (1)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
            ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[2] -> (2)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_1)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[1] -> (1)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            Foo†@A.qs:8:8 -> Y@qsharp-library-source:Std/Intrinsic.qs:1082:8 -> gate(Y, targets=(q_0), controls=())
            ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[1] -> (1)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
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
        ExecGraphConfig::Debug,
        &expect![[r#"
            AllocateQubitArray@qsharp-library-source:core/qir.qs:17:8 -> loop: 0..size - 1@qsharp-library-source:core/qir.qs:17:29[1] -> (1)@qsharp-library-source:core/qir.qs:18:23 -> qubit_allocate(q_0)
            <lambda>@<entry>:2:10 -> H@qsharp-library-source:Std/Intrinsic.qs:205:8 -> gate(H, targets=(q_0), controls=())
            ReleaseQubitArray@qsharp-library-source:core/qir.qs:24:8 -> loop: qs@qsharp-library-source:core/qir.qs:24:20[1] -> (1)@qsharp-library-source:core/qir.qs:25:12 -> qubit_release(q_0)
        "#]],
    );
}
