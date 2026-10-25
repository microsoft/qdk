// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The A100 qualification: the cases of `cases` on cuTensorNet, against
//! hand-computed values, the exact reference, and M0's pinned B1–B4
//! values; and H₂ QPE, M1's acceptance test, through the walk. Every test
//! is `#[ignore]`d; `qualify-simulator-on-a100.sh` runs them on the pinned
//! VM with `cargo test -p tensornet_simulator --release -- --ignored
//! --nocapture --test-threads=1 tests::qualification`, skipping H₂, which
//! `--h2` runs alone.
//!
//! The engine is `Availability::mps_engine`, whose policy is M0's base
//! qualification policy (absolute cutoff 1e-10). M0 ran B3 and B4 with an
//! absolute cutoff of 1e-12, which the engine does not expose; their
//! values and tolerances are M0's, and a miss is to be read against that
//! difference first (see the crate README).

use std::{
    collections::BTreeMap,
    env, fs,
    io::Write as _,
    time::{Duration, Instant},
};

use executor_protocol::{Batch, Executor, Rejected, ShotSet, SubmissionWrapper, open};
use qdk_cutensornet::CuTensorNetEngine;
use qdk_simulators::{
    OutputRecord,
    execution::{PreparedAdaptiveProgram, RegionInstruction, RuntimeAdapter},
};
use tensornet::reference::ExactReference;

use super::{
    adaptive_program_json::adaptive_program_from_json,
    cases::{
        bell_samples_are_correlated, bit, expectations_agree,
        expectations_are_the_hand_computed_ones, per_shot_bell_pairs, per_shot_feedback_and_resets,
        per_shot_phase_estimation, trotter_matches_m0, x_pattern_reads_back,
    },
};
use crate::MpsStack;

