// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! H₂ QPE through the walk, the driver of M1's acceptance test
//! (`qualification::h2_qpe_on_the_a100`). It does not depend on the
//! engine, so the CPU can check it (`tests::fusion::h2_driver_on_a_coin_engine`).

use std::{
    collections::BTreeMap,
    env, fs,
    io::Write as _,
    time::{Duration, Instant},
};

use executor_protocol::{Batch, Close as _, Executor, Rejected, ShotSet, SubmissionWrapper, open};
use qdk_simulators::{
    OutputRecord,
    execution::{PreparedAdaptiveProgram, RegionInstruction, RuntimeAdapter},
};
use tensornet::MpsEngine;

use super::{adaptive_program_json::adaptive_program_from_json, cases::bit};
use crate::{GateFusion, MpsStack};

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

/// How often, in measurements (1,404 per H₂ shot), a shot prints its
/// progress, its engine operators and the device memory.
const H2_PROGRESS_EVERY: u64 = 10;

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

/// The walk's terminal for one H₂ shot, which prints the shot's progress,
/// its engine operators and the device memory every [`H2_PROGRESS_EVERY`]
/// measurements, so that a run that runs out of device memory shows how it
/// got there, and the time per measurement shows what fusion saves. With
/// `stop_after`, it refuses the batch after that many measurements, which
/// ends the shot, and records that it did (`stopped`), so that the driver
/// tells this refusal from an engine error.
///
/// It counts the operators by fusing each batch's regions as the terminal
/// does ([`GateFusion::operators`]), on the CPU; the test checks the count
/// against the terminal's report when the shot closes.
struct ShotProgress<E> {
    inner: E,
    shot: u32,
    fusion: GateFusion,
    stop_after: Option<u64>,
    stopped: bool,
    measurements: u64,
    operators: u64,
    start: Instant,
    last: Duration,
}

impl<E> ShotProgress<E> {
    fn new(inner: E, shot: u32, fusion: GateFusion, stop_after: Option<u64>) -> Self {
        Self {
            inner,
            shot,
            fusion,
            stop_after,
            stopped: false,
            measurements: 0,
            operators: 0,
            start: Instant::now(),
            last: Duration::ZERO,
        }
    }

    fn print(&mut self) {
        let now = self.start.elapsed();
        println!(
            "h2 shot {}: {} measurements, {:.1} s (+{:.1} s), operators {}, device_mib {}",
            self.shot,
            self.measurements,
            now.as_secs_f64(),
            now.saturating_sub(self.last).as_secs_f64(),
            self.operators,
            device_mib()
        );
        std::io::stdout().flush().ok();
        self.last = now;
    }
}

impl<E: Executor<Batch<RegionInstruction>>> Executor<Batch<RegionInstruction>> for ShotProgress<E> {
    type Response = E::Response;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        if self.stop_after.is_some_and(|m| self.measurements >= m) {
            self.stopped = true;
            return Err(Rejected::Unavailable(format!(
                "stopped after {} measurements (QDK_H2_STOP_AFTER)",
                self.measurements
            )));
        }
        let before = self.measurements;
        for instruction in batch {
            match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => {
                    self.operators += self.fusion.operators(region.operations()).len() as u64;
                }
                RegionInstruction::Measure(_) => self.measurements += 1,
                RegionInstruction::Reset { .. } => {}
            }
        }
        let response = self.inner.call(batch);
        if self.measurements / H2_PROGRESS_EVERY > before / H2_PROGRESS_EVERY {
            self.print();
        }
        response
    }
}

/// `QDK_H2_FUSION`: `pairs` (the default) or `unfused`.
fn h2_fusion() -> GateFusion {
    match env::var("QDK_H2_FUSION").as_deref() {
        Ok("unfused") => GateFusion::Unfused,
        Err(_) | Ok("pairs") => GateFusion::Pairs,
        Ok("3") => panic!(
            "QDK_H2_FUSION=3: blocks of three sites are counted on CPU only \
             (tests::fusion::h2_operators_per_shot); the cuTensorNet engine \
             applies operators on one or two sites"
        ),
        Ok(other) => panic!("QDK_H2_FUSION={other} should be unfused or pairs"),
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    env::var(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name}={value} should be a number"))
    })
}

/// `QDK_H2_BYTECODE`, and H₂'s program read from it.
fn h2_program() -> (String, PreparedAdaptiveProgram<u64>) {
    let path = env::var("QDK_H2_BYTECODE").unwrap_or_else(|_| {
        panic!(
            "QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict() \
             (qualify-simulator-on-a100.sh --h2 FILE sets it)"
        )
    });
    let json = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = PreparedAdaptiveProgram::new(adaptive_program_from_json(&json))
        .unwrap_or_else(|e| panic!("H₂ should prepare: {e}"));
    (path, program)
}

/// A row in record order, qubit 0 first.
fn row(records: &[OutputRecord]) -> String {
    records
        .iter()
        .map(|record| if bit(record) { '1' } else { '0' })
        .collect()
}

