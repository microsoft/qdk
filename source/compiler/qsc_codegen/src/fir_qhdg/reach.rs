// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How far the FIR producer reaches over the `adaptive_ri` corpus, and why it
//! stops.
//!
//! The stage's reach was an inference until this module. It was read off a
//! static enumeration of the corpus, and the enumeration reported a *first*
//! refusal per callable, which is the wrong instrument for attributing later
//! work. `loop_unification` desugars every `for` and every `repeat` into a
//! `while` wrapped in a block, and `"block"` is the name the producer reaches
//! first, so a loop refusal never appears as a loop refusal. Under a
//! first-refusal count a classical-statement policy would legitimately claim
//! every point a loop producer earns.
//!
//! So this records every refusal a callable would hit. Two instruments, read
//! side by side:
//!
//! * The producer itself, through `lower_callable_with_census`, which gives the
//!   authoritative answer to "did this callable lower" and names the first
//!   refusal when it did not.
//! * A survey that walks the same body and asks the same questions through the
//!   same code — `models_effect_expr`, `resolve_callee`, `target_of`, and
//!   `operation_name` — but records instead of returning and keeps walking.
//!
//! ## What the survey does not model
//!
//! Identity. `UnresolvedQubit`, `AmbiguousIdentity`, `RepeatedQubit`, and
//! `UnknownQubit` are answers about the wire a name holds, and reproducing them
//! would mean a second definition of the wire abstraction, free to disagree
//! with the first. The survey reports program *forms*; the producer reports
//! what actually happened. `every_first_refusal_is_a_form_the_survey_saw_or_an_identity_refusal`
//! is the check that keeps the two from drifting apart.
//!
//! Interpolated string components and struct field values are not descended
//! into. UDT erasure runs before this point, so a struct constructor should not
//! survive to be walked, and an interpolated expression carries no quantum
//! effect the producer would model.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use expect_test::expect;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_fir::fir::{
    BlockId, CallableImpl, ExprId, ExprKind, Global, ItemKind, LocalItemId, PackageId,
    PackageLookup, PackageStore, PackageStoreLookup, StmtKind, StoreItemId,
};
use qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize;
use qsc_fir_transforms::test_utils::{assert_no_pipeline_errors, compile_to_fir_with_capabilities};

use super::{
    Error, QUBIT_ALLOCATE, expr_kind_name, is_effect_free, lower_callable_with_census,
    models_effect_expr, operation_name, resolve_callee, target_of,
};
use crate::fir_optimize::{StageReport, optimize};
use crate::pass_pipeline::OptimizationLevel;

/// The corpus this stage's reach has always been discussed in terms of.
///
/// The same twenty-nine programs the golden-corpus pre-commitment was made
/// over, so a reach figure and a pair figure are figures about one population.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../qdk_package/tests-integration/resources/adaptive_ri/input")
}