/// cuTensorNet on device 0 with χ = `max_bond`, after printing what was
/// discovered.
fn stack(max_bond: u32) -> MpsStack<CuTensorNetEngine> {
    let availability = qdk_cutensornet::discover().expect("the native libraries are available");
    let report = availability.report();
    println!(
        "cutensornet_library={}",
        report.cutensornet_library.display()
    );
    println!("cudart_library={}", report.cuda_runtime_library.display());
    println!("cutensornet_version={}", report.cutensornet_version);
    println!(
        "cutensornet_cuda_runtime_version={}",
        report.cutensornet_cuda_runtime_version
    );
    println!("cudart_version={}", report.cuda_runtime_version);
    println!("cuda_driver_version={}", report.cuda_driver_version);
    let engine = availability.mps_engine().expect("an engine on device 0");
    MpsStack::new(engine).with_max_bond_dimension(Some(max_bond))
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn bell_on_the_a100() {
    let stack = stack(128);
    bell_samples_are_correlated(&stack, 1000);
    expectations_are_the_hand_computed_ones(&stack, 1e-10);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b0_expectations_agree_with_the_reference() {
    // χ = 128 exceeds 2^(4/2), so both chains are exact up to round-off.
    expectations_agree(
        &stack(128),
        &MpsStack::new(ExactReference::new()),
        0..16,
        1e-10,
    );
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b1_widths() {
    let stack = stack(128);
    for width in [2, 3, 63, 64, 128] {
        x_pattern_reads_back(&stack, width);
        println!("b1 width={width} ok");
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b2_trotter() {
    // M0's B2: three widths pin extensivity.
    let stack = stack(128);
    for (width, expected) in [
        (12, 4.332_869_154_633),
        (16, 6.347_012_657_087),
        (20, 8.361_156_159_877),
    ] {
        trotter_matches_m0(&stack, width, 8, expected, 1e-9);
        // The reference, where it fits (up to 20 sites).
        trotter_matches_m0(
            &MpsStack::new(ExactReference::new()),
            width,
            8,
            expected,
            1e-9,
        );
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b3_matched_bond() {
    trotter_matches_m0(&stack(128), 128, 16, 60.319_518_034_172_646, 1e-10);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn b4_convergence() {
    for (max_bond, expected) in [
        (32, 122.350_509_319_616),
        (64, 122.350_509_321_997),
        (128, 122.350_509_322_001),
        (256, 122.350_509_322_001),
    ] {
        trotter_matches_m0(&stack(max_bond), 256, 16, expected, 1e-9);
    }
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn per_shot_bell_pairs_on_the_a100() {
    per_shot_bell_pairs(&stack(128), 200);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn per_shot_feedback_and_resets_on_the_a100() {
    // M0's B5 checked a branch's capture and continuation; here a shot's
    // state continues after its measurements, resets and feedback.
    per_shot_feedback_and_resets(&stack(4), 40);
}

#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
fn per_shot_phase_estimation_on_the_a100() {
    // The bond is at most 2, so χ = 4 is exact; ln p to rounding.
    per_shot_phase_estimation(&stack(4), 200, 1e-8);
}

/// H₂'s peaks as the chemistry user writes its rows: qubit 4 first, the
/// reverse of the record order (`qdk_chemistry`'s `_process_raw_results`
/// builds each string from `reversed(one_run)`). They are the ±θ eigenphase
/// pair, 10 + 22 = 32.
const H2_PEAKS: [&str; 2] = ["01010", "10110"];

/// The shots from which the H₂ test asserts its peaks. The sparse CPU
/// simulator's 11 shots (agent E, Oct 25) gave the peaks 6 and 4 times and a
/// third row once; at those frequencies, 50 shots expect about 27, 18 and 5,
/// and the second peak leads the third by about three standard deviations.
/// Fewer shots cannot tell the ranking, so the test only prints it.
const H2_SHOTS_TO_ASSERT_PEAKS: u32 = 50;

/// How often, in batches (one per measurement, 1,404 per H₂ shot), a shot
/// prints its progress and the device memory.
const H2_PROGRESS_EVERY: u64 = 100;

/// The device memory this process holds, in MiB, as `nvidia-smi` lists its
/// compute processes (as `qdk_cutensornet`'s qualification reads it), or
/// `unavailable`.
fn device_mib() -> String {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let pid = std::process::id().to_string();
    output
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.split_once(','))
                .filter(|(process, _)| process.trim() == pid)
                .find_map(|(_, used)| used.trim().parse::<u64>().ok())
        })
        .map_or_else(|| "unavailable".to_string(), |mib| mib.to_string())
}

/// The walk's terminal for one H₂ shot, which prints the shot's progress and
/// the device memory every [`H2_PROGRESS_EVERY`] batches, so that a run
/// that runs out of device memory shows how it got there.
struct ShotProgress<E> {
    inner: E,
    shot: u32,
    batches: u64,
    start: Instant,
}

impl<E: Executor<Batch<RegionInstruction>>> Executor<Batch<RegionInstruction>> for ShotProgress<E> {
    type Response = E::Response;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        self.batches += 1;
        if self.batches.is_multiple_of(H2_PROGRESS_EVERY) {
            println!(
                "h2 shot {}: {} batches, {:.1} s, device_mib {}",
                self.shot,
                self.batches,
                self.start.elapsed().as_secs_f64(),
                device_mib()
            );
            std::io::stdout().flush().ok();
        }
        self.inner.call(batch)
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    env::var(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name}={value} should be a number"))
    })
}

/// A row in record order, qubit 0 first.
fn row(records: &[OutputRecord]) -> String {
    records
        .iter()
        .map(|record| if bit(record) { '1' } else { '0' })
        .collect()
}

/// H₂ QPE through the walk over `MpsStack::per_shot` on cuTensorNet, M1's
/// acceptance test. Shot `j` (from 1) runs in its own session, opened on
/// shot `j` with the run's seed, as `qdk_simulators`'s `h2_qpe` runs it on
/// CPU. The environment:
///
/// - `QDK_H2_BYTECODE` (required): the JSON of H₂'s
///   `AdaptiveProgram.as_dict()` (see `qdk_simulators`'s `h2_qpe`).
/// - `QDK_H2_SHOTS` (1), `QDK_H2_SEED` (0), `QDK_H2_CHI` (32): the shots,
///   the run's seed and χ.
///
/// Per shot it prints its time, the device memory with its state still held
/// and after its session closes, and its row in record order and as the
/// chemistry user writes it (qubit 4 first, [`H2_PEAKS`]). Then the histogram, the ten most
/// frequent rows, and whether [`H2_PEAKS`] are the two most frequent, which
/// it asserts from [`H2_SHOTS_TO_ASSERT_PEAKS`] shots. An engine error ends
/// the run with the shot, its progress and the error.
#[test]
#[ignore = "requires the pinned A100 environment and H₂'s bytecode in QDK_H2_BYTECODE"]
fn h2_qpe_on_the_a100() {
    let path = env::var("QDK_H2_BYTECODE").unwrap_or_else(|_| {
        panic!(
            "QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict() \
             (qualify-simulator-on-a100.sh --h2 FILE sets it)"
        )
    });
    let shots = u32::try_from(env_or("QDK_H2_SHOTS", 1)).expect("the shots should fit u32");
    let seed = env_or("QDK_H2_SEED", 0);
    let chi = u32::try_from(env_or("QDK_H2_CHI", 32)).expect("χ should fit u32");
    let json = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = PreparedAdaptiveProgram::new(adaptive_program_from_json(&json))
        .unwrap_or_else(|e| panic!("H₂ should prepare: {e}"));
    let qubits = usize::try_from(program.program().num_qubits).expect("fits usize");
    println!(
        "h2 bytecode={path} qubits={qubits} results={} shots={shots} seed={seed} chi={chi}",
        program.program().num_results
    );
    let per_shot = stack(chi).per_shot(qubits).expect("a chain");
    println!("h2 device_mib before the first shot: {}", device_mib());

    let mut histogram = BTreeMap::<String, u32>::new();
    let mut total = Duration::ZERO;
    for shot in 1..=shots {
        let start = Instant::now();
        let session = open(&per_shot, seed, &ShotSet::one(shot))
            .unwrap_or_else(|e| panic!("h2 shot {shot}: the session does not open: {e}"));
        let mut adapter = RuntimeAdapter::new(ShotProgress {
            inner: SubmissionWrapper::new(session, shot),
            shot,
            batches: 0,
            start,
        });
        let mut records = Vec::new();
        if let Err(error) = adapter.run(&program, &mut records) {
            panic!(
                "h2 shot {shot} failed after {} batches, {:.1} s, device_mib {}: {error}",
                adapter.inner().batches,
                start.elapsed().as_secs_f64(),
                device_mib()
            );
        }
        let time = start.elapsed();
        total += time;
        let held = device_mib();
        drop(adapter);
        let row = row(&records);
        let chemistry: String = row.chars().rev().collect();
        println!(
            "h2 shot {shot}: row {row} chemistry {chemistry} time {:.1} s device_mib {held} \
             after_close {}",
            time.as_secs_f64(),
            device_mib()
        );
        std::io::stdout().flush().ok();
        *histogram.entry(chemistry).or_default() += 1;
    }

    println!(
        "h2 {shots} shots, seed {seed}, chi {chi}: {:.1} s per shot",
        total.as_secs_f64() / f64::from(shots.max(1))
    );
    println!("h2 histogram (chemistry order): {histogram:?}");
    let mut ranked: Vec<_> = histogram.into_iter().collect();
    ranked.sort_by(|(a, m), (b, n)| n.cmp(m).then(a.cmp(b)));
    for (chemistry, count) in ranked.iter().take(10) {
        let row: String = chemistry.chars().rev().collect();
        println!("h2 rank: chemistry {chemistry} row {row} count {count}");
    }
    let count = |k: usize| ranked.get(k).map_or(0, |(_, n)| *n);
    let mut top: Vec<&str> = ranked.iter().take(2).map(|(r, _)| r.as_str()).collect();
    top.sort_unstable();
    let peaks = top == H2_PEAKS && count(1) > count(2);
    println!(
        "h2 peaks {} and {} are the two most frequent: {}",
        H2_PEAKS[0],
        H2_PEAKS[1],
        if peaks { "yes" } else { "no" }
    );
    if shots >= H2_SHOTS_TO_ASSERT_PEAKS {
        assert!(
            peaks,
            "H₂'s two most frequent rows are not {H2_PEAKS:?}: {ranked:?}"
        );
    } else {
        println!("h2 {shots} shots are fewer than {H2_SHOTS_TO_ASSERT_PEAKS}: peaks not asserted");
    }
}