/// H₂ QPE through the walk over `MpsStack::per_shot` on the stack
/// `stack(χ)` makes (cuTensorNet in M1's acceptance test,
/// `qualification::h2_qpe_on_the_a100`). Shot `j` (from 1) runs in its own
/// session, opened on shot `j` with the run's seed, as `qdk_simulators`'s
/// `h2_qpe` runs it on CPU. The environment:
///
/// - `QDK_H2_BYTECODE` (required): the JSON of H₂'s
///   `AdaptiveProgram.as_dict()` (see `qdk_simulators`'s `h2_qpe`).
/// - `QDK_H2_SHOTS` (1), `QDK_H2_SEED` (0), `QDK_H2_CHI` (32): the shots,
///   the run's seed and χ.
/// - `QDK_H2_FUSION` (`pairs`): the gate fusion, `pairs` or `unfused`.
/// - `QDK_H2_STOP_AFTER` (unset): stop each shot after this many
///   measurements, to time the first part of a shot; such a shot has no
///   row, and the run asserts nothing about the peaks.
///
/// Per shot it prints its progress every [`H2_PROGRESS_EVERY`]
/// measurements ([`ShotProgress`]); its time, engine operators and gates,
/// the device memory with its state still held and after its session
/// closes; and its row in record order and as the chemistry user writes it
/// (qubit 4 first, [`H2_PEAKS`]). Then the histogram, the ten most frequent
/// rows, and whether [`H2_PEAKS`] are the two most frequent, which it
/// asserts from [`H2_SHOTS_TO_ASSERT_PEAKS`] shots. An engine error ends the
/// run with the shot, its progress and the error.
///
/// It asserts the peaks only if `assert_peaks`: `tests::fusion` runs it on an
/// engine that applies nothing, to check the driver on CPU before a VM
/// runs it.
pub(super) fn h2_qpe<E: MpsEngine>(stack: impl FnOnce(u32) -> MpsStack<E>, assert_peaks: bool) {
    let (path, program) = h2_program();
    let shots = u32::try_from(env_or("QDK_H2_SHOTS", 1)).expect("the shots should fit u32");
    let seed = env_or("QDK_H2_SEED", 0);
    let chi = u32::try_from(env_or("QDK_H2_CHI", 32)).expect("χ should fit u32");
    let fusion = h2_fusion();
    let stop_after = env::var("QDK_H2_STOP_AFTER")
        .is_ok()
        .then(|| env_or("QDK_H2_STOP_AFTER", 0));
    let qubits = usize::try_from(program.program().num_qubits).expect("fits usize");
    println!(
        "h2 bytecode={path} qubits={qubits} results={} shots={shots} seed={seed} chi={chi} \
         fusion={fusion:?} stop_after={}",
        program.program().num_results,
        stop_after.map_or_else(|| "none".to_string(), |m| m.to_string())
    );
    let per_shot = stack(chi)
        .with_gate_fusion(fusion)
        .per_shot(qubits)
        .expect("a chain");
    println!("h2 device_mib before the first shot: {}", device_mib());

    let mut histogram = BTreeMap::<String, u32>::new();
    let mut total = Duration::ZERO;
    let mut stopped = 0;
    for shot in 1..=shots {
        let start = Instant::now();
        let session = open(&per_shot, seed, &ShotSet::one(shot))
            .unwrap_or_else(|e| panic!("h2 shot {shot}: the session does not open: {e}"));
        let mut adapter = RuntimeAdapter::new(ShotProgress::new(
            SubmissionWrapper::new(session, shot),
            shot,
            fusion,
            stop_after,
        ));
        let mut records = Vec::new();
        let result = adapter.run(&program, &mut records);
        let progress = adapter.into_inner();
        let stop = stop_after.filter(|_| progress.stopped);
        if let (Err(error), None) = (&result, stop) {
            panic!(
                "h2 shot {shot} failed after {} measurements, {:.1} s, device_mib {}: {error}",
                progress.measurements,
                start.elapsed().as_secs_f64(),
                device_mib()
            );
        }
        let time = start.elapsed();
        total += time;
        let held = device_mib();
        let ShotProgress {
            mut inner,
            measurements,
            operators,
            ..
        } = progress;
        let report = inner.close().shots;
        drop(inner);
        assert_eq!(
            report.operators, operators,
            "h2 shot {shot}: the terminal applied another count of operators"
        );
        println!(
            "h2 shot {shot}: {measurements} measurements, {} gates, {operators} operators \
             ({fusion:?}), time {:.1} s, {:.3} s per measurement",
            report.operations,
            time.as_secs_f64(),
            time.as_secs_f64() / f64::from(u32::try_from(measurements.max(1)).expect("fits u32"))
        );
        if let Some(m) = stop {
            println!(
                "h2 shot {shot}: stopped after {m} measurements (QDK_H2_STOP_AFTER), \
                 device_mib {held} after_close {}",
                device_mib()
            );
            std::io::stdout().flush().ok();
            stopped += 1;
            continue;
        }
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
        "h2 {shots} shots, seed {seed}, chi {chi}, fusion {fusion:?}: {:.1} s per shot",
        total.as_secs_f64() / f64::from(shots.max(1))
    );
    if stopped > 0 {
        println!("h2 {stopped} shots stopped (QDK_H2_STOP_AFTER): no histogram");
    } else {
        rank(histogram, shots, assert_peaks);
    }
}

/// Prints the histogram of the rows (in chemistry order), the ten most
/// frequent, and whether [`H2_PEAKS`] are the two most frequent, which it
/// asserts if `assert_peaks` and there are [`H2_SHOTS_TO_ASSERT_PEAKS`]
/// shots or more.
fn rank(histogram: BTreeMap<String, u32>, shots: u32, assert_peaks: bool) {
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
    if !assert_peaks {
        println!("h2 peaks not asserted on this engine");
    } else if shots >= H2_SHOTS_TO_ASSERT_PEAKS {
        assert!(
            peaks,
            "H₂'s two most frequent rows are not {H2_PEAKS:?}: {ranked:?}"
        );
    } else {
        println!("h2 {shots} shots are fewer than {H2_SHOTS_TO_ASSERT_PEAKS}: peaks not asserted");
    }
}