fn corpus() -> Vec<(String, String)> {
    let mut programs: Vec<(String, String)> = std::fs::read_dir(corpus_dir())
        .expect("the corpus directory should exist")
        .map(|entry| entry.expect("the corpus entry should be readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "qs"))
        .map(|path| {
            let label = path
                .file_stem()
                .expect("a corpus program has a file name")
                .to_string_lossy()
                .into_owned();
            let source = std::fs::read_to_string(&path).expect("a corpus program should be read");
            (label, source)
        })
        .collect();
    programs.sort_by(|left, right| left.0.cmp(&right.0));
    programs
}

/// The short name one refusal is counted under.
///
/// An expression refusal keeps `expr_kind_name`'s own name, so a row in this
/// table and a refusal a user would see are the same word.
fn label(error: &Error) -> &'static str {
    match error {
        Error::NotACallable { .. } => "not a callable",
        Error::NoBody { .. } => "no body",
        Error::UnsupportedCallable { .. } => "unsupported callable",
        Error::RecursiveCallable { .. } => "recursive callable",
        Error::ControlledCall { .. } => "controlled call",
        Error::ValuedCall { .. } => "valued call",
        Error::UnresolvedCallee => "unresolved callee",
        Error::UnsupportedStatement { kind } | Error::UnsupportedExpression { kind } => kind,
        Error::UnsupportedLoop { .. } => "loop body",
        Error::ArgumentShape => "argument shape",
        Error::UnresolvedQubit => "unresolved qubit",
        Error::AmbiguousIdentity { .. } => "ambiguous identity",
        Error::RepeatedQubit => "repeated qubit",
        Error::UnknownQubit => "unknown qubit",
        Error::Build { .. } => "build",
        Error::Verification(_) => "verification",
    }
}

/// The refusals the survey answers for, which is every one that is a question
/// about a program form rather than about a wire.
fn is_identity_refusal(error: &Error) -> bool {
    matches!(
        error,
        Error::UnresolvedQubit
            | Error::AmbiguousIdentity { .. }
            | Error::RepeatedQubit
            | Error::UnknownQubit
            | Error::ArgumentShape
    )
}

/// Walks a callable body recording every refusal it would hit.
struct Survey<'a> {
    store: &'a PackageStore,
    call_stack: Vec<StoreItemId>,
    /// How many loop bodies deep the walk currently is, because a qubit
    /// allocated inside one is refused where an identical allocation outside
    /// one is not.
    in_loop: usize,
    found: BTreeSet<&'static str>,
}

