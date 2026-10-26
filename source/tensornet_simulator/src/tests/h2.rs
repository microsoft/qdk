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
use crate::{GateFusion, MeasurementRun, MpsStack, ReadoutDistribution, TensorNetTerminal};

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

/// The exact distribution of H₂'s readout that `QDK_H2_READOUT_TABLE`
/// names by default: br-a's table from the sparse simulator (record order,
/// chemistry order, probability; data, never committed).
const H2_READOUT_TABLE: &str =
    "/home/domingom/Work/qdk-worktrees/scratch/m1-run/probes/h2-seed1-shot0.readout";

/// What the H₂ test adds to the bounds it derives from the retained weight:
/// the sparse simulator's own distance from ½ (4.3e-10 at most over H₂'s
/// mid-circuit measurements), the table's 17 digits and the sums' rounding,
/// all below 1e-9.
const H2_ROUNDING: f64 = 1e-9;

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

/// `QDK_H2_READOUT`: `values` (the default; the rows drawn) or
/// `distribution` (also the exact readout distribution,
/// [`ReadoutDistribution::Exact`]).
fn h2_readout() -> ReadoutDistribution {
    match env::var("QDK_H2_READOUT").as_deref() {
        Err(_) | Ok("values") => ReadoutDistribution::Off,
        Ok("distribution") => ReadoutDistribution::Exact,
        Ok(other) => panic!("QDK_H2_READOUT={other} should be values or distribution"),
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

/// The H₂ run's options, from the environment that [`h2_qpe`] documents.
struct H2Options {
    shots: u32,
    seed: u64,
    chi: u32,
    fusion: GateFusion,
    stop_after: Option<u64>,
    readout: ReadoutDistribution,
}

impl H2Options {
    fn from_env() -> Self {
        Self {
            shots: u32::try_from(env_or("QDK_H2_SHOTS", 1)).expect("the shots should fit u32"),
            seed: env_or("QDK_H2_SEED", 0),
            chi: u32::try_from(env_or("QDK_H2_CHI", 32)).expect("χ should fit u32"),
            fusion: h2_fusion(),
            stop_after: env::var("QDK_H2_STOP_AFTER")
                .is_ok()
                .then(|| env_or("QDK_H2_STOP_AFTER", 0)),
            readout: h2_readout(),
        }
    }
}

impl std::fmt::Display for H2Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "shots={} seed={} chi={} fusion={:?} stop_after={} readout={:?}",
            self.shots,
            self.seed,
            self.chi,
            self.fusion,
            self.stop_after
                .map_or_else(|| "none".to_string(), |m| m.to_string()),
            self.readout
        )
    }
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
/// - `QDK_H2_READOUT` (`values`): `distribution` reads the exact readout
///   distribution and every P(One) along the shot ([`ReadoutCheck`]).
/// - `QDK_H2_READOUT_TABLE` ([`H2_READOUT_TABLE`]): the exact readout
///   distribution to compare it with.
/// - `QDK_H2_TRACE` (unset): a file for the trace of each shot's
///   measurements (shot, index, qubit, P(One), value).
///
/// Per shot it prints its progress every [`H2_PROGRESS_EVERY`]
/// measurements ([`ShotProgress`]); its time, engine operators and gates,
/// the device memory with its state still held and after its session
/// closes; and its row in record order and as the chemistry user writes it
/// (qubit 4 first, [`H2_PEAKS`]). Then the histogram, the ten most frequent
/// rows, and whether [`H2_PEAKS`] are the two most frequent, which it
/// asserts from [`H2_SHOTS_TO_ASSERT_PEAKS`] shots. An engine error ends the
/// run with the shot, its progress and the error. With the readout
/// distribution, it also prints and asserts what [`ReadoutCheck`] says,
/// from one shot.
///
/// It asserts the peaks only if `assert_peaks`: `tests::fusion` runs it on an
/// engine that applies nothing, to check the driver on CPU before a VM
/// runs it.
pub(super) fn h2_qpe<E: MpsEngine>(stack: impl FnOnce(u32) -> MpsStack<E>, assert_peaks: bool) {
    let (path, program) = h2_program();
    let options = H2Options::from_env();
    let qubits = usize::try_from(program.program().num_qubits).expect("fits usize");
    println!(
        "h2 bytecode={path} qubits={qubits} results={} {options}",
        program.program().num_results
    );
    let per_shot = stack(options.chi)
        .with_gate_fusion(options.fusion)
        .with_readout_distribution(options.readout)
        .per_shot(qubits)
        .expect("a chain");
    let mut check = (options.readout == ReadoutDistribution::Exact).then(ReadoutCheck::new);
    println!("h2 device_mib before the first shot: {}", device_mib());

    let mut histogram = BTreeMap::<String, u32>::new();
    let mut total = Duration::ZERO;
    let mut stopped = 0;
    for shot in 1..=options.shots {
        let start = Instant::now();
        let session = open(&per_shot, options.seed, &ShotSet::one(shot))
            .unwrap_or_else(|e| panic!("h2 shot {shot}: the session does not open: {e}"));
        let mut adapter = RuntimeAdapter::new(ShotProgress::new(
            SubmissionWrapper::new(session, shot),
            shot,
            options.fusion,
            options.stop_after,
        ));
        let mut records = Vec::new();
        let result = adapter.run(&program, &mut records);
        let progress = adapter.into_inner();
        let stop = options.stop_after.filter(|_| progress.stopped);
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
        // The terminal's runs and retained weight, read before the session
        // closes and drops it.
        let runs = check
            .is_some()
            .then(|| ReadoutCheck::read(inner.inner().shot(shot).expect("its terminal"), shot));
        let report = inner.close().shots;
        drop(inner);
        assert_eq!(
            report.operators, operators,
            "h2 shot {shot}: the terminal applied another count of operators"
        );
        println!(
            "h2 shot {shot}: {measurements} measurements, {} gates, {operators} operators \
             ({:?}), time {:.1} s, {:.3} s per measurement",
            report.operations,
            options.fusion,
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
        if let (Some(check), Some((runs, weight))) = (&mut check, runs) {
            check.add(shot, &runs, weight, report.max_bond, &records, assert_peaks);
        }
        *histogram.entry(chemistry).or_default() += 1;
    }

    summarize(&options, total, stopped, histogram, check, assert_peaks);
}

/// Prints the time per shot, then, unless the shots stopped early, ranks
/// the rows ([`rank`]) and checks the readout distribution
/// ([`ReadoutCheck::finish`]).
fn summarize(
    options: &H2Options,
    total: Duration,
    stopped: u32,
    histogram: BTreeMap<String, u32>,
    check: Option<ReadoutCheck>,
    assert_peaks: bool,
) {
    println!(
        "h2 {} shots, seed {}, chi {}, fusion {:?}: {:.1} s per shot",
        options.shots,
        options.seed,
        options.chi,
        options.fusion,
        total.as_secs_f64() / f64::from(options.shots.max(1))
    );
    if stopped > 0 {
        println!("h2 {stopped} shots stopped (QDK_H2_STOP_AFTER): no histogram");
    } else {
        rank(histogram, options.shots, assert_peaks);
        if let Some(check) = check {
            check.finish(assert_peaks);
        }
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

/// The record row (qubit 0 first) of `row`, a row of a run of `qubits`,
/// whose first measurement is the most significant bit: H₂ records each
/// readout qubit's result in the order of the qubits.
fn record_row(qubits: &[usize], row: usize) -> usize {
    let mut order: Vec<usize> = (0..qubits.len()).collect();
    order.sort_by_key(|&i| qubits[i]);
    let k = qubits.len();
    order
        .iter()
        .fold(0, |record, &i| (record << 1) | (row >> (k - 1 - i) & 1))
}

/// A row of `k` bits as a string, the most significant first.
fn bits(row: usize, k: usize) -> String {
    (0..k)
        .map(|i| {
            if row >> (k - 1 - i) & 1 == 1 {
                '1'
            } else {
                '0'
            }
        })
        .collect()
}

/// `QDK_H2_READOUT_TABLE`: the exact readout distribution, by record row
/// (qubit 0 the most significant bit), or `None` if the file cannot be
/// read.
fn readout_table() -> (String, Option<Vec<f64>>) {
    let path = env::var("QDK_H2_READOUT_TABLE").unwrap_or_else(|_| H2_READOUT_TABLE.to_string());
    let Ok(text) = fs::read_to_string(&path) else {
        return (path, None);
    };
    let mut table = Vec::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [record, _chemistry, probability] = fields.as_slice() else {
            panic!("{path}: `{line}` should be: record row, chemistry row, probability");
        };
        let row = usize::from_str_radix(record, 2)
            .unwrap_or_else(|e| panic!("{path}: `{record}` is not a row: {e}"));
        let probability: f64 = probability
            .parse()
            .unwrap_or_else(|e| panic!("{path}: `{probability}` is not a probability: {e}"));
        if table.len() <= row {
            table.resize(row + 1, f64::NAN);
        }
        table[row] = probability;
    }
    assert!(
        table.iter().all(|p| p.is_finite()),
        "{path}: every row should have a probability"
    );
    (path, Some(table))
}

/// What H₂'s driver checks with the readout distribution
/// (`QDK_H2_READOUT=distribution`), the acceptance M1 agreed (Oct 26):
///
/// - the exact readout distribution: the last run of each shot, the five
///   readout measurements, put in record order. Its mean over the shots is
///   printed next to the table (`QDK_H2_READOUT_TABLE`) with the total
///   variation distance (TVD) and the rows that differ most. Asserted: the
///   two largest rows are [`H2_PEAKS`], and the TVD is at most
///   √(1 − w) + [`H2_ROUNDING`], w the smallest retained weight of the
///   shots;
/// - every P(One) along the shot (the trace, to `QDK_H2_TRACE`). Asserted
///   per shot: every mid-circuit measurement's |P(One) − ½| is at most
///   √(1 − w) + [`H2_ROUNDING`] (H₂'s are ½: the sparse simulator reads
///   them within 4.3e-10).
///
/// Why √(1 − w): a truncation that keeps the weight w of a normalized state
/// moves it, renormalized, by a trace distance of √(1 − w), which bounds the
/// TVD of any measurement's distribution. Over many truncations between
/// gates and projections w is the product of the weights each kept; that
/// the bound still holds is the usual estimate of an MPS's fidelity, not a
/// theorem (DECISIONS). Each P(One) is read when the weight is at least the
/// final one, so the final one bounds them all.
struct ReadoutCheck {
    shots: u32,
    mean: Vec<f64>,
    weight: f64,
    max_bond: usize,
    trace: Option<(String, fs::File)>,
}

impl ReadoutCheck {
    fn new() -> Self {
        let trace = env::var("QDK_H2_TRACE").ok().map(|path| {
            let mut file =
                fs::File::create(&path).unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
            writeln!(
                file,
                "# H₂'s measurements, per shot: shot, index, qubit, P(One) given the shot's \
                 history, value"
            )
            .expect("writes");
            (path, file)
        });
        Self {
            shots: 0,
            mean: Vec::new(),
            weight: 1.0,
            max_bond: 0,
            trace,
        }
    }

    /// The runs of shot `shot`'s `terminal` and its retained weight, read
    /// before the session closes and drops it.
    fn read<E: MpsEngine>(
        terminal: &TensorNetTerminal<E>,
        shot: u32,
    ) -> (Vec<MeasurementRun>, f64) {
        let state = terminal.state().expect("the shot's state");
        let weight = terminal
            .engine()
            .retained_weight(state)
            .unwrap_or_else(|e| panic!("h2 shot {shot}: the retained weight: {e}"));
        (terminal.measurement_runs().to_vec(), weight)
    }

    /// Adds shot `shot`: its `runs`, the retained `weight` and largest bond
    /// at its end, and its `records`.
    fn add(
        &mut self,
        shot: u32,
        runs: &[MeasurementRun],
        weight: f64,
        max_bond: usize,
        records: &[OutputRecord],
        assert_trace: bool,
    ) {
        let (readout, mid_circuit) = runs.split_last().expect("H₂ measures");
        let k = readout.len();
        assert_eq!(
            k,
            records.len(),
            "h2 shot {shot}: the last run should be the readout, one measurement per record; \
             it measures {:?}",
            readout.qubits()
        );
        let mut distribution = vec![0.0; 1 << k];
        for (row, &p) in readout.distribution().iter().enumerate() {
            distribution[record_row(readout.qubits(), row)] += p;
        }
        let drawn = usize::from_str_radix(&row(records), 2).expect("a row of bits");
        assert_eq!(
            drawn,
            record_row(readout.qubits(), readout.row()),
            "h2 shot {shot}: the records are not the readout's values in the order of its qubits \
             {:?}",
            readout.qubits()
        );

        let mut index = 0;
        let mut farthest = (0.0, 0, 0);
        for run in mid_circuit {
            for (&qubit, &p) in run.qubits().iter().zip(run.probabilities_of_one()) {
                let distance = (p - 0.5).abs();
                if distance > farthest.0 {
                    farthest = (distance, index, qubit);
                }
                index += 1;
            }
        }
        if let Some((path, file)) = &mut self.trace {
            let mut index = 0;
            for run in runs {
                for ((&qubit, &p), &one) in run
                    .qubits()
                    .iter()
                    .zip(run.probabilities_of_one())
                    .zip(run.values())
                {
                    writeln!(file, "{shot} {index} {qubit} {p:.17e} {}", u8::from(one))
                        .unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
                    index += 1;
                }
            }
        }
        let tolerance = (1.0 - weight).max(0.0).sqrt() + H2_ROUNDING;
        let lengths: BTreeMap<usize, usize> = runs.iter().fold(BTreeMap::new(), |mut m, run| {
            *m.entry(run.len()).or_default() += 1;
            m
        });
        println!(
            "h2 shot {shot}: readout qubits {:?} tags {:?}; runs by length {lengths:?}; \
             retained weight {weight:.12}, largest bond {max_bond}; {index} mid-circuit \
             measurements, max |P(One) - 1/2| {:.3e} (measurement {}, qubit {}), tolerance \
             {tolerance:.3e}",
            readout.qubits(),
            readout.tags(),
            farthest.0,
            farthest.1,
            farthest.2
        );
        if assert_trace {
            assert!(
                farthest.0 <= tolerance,
                "h2 shot {shot}: measurement {} on qubit {} reads P(One) {:.3e} from 1/2, more \
                 than √(1 − w) + {H2_ROUNDING:e} = {tolerance:.3e} at the retained weight w = \
                 {weight}",
                farthest.1,
                farthest.2,
                farthest.0
            );
        }
        if self.mean.is_empty() {
            self.mean = vec![0.0; 1 << k];
        }
        assert_eq!(
            self.mean.len(),
            distribution.len(),
            "h2 shot {shot}: another readout"
        );
        // A running mean over the shots.
        self.shots += 1;
        for (m, p) in self.mean.iter_mut().zip(&distribution) {
            *m += (p - *m) / f64::from(self.shots);
        }
        self.weight = self.weight.min(weight);
        self.max_bond = self.max_bond.max(max_bond);
    }

    /// Prints the mean readout distribution next to the table, and asserts
    /// the peaks and the TVD if `assert_peaks`.
    fn finish(self, assert_peaks: bool) {
        if let Some((path, mut file)) = self.trace {
            file.flush()
                .unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
            println!("h2 trace written to {path}");
        }
        let k = self.mean.len().trailing_zeros() as usize;
        let (path, table) = readout_table();
        println!(
            "h2 readout distribution over {} shots (record order, chemistry order, P): retained \
             weight {:.12} (smallest), largest bond {}",
            self.shots, self.weight, self.max_bond
        );
        for (row, p) in self.mean.iter().enumerate() {
            let record = bits(row, k);
            let chemistry: String = record.chars().rev().collect();
            let exact = table
                .as_ref()
                .and_then(|t| t.get(row))
                .map_or_else(String::new, |e| {
                    format!(" exact {e:.17} diff {:+.3e}", p - e)
                });
            println!("h2 readout {record} {chemistry} {p:.17}{exact}");
        }
        let mut ranked: Vec<(usize, f64)> = self.mean.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut top: Vec<String> = ranked[..2]
            .iter()
            .map(|&(row, _)| bits(row, k).chars().rev().collect())
            .collect();
        top.sort_unstable();
        let peaks = top == H2_PEAKS;
        println!(
            "h2 readout: the two largest rows (chemistry order) are {top:?}; peaks {}",
            if peaks { "yes" } else { "no" }
        );
        let bound = (1.0 - self.weight).max(0.0).sqrt() + H2_ROUNDING;
        let tvd = table.as_ref().map(|table| {
            assert_eq!(
                table.len(),
                self.mean.len(),
                "{path}: another number of rows than the readout's"
            );
            let mut differences: Vec<(usize, f64)> = self
                .mean
                .iter()
                .zip(table)
                .map(|(p, e)| p - e)
                .enumerate()
                .collect();
            differences.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
            let tvd = differences.iter().map(|(_, d)| d.abs()).sum::<f64>() / 2.0;
            println!(
                "h2 readout against {path}: TVD {tvd:.3e}, max |ΔP| {:.3e}, bound √(1 − w) + \
                 {H2_ROUNDING:e} = {bound:.3e}",
                differences[0].1.abs()
            );
            for &(row, d) in differences.iter().take(5) {
                let record = bits(row, k);
                let chemistry: String = record.chars().rev().collect();
                println!("h2 readout differs most: {record} {chemistry} ΔP {d:+.3e}");
            }
            tvd
        });
        if !assert_peaks {
            println!("h2 readout distribution not asserted on this engine");
            return;
        }
        assert!(
            peaks,
            "H₂'s two largest readout rows are not {H2_PEAKS:?}: {top:?}"
        );
        let tvd = tvd.unwrap_or_else(|| {
            panic!("cannot read the exact readout distribution {path} (QDK_H2_READOUT_TABLE)")
        });
        assert!(
            tvd <= bound,
            "H₂'s readout distribution is {tvd:.3e} from {path} in TVD, more than √(1 − w) + \
             {H2_ROUNDING:e} = {bound:.3e} at the retained weight w = {}",
            self.weight
        );
        println!("h2 readout distribution accepted: TVD {tvd:.3e} <= {bound:.3e}");
    }
}
