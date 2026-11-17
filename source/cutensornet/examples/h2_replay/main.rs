// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): recorded H₂ shots replayed on
//! `canonical_mps`'s CPU engine (`cpu`) or `qdk_cutensornet::CudaMpsEngine`
//! (`cuda-mps`, on the GPU), every value a comparison of the two engines
//! needs printed at full precision.
//!
//! A shot of H₂'s phase estimation (29 qubits, about 390,000 gates and
//! 1,400 mid-circuit measurements) is recorded as the operations a sparse
//! state received, each measurement with the outcome the shot drew. The
//! binary replays each shot gate by gate, projects each measurement on the
//! recorded outcome, and prints, per shot:
//!
//! - `shot <file name>`;
//! - `p <probability>` for each projection, the engine's own probability;
//! - `w <retained weight>` at the end;
//! - `r <probability>` for each outcome of the readout qubits, the first
//!   readout qubit the most significant bit, read from the expectations of
//!   every Z string;
//! - `stats splits … mpos … moves … leftward … rest … transient …`, the
//!   engine's split statistics (splits, windows applied as an MPO, moves
//!   of the centre, the MPOs swept leftward, the largest bonds at rest and
//!   transient);
//! - `time seconds=… tvd=…`: the replay's seconds (to the end of the
//!   readout) and the readout's total variation distance to the recorded
//!   exact distribution (`SHOT.readout`, when present).
//!
//! These are the lines of the CPU engine's acceptance dumps, with the
//! `time` line added, so that a dump of either engine compares with them;
//! `cuda-mps` first prints `availability …`, the libraries it found.
//!
//! ```text
//! cargo run --release -p qdk_cutensornet --example h2_replay -- \
//!     --engine cpu|cuda-mps [--chi 64] [--order 26,25,…] SHOT.replay…
//! ```
//!
//! `--order` puts qubit `q` on site `order[q]`. The replay format: one
//! operation per line (`#` starts a comment): `<Gate> <q> [<angle>]` for a
//! one-qubit gate (`X`, `Y`, `Z`, `H`, `S`, `SAdj`, `T`, `TAdj`, or `Rx`,
//! `Ry`, `Rz` with an angle in radians); `Cx|Cy|Cz <a> <b>`, control `a`;
//! `P <q> <b> <p>`, the projection of `q` on `|b⟩`; last,
//! `READOUT <q0> <q1> …`. The readout file: `<bits in record order>
//! <bits reversed> <probability>` per line.

use num_complex::Complex64 as C;
use std::{error::Error, f64::consts::PI, fs, time::Instant};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

type Res<T> = Result<T, Box<dyn Error>>;

/// One step of a recorded shot, on qubits.
enum Op {
    /// A gate on one or two qubits, its matrix row-major with `qubits[0]`
    /// the most significant.
    Gate { qubits: Vec<usize>, matrix: Vec<C> },
    /// A projection on the outcome the shot drew.
    Project { qubit: usize, one: bool },
}

/// A recorded shot: its qubits, its steps and its readout qubits.
struct Shot {
    qubits: usize,
    ops: Vec<Op>,
    readout: Vec<usize>,
}

fn main() {
    if let Err(error) = run() {
        // Debug, not Display: it shows the cause an algebra error wraps.
        eprintln!("h2_replay error: {error:?}");
        std::process::exit(1);
    }
}

