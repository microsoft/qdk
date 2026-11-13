// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): Stage 0 of the GPU speed plan (§2.2).
//! It measures what one MPS update costs when it is made of cuTensorNet's
//! tensor calls or cuSOLVER's batched SVDs, before anything is built on
//! them. For each bond `b` it times:
//!
//! - the floor: waiting for an idle stream, and the smallest launch (a
//!   4-byte `cudaMemsetAsync`) with its wait;
//! - `cutensornetGateSplit` of a two-qubit gate on two `b × 2 × b`
//!   tensors, with the read of the reduced extent (stage 1's update), once
//!   for each pair of an SVD algorithm (`--svd-algos`: `gesvd`,
//!   cuTensorNet's default, `gesvdj`, `gesvdp`, `gesvdr`) and a gate-split
//!   algorithm (`--gate-split-algos`: `reduced`, `direct`). Every SVD
//!   algorithm runs with the library's default parameters. All but GESVDR
//!   keep at most `b` values under the engine's discarded-weight cutoff
//!   (`truncation=discarded-weight`); GESVDR accepts no such cutoff, so it
//!   keeps exactly the CPU split's count, set as the shared mode's extent
//!   (`truncation=fixed-extent`), and reports no discarded weight;
//! - `cutensornetTensorQR` of one `b × 2 × b` tensor: moving the centre;
//! - cuSOLVER's `gesvdjBatched` (`2b ≤ 32`) and `gesvdaStridedBatched`
//!   (the `b` largest of `2b × 2b`) for `K` matrices in one launch.
//!
//! The tensors are random, with left and right bonds whose weights decay
//! as an MPS's Schmidt values do. Each call's answer is checked on the
//! host before it is timed: singular values against faer's within 1e-12,
//! the kept weight and the reduced extent against the CPU split's (the
//! same truncation rule on faer's values), `Q·R` against the input and
//! `Q`'s orthonormality. A call that disagrees prints its deviation and no
//! time, with one exception: a GESVDJ split whose singular values and kept
//! weight are within 1e-10 (and its extent right) is also timed, its line
//! marked `timed_despite_disagreement`, so that the cost of Jacobi's
//! looser convergence is known.
//!
//! Per bond, a `BEST` line names the fastest gate split that agreed, and
//! its median over GESVD with `reduced` (`vs_gesvd`, below 1 is faster).
//! The `DECISION` line, PLAN §2.2's rule at `b = 24`, reads GESVD with
//! `reduced` only, as before.
//!
//! ```text
//! cargo run --release -p qdk_cutensornet --example gate_split_bench -- \
//!     [--engine cutensornet|cpu] [--bonds 8,16,24] [--batches 1,4,16] \
//!     [--svd-algos gesvd,gesvdj,gesvdp,gesvdr] \
//!     [--gate-split-algos reduced,direct] \
//!     [--cusolver /path/to/libcusolver.so.11]
//! ```
//!
//! The defaults: every SVD algorithm, `reduced` only. cuSOLVER's path may
//! also come from `QDK_CUSOLVER_LIBRARY`; without one its lines say
//! `skipped`. `--engine cpu` builds the problems and runs the CPU
//! reference only, which checks the bench anywhere; it ignores the
//! algorithm flags.

// The device parts exist on Linux x86-64 only; elsewhere the CPU
// reference runs and their helpers are unused.
#![cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]

use num_complex::Complex64;
use std::time::Instant;
use t4a_faer::{Mat, Par};

/// The agreement of singular values, kept weights and factors.
const TOLERANCE: f64 = 1e-12;

/// The agreement within which a GESVDJ split that misses [`TOLERANCE`] is
/// still timed.
const GESVDJ_TIMING_TOLERANCE: f64 = 1e-10;

/// The engine's discarded-weight cutoff (`policy.rs`).
const DISCARDED_WEIGHT_CUTOFF: f64 = 1e-14;

/// The bond at which PLAN §2.2 decides stage 1.
const DECIDING_BOND: usize = 24;

