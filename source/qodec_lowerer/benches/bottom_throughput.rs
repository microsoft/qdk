// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Time per bottom-ISA instruction emitted by the lowering stack, for every
//! example qodec in `examples/`.
//!
//! Each benchmark lowers whole shots, a program written in the REPL's syntax,
//! into a bottom backend that does no work, so the time is spent in the layer
//! and decoder runtimes. One criterion iteration is one bottom instruction: a
//! shot's time is spread evenly over the bottom instructions it emits. Building
//! and dropping a shot's runtime is not timed.

#[allow(dead_code)]
#[path = "../src/style.rs"]
mod style;
#[allow(dead_code)]
#[path = "../src/syntax.rs"]
mod syntax;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use qodec::Qodec;
use qodec_runtime::{
    Backend, BackendInstruction, BasicReadout, BatchResult, FrameDecoder, OwnedParam, QodecPlan,
    Readout, Status,
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

impl Backend<usize, OwnedParam, BasicReadout> for Sink {
    type Error = Infallible;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<usize, OwnedParam>],
    ) -> Result<BatchResult<BasicReadout>, Infallible> {
        self.count.set(self.count.get() + instructions.len() as u64);
        let results = instructions
            .iter()
            .filter_map(|ins| self.readouts[ins.instruction_id].clone())
            .collect();
        Ok(BatchResult { results })
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

fn bottom_throughput(c: &mut Criterion) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut group = c.benchmark_group("bottom_throughput");
    // One iteration is one bottom instruction.
    group.throughput(Throughput::Elements(1));
    for (path, program) in examples() {
        let qodec = Qodec::load(dir.join(path)).expect("example should load");
        let plan = QodecPlan::new(&qodec, &FrameDecoder).expect("example should plan");
        let top = &qodec.layers()[0].instruction_set;
        let instructions = syntax::statements(&program)
            .and_then(|statements| {
                statements
                    .into_iter()
                    .map(|s| syntax::parse_instruction(top, s))
                    .collect::<Result<Vec<Ins>, _>>()
            })
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let readouts = zero_readouts(&qodec);
        let count = Rc::new(Cell::new(0));
        let shot = || {
            plan.instantiate(Sink {
                readouts: Rc::clone(&readouts),
                count: Rc::clone(&count),
            })
        };

        // An untimed shot counts the bottom instructions, and checks the
        // program lowers.
        shot()
            .execute_batch(&instructions)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let per_shot = count.get();

        let name = path
            .split('/')
            .next()
            .expect("a path has a first component");
        group.bench_function(name, |b| {
            // Runs whole shots until `iters` bottom instructions are covered,
            // and scales their time to exactly `iters` instructions.
            b.iter_custom(|iters| {
                let shots = iters.div_ceil(per_shot);
                let mut elapsed = Duration::ZERO;
                for _ in 0..shots {
                    let mut runtime = shot();
                    let start = Instant::now();
                    black_box(runtime.execute_batch(black_box(&instructions))).ok();
                    elapsed += start.elapsed();
                }
                #[allow(clippy::cast_precision_loss)]
                elapsed.mul_f64(iters as f64 / (shots * per_shot) as f64)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bottom_throughput);
criterion_main!(benches);