fn run() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let chi: usize = flag("--chi").map_or(Ok(64), |x| x.parse())?;
    let shots: Vec<&String> = args.iter().filter(|a| a.ends_with(".replay")).collect();
    match flag("--engine").as_deref() {
        Some("cpu") => replay_all(
            &canonical_mps::CpuMpsEngine::new(canonical_mps::CpuAlgebra),
            &shots,
            flag("--order").as_deref(),
            chi,
            statistics::<canonical_mps::CpuAlgebra>,
        ),
        Some("cuda-mps") => replay_on_cuda_mps(&shots, flag("--order").as_deref(), chi),
        other => Err(format!("--engine {other:?}: cpu or cuda-mps").into()),
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn replay_on_cuda_mps(shots: &[&String], order: Option<&str>, chi: usize) -> Res<()> {
    let availability = qdk_cutensornet::discover()?;
    println!("availability {:?}", availability.report());
    let engine =
        qdk_cutensornet::CudaMpsEngine::new(qdk_cutensornet::CudaAlgebra::new(&availability)?);
    replay_all(
        &engine,
        shots,
        order,
        chi,
        statistics::<qdk_cutensornet::CudaAlgebra>,
    )
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn replay_on_cuda_mps(_shots: &[&String], _order: Option<&str>, _chi: usize) -> Res<()> {
    Err("cuda-mps runs on Linux x86-64 only".into())
}

/// The `stats` line's words: the engine's split statistics.
fn statistics<A: canonical_mps::algebra::SiteAlgebra>(
    state: &canonical_mps::CanonicalMpsState<A>,
) -> String {
    let s = state.split_statistics();
    format!(
        "splits {} mpos {} moves {} leftward {} rest {} transient {}",
        s.splits(),
        s.mpo_applications(),
        s.centre_moves(),
        canonical_mps::leftward_sweeps(),
        s.largest_bond_at_rest(),
        s.largest_transient_bond()
    )
}

fn replay_all<E: MpsEngine>(
    engine: &E,
    shots: &[&String],
    order: Option<&str>,
    chi: usize,
    statistics_of: fn(&E::State) -> String,
) -> Res<()>
where
    E::Error: Error + 'static,
{
    for path in shots {
        let shot = replay(path)?;
        let site_of = site_order(order, shot.qubits)?;
        let started = Instant::now();
        let mut state = engine.prepare(&MpsShape::qubit_chain(shot.qubits, chi)?)?;
        println!("shot {}", path.rsplit('/').next().unwrap_or(path));
        for op in &shot.ops {
            match op {
                Op::Gate { qubits, matrix } => {
                    let sites: Vec<usize> = qubits.iter().map(|&q| site_of[q]).collect();
                    engine.apply(&mut state, &sites, matrix)?;
                }
                Op::Project { qubit, one } => {
                    let p = engine.project(&mut state, site_of[*qubit], *one)?;
                    println!("p {p:.17e}");
                }
            }
        }
        println!("w {:.17e}", engine.retained_weight(&state)?);
        let sites: Vec<usize> = shot.readout.iter().map(|&q| site_of[q]).collect();
        let distribution = readout_distribution(engine, &state, &sites)?;
        for value in &distribution {
            println!("r {value:.17e}");
        }
        let seconds = started.elapsed().as_secs_f64();
        println!("stats {}", statistics_of(&state));
        let readout = path.replace(".replay", ".readout");
        let tvd = if std::path::Path::new(&readout).exists() {
            format!("{:.3e}", tvd_to(&distribution, &readout_file(&readout)?)?)
        } else {
            "-".to_string()
        };
        println!("time seconds={seconds:.3} tvd={tvd}");
    }
    Ok(())
}

/// The order `a,b,c,…` (the site of each qubit), else the identity.
fn site_order(text: Option<&str>, qubits: usize) -> Res<Vec<usize>> {
    let Some(text) = text else {
        return Ok((0..qubits).collect());
    };
    let order: Vec<usize> = text
        .split(',')
        .map(|x| x.trim().parse())
        .collect::<Result<_, _>>()?;
    let mut sorted = order.clone();
    sorted.sort_unstable();
    if sorted != (0..qubits).collect::<Vec<_>>() {
        return Err(format!("--order is not a permutation of 0..{qubits}").into());
    }
    Ok(order)
}

/// The distribution of `sites` read together, outcome `x` with `sites[0]`
/// as its most significant bit, from the expectations of every Z string.
fn readout_distribution<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    sites: &[usize],
) -> Res<Vec<f64>>
where
    E::Error: Error + 'static,
{
    let k = sites.len();
    let mut z = vec![0.0; 1 << k];
    z[0] = 1.0;
    for (subset, slot) in z.iter_mut().enumerate().skip(1) {
        let factors: Vec<(usize, Pauli)> = (0..k)
            .filter(|bit| subset >> (k - 1 - bit) & 1 == 1)
            .map(|bit| (sites[bit], Pauli::Z))
            .collect();
        let observable = PauliSum::new(
            C::new(0.0, 0.0),
            vec![PauliTerm::new(C::new(1.0, 0.0), factors)?],
        )?;
        *slot = engine.expectation(state, &observable)?.re;
    }
    Ok((0..1usize << k)
        .map(|outcome| {
            z.iter()
                .enumerate()
                .map(|(subset, value)| {
                    if (outcome & subset).count_ones() % 2 == 1 {
                        -value
                    } else {
                        *value
                    }
                })
                .sum::<f64>()
                / f64::from(1 << k)
        })
        .collect())
}

/// The total variation distance of `distribution` to a recorded one; an
/// outcome the file does not list counts as probability 0.
fn tvd_to(distribution: &[f64], recorded: &[(String, f64)]) -> Res<f64> {
    let mut exact = vec![0.0; distribution.len()];
    for (bits, p) in recorded {
        let outcome = usize::from_str_radix(bits, 2)?;
        *exact
            .get_mut(outcome)
            .ok_or_else(|| format!("outcome {bits} outside the readout"))? += p;
    }
    Ok(distribution
        .iter()
        .zip(exact)
        .map(|(p, q)| 0.5 * (p - q).abs())
        .sum())
}

/// A recorded shot (a `SHOT.replay` file).
fn replay(path: &str) -> Res<Shot> {
    let text = fs::read_to_string(path)?;
    let mut shot = Shot {
        qubits: 0,
        ops: Vec::new(),
        readout: Vec::new(),
    };
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        replay_line(&mut shot, line)
            .map_err(|error| format!("{path}:{}: {error}: {line}", index + 1))?;
    }
    shot.qubits = shot
        .ops
        .iter()
        .flat_map(|op| match op {
            Op::Gate { qubits, .. } => qubits.clone(),
            Op::Project { qubit, .. } => vec![*qubit],
        })
        .chain(shot.readout.iter().copied())
        .max()
        .map_or(0, |q| q + 1);
    Ok(shot)
}