/// The bytes of device memory one batched SVD may take.
const BATCH_MEMORY_LIMIT: usize = 4 << 30;

/// A launch longer than this ends a method's larger batches at that bond.
const BATCH_TIME_LIMIT_SECONDS: f64 = 2.0;

struct Options {
    on_gpu: bool,
    bonds: Vec<usize>,
    batches: Vec<usize>,
    /// The SVD algorithms' names, checked when the GPU run starts.
    svd_algos: Vec<String>,
    /// The gate-split algorithms' names, checked likewise.
    gate_split_algos: Vec<String>,
    cusolver: Option<std::path::PathBuf>,
}

fn main() {
    let options = parse_options();
    let started = Instant::now();
    let succeeded = if options.on_gpu {
        run_on_gpu(&options)
    } else {
        run_on_cpu(&options)
    };
    println!(
        "STAGE0 done engine={} seconds={:.1}",
        if options.on_gpu { "cutensornet" } else { "cpu" },
        started.elapsed().as_secs_f64()
    );
    if !succeeded {
        std::process::exit(1);
    }
}

fn parse_options() -> Options {
    let mut options = Options {
        on_gpu: true,
        bonds: vec![8, 16, 24, 32, 48, 64, 128],
        batches: vec![1, 4, 16, 64, 256, 1024],
        svd_algos: names("gesvd,gesvdj,gesvdp,gesvdr"),
        gate_split_algos: names("reduced"),
        cusolver: std::env::var_os("QDK_CUSOLVER_LIBRARY").map(Into::into),
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .unwrap_or_else(|| panic!("{flag} takes a value"));
        match flag.as_str() {
            "--engine" => options.on_gpu = value != "cpu",
            "--bonds" => options.bonds = list(&value),
            "--batches" => options.batches = list(&value),
            "--svd-algos" => options.svd_algos = names(&value),
            "--gate-split-algos" => options.gate_split_algos = names(&value),
            "--cusolver" => options.cusolver = Some(value.into()),
            _ => panic!("unknown flag {flag}"),
        }
    }
    options
}

fn list(value: &str) -> Vec<usize> {
    value
        .split(',')
        .map(|item| item.parse().expect("a list of integers"))
        .collect()
}

fn names(value: &str) -> Vec<String> {
    value.split(',').map(str::to_owned).collect()
}

/// A gate applied to the bond between two MPS tensors, with the matrix it
/// splits.
struct Problem {
    bond: usize,
    /// `bond × 2 × bond`, modes `(i, p, j)`, `i` fastest.
    left: Vec<Complex64>,
    /// `bond × 2 × bond`, modes `(j, q, k)`.
    right: Vec<Complex64>,
    /// `2 × 2 × 2 × 2`, modes `(P, Q, p, q)`.
    gate: Vec<Complex64>,
    /// `θ`, `2·bond × 2·bond`, column-major, row `i + bond·P`, column
    /// `Q + 2·k`: the contraction the split factors, of norm 1.
    theta: Vec<Complex64>,
}

impl Problem {
    fn new(bond: usize, random: &mut Random) -> Self {
        let decay = |index: usize| 10_f64.powf(-3.0 * ratio(index, bond));
        let mut left: Vec<Complex64> = (0..2 * bond * bond)
            .map(|index| random.normal() * decay(index % bond))
            .collect();
        let right: Vec<Complex64> = (0..2 * bond * bond)
            .map(|index| random.normal() * decay(index / (2 * bond)))
            .collect();
        let gate: Vec<Complex64> = (0..16).map(|_| random.normal()).collect();
        let mut theta = contract(bond, &left, &right, &gate);
        let norm = theta.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
        for value in left.iter_mut().chain(theta.iter_mut()) {
            *value /= norm;
        }
        Self {
            bond,
            left,
            right,
            gate,
            theta,
        }
    }
}