impl<'a> Survey<'a> {
    fn of(store: &'a PackageStore, callable: StoreItemId) -> BTreeSet<&'static str> {
        let mut survey = Survey {
            store,
            call_stack: Vec::new(),
            in_loop: 0,
            found: BTreeSet::new(),
        };
        survey.callable(callable);
        survey.found
    }

    fn callable(&mut self, callable: StoreItemId) {
        let Some(Global::Callable(decl)) = self.store.get_global(callable) else {
            return;
        };
        let CallableImpl::Spec(spec) = &decl.implementation else {
            return;
        };
        let block = spec.body.block;
        self.call_stack.push(callable);
        self.block(callable.package, block);
        self.call_stack.pop();
    }

    fn block(&mut self, package: PackageId, block: BlockId) {
        let stmts = self.store.get(package).get_block(block).stmts.clone();
        for stmt in stmts {
            match &self.store.get(package).get_stmt(stmt).kind {
                StmtKind::Item(_) => {}
                StmtKind::Expr(expr) | StmtKind::Semi(expr) | StmtKind::Local(_, _, expr) => {
                    let expr = *expr;
                    self.expr(package, expr);
                }
            }
        }
    }

    fn expr(&mut self, package: PackageId, expr: ExprId) {
        let kind = self.store.get(package).get_expr(expr).kind.clone();
        // A block, a call, and a loop are walked by the producer rather than
        // cleared or refused, so none of them is a refusal of its own here.
        match kind {
            ExprKind::Block(block) => {
                self.block(package, block);
                return;
            }
            ExprKind::Call(callee, arg) => {
                self.call(package, expr, callee);
                self.expr(package, arg);
                return;
            }
            ExprKind::While(cond, body) => {
                if !is_effect_free(self.store, package, cond) {
                    self.found.insert("loop body");
                    self.expr(package, cond);
                }
                self.in_loop += 1;
                self.block(package, body);
                self.in_loop -= 1;
                return;
            }
            _ => {}
        }
        if !models_effect_expr(&kind) {
            // A form the judgement proves classical contributes nothing and
            // refuses nothing, and nothing below it can refuse either.
            if is_effect_free(self.store, package, expr) {
                return;
            }
            self.found.insert(expr_kind_name(&kind));
        }
        match kind {
            ExprKind::Array(items) | ExprKind::ArrayLit(items) | ExprKind::Tuple(items) => {
                for item in items {
                    self.expr(package, item);
                }
            }
            ExprKind::ArrayRepeat(left, right)
            | ExprKind::Assign(left, right)
            | ExprKind::AssignOp(_, left, right)
            | ExprKind::AssignField(left, _, right)
            | ExprKind::BinOp(_, left, right)
            | ExprKind::Index(left, right)
            | ExprKind::UpdateField(left, _, right) => {
                self.expr(package, left);
                self.expr(package, right);
            }
            ExprKind::AssignIndex(first, second, third)
            | ExprKind::UpdateIndex(first, second, third) => {
                self.expr(package, first);
                self.expr(package, second);
                self.expr(package, third);
            }
            ExprKind::Block(block) => self.block(package, block),
            ExprKind::Call(callee, arg) => {
                self.call(package, expr, callee);
                self.expr(package, arg);
            }
            ExprKind::Fail(inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Return(inner)
            | ExprKind::UnOp(_, inner) => self.expr(package, inner),
            ExprKind::If(cond, body, otherwise) => {
                self.expr(package, cond);
                self.expr(package, body);
                if let Some(otherwise) = otherwise {
                    self.expr(package, otherwise);
                }
            }
            ExprKind::Parallel(limit, body) => {
                if let Some(limit) = limit {
                    self.expr(package, limit);
                }
                self.expr(package, body);
            }
            ExprKind::Range(start, step, end) => {
                for part in [start, step, end].into_iter().flatten() {
                    self.expr(package, part);
                }
            }
            ExprKind::While(cond, body) => {
                self.expr(package, cond);
                self.block(package, body);
            }
            ExprKind::Closure(..)
            | ExprKind::Hole
            | ExprKind::Lit(_)
            | ExprKind::String(_)
            | ExprKind::Struct(..)
            | ExprKind::Var(..) => {}
        }
    }

    fn call(&mut self, package: PackageId, expr: ExprId, callee: ExprId) {
        let (item, adjoint, controlled) = match resolve_callee(self.store, package, callee) {
            Ok(resolved) => resolved,
            Err(error) => {
                self.found.insert(label(&error));
                return;
            }
        };
        let target = match target_of(self.store, item, adjoint, controlled) {
            Ok(target) => target,
            Err(error) => {
                // A valued call the judgement clears is not a refusal: the
                // callee performs no quantum operation and the graph never
                // holds its value.
                if matches!(error, Error::ValuedCall { .. })
                    && is_effect_free(self.store, package, expr)
                {
                    return;
                }
                self.found.insert(label(&error));
                return;
            }
        };
        let Some(block) = target.block else {
            // A qubit allocated inside a loop body would have its release left
            // outside the region the allocation sits in.
            if self.in_loop > 0 && target.name == QUBIT_ALLOCATE {
                self.found.insert("loop body");
            }
            // An intrinsic with no vocabulary name is the refusal that hides
            // every measurement in this corpus.
            if operation_name(&target.name).is_none() {
                self.found.insert("unsupported callable");
            }
            return;
        };
        if self.call_stack.contains(&item) {
            self.found.insert("recursive callable");
            return;
        }
        self.call_stack.push(item);
        self.block(target.package, block);
        self.call_stack.pop();
    }
}

/// One callable, measured both ways.
struct Callable {
    name: String,
    /// The producer's own first refusal, or `None` when it lowered the body.
    first: Option<&'static str>,
    /// Whether the producer's first refusal was one the survey does not model.
    identity: bool,
    /// Every form refusal the survey found in the body as reached.
    every: BTreeSet<&'static str>,
}

/// One corpus program, with every callable of its own package measured.
struct Program {
    label: String,
    callables: Vec<Callable>,
    /// What the stage itself did with the program, from its one assembly.
    report: StageReport,
}

fn measure(program_label: &str, source: &str) -> Program {
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    let (mut store, package) = compile_to_fir_with_capabilities(source, capabilities);
    // Measured at the point the stage runs at rather than at the point FIR was
    // lowered, because the producer never sees the untransformed form.
    let mut measured = Vec::new();
    let mut report = StageReport::default();
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let items: Vec<LocalItemId> = store
                .get(package)
                .items
                .iter()
                .filter_map(|(id, item)| matches!(item.kind, ItemKind::Callable(_)).then_some(id))
                .collect();
            for item in items {
                let callable = StoreItemId { package, item };
                let Some(Global::Callable(decl)) = store.get_global(callable) else {
                    continue;
                };
                let name = decl.name.name.to_string();
                let outcome = lower_callable_with_census(store, callable).0;
                let (first, identity) = match &outcome {
                    Ok(_) => (None, false),
                    Err(error) => (Some(label(error)), is_identity_refusal(error)),
                };
                measured.push(Callable {
                    name,
                    first,
                    identity,
                    every: Survey::of(store, callable),
                });
            }
            // Last, because it edits the store the survey above reads.
            report = optimize(store, package, OptimizationLevel::Default);
        });
    assert_no_pipeline_errors("fir_qhdg::reach", &result.errors);
    measured.sort_by(|left, right| left.name.cmp(&right.name));
    Program {
        label: program_label.to_string(),
        callables: measured,
        report,
    }
}

