// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Time per bottom-ISA instruction, for every example qodec in `examples/`,
//! in two groups that lower the same programs:
//!
//! - `compile_throughput`: a [`Compiler`], the encoders of every layer without
//!   decoders, lowers the program ahead of time. Examples whose gadgets feed a
//!   readout into a later call are adaptive, so they can't be compiled and are
//!   skipped.
//! - `runtime_throughput`: a runtime built with [`FrameDecoding`], a frame
//!   decoder on top of the encoder in every layer, runs the program over a
//!   bottom backend that does no work but answer 0 for every bit. The time
//!   includes decoding and the results' way back up.
//!
//! Each benchmark lowers whole shots, a program written in the REPL's syntax.
//! One criterion iteration is one bottom instruction: a shot's time is spread
//! evenly over the bottom instructions it produces. Building a shot's compiler
//! or runtime, copying its program, and dropping what it produced are not
//! timed.
//!
//! To run the entire bench:
//! `cargo bench -p qodec_repl --bench bottom_throughput`
//!
//! To run one group:
//! `cargo bench -p qodec_repl --bench bottom_throughput -- compile_throughput`
//!

#[allow(dead_code)]
#[path = "../src/style.rs"]
mod style;
#[allow(dead_code)]
#[path = "../src/syntax.rs"]
mod syntax;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use qodec::Qodec;
use qodec_runtime_builder::{
    Backend, BasicReadout, Call, Compiler, FrameDecoding, QodecPlan, Status,
};
use std::{
    cell::Cell,
    convert::Infallible,
    hint::black_box,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};
use syntax::Ins;

/// QEC rounds per shot, in examples that have them.
const ROUNDS: usize = 10;

/// A bottom backend that counts the instructions it receives and returns 0
/// for every outcome and flag.
struct Sink {
    /// The result of each bottom instruction, by id, if it reports one.
    readouts: Rc<[Option<BasicReadout>]>,
    count: Rc<Cell<u64>>,
}

impl Backend<Vec<Call>, Result<Vec<BasicReadout>, Infallible>> for Sink {
    fn execute(&mut self, instructions: Vec<Call>) -> Result<Vec<BasicReadout>, Infallible> {
        self.count.set(self.count.get() + instructions.len() as u64);
        let results = instructions
            .iter()
            .filter_map(|ins| self.readouts[ins.instruction_id].clone())
            .collect();
        Ok(results)
    }
}

/// `statements` repeated `n` times, `;`-separated.
fn repeat(statements: &str, n: usize) -> String {
    vec![statements; n].join("; ")
}

/// Each example's manifest, and the program of one shot.
fn examples() -> Vec<(&'static str, String)> {
    let idle = repeat("idle 0", ROUNDS);
    vec![
        (
            "bacon-shor/bacon-shor.qodec.yaml",
            format!("prepare_z 0; {idle}; measure_z 0"),
        ),
        (
            "bb-72/bb-72.qodec.yaml",
            format!("prepare_z_all 0; {idle}; measure_z_all 0"),
        ),
        (
            "c422-c832-arch/qodec.yaml",
            format!(
                "prepare_z_all_c832 0; {}; measure_z_all_c832 0",
                repeat(
                    "ccz_c832 0; teleport_c832_to_c422 0 1; h_all_c422 1; teleport_c422_to_c832 1 0",
                    ROUNDS / 2
                )
            ),
        ),
        (
            "c4c6/qodec.yaml",
            format!("prepare_z_all 0; {idle}; measure_z_all 0"),
        ),
        (
            // The factory ISA only distills, so every shot distills ROUNDS states.
            "distillation-15/distillation-15.qodec.yaml",
            (0..ROUNDS)
                .map(|block| format!("distill_t {block}"))
                .collect::<Vec<_>>()
                .join("; "),
        ),
        (
            "five_qubit/five_qubit.qodec.yaml",
            format!("prepare_z 0; {idle}; {}; measure_z 0", repeat("cycle 0", 3)),
        ),
        (
            "honeycomb/honeycomb.qodec.yaml",
            format!(
                "prepare_red_all 0; {}; measure_red_all 0",
                repeat("round_green 0; round_blue 0; round_red 0", ROUNDS / 2)
            ),
        ),
        (
            "iceberg/iceberg.qodec.yaml",
            format!("prepare_z_all 0; {idle}; measure_z_all 0"),
        ),
        (
            "reed-muller-15/reed-muller-15.qodec.yaml",
            format!("prepare_z 0; {idle}; t 0; measure_z 0"),
        ),
        (
            "repetition3/repetition3.qodec.yaml",
            format!("prepare_z 0; {idle}; rotate_z 0 theta=0.5; measure_z 0"),
        ),
        (
            "steane/steane.qodec.yaml",
            format!(
                "prepare_z 0; prepare_x 1; {}; h 0; cnot 0 1; measure_z 0; measure_x 1",
                repeat("idle 0; idle_ft 1", ROUNDS)
            ),
        ),
        (
            "surface/surface.qodec.yaml",
            format!(
                "prepare_z 0; prepare_z 1; {}; merge_zz 0 1; measure_merged_z 0",
                repeat("idle 0; idle 1", ROUNDS)
            ),
        ),
    ]
}