fn replay_line(shot: &mut Shot, line: &str) -> Res<()> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let qubit_at = |i: usize| -> Res<usize> { Ok(words.get(i).ok_or("short line")?.parse()?) };
    let zero = C::new(0.0, 0.0);
    let one = C::new(1.0, 0.0);
    let controlled = |target: [C; 4]| {
        let mut matrix_of = vec![zero; 16];
        matrix_of[0] = one;
        matrix_of[5] = one;
        matrix_of[10] = target[0];
        matrix_of[11] = target[1];
        matrix_of[14] = target[2];
        matrix_of[15] = target[3];
        matrix_of
    };
    let x = [zero, one, one, zero];
    let y = [zero, C::new(0.0, -1.0), C::new(0.0, 1.0), zero];
    let z = [one, zero, zero, -one];
    let phase = |theta: f64| [one, zero, zero, C::from_polar(1.0, theta)];
    match words[0] {
        "READOUT" => shot.readout = (1..words.len()).map(qubit_at).collect::<Res<_>>()?,
        "P" => shot.ops.push(Op::Project {
            qubit: qubit_at(1)?,
            one: match words.get(2) {
                Some(&"0") => false,
                Some(&"1") => true,
                _ => return Err("an outcome other than 0 or 1".into()),
            },
        }),
        name @ ("Cx" | "Cy" | "Cz") => shot.ops.push(Op::Gate {
            qubits: vec![qubit_at(1)?, qubit_at(2)?],
            matrix: controlled(match name {
                "Cx" => x,
                "Cy" => y,
                _ => z,
            }),
        }),
        name => {
            let angle = || -> Res<f64> { Ok(words.get(2).ok_or("missing angle")?.parse()?) };
            let half = std::f64::consts::FRAC_1_SQRT_2;
            let matrix = match name {
                "X" => x,
                "Y" => y,
                "Z" => z,
                "H" => [
                    C::new(half, 0.0),
                    C::new(half, 0.0),
                    C::new(half, 0.0),
                    C::new(-half, 0.0),
                ],
                "S" => phase(PI / 2.0),
                "SAdj" => phase(-PI / 2.0),
                "T" => phase(PI / 4.0),
                "TAdj" => phase(-PI / 4.0),
                "Rx" => {
                    let (sine, cosine) = (angle()? / 2.0).sin_cos();
                    [
                        C::new(cosine, 0.0),
                        C::new(0.0, -sine),
                        C::new(0.0, -sine),
                        C::new(cosine, 0.0),
                    ]
                }
                "Ry" => {
                    let (sine, cosine) = (angle()? / 2.0).sin_cos();
                    [
                        C::new(cosine, 0.0),
                        C::new(-sine, 0.0),
                        C::new(sine, 0.0),
                        C::new(cosine, 0.0),
                    ]
                }
                "Rz" => {
                    let theta = angle()?;
                    [
                        C::from_polar(1.0, -theta / 2.0),
                        zero,
                        zero,
                        C::from_polar(1.0, theta / 2.0),
                    ]
                }
                _ => return Err(format!("unknown one-qubit gate {name}").into()),
            };
            shot.ops.push(Op::Gate {
                qubits: vec![qubit_at(1)?],
                matrix: matrix.to_vec(),
            });
        }
    }
    Ok(())
}

/// The exact readout distribution of a recorded shot, keyed by the outcome
/// in record order.
fn readout_file(path: &str) -> Res<Vec<(String, f64)>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        out.push((
            w[0].to_string(),
            w.get(2).ok_or("short readout line")?.parse()?,
        ));
    }
    Ok(out)
}