/// The measured corpus, taken once.
///
/// Every test here reads the same measurement. Compiling twenty-nine programs
/// through the transform pipeline is the cost of this module, and taking it
/// once per test would multiply it by the number of questions asked.
fn measured_corpus() -> &'static [Program] {
    static MEASURED: std::sync::OnceLock<Vec<Program>> = std::sync::OnceLock::new();
    MEASURED.get_or_init(|| {
        corpus()
            .iter()
            .map(|(label, source)| measure(label, source))
            .collect()
    })
}

fn reach_table(programs: &[Program]) -> String {
    use std::fmt::Write as _;

    let mut rendered = format!(
        "{:<34} {:>9} {:>7} {:>7}  {}\n",
        "program", "callables", "lowered", "refused", "every refusal in the program"
    );
    for program in programs {
        let lowered = program
            .callables
            .iter()
            .filter(|callable| callable.first.is_none())
            .count();
        let every: BTreeSet<&str> = program
            .callables
            .iter()
            .flat_map(|callable| callable.every.iter().copied())
            .collect();
        writeln!(
            rendered,
            "{:<34} {:>9} {:>7} {:>7}  {}",
            program.label,
            program.callables.len(),
            lowered,
            program.callables.len() - lowered,
            every.into_iter().collect::<Vec<_>>().join(", ")
        )
        .expect("writing to a string cannot fail");
    }
    rendered
}

/// How many callables each refusal is the producer's first, beside how many
/// would hit it at all.
///
/// The two columns are the whole reason this module exists. A refusal whose
/// `first` is zero and whose `every` is not is a refusal no first-refusal
/// instrument can attribute anything to.
fn refusal_table(programs: &[Program]) -> String {
    use std::fmt::Write as _;

    let mut first: BTreeMap<&str, usize> = BTreeMap::new();
    let mut every: BTreeMap<&str, usize> = BTreeMap::new();
    for callable in programs.iter().flat_map(|program| &program.callables) {
        if let Some(refusal) = callable.first {
            *first.entry(refusal).or_default() += 1;
        }
        for refusal in &callable.every {
            *every.entry(refusal).or_default() += 1;
        }
    }

    let mut rendered = format!("{:<24} {:>7} {:>7}\n", "refusal", "first", "every");
    for refusal in first.keys().chain(every.keys()).collect::<BTreeSet<_>>() {
        writeln!(
            rendered,
            "{:<24} {:>7} {:>7}",
            refusal,
            first.get(*refusal).copied().unwrap_or_default(),
            every.get(*refusal).copied().unwrap_or_default()
        )
        .expect("writing to a string cannot fail");
    }
    rendered
}

