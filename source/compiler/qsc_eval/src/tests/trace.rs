// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;

use crate::{
    Env, Error, ErrorBehavior, State, StepAction, Value,
    backend::{SparseSim, SymbolicStackTrace, Tracer, TracingBackend},
    debug::Frame,
    output::{GenericReceiver, Receiver},
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::{self, ExecGraph, ExecGraphConfig};
use qsc_fir::fir::{PackageId, PackageStoreLookup};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_passes::{PackageType, run_core_passes, run_default_passes};

struct TestTracer {
    trace: String,
    is_stack_tracing_enabled: bool,
}

impl Tracer for TestTracer {
    fn qubit_allocate(&mut self, stack: &crate::StackTrace, q: usize) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "qubit_allocate q_{q}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn qubit_release(&mut self, stack: &crate::StackTrace, q: usize) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "qubit_release q_{q}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn qubit_swap_id(&mut self, stack: &crate::StackTrace, q0: usize, q1: usize) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "qubit_swap_id q_{q0} q_{q1}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn gate(
        &mut self,
        stack: &crate::StackTrace,
        name: &str,
        is_adjoint: bool,
        targets: &[usize],
        controls: &[usize],
        theta: Option<f64>,
    ) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(
            self.trace,
            "gate {}{}{} targets=({}) controls=({})",
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
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn measure(&mut self, stack: &crate::StackTrace, name: &str, q: usize, r: &crate::val::Result) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "measure {name} q_{q} {r:?}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn reset(&mut self, stack: &crate::StackTrace, q: usize) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "reset q_{q}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn custom_intrinsic(&mut self, stack: &crate::StackTrace, name: &str, arg: Value) {
        let stack = SymbolicStackTrace::from_trace(stack);
        let _ = write!(self.trace, "intrinsic {name} {arg:?}");
        if stack.0.is_empty() {
            let _ = writeln!(self.trace);
        } else {
            let _ = writeln!(self.trace, " stack {stack}");
        }
    }

    fn is_stack_tracing_enabled(&self) -> bool {
        self.is_stack_tracing_enabled
    }
}

/// Evaluates the given control flow graph with the given context.
/// Creates a new environment and simulator.
/// # Errors
/// Returns the first error encountered during execution.
pub(super) fn eval_graph(
    graph: ExecGraph,
    globals: &impl PackageStoreLookup,
    exec_graph_config: ExecGraphConfig,
    package: PackageId,
    env: &mut Env,
    out: &mut impl Receiver,
) -> Result<String, (Error, Vec<Frame>)> {
    let mut state = State::new(
        package,
        graph,
        exec_graph_config,
        None,
        ErrorBehavior::FailOnError,
    );

    let mut tracer = TestTracer {
        trace: String::new(),
        is_stack_tracing_enabled: true,
    };
    let mut tracing_backend = TracingBackend::<SparseSim>::no_backend(&mut tracer);
    let _ = state.eval(
        globals,
        env,
        &mut tracing_backend,
        out,
        &[],
        StepAction::Continue,
    );
    Ok(tracer.trace)
}

fn check_trace(file: &str, expr: &str, expect: &Expect) {
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

    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
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
    let value = eval_graph(
        entry,
        &fir_store,
        ExecGraphConfig::NoDebug,
        map_hir_package_to_fir(id),
        &mut Env::default(),
        &mut GenericReceiver::new(&mut out),
    )
    .expect("eval should succeed");

    expect.assert_eq(&value);
}

#[test]
fn block_qubit_use_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = Qubit();
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0
            qubit_release q_0
        "#]],
    );
}

#[test]
fn block_qubit_use_use_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = Qubit();
            use q1 = Qubit();
            $"{q1}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0
            qubit_allocate q_1
            qubit_release q_1
            qubit_release q_0
        "#]],
    );
}

#[test]
fn block_qubit_use_reuse_expr() {
    check_trace(
        "",
        indoc! {r#"{
            {
                use q = Qubit();
            }
            use q = Qubit();
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0
            qubit_release q_0
            qubit_allocate q_0
            qubit_release q_0
        "#]],
    );
}

#[test]
fn block_qubit_use_scope_reuse_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = Qubit() {
            }
            use q = Qubit();
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0
            qubit_release q_0
            qubit_allocate q_0
            qubit_release q_0
        "#]],
    );
}