/// The bottom backend's result for each bottom instruction, by id.
fn zero_readouts(qodec: &Qodec) -> Rc<[Option<BasicReadout>]> {
    let bottom = &qodec
        .layers()
        .last()
        .expect("a qodec has a layer")
        .instruction_set;
    bottom
        .instructions
        .iter()
        .map(|decl| {
            let (observes, flags) = (decl.observe_count(), decl.flags.len());
            (observes + flags > 0).then(|| {
                BasicReadout::new(
                    &vec![Some(false); observes],
                    &vec![Some(false); flags],
                    Status::default(),
                )
            })
        })
        .collect()
}

/// One example, loaded and planned.
struct Example {
    name: &'static str,
    plan: QodecPlan,
    program: Vec<Ins>,
    readouts: Rc<[Option<BasicReadout>]>,
}

fn load_examples() -> Vec<Example> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    examples()
        .into_iter()
        .map(|(path, program)| {
            let qodec = Qodec::load(dir.join(path)).expect("example should load");
            let plan = QodecPlan::new(&qodec, &FrameDecoding).expect("example should plan");
            let top = &qodec.layers()[0].instruction_set;
            let program = syntax::statements(&program)
                .and_then(|statements| {
                    statements
                        .into_iter()
                        .map(|s| syntax::parse_instruction(top, s))
                        .collect::<Result<Vec<Ins>, _>>()
                })
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            Example {
                name: path
                    .split('/')
                    .next()
                    .expect("a path has a first component"),
                plan,
                program,
                readouts: zero_readouts(&qodec),
            }
        })
        .collect()
}

/// Runs whole shots until `iters` bottom instructions are covered, and scales
/// their time to exactly `iters` instructions. Only `run` is timed: `setup`
/// prepares a shot, and what `run` returns is dropped after the clock stops.
fn time_shots<S, R>(
    iters: u64,
    per_shot: u64,
    mut setup: impl FnMut() -> S,
    mut run: impl FnMut(&mut S) -> R,
) -> Duration {
    let shots = iters.div_ceil(per_shot);
    let mut elapsed = Duration::ZERO;
    for _ in 0..shots {
        let mut shot = setup();
        let start = Instant::now();
        let out = black_box(run(&mut shot));
        elapsed += start.elapsed();
        drop(out);
        drop(shot);
    }
    #[allow(clippy::cast_precision_loss)]
    elapsed.mul_f64(iters as f64 / (shots * per_shot) as f64)
}

fn compile_throughput(c: &mut Criterion, examples: &[Example]) {
    let mut group = c.benchmark_group("compile_throughput");
    // One iteration is one bottom instruction.
    group.throughput(Throughput::Elements(1));
    for example in examples {
        let compiler = || Compiler::for_plan(&example.plan).expect("encoders plan");
        // An untimed shot counts the bottom instructions, and checks that the
        // program compiles.
        let per_shot = match compiler().lower(example.program.clone()) {
            Ok(calls) => calls.len() as u64,
            Err(e) => {
                eprintln!("compile_throughput/{}: skipped: {e}", example.name);
                continue;
            }
        };
        group.bench_function(example.name, |b| {
            b.iter_custom(|iters| {
                time_shots(
                    iters,
                    per_shot,
                    || (compiler(), example.program.clone()),
                    |(compiler, program)| {
                        compiler
                            .lower(black_box(std::mem::take(program)))
                            .expect("compiles")
                    },
                )
            });
        });
    }
    group.finish();
}

fn runtime_throughput(c: &mut Criterion, examples: &[Example]) {
    let mut group = c.benchmark_group("runtime_throughput");
    // One iteration is one bottom instruction.
    group.throughput(Throughput::Elements(1));
    for example in examples {
        let count = Rc::new(Cell::new(0));
        let runtime = || {
            example.plan.instantiate(Sink {
                readouts: Rc::clone(&example.readouts),
                count: Rc::clone(&count),
            })
        };
        // An untimed shot counts the bottom instructions, and checks that the
        // program runs.
        runtime()
            .execute(example.program.as_slice())
            .unwrap_or_else(|e| panic!("{}: {e}", example.name));
        let per_shot = count.get();
        group.bench_function(example.name, |b| {
            b.iter_custom(|iters| {
                time_shots(iters, per_shot, runtime, |runtime| {
                    runtime.execute(black_box(example.program.as_slice())).ok()
                })
            });
        });
    }
    group.finish();
}

fn throughput(c: &mut Criterion) {
    let examples = load_examples();
    compile_throughput(c, &examples);
    runtime_throughput(c, &examples);
}

criterion_group!(benches, throughput);
criterion_main!(benches);