fn totals(programs: &[Program]) -> String {
    let callables: usize = programs.iter().map(|program| program.callables.len()).sum();
    let lowered: usize = programs
        .iter()
        .flat_map(|program| &program.callables)
        .filter(|callable| callable.first.is_none())
        .count();
    let edits: usize = programs.iter().map(|program| program.report.edits).sum();
    let declined: usize = programs.iter().map(|program| program.report.declined).sum();
    let unchanged: usize = programs
        .iter()
        .map(|program| program.report.unchanged)
        .sum();
    format!(
        "{} programs, {} callables, {} lowered, {} refused\n\
         stage report: {} edits, {} declined, {} lowered without change\n",
        programs.len(),
        callables,
        lowered,
        callables - lowered,
        edits,
        declined,
        unchanged,
    )
}

/// The whole baseline as one recorded artifact.
fn baseline(programs: &[Program]) -> String {
    format!(
        "{}\n{}\n{}",
        totals(programs),
        reach_table(programs),
        refusal_table(programs)
    )
}

#[test]
fn the_producer_reaches_this_much_of_the_corpus() {
    // The baseline. Every reach claim later in this work is a delta against
    // this snapshot, which is why it is recorded rather than described, and why
    // regenerating it is a claim about the compiler rather than mechanical
    // churn.
    //
    // The current corpus holds 93 callables at this point in the pipeline,
    // and the producer reaches three of them.
    //
    // The loop refusal is gone from this table and the lowered count did not
    // move, which is the finding rather than a disappointment. Every callable
    // that used to stop at a loop now walks into it and stops at something
    // else: `unresolved qubit` went from 10 first refusals to 29, because a
    // `for q in qs` binds a qubit by array element, and `conditional` and
    // `valued call` were already waiting behind it.
    expect![[r#"
        34 programs, 93 callables, 3 lowered, 90 refused
        stage report: 0 edits, 90 declined, 3 lowered without change

        program                            callables lowered refused  every refusal in the program
        ArithmeticOps                              1       0       1  conditional, valued call
        BernsteinVaziraniNISQ                     11       1      10  closure, conditional, fail, return, unresolved callee, unsupported callable, valued call
        BooleanNot                                 2       1       1  binary operator, valued call
        CallReturnMath                             3       0       3  conditional, valued call
        ConstantFolding                            1       0       1  binary operator, conditional, valued call
        CopyAndUpdateExpressions                   2       0       2  assignment, conditional, index update, valued call
        DeutschJozsaNISQ                           7       1       6  conditional, return, unresolved callee, valued call
        DynamicValueCapturedInAggregates           1       0       1  binary operator, conditional, valued call
        DynamicValueUpdatedInArray                 1       0       1  binary operator, conditional, valued call
        ExpandedTests                             12       0      12  binary operator, conditional, controlled call, range, recursive callable, tuple, valued call
        Functors                                   2       0       2  conditional, controlled call, valued call
        HiddenShiftNISQ                           11       0      11  conditional, return, unresolved callee, unsupported callable, valued call
        IntegerComparison                          1       0       1  assignment, binary operator, conditional, valued call
        IntrinsicCCNOT                             1       0       1  conditional, valued call
        IntrinsicCNOT                              1       0       1  conditional, valued call
        IntrinsicHIXYZ                             1       0       1  conditional, valued call
        IntrinsicM                                 1       0       1  conditional, valued call
        IntrinsicMeasureWithBitFlipCode            2       0       2  array, conditional, tuple, valued call
        IntrinsicMeasureWithPhaseFlipCode          2       0       2  array, conditional, tuple, valued call
        IntrinsicRotationsWithPeriod               5       0       5  array, conditional, unsupported callable, valued call
        IntrinsicSTSWAP                            1       0       1  array, conditional, valued call
        LoopWithMutableVariableUpdates             1       0       1  conditional, valued call
        MeasureAndReuse                            1       0       1  conditional, valued call
        MeasurementComparison                      1       0       1  conditional, tuple, valued call
        MutationAcrossClassicalBranches            1       0       1  binary operator, conditional, valued call
        NestedBranching                            2       0       2  binary operator, conditional, tuple, valued call
        RandomBit                                  1       0       1  conditional, valued call
        SampleTeleport                             3       0       3  assignment, binary operator, conditional, valued call
        ShortcuttingMeasurement                    1       0       1  binary operator, conditional, tuple, valued call
        Slicing                                    1       0       1  conditional, valued call
        SuperdenseCoding                           5       0       5  binary operator, conditional, valued call
        SwitchHandling                             1       0       1  conditional, unsupported callable, valued call
        ThreeQubitRepetitionCode                   5       0       5  binary operator, conditional, unsupported callable, valued call
        WithinApply                                1       0       1  conditional, controlled call, valued call

        refusal                    first   every
        ambiguous identity             1       0
        array                          1       4
        assignment                     1       3
        binary operator                5      13
        closure                        1       1
        conditional                    6      51
        controlled call                1       6
        fail                           1       1
        index update                   1       1
        range                          0       1
        recursive callable             0       1
        return                         0       3
        tuple                          1       9
        unresolved callee              0       6
        unresolved qubit              29       0
        unsupported callable           6      10
        valued call                   36      58
    "#]]
    .assert_eq(&baseline(measured_corpus()));
}

#[test]
fn every_first_refusal_is_a_form_the_survey_saw_or_an_identity_refusal() {
    // The check that keeps the two instruments from drifting. The survey reads
    // the same predicates the producer reads, so a first refusal it did not see
    // means either the survey stopped walking somewhere the producer walks, or
    // the refusal is one about a wire rather than about a form.
    for program in measured_corpus() {
        for callable in &program.callables {
            let Some(first) = callable.first else {
                continue;
            };
            assert!(
                callable.identity || callable.every.contains(first),
                "'{}' in '{}' refused with '{}', which the survey did not see in {:?}",
                callable.name,
                program.label,
                first,
                callable.every
            );
        }
    }
}

#[test]
fn the_four_programs_known_to_emit_identical_qir_change_nothing() {
    // The check that the instrument is reading what it thinks it is. These four
    // were measured emitting byte-identical QIR through the FIR stage before
    // this table existed.
    //
    // The property is zero edits rather than zero reach, and the difference is
    // a correction the measurement made to the enumeration it replaced. One
    // callable in `DeutschJozsaNISQ` does lower: the producer reaches it, the
    // rule finds no pair in it, and the emitted program is identical anyway.
    // That is the outcome the stage report gained a third counter for.
    let known = [
        "BernsteinVaziraniNISQ",
        "DeutschJozsaNISQ",
        "Functors",
        "MeasureAndReuse",
    ];
    for program in measured_corpus() {
        if !known.contains(&program.label.as_str()) {
            continue;
        }
        assert_eq!(
            program.report.edits, 0,
            "'{}' emits byte-identical QIR, so the stage removed no statement from it",
            program.label
        );
    }
}

#[test]
fn recording_every_refusal_attributes_more_than_the_first_one_would() {
    // The reason this module records every refusal rather than the first.
    //
    // It was demonstrated most sharply by loops, which no longer appear here at
    // all: `loop_unification` wraps every desugared loop in a block, `"block"`
    // was reached first, and a loop was the first refusal for nobody until the
    // classical-statement policy walked the wrapper. Under a first-refusal
    // instrument the classical policy would have claimed every callable the
    // loop producer later unblocked.
    //
    // The property is asserted over the whole table rather than over one
    // refusal, so it survives the frontier moving again.
    let programs = measured_corpus();
    let mut hidden = 0;
    for callable in programs.iter().flat_map(|program| &program.callables) {
        hidden += callable
            .every
            .iter()
            .filter(|refusal| callable.first != Some(**refusal))
            .count();
    }
    assert!(
        hidden > 0,
        "a first-refusal instrument would attribute nothing to {hidden} refusals \
         callables in this corpus would hit"
    );
}