#[test]
fn block_qubit_use_array_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = Qubit[3];
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn block_qubit_use_tuple_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = (Qubit[3], Qubit(), Qubit());
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_allocate q_3
            qubit_allocate q_4
            qubit_release q_4
            qubit_release q_3
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn block_qubit_use_nested_tuple_expr() {
    check_trace(
        "",
        indoc! {r#"{
            use q = (Qubit[3], (Qubit(), Qubit()));
            $"{q}"
        }"#},
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_allocate q_3
            qubit_allocate q_4
            qubit_release q_4
            qubit_release q_3
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn check_ctls_count_expr() {
    check_trace(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs = Qubit[3];
                Controlled Test.Foo(qs, ());
            }
        "},
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn check_ctls_count_nested_expr() {
    check_trace(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs1 = Qubit[1];
                use qs2 = Qubit[2];
                Controlled Controlled Test.Foo(qs2, (qs1, ()));
            }
        "},
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
            qubit_release q_0 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn check_generated_ctl_expr() {
    check_trace(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl {
                    body ... {}
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl {
                    A();
                }
            }
        "#},
        "{use qs = Qubit[3]; Controlled Test.B(qs, ())}",
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_allocate q_2 stack callable0-10@(0-2812)
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
            qubit_release q_2 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn check_generated_ctladj_distrib_expr() {
    check_trace(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl + Adj {
                    body ... { fail "Shouldn't get here"; }
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                    controlled adjoint (ctls, ...) {
                        if Length(ctls) != 2 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl + Adj {
                    body ... { A(); }
                    adjoint ... { Adjoint A(); }
                }
            }
        "#},
        "{use qs = Qubit[2]; Controlled Adjoint Test.B(qs, ())}",
        &expect![[r#"
            qubit_allocate q_0 stack callable0-10@(0-2812)
            qubit_allocate q_1 stack callable0-10@(0-2812)
            qubit_release q_0 stack callable0-11@(0-2963)
            qubit_release q_1 stack callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn lambda_operation_empty_closure() {
    check_trace(
        "
            namespace A {
                operation Foo(op : Qubit => ()) : Result {
                    use q = Qubit();
                    op(q);
                    MResetZ(q)
                }

                operation Bar() : Result { Foo(q => X(q)) }
            }
        ",
        "A.Bar()",
        &expect![[r#"
            qubit_allocate q_0 stack callable2-2@(2-251) -> callable2-1@(2-114)
            gate X targets=(q_0) controls=() stack callable2-2@(2-251) -> callable2-1@(2-151) -> callable2-3@(2-260) -> callable1-273@(1-133092)
            measure MResetZ q_0 Id(0) stack callable2-2@(2-251) -> callable2-1@(2-178) -> callable1-506@(1-181043)
            qubit_release q_0 stack callable2-2@(2-251) -> callable2-1@(2-114)
        "#]],
    );
}

#[test]
fn lambda_operation_closure() {
    check_trace(
        "
            namespace A {
                operation Foo(op : () => Result) : Result { op() }
                operation Bar() : Result {
                    use q = Qubit();
                    X(q);
                    Foo(() => MResetZ(q))
                }
            }
        ",
        "A.Bar()",
        &expect![[r#"
            qubit_allocate q_0 stack callable2-2@(2-165)
            gate X targets=(q_0) controls=() stack callable2-2@(2-202) -> callable1-273@(1-133092)
            measure MResetZ q_0 Id(0) stack callable2-2@(2-228) -> callable2-1@(2-95) -> callable2-3@(2-238) -> callable1-506@(1-181043)
            qubit_release q_0 stack callable2-2@(2-165)
        "#]],
    );
}

#[test]
fn lambda_operation_controlled() {
    check_trace(
        "
            namespace A {
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls, q) = (Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    X(ctls[0]);
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    ResetAll(ctls);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect![[r#"
            qubit_allocate q_0 stack callable2-2@(2-262) -> callable0-10@(0-2812)
            qubit_allocate q_1 stack callable2-2@(2-272)
            gate X targets=(q_1) controls=() stack callable2-2@(2-342) -> callable2-1@(2-131) -> callable2-3@(2-316) -> callable1-273@(1-133092)
            measure MResetZ q_1 Id(0) stack callable2-2@(2-389) -> callable1-506@(1-181043)
            gate X targets=(q_1) controls=(q_0) stack callable2-2@(2-422) -> callable2-1@(2-131) -> callable2-3@(2-316) -> callable1-273@(1-133281)
            measure MResetZ q_1 Id(1) stack callable2-2@(2-488) -> callable1-506@(1-181043)
            gate X targets=(q_0) controls=() stack callable2-2@(2-521) -> callable1-273@(1-133092)
            gate X targets=(q_1) controls=(q_0) stack callable2-2@(2-553) -> callable2-1@(2-131) -> callable2-3@(2-316) -> callable1-273@(1-133281)
            measure MResetZ q_1 Id(2) stack callable2-2@(2-619) -> callable1-506@(1-181043)
            reset q_0 stack callable2-2@(2-652) -> callable1-261@(1-116710) -> callable1-260@(1-116364)
            qubit_release q_1 stack callable2-2@(2-272)
            qubit_release q_0 stack callable2-2@(2-262) -> callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn lambda_operation_controlled_controlled() {
    check_trace(
        "
            namespace A {
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls1, ctls2, q) = (Qubit[1], Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    X(ctls1[0]);
                    X(ctls2[0]);
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    ResetAll(ctls1 + ctls2);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect![[r#"
            qubit_allocate q_0 stack callable2-2@(2-270) -> callable0-10@(0-2812)
            qubit_allocate q_1 stack callable2-2@(2-280) -> callable0-10@(0-2812)
            qubit_allocate q_2 stack callable2-2@(2-290)
            gate X targets=(q_2) controls=() stack callable2-2@(2-360) -> callable2-1@(2-131) -> callable2-3@(2-334) -> callable1-273@(1-133092)
            measure MResetZ q_2 Id(0) stack callable2-2@(2-407) -> callable1-506@(1-181043)
            gate X targets=(q_2) controls=(q_0, q_1) stack callable2-2@(2-440) -> callable2-1@(2-131) -> callable2-3@(2-334) -> callable1-273@(1-133370)
            measure MResetZ q_2 Id(1) stack callable2-2@(2-527) -> callable1-506@(1-181043)
            gate X targets=(q_0) controls=() stack callable2-2@(2-560) -> callable1-273@(1-133092)
            gate X targets=(q_1) controls=() stack callable2-2@(2-593) -> callable1-273@(1-133092)
            gate X targets=(q_2) controls=(q_0, q_1) stack callable2-2@(2-626) -> callable2-1@(2-131) -> callable2-3@(2-334) -> callable1-273@(1-133370)
            measure MResetZ q_2 Id(2) stack callable2-2@(2-713) -> callable1-506@(1-181043)
            reset q_0 stack callable2-2@(2-746) -> callable1-261@(1-116710) -> callable1-260@(1-116364)
            reset q_1 stack callable2-2@(2-746) -> callable1-261@(1-116710) -> callable1-260@(1-116364)
            qubit_release q_2 stack callable2-2@(2-290)
            qubit_release q_1 stack callable2-2@(2-280) -> callable0-11@(0-2963)
            qubit_release q_0 stack callable2-2@(2-270) -> callable0-11@(0-2963)
        "#]],
    );
}

#[test]
fn partial_app_arg_with_side_effect() {
    check_trace(
        "",
        "{
            operation F(_ : (), x : Int) : Int { x }
            use q = Qubit();
            let f = F(X(q), _);
            let r1 = M(q);
            f(1);
            let r2 = M(q);
            f(2);
            let r3 = M(q);
            Reset(q);
            (r1, r2, r3)
        }",
        &expect![[r#"
            qubit_allocate q_0
            gate X targets=(q_0) controls=() stack callable1-273@(1-133092)
            measure M q_0 Id(0) stack callable1-255@(1-111973) -> callable1-256@(1-113160)
            measure M q_0 Id(1) stack callable1-255@(1-111973) -> callable1-256@(1-113160)
            measure M q_0 Id(2) stack callable1-255@(1-111973) -> callable1-256@(1-113160)
            reset q_0 stack callable1-260@(1-116364)
            qubit_release q_0
        "#]],
    );
}

#[test]
fn grouping_nested_callables() {
    check_trace(
        "operation Main() : Unit {
            use q = Qubit();
            for i in 0..5 {
                Foo(q);
            }
            MResetZ(q);
        }

        operation Foo(q: Qubit) : Unit {
            H(q);
        }",
        "test.Main()",
        &expect![[r#"
            qubit_allocate q_0 stack callable2-1@(2-50)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            gate H targets=(q_0) controls=() stack callable2-1@(2-111) -> callable2-2@(2-221) -> callable1-253@(1-110294)
            measure MResetZ q_0 Id(0) stack callable2-1@(2-145) -> callable1-506@(1-181043)
            qubit_release q_0 stack callable2-1@(2-50)
        "#]],
    );
}