/// `θ[i + b·P, Q + 2·k] = Σ G[P, Q, p, q]·A[i, p, j]·B[j, q, k]`.
fn contract(
    bond: usize,
    left: &[Complex64],
    right: &[Complex64],
    gate: &[Complex64],
) -> Vec<Complex64> {
    let b = bond;
    // pair[i, p, q, k] = Σ_j A[i, p, j]·B[j, q, k], `i` fastest.
    let mut pair = vec![Complex64::default(); 4 * b * b];
    for k in 0..b {
        for q in 0..2 {
            for j in 0..b {
                let factor = right[j + b * (q + 2 * k)];
                for p in 0..2 {
                    let column = &left[b * (p + 2 * j)..b * (p + 2 * j) + b];
                    let target = b * (p + 2 * (q + 2 * k));
                    for (i, value) in column.iter().enumerate() {
                        pair[target + i] += value * factor;
                    }
                }
            }
        }
    }
    let rows = 2 * b;
    let mut theta = vec![Complex64::default(); rows * rows];
    for k in 0..b {
        for big_q in 0..2 {
            for big_p in 0..2 {
                for p in 0..2 {
                    for q in 0..2 {
                        let g = gate[big_p + 2 * (big_q + 2 * (p + 2 * q))];
                        let source = b * (p + 2 * (q + 2 * k));
                        let target = b * big_p + rows * (big_q + 2 * k);
                        for i in 0..b {
                            theta[target + i] += g * pair[source + i];
                        }
                    }
                }
            }
        }
    }
    theta
}

/// faer's singular values of the column-major `rows × columns` `matrix`,
/// largest first.
fn reference_singular_values(rows: usize, columns: usize, matrix: &[Complex64]) -> Vec<f64> {
    let matrix = Mat::from_fn(rows, columns, |row, column| matrix[row + rows * column]);
    let mut values = matrix
        .singular_values(Par::Seq)
        .expect("faer's SVD converges on these matrices");
    values.sort_by(|a, b| b.total_cmp(a));
    values
}

/// The CPU split's truncation: the fewest values, at most `bond`, whose
/// discarded weight is within the cutoff, and their weight.
fn cpu_split(bond: usize, values: &[f64]) -> (usize, f64) {
    let total: f64 = values.iter().map(|value| value * value).sum();
    let mut kept = values.len();
    let mut discarded = 0.0;
    while kept > 0 {
        let next = discarded + values[kept - 1] * values[kept - 1];
        if next / total > DISCARDED_WEIGHT_CUTOFF {
            break;
        }
        discarded = next;
        kept -= 1;
    }
    let kept = kept.min(bond);
    let weight = values[..kept]
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        / total;
    (kept, weight)
}

/// Random graded matrices for the batched SVDs: Gaussian entries whose
/// rows and columns decay as `Problem`'s bonds do.
fn graded_matrices(size: usize, batch: usize, random: &mut Random) -> Vec<Complex64> {
    let decay = |index: usize| 10_f64.powf(-3.0 * ratio(index, size));
    (0..size * size * batch)
        .map(|index| {
            let row = index % size;
            let column = (index / size) % size;
            random.normal() * decay(row) * decay(column)
        })
        .collect()
}

/// The largest deviation between two lists of values, entry by entry.
fn deviation(values: &[f64], reference: &[f64]) -> f64 {
    values
        .iter()
        .zip(reference)
        .map(|(value, expected)| (value - expected).abs())
        .fold(0.0, f64::max)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    let float = |value: usize| f64::from(u32::try_from(value).expect("a small count"));
    float(numerator) / float(denominator)
}

/// Repeated timings of one call, in milliseconds.
struct Timing {
    repetitions: usize,
    median: f64,
    minimum: f64,
}

impl Timing {
    /// Runs `call` once to warm up, then at least 5 times and until 0.3 s
    /// have passed (at most 2000 times).
    fn of(mut call: impl FnMut() -> Result<(), String>) -> Result<Self, String> {
        call()?;
        let mut samples = Vec::new();
        let started = Instant::now();
        while samples.len() < 5 || (started.elapsed().as_secs_f64() < 0.3 && samples.len() < 2000) {
            let start = Instant::now();
            call()?;
            samples.push(start.elapsed().as_secs_f64() * 1e3);
        }
        samples.sort_by(f64::total_cmp);
        Ok(Self {
            repetitions: samples.len(),
            median: samples[samples.len() / 2],
            minimum: samples[0],
        })
    }
}

impl std::fmt::Display for Timing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ms_median={:.4} ms_min={:.4} reps={}",
            self.median, self.minimum, self.repetitions
        )
    }
}

/// A seeded generator (`SplitMix64`) of standard complex normals.
struct Random(u64);

impl Random {
    fn next_unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // The top 53 bits, as a value in (0, 1].
        (f64::from(u32::try_from(z >> 32).expect("32 bits")) * 2_f64.powi(-32)
            + f64::from(u32::try_from(z & 0xFFFF_FFFF).expect("32 bits")) * 2_f64.powi(-64))
        .max(f64::MIN_POSITIVE)
    }

    fn normal(&mut self) -> Complex64 {
        let radius = (-2.0 * self.next_unit().ln()).sqrt();
        let angle = 2.0 * std::f64::consts::PI * self.next_unit();
        Complex64::from_polar(radius, angle) * std::f64::consts::FRAC_1_SQRT_2
    }
}

fn run_on_cpu(options: &Options) -> bool {
    let mut random = Random(1);
    for &bond in &options.bonds {
        let started = Instant::now();
        let problem = Problem::new(bond, &mut random);
        let built = started.elapsed().as_secs_f64() * 1e3;
        let started = Instant::now();
        let values = reference_singular_values(2 * bond, 2 * bond, &problem.theta);
        let svd = started.elapsed().as_secs_f64() * 1e3;
        let (kept, weight) = cpu_split(bond, &values);
        println!(
            "CPU b={bond} full={} kept={kept} kept_weight={weight:.15} s_max={:.6} build_ms={built:.2} svd_ms={svd:.2}",
            values.len(),
            values[0],
        );
    }
    true
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_gpu(_options: &Options) -> bool {
    println!("GPU status=refused reason=cuTensorNet runs on Linux x86-64 only");
    false
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_gpu(options: &Options) -> bool {
    use qdk_cutensornet::dev_gate_split::{BenchDevice, Cusolver, GateSplitAlgo, TensorSvdAlgo};

    let svd_algos: Vec<TensorSvdAlgo> = options
        .svd_algos
        .iter()
        .map(|name| {
            TensorSvdAlgo::from_name(name)
                .unwrap_or_else(|| panic!("--svd-algos: unknown algorithm {name}"))
        })
        .collect();
    let split_algos: Vec<GateSplitAlgo> = options
        .gate_split_algos
        .iter()
        .map(|name| {
            GateSplitAlgo::from_name(name)
                .unwrap_or_else(|| panic!("--gate-split-algos: unknown algorithm {name}"))
        })
        .collect();
    let availability = match qdk_cutensornet::discover() {
        Ok(availability) => availability,
        Err(error) => {
            println!("GPU status=refused reason={error}");
            return false;
        }
    };
    let device = match BenchDevice::open(&availability) {
        Ok(device) => device,
        Err(error) => {
            println!("GPU status=refused reason={error}");
            return false;
        }
    };
    report_floor(&device);
    let mut random = Random(1);
    let mut deciding = None;
    for &bond in &options.bonds {
        let problem = Problem::new(bond, &mut random);
        let reference = reference_singular_values(2 * bond, 2 * bond, &problem.theta);
        let mut agreed = Vec::new();
        for &svd in &svd_algos {
            for &split in &split_algos {
                if let Some(median) = report_gate_split(&device, &problem, &reference, svd, split) {
                    agreed.push((svd, split, median));
                }
            }
        }
        let baseline = agreed
            .iter()
            .find(|&&(svd, split, _)| {
                svd == TensorSvdAlgo::Gesvd && split == GateSplitAlgo::Reduced
            })
            .map(|&(_, _, median)| median);
        if bond == DECIDING_BOND {
            deciding = baseline;
        }
        report_best(bond, &agreed, baseline);
        report_qr(&device, &problem);
    }
    match &options.cusolver {
        None => println!("CUSOLVER status=skipped reason=no --cusolver or QDK_CUSOLVER_LIBRARY"),
        Some(path) => match Cusolver::open(path.clone(), &device) {
            Err(error) => println!("CUSOLVER status=refused reason={error}"),
            Ok(solver) => report_batched(&device, &solver, options, &mut random),
        },
    }
    report_decision(deciding);
    true
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_floor(device: &qdk_cutensornet::dev_gate_split::BenchDevice) {
    match Timing::of(|| device.synchronize()) {
        Ok(timing) => println!("FLOOR call=idle_sync {timing}"),
        Err(error) => println!("FLOOR call=idle_sync status=refused reason={error}"),
    }
    match Timing::of(|| device.empty_launch()) {
        Ok(timing) => println!("FLOOR call=memset_launch_sync {timing}"),
        Err(error) => println!("FLOOR call=memset_launch_sync status=refused reason={error}"),
    }
}

/// Checks and times one gate split; its median time, when it agreed.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_gate_split(
    device: &qdk_cutensornet::dev_gate_split::BenchDevice,
    problem: &Problem,
    reference: &[f64],
    svd: qdk_cutensornet::dev_gate_split::TensorSvdAlgo,
    split: qdk_cutensornet::dev_gate_split::GateSplitAlgo,
) -> Option<f64> {
    use qdk_cutensornet::dev_gate_split::{GateSplitInputs, TensorSvdAlgo, Truncation};

    let bond = problem.bond;
    let (kept, kept_weight) = cpu_split(bond, reference);
    let (truncation, truncation_name) = if svd == TensorSvdAlgo::Gesvdr {
        (Truncation::FixedExtent(kept), "fixed-extent")
    } else {
        (
            Truncation::DiscardedWeight(DISCARDED_WEIGHT_CUTOFF),
            "discarded-weight",
        )
    };
    let pair = format!(
        "b={bond} svd={} split={} truncation={truncation_name}",
        svd.name(),
        split.name()
    );
    let inputs = GateSplitInputs {
        left: &problem.left,
        right: &problem.right,
        gate: &problem.gate,
    };
    let mut prepared = match device.gate_split(bond, svd, split, truncation, inputs) {
        Ok(prepared) => prepared,
        Err(error) => {
            println!("GATESPLIT status=refused {pair} reason={error}");
            return None;
        }
    };
    let outcome = match prepared.outcome() {
        Ok(outcome) => outcome,
        Err(error) => {
            println!("GATESPLIT status=refused {pair} reason={error}");
            return None;
        }
    };
    let values = &outcome.singular_values;
    let value_deviation = deviation(values, reference);
    let weight: f64 = values.iter().map(|value| value * value).sum();
    let weight_deviation = (weight - kept_weight).abs();
    let extent_agrees = usize::try_from(outcome.reduced_extent) == Ok(kept);
    let discarded = outcome.discarded_weight.map_or_else(
        || "not-computed".to_owned(),
        |weight| format!("{weight:.3e}"),
    );
    let algo_status = match prepared.algo_status() {
        Ok(None) => String::new(),
        Ok(Some(status)) => format!(" {status}"),
        Err(error) => format!(" algo_status=unreadable({error})"),
    };
    let facts = format!(
        "{pair} full={} reduced={} cpu_kept={kept} sv_dev={value_deviation:.2e} kept_weight_dev={weight_deviation:.2e} discarded_weight={discarded} cpu_discarded={:.3e}{algo_status} setup_ms={:.3}",
        outcome.full_extent,
        outcome.reduced_extent,
        1.0 - kept_weight,
        prepared.setup_seconds() * 1e3,
    );
    let within = |tolerance: f64| {
        extent_agrees && value_deviation <= tolerance && weight_deviation <= tolerance
    };
    if !within(TOLERANCE) {
        if svd == TensorSvdAlgo::Gesvdj && within(GESVDJ_TIMING_TOLERANCE) {
            match Timing::of(|| prepared.run().map(|_| ())) {
                Ok(timing) => {
                    println!(
                        "GATESPLIT status=disagrees {facts} timed_despite_disagreement {timing}"
                    );
                }
                Err(error) => println!("GATESPLIT status=disagrees {facts} reason={error}"),
            }
        } else {
            println!("GATESPLIT status=disagrees {facts}");
        }
        return None;
    }
    match Timing::of(|| prepared.run().map(|_| ())) {
        Ok(timing) => {
            println!("GATESPLIT status=ok {facts} {timing}");
            Some(timing.median)
        }
        Err(error) => {
            println!("GATESPLIT status=refused {facts} reason={error}");
            None
        }
    }
}

/// The fastest gate split of `bond` that agreed, against GESVD with
/// `reduced` (`baseline`, its median when it agreed).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_best(
    bond: usize,
    agreed: &[(
        qdk_cutensornet::dev_gate_split::TensorSvdAlgo,
        qdk_cutensornet::dev_gate_split::GateSplitAlgo,
        f64,
    )],
    baseline: Option<f64>,
) {
    let Some(&(svd, split, median)) = agreed
        .iter()
        .min_by(|left, right| left.2.total_cmp(&right.2))
    else {
        println!("BEST b={bond} status=none-agreed");
        return;
    };
    let versus = baseline.map_or_else(
        || "unmeasured".to_owned(),
        |baseline| format!("{:.3}", median / baseline),
    );
    println!(
        "BEST b={bond} svd={} split={} ms_median={median:.4} vs_gesvd={versus}",
        svd.name(),
        split.name()
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_qr(device: &qdk_cutensornet::dev_gate_split::BenchDevice, problem: &Problem) {
    let bond = problem.bond;
    let mut qr = match device.tensor_qr(bond, &problem.left) {
        Ok(qr) => qr,
        Err(error) => {
            println!("QR b={bond} status=refused reason={error}");
            return;
        }
    };
    let (q, r) = match qr.factors() {
        Ok(factors) => factors,
        Err(error) => {
            println!("QR b={bond} status=refused reason={error}");
            return;
        }
    };
    let (reconstruction, orthonormality) = qr_deviations(bond, &problem.left, &q, &r);
    let facts = format!("b={bond} recon_dev={reconstruction:.2e} orth_dev={orthonormality:.2e}");
    if reconstruction > TOLERANCE || orthonormality > TOLERANCE {
        println!("QR status=disagrees {facts}");
        return;
    }
    match Timing::of(|| qr.run()) {
        Ok(timing) => println!("QR status=ok {facts} {timing}"),
        Err(error) => println!("QR status=refused {facts} reason={error}"),
    }
}

/// `‖Q·R − A‖/‖A‖` and `max |Q†Q − I|`, `A` and `Q` as `2b × b` matrices.
fn qr_deviations(bond: usize, input: &[Complex64], q: &[Complex64], r: &[Complex64]) -> (f64, f64) {
    let rows = 2 * bond;
    let mut residual = 0.0;
    let mut norm = 0.0;
    for column in 0..bond {
        for row in 0..rows {
            let product: Complex64 = (0..bond)
                .map(|inner| q[row + rows * inner] * r[inner + bond * column])
                .sum();
            residual += (product - input[row + rows * column]).norm_sqr();
            norm += input[row + rows * column].norm_sqr();
        }
    }
    let mut orthonormality = 0.0_f64;
    for left in 0..bond {
        for right in 0..bond {
            let product: Complex64 = (0..rows)
                .map(|row| q[row + rows * left].conj() * q[row + rows * right])
                .sum();
            let expected = if left == right { 1.0 } else { 0.0 };
            orthonormality = orthonormality.max((product - expected).norm());
        }
    }
    ((residual / norm).sqrt(), orthonormality)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_batched(
    device: &qdk_cutensornet::dev_gate_split::BenchDevice,
    solver: &qdk_cutensornet::dev_gate_split::Cusolver,
    options: &Options,
    random: &mut Random,
) {
    use qdk_cutensornet::dev_gate_split::{BatchShape, SvdMethod};

    for method in [SvdMethod::Gesvdj, SvdMethod::Gesvda] {
        for &bond in &options.bonds {
            let size = 2 * bond;
            if method == SvdMethod::Gesvdj && size > 32 {
                continue;
            }
            for &batch in &options.batches {
                let shape = BatchShape {
                    rows: size,
                    columns: size,
                    rank: bond,
                    batch,
                };
                let name = format!("{method:?}").to_uppercase();
                let facts = format!("m={size} n={size} rank={bond} k={batch}");
                if 4 * size * size * batch * 16 > BATCH_MEMORY_LIMIT {
                    println!("{name} status=skipped {facts} reason=memory");
                    continue;
                }
                let matrices = graded_matrices(size, batch, random);
                match report_one_batch(device, solver, method, shape, &matrices) {
                    Ok((line, seconds)) => {
                        println!("{name} {line} {facts}");
                        if seconds > BATCH_TIME_LIMIT_SECONDS {
                            println!(
                                "{name} status=skipped m={size} reason=larger batches exceed the time limit"
                            );
                            break;
                        }
                    }
                    Err(error) => println!("{name} status=refused {facts} reason={error}"),
                }
            }
        }
    }
}

/// Checks and times one batched SVD: its line, and one launch's seconds.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_one_batch(
    device: &qdk_cutensornet::dev_gate_split::BenchDevice,
    solver: &qdk_cutensornet::dev_gate_split::Cusolver,
    method: qdk_cutensornet::dev_gate_split::SvdMethod,
    shape: qdk_cutensornet::dev_gate_split::BatchShape,
    matrices: &[Complex64],
) -> Result<(String, f64), String> {
    use qdk_cutensornet::dev_gate_split::SvdMethod;

    let started = Instant::now();
    let mut svd = solver.batched_svd(device, method, shape, matrices)?;
    let first = started.elapsed().as_secs_f64();
    let (values, info) = svd.outcome()?;
    let size = shape.rows * shape.columns;
    let batch = shape.batch;
    let sampled: Vec<usize> = if batch <= 4 {
        (0..batch).collect()
    } else {
        vec![0, batch / 3, 2 * batch / 3, batch - 1]
    };
    let mut value_deviation = 0.0_f64;
    for &index in &sampled {
        let reference = reference_singular_values(
            shape.rows,
            shape.columns,
            &matrices[index * size..(index + 1) * size],
        );
        value_deviation = value_deviation.max(deviation(&values[index], &reference));
    }
    let unconverged = info.iter().filter(|&&status| status != 0).count();
    let facts = format!(
        "sv_dev={value_deviation:.2e} checked={} unconverged={unconverged}",
        sampled.len()
    );
    if value_deviation > TOLERANCE || unconverged > 0 {
        return Ok((format!("status=disagrees {facts}"), first));
    }
    let timing = Timing::of(|| svd.run())?;
    let per_matrix = timing.median * 1e3 / f64::from(u32::try_from(batch).expect("a small batch"));
    let restore = if method == SvdMethod::Gesvdj {
        let restore = Timing::of(|| svd.restore_input())?;
        format!(" restore_ms_median={:.4}", restore.median)
    } else {
        String::new()
    };
    Ok((
        format!("status=ok {facts} {timing} us_per_matrix={per_matrix:.2}{restore}"),
        timing.median / 1e3,
    ))
}

/// PLAN §2.2's rule at `b = 24`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn report_decision(deciding: Option<f64>) {
    match deciding {
        None => println!("DECISION b={DECIDING_BOND} status=unmeasured"),
        Some(median) => {
            let verdict = if median <= 0.3 {
                "stage1-alone"
            } else if median <= 2.0 {
                "stage1-as-base-for-stage2"
            } else {
                "own-kernels"
            };
            println!(
                "DECISION b={DECIDING_BOND} gatesplit_ms_median={median:.4} verdict={verdict} rule=<=0.3:stage1-alone,~1:base-for-stage2,larger:own-kernels"
            );
        }
    }
}
