// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The circuits the examples measure the engine on, the exact state vector
//! they compare it with, and the sparse simulator's runs of the same
//! circuits. Shared by the examples through `#[path]`; each uses a part.

#![allow(dead_code, reason = "each example uses a part")]

use num_complex::Complex64;
use qdk_cpu_mps::{CanonicalMps, CpuMpsEngine};
use qdk_simulators::SparseStateSim;
use std::{error::Error, f64::consts::PI, fs, hint::black_box, time::Instant};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

pub type C = Complex64;
pub type Res<T> = Result<T, Box<dyn Error>>;

pub fn c(re: f64, im: f64) -> C {
    C::new(re, im)
}

/// A deterministic generator, so that runs compare: `SplitMix64` (Steele,
/// Lea and Flood, 2014).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x5DEE_CE66_D1CE_4E5B)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next value's top 53 bits as a multiple of 2⁻⁵³, in [0, 1).
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    #[expect(clippy::cast_possible_truncation, reason = "below a usize bound")]
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    fn normal(&mut self) -> f64 {
        let u = self.uniform().max(1e-300);
        let v = self.uniform();
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// An operation of the sparse simulator, which runs a circuit only when
/// every gate has its native form.
#[derive(Clone, Copy, Debug)]
pub enum Native {
    X(usize),
    Y(usize),
    Z(usize),
    H(usize),
    S(usize),
    SAdj(usize),
    T(usize),
    TAdj(usize),
    Rx(f64, usize),
    Ry(f64, usize),
    Rz(f64, usize),
    Cx(usize, usize),
    Cy(usize, usize),
    Cz(usize, usize),
    /// A phase `e^{iθ}` on the target, controlled by the first qubit.
    ControlledPhase(usize, usize, f64),
}

impl Native {
    fn on(self, sim: &mut SparseStateSim) {
        match self {
            Self::X(q) => sim.x(q),
            Self::Y(q) => sim.y(q),
            Self::Z(q) => sim.z(q),
            Self::H(q) => sim.h(q),
            Self::S(q) => sim.s(q),
            Self::SAdj(q) => sim.sadj(q),
            Self::T(q) => sim.t(q),
            Self::TAdj(q) => sim.tadj(q),
            Self::Rx(theta, q) => sim.rx(theta, q),
            Self::Ry(theta, q) => sim.ry(theta, q),
            Self::Rz(theta, q) => sim.rz(theta, q),
            Self::Cx(a, b) => sim.mcx(&[a], b),
            Self::Cy(a, b) => sim.mcy(&[a], b),
            Self::Cz(a, b) => sim.mcz(&[a], b),
            Self::ControlledPhase(a, b, theta) => {
                sim.mcphase(&[a], C::from_polar(1.0, theta), b);
            }
        }
    }
}

/// One step of a circuit, on qubits.
pub enum Op {
    /// A gate on one or two qubits, its matrix row-major with `qubits[0]`
    /// the most significant; `natives` is the same gate on the sparse
    /// simulator, when it has one.
    Gate {
        qubits: Vec<usize>,
        matrix: Vec<C>,
        natives: Option<Vec<Native>>,
    },
    /// A projection on the value a recorded shot drew, with the
    /// probability the recording read.
    Project {
        qubit: usize,
        one: bool,
        probability: f64,
    },
}

/// A circuit: its qubits, its steps and the qubits it reads out at the end.
pub struct Circuit {
    pub qubits: usize,
    pub ops: Vec<Op>,
    pub readout: Vec<usize>,
}

impl Circuit {
    fn new(qubits: usize) -> Self {
        Self {
            qubits,
            ops: Vec::new(),
            readout: Vec::new(),
        }
    }

    fn one(&mut self, qubit: usize, matrix: [C; 4], natives: Option<Vec<Native>>) {
        self.ops.push(Op::Gate {
            qubits: vec![qubit],
            matrix: matrix.to_vec(),
            natives,
        });
    }

    fn two(&mut self, a: usize, b: usize, matrix: [C; 16], natives: Option<Vec<Native>>) {
        self.ops.push(Op::Gate {
            qubits: vec![a, b],
            matrix: matrix.to_vec(),
            natives,
        });
    }

    /// Whether the sparse simulator runs the circuit.
    pub fn is_native(&self) -> bool {
        self.ops.iter().all(|op| match op {
            Op::Gate { natives, .. } => natives.is_some(),
            Op::Project { .. } => true,
        })
    }

    pub fn two_qubit_gates(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, Op::Gate { qubits, .. } if qubits.len() == 2))
            .count()
    }

    pub fn projections(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, Op::Project { .. }))
            .count()
    }
}

const H: [C; 4] = [
    C::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    C::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    C::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    C::new(-std::f64::consts::FRAC_1_SQRT_2, 0.0),
];

fn phase(theta: f64) -> [C; 4] {
    [
        c(1.0, 0.0),
        c(0.0, 0.0),
        c(0.0, 0.0),
        C::from_polar(1.0, theta),
    ]
}

fn rx(theta: f64) -> [C; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    [c(k, 0.0), c(0.0, -s), c(0.0, -s), c(k, 0.0)]
}

fn ry(theta: f64) -> [C; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)]
}

fn rz(theta: f64) -> [C; 4] {
    [
        C::from_polar(1.0, -theta / 2.0),
        c(0.0, 0.0),
        c(0.0, 0.0),
        C::from_polar(1.0, theta / 2.0),
    ]
}

/// `Rz(φ)·Ry(θ)·Rz(λ)`.
fn u3(phi: f64, theta: f64, lambda: f64) -> [C; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    let ph = |a: f64| C::from_polar(1.0, a / 2.0);
    [
        k * ph(-phi - lambda),
        -s * ph(-phi + lambda),
        s * ph(phi - lambda),
        k * ph(phi + lambda),
    ]
}

/// `|0⟩⟨0| ⊗ 1 + |1⟩⟨1| ⊗ u`, the control first.
fn controlled(u: [C; 4]) -> [C; 16] {
    let mut m = [c(0.0, 0.0); 16];
    m[0] = c(1.0, 0.0);
    m[5] = c(1.0, 0.0);
    m[10] = u[0];
    m[11] = u[1];
    m[14] = u[2];
    m[15] = u[3];
    m
}

fn pauli_x() -> [C; 4] {
    [c(0.0, 0.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)]
}

fn pauli_y() -> [C; 4] {
    [c(0.0, 0.0), c(0.0, -1.0), c(0.0, 1.0), c(0.0, 0.0)]
}

fn pauli_z() -> [C; 4] {
    [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(-1.0, 0.0)]
}

/// `exp(−iθ/2 · Z⊗Z)`.
fn rzz(theta: f64) -> [C; 16] {
    let mut m = [c(0.0, 0.0); 16];
    let (a, b) = (
        C::from_polar(1.0, -theta / 2.0),
        C::from_polar(1.0, theta / 2.0),
    );
    m[0] = a;
    m[5] = b;
    m[10] = b;
    m[15] = a;
    m
}

/// A Haar-like random unitary of dimension `d`, row-major: Gram–Schmidt
/// of a complex Gaussian matrix.
fn random_unitary<const N: usize>(rng: &mut Rng, d: usize) -> [C; N] {
    let mut cols: Vec<Vec<C>> = (0..d)
        .map(|_| (0..d).map(|_| c(rng.normal(), rng.normal())).collect())
        .collect();
    for k in 0..d {
        for j in 0..k {
            let dot: C = (0..d).map(|i| cols[j][i].conj() * cols[k][i]).sum();
            let (done, rest) = cols.split_at_mut(k);
            for (x, y) in rest[0].iter_mut().zip(&done[j]) {
                *x -= dot * y;
            }
        }
        let norm = cols[k].iter().map(C::norm_sqr).sum::<f64>().sqrt();
        for x in &mut cols[k] {
            *x /= norm;
        }
    }
    std::array::from_fn(|at| cols[at % d][at / d])
}

/// `u` as `Rz(γ)·Ry(β)·Rz(α)` up to a global phase, for the sparse
/// simulator: the angles from the phases of `u`'s entries. Needs no entry
/// of `u` to be zero, which holds for a random unitary.
fn euler_natives(u: [C; 4], q: usize) -> Vec<Native> {
    let beta = 2.0 * u[2].norm().atan2(u[0].norm());
    let sum = (u[3] * u[0].conj()).arg();
    let difference = (u[2] * (-u[1]).conj()).arg();
    let (gamma, alpha) = (f64::midpoint(sum, difference), (sum - difference) / 2.0);
    // The halves of the two phases are known up to a sign each; a sign on
    // one of them is a sign on β.
    let overlap = |beta: f64| {
        let v = u3(gamma, beta, alpha);
        (0..4).map(|i| v[i].conj() * u[i]).sum::<C>().norm()
    };
    let beta = if overlap(beta) > overlap(-beta) {
        beta
    } else {
        -beta
    };
    vec![
        Native::Rz(alpha, q),
        Native::Ry(beta, q),
        Native::Rz(gamma, q),
    ]
}

/// Random `u3` on every qubit, then CNOT or CY on alternating neighbours:
/// a brickwork the sparse simulator runs natively.
pub fn brickwork(n: usize, depth: usize, seed: u64) -> Circuit {
    let mut rng = Rng::new(seed);
    let mut circuit = Circuit::new(n);
    let tau = std::f64::consts::TAU;
    for layer in 0..depth {
        for q in 0..n {
            let (phi, theta, lambda) = (
                tau * rng.uniform(),
                tau * rng.uniform(),
                tau * rng.uniform(),
            );
            let natives = vec![
                Native::Rz(lambda, q),
                Native::Ry(theta, q),
                Native::Rz(phi, q),
            ];
            circuit.one(q, u3(phi, theta, lambda), Some(natives));
        }
        for a in (layer % 2..n.saturating_sub(1)).step_by(2) {
            if (a / 2 + layer) % 2 == 0 {
                circuit.two(
                    a,
                    a + 1,
                    controlled(pauli_x()),
                    Some(vec![Native::Cx(a, a + 1)]),
                );
            } else {
                circuit.two(
                    a + 1,
                    a,
                    controlled(pauli_y()),
                    Some(vec![Native::Cy(a + 1, a)]),
                );
            }
        }
    }
    circuit
}

/// A quench of the transverse-field Ising chain at its critical point from
/// `|0…0⟩`: per layer, one Trotter step of 0.2, `exp(i·0.2·ZZ)` on even
/// then odd neighbours, then `exp(i·0.2·X)` on every qubit.
pub fn ising(n: usize, depth: usize) -> Circuit {
    let dt = 0.2;
    let mut circuit = Circuit::new(n);
    for _ in 0..depth {
        for parity in 0..2 {
            for a in (parity..n.saturating_sub(1)).step_by(2) {
                let natives = vec![
                    Native::Cx(a, a + 1),
                    Native::Rz(-2.0 * dt, a + 1),
                    Native::Cx(a, a + 1),
                ];
                circuit.two(a, a + 1, rzz(-2.0 * dt), Some(natives));
            }
        }
        for q in 0..n {
            circuit.one(q, rx(-2.0 * dt), Some(vec![Native::Rx(-2.0 * dt, q)]));
        }
    }
    circuit
}

/// A random product state (a random unitary on every qubit), then the
/// quantum Fourier transform without its final SWAPs: controlled phases on
/// every pair.
pub fn qft(n: usize, seed: u64) -> Circuit {
    let mut rng = Rng::new(seed);
    let mut circuit = Circuit::new(n);
    for q in 0..n {
        let u = random_unitary::<4>(&mut rng, 2);
        circuit.one(q, u, Some(euler_natives(u, q)));
    }
    for j in 0..n {
        circuit.one(j, H, Some(vec![Native::H(j)]));
        for k in j + 1..n {
            let theta = PI / f64::powi(2.0, i32::try_from(k - j).expect("a small distance"));
            let natives = vec![Native::ControlledPhase(k, j, theta)];
            circuit.two(k, j, controlled(phase(theta)), Some(natives));
        }
    }
    circuit
}

/// `depth` layers of a random one-qubit unitary on every qubit, then
/// random two-qubit unitaries on random pairs: all-to-all, so most gates
/// are on distant sites.
pub fn random(n: usize, depth: usize, seed: u64) -> Circuit {
    let mut rng = Rng::new(seed);
    let mut circuit = Circuit::new(n);
    let mut qubits: Vec<usize> = (0..n).collect();
    for _ in 0..depth {
        for q in 0..n {
            circuit.one(q, random_unitary::<4>(&mut rng, 2), None);
        }
        rng.shuffle(&mut qubits);
        for pair in qubits.chunks_exact(2) {
            circuit.two(pair[0], pair[1], random_unitary::<16>(&mut rng, 4), None);
        }
    }
    circuit
}

/// [`random`] then its inverse: the exact end state is `|0…0⟩`.
pub fn mirror(n: usize, depth: usize, seed: u64) -> Circuit {
    let mut circuit = random(n, depth, seed);
    let inverse: Vec<Op> = circuit
        .ops
        .iter()
        .rev()
        .map(|op| match op {
            Op::Gate { qubits, matrix, .. } => Op::Gate {
                qubits: qubits.clone(),
                matrix: adjoint(matrix),
                natives: None,
            },
            Op::Project { .. } => unreachable!("a random circuit has no projections"),
        })
        .collect();
    circuit.ops.extend(inverse);
    circuit
}

fn adjoint(m: &[C]) -> Vec<C> {
    let k = if m.len() == 4 { 2 } else { 4 };
    (0..m.len())
        .map(|at| m[(at % k) * k + at / k].conj())
        .collect()
}

/// Textbook phase estimation: `m` phase qubits (H each), then the
/// controlled `U^(2^k)` from phase qubit `k`, `U` one Ising Trotter step
/// on qubits `m..n`, then the inverse QFT on the phase qubits.
pub fn qpe(n: usize, m: usize) -> Circuit {
    assert!(m >= 1 && m + 2 <= n, "1 <= m <= n - 2");
    let dt = 0.2;
    let mut circuit = Circuit::new(n);
    for q in 0..m {
        circuit.one(q, H, None);
    }
    let cnot = controlled(pauli_x());
    for k in 0..m {
        for _ in 0..1usize << k {
            for parity in 0..2 {
                for a in (m + parity..n - 1).step_by(2) {
                    circuit.two(a, a + 1, cnot, None);
                    circuit.two(k, a + 1, controlled(rz(-2.0 * dt)), None);
                    circuit.two(a, a + 1, cnot, None);
                }
            }
            for q in m..n {
                circuit.two(k, q, controlled(rx(-2.0 * dt)), None);
            }
        }
    }
    for j in (0..m).rev() {
        for k in (j + 1..m).rev() {
            let theta = -PI / f64::powi(2.0, i32::try_from(k - j).expect("a small distance"));
            circuit.two(k, j, controlled(phase(theta)), None);
        }
        circuit.one(j, H, None);
    }
    circuit
}

/// A gate list, one gate per line: `cx a b`, `h q`, `s q`, `sadj q`,
/// `x q`, `rz θ q` or `ry θ q`. The polyene's phase estimation is one.
pub fn gate_list(path: &str) -> Res<Circuit> {
    let text = fs::read_to_string(path)?;
    let mut ops = Vec::new();
    let mut qubits = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let w: Vec<&str> = line.split_whitespace().collect();
        let q = |i: usize| -> Res<usize> { Ok(w.get(i).ok_or("short line")?.parse()?) };
        let (on, matrix, native): (Vec<usize>, Vec<C>, Native) = match w[0] {
            "cx" => (
                vec![q(1)?, q(2)?],
                controlled(pauli_x()).to_vec(),
                Native::Cx(q(1)?, q(2)?),
            ),
            "h" => (vec![q(1)?], H.to_vec(), Native::H(q(1)?)),
            "s" => (vec![q(1)?], phase(PI / 2.0).to_vec(), Native::S(q(1)?)),
            "sadj" => (vec![q(1)?], phase(-PI / 2.0).to_vec(), Native::SAdj(q(1)?)),
            "x" => (vec![q(1)?], pauli_x().to_vec(), Native::X(q(1)?)),
            "rz" => {
                let t: f64 = w[1].parse()?;
                (vec![q(2)?], rz(t).to_vec(), Native::Rz(t, q(2)?))
            }
            "ry" => {
                let t: f64 = w[1].parse()?;
                (vec![q(2)?], ry(t).to_vec(), Native::Ry(t, q(2)?))
            }
            other => return Err(format!("unknown gate {other}").into()),
        };
        qubits = on.iter().fold(qubits, |n, &q| n.max(q + 1));
        ops.push(Op::Gate {
            qubits: on,
            matrix,
            natives: Some(vec![native]),
        });
    }
    Ok(Circuit {
        qubits,
        ops,
        readout: Vec::new(),
    })
}

fn named_one(name: &str, angle: Option<f64>, q: usize) -> Res<([C; 4], Native)> {
    let angle = || angle.ok_or("missing angle");
    let quarter = C::from_polar(1.0, PI / 4.0);
    Ok(match name {
        "X" => (pauli_x(), Native::X(q)),
        "Y" => (pauli_y(), Native::Y(q)),
        "Z" => (pauli_z(), Native::Z(q)),
        "H" => (H, Native::H(q)),
        "S" => (phase(PI / 2.0), Native::S(q)),
        "SAdj" => (phase(-PI / 2.0), Native::SAdj(q)),
        "T" => (
            [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), quarter],
            Native::T(q),
        ),
        "TAdj" => (
            [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), quarter.conj()],
            Native::TAdj(q),
        ),
        "Rx" => (rx(angle()?), Native::Rx(angle()?, q)),
        "Ry" => (ry(angle()?), Native::Ry(angle()?, q)),
        "Rz" => (rz(angle()?), Native::Rz(angle()?, q)),
        _ => return Err(format!("unknown one-qubit gate {name}").into()),
    })
}

/// A recorded shot: the operations a sparse state received, in order, one
/// per line (`#` starts a comment):
///
/// - `<Gate> <q> [<angle>]`: a one-qubit gate;
/// - `Cx|Cy|Cz <a> <b>`: a controlled gate, control `a`;
/// - `P <q> <b> <p>`: the projection of `q` on `b`, which had probability
///   `p`;
/// - `READOUT <q0> <q1> …`: the qubits read out at the end, in record
///   order.
pub fn replay(path: &str) -> Res<Circuit> {
    let text = fs::read_to_string(path)?;
    let mut circuit = Circuit::new(0);
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let q = |i: usize| -> Res<usize> { Ok(w.get(i).ok_or("short line")?.parse()?) };
        match w[0] {
            "READOUT" => {
                circuit.readout = (1..w.len()).map(q).collect::<Res<_>>()?;
            }
            "P" => circuit.ops.push(Op::Project {
                qubit: q(1)?,
                one: w.get(2) == Some(&"1"),
                probability: w.get(3).ok_or("short line")?.parse()?,
            }),
            "Cx" => circuit.two(
                q(1)?,
                q(2)?,
                controlled(pauli_x()),
                Some(vec![Native::Cx(q(1)?, q(2)?)]),
            ),
            "Cy" => circuit.two(
                q(1)?,
                q(2)?,
                controlled(pauli_y()),
                Some(vec![Native::Cy(q(1)?, q(2)?)]),
            ),
            "Cz" => circuit.two(
                q(1)?,
                q(2)?,
                controlled(pauli_z()),
                Some(vec![Native::Cz(q(1)?, q(2)?)]),
            ),
            name => {
                let angle = w.get(2).map(|x| x.parse()).transpose()?;
                let (matrix, native) = named_one(name, angle, q(1)?)?;
                circuit.one(q(1)?, matrix, Some(vec![native]));
            }
        }
    }
    circuit.qubits = circuit
        .ops
        .iter()
        .flat_map(|op| match op {
            Op::Gate { qubits, .. } => qubits.clone(),
            Op::Project { qubit, .. } => vec![*qubit],
        })
        .chain(circuit.readout.iter().copied())
        .max()
        .map_or(0, |q| q + 1);
    Ok(circuit)
}

/// The exact readout distribution of a recorded shot, keyed by the
/// outcome in record order (the first readout qubit first).
pub fn readout_file(path: &str) -> Res<Vec<(String, f64)>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        out.push((w[0].to_string(), w.get(2).ok_or("short line")?.parse()?));
    }
    Ok(out)
}

/// A chain of `n` qubits with every bond at most `chi`.
pub fn chain(n: usize, chi: usize) -> MpsShape {
    MpsShape::qubit_chain(n, chi).expect("a chain of at least one site")
}

/// The engine's run of a circuit: its state, the seconds it took, and the
/// largest gap between a projection's probability and the recorded one.
pub struct EngineRun {
    pub state: CanonicalMps,
    pub seconds: f64,
    pub max_projection_gap: f64,
}

/// Runs `circuit` on `engine`, qubit `q` on site `order[q]`, every bond at
/// most `chi`. A projection takes the engine's own probability.
pub fn run_engine(
    engine: &CpuMpsEngine,
    circuit: &Circuit,
    order: &[usize],
    chi: usize,
) -> Res<EngineRun> {
    let mut state = engine.prepare(&chain(circuit.qubits, chi))?;
    let mut max_projection_gap: f64 = 0.0;
    let started = Instant::now();
    for op in &circuit.ops {
        match op {
            Op::Gate { qubits, matrix, .. } => {
                let sites: Vec<usize> = qubits.iter().map(|&q| order[q]).collect();
                engine.apply(&mut state, &sites, matrix)?;
            }
            Op::Project {
                qubit,
                one,
                probability,
            } => {
                let p = engine.project(&mut state, order[*qubit], *one)?;
                max_projection_gap = max_projection_gap.max((p - probability).abs());
            }
        }
    }
    // The engine leaves the last fused pair unsplit until a read; this read
    // splits it, so the time and the split statistics include that split.
    black_box(engine.retained_weight(&state)?);
    Ok(EngineRun {
        state,
        seconds: started.elapsed().as_secs_f64(),
        max_projection_gap,
    })
}

/// The sparse simulator's run of a circuit: its state, the seconds it took
/// and the largest gap between a projection's probability and the
/// recorded one.
pub struct SparseRun {
    pub sim: SparseStateSim,
    pub seconds: f64,
    pub max_projection_gap: f64,
}

/// Runs `circuit` on the sparse simulator, through its native operations,
/// and reads it once so that no gate stays queued.
pub fn run_sparse(circuit: &Circuit) -> Res<SparseRun> {
    let mut sim = SparseStateSim::new(None);
    for _ in 0..circuit.qubits {
        let _ = sim.allocate();
    }
    let mut max_projection_gap: f64 = 0.0;
    let started = Instant::now();
    for op in &circuit.ops {
        match op {
            Op::Gate { natives, .. } => {
                for native in natives.as_ref().ok_or("a gate with no native form")? {
                    native.on(&mut sim);
                }
            }
            Op::Project {
                qubit,
                one,
                probability,
            } => {
                let p = sim.force_collapse(*one, *qubit);
                max_projection_gap = max_projection_gap.max((p - probability).abs());
            }
        }
    }
    // The simulator queues some gates until a read; this read applies them.
    let every: Vec<usize> = (0..circuit.qubits).collect();
    black_box(sim.joint_probability(&every));
    Ok(SparseRun {
        sim,
        seconds: started.elapsed().as_secs_f64(),
        max_projection_gap,
    })
}

/// A state vector, qubit `q` as bit `q` of the index.
pub struct Dense {
    pub psi: Vec<C>,
}

impl Dense {
    /// `circuit`'s exact state, from `|0…0⟩`.
    pub fn run(circuit: &Circuit) -> Self {
        let mut psi = vec![c(0.0, 0.0); 1 << circuit.qubits];
        psi[0] = c(1.0, 0.0);
        let mut dense = Self { psi };
        for op in &circuit.ops {
            match op {
                Op::Gate { qubits, matrix, .. } => dense.apply(qubits, matrix),
                Op::Project { .. } => unimplemented!("a unitary circuit"),
            }
        }
        dense
    }

    /// The sparse simulator's state, normalized.
    pub fn of_sparse(sim: &mut SparseStateSim) -> Self {
        let (entries, qubits) = sim.get_state();
        let mut psi = vec![c(0.0, 0.0); 1 << qubits];
        for (index, amplitude) in entries {
            let index = index.iter_u64_digits().next().unwrap_or(0);
            psi[usize::try_from(index).expect("a small index")] = amplitude;
        }
        let norm = psi.iter().map(C::norm_sqr).sum::<f64>().sqrt();
        for x in &mut psi {
            *x /= norm;
        }
        Self { psi }
    }

    fn apply(&mut self, qubits: &[usize], m: &[C]) {
        match *qubits {
            [q] => {
                let s = 1 << q;
                for i in (0..self.psi.len()).filter(|i| i & s == 0) {
                    let (v0, v1) = (self.psi[i], self.psi[i | s]);
                    self.psi[i] = m[0] * v0 + m[1] * v1;
                    self.psi[i | s] = m[2] * v0 + m[3] * v1;
                }
            }
            [a, b] => {
                let (sa, sb) = (1 << a, 1 << b);
                for i in (0..self.psi.len()).filter(|i| i & (sa | sb) == 0) {
                    let index = [i, i | sb, i | sa, i | sa | sb];
                    let v = index.map(|k| self.psi[k]);
                    for (row, &k) in index.iter().enumerate() {
                        self.psi[k] = (0..4).map(|col| m[4 * row + col] * v[col]).sum();
                    }
                }
            }
            _ => unimplemented!("gates on one or two qubits"),
        }
    }

    /// `⟨ψ|P|ψ⟩` for the Pauli string `factors` on qubits.
    pub fn pauli_expectation(&self, factors: &[(usize, Pauli)]) -> f64 {
        let mut copy = Self {
            psi: self.psi.clone(),
        };
        for &(q, p) in factors {
            let m = match p {
                Pauli::X => pauli_x(),
                Pauli::Y => pauli_y(),
                Pauli::Z => pauli_z(),
            };
            copy.apply(&[q], &m);
        }
        self.psi
            .iter()
            .zip(&copy.psi)
            .map(|(a, b)| (a.conj() * b).re)
            .sum()
    }

    pub fn probability_of_one(&self, q: usize) -> f64 {
        self.psi
            .iter()
            .enumerate()
            .filter(|(i, _)| i >> q & 1 == 1)
            .map(|(_, v)| v.norm_sqr())
            .sum()
    }
}

/// How far the engine's state is from an exact one: the largest gap in a
/// qubit's P(One) and in a random Pauli string's expectation, and the total
/// variation distance of the whole distribution, estimated from samples.
pub struct Agreement {
    pub max_probability_gap: f64,
    pub max_pauli_gap: f64,
    pub sampled_tvd: f64,
}

/// Compares `state` (qubit `q` on site `order[q]`) with `exact`, on every
/// qubit's P(One), `strings` random Pauli strings of weight 1 to 4, and the
/// TVD from `samples` outcomes drawn from `exact`.
pub fn agreement(
    engine: &CpuMpsEngine,
    state: &CanonicalMps,
    order: &[usize],
    exact: &Dense,
    strings: usize,
    samples: usize,
) -> Res<Agreement> {
    let n = order.len();
    let mut rng = Rng::new(0x00C0_FFEE);
    let mut max_probability_gap: f64 = 0.0;
    for (q, &site) in order.iter().enumerate() {
        let p = engine.probability_of_one(state, site)?;
        max_probability_gap = max_probability_gap.max((p - exact.probability_of_one(q)).abs());
    }
    let mut max_pauli_gap: f64 = 0.0;
    for _ in 0..strings {
        let weight = 1 + rng.below(4.min(n));
        let mut qubits: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut qubits);
        let factors: Vec<(usize, Pauli)> = qubits[..weight]
            .iter()
            .map(|&q| (q, [Pauli::X, Pauli::Y, Pauli::Z][rng.below(3)]))
            .collect();
        let on_sites = factors.iter().map(|&(q, p)| (order[q], p)).collect();
        let observable = PauliSum::new(c(0.0, 0.0), vec![PauliTerm::new(c(1.0, 0.0), on_sites)?])?;
        let value = engine.expectation(state, &observable)?.re;
        max_pauli_gap = max_pauli_gap.max((value - exact.pauli_expectation(&factors)).abs());
    }
    let sampled_tvd = sampled_tvd(engine, state, order, exact, samples, &mut rng)?;
    Ok(Agreement {
        max_probability_gap,
        max_pauli_gap,
        sampled_tvd,
    })
}

/// The TVD of the whole distribution, estimated from outcomes `x` drawn
/// from the exact `p`: with `q(x)` the engine's probability (projections on
/// a fork), `Σ|p − q| = E_p|1 − q/p| + (1 − E_p[q/p])`, the second term
/// being `q`'s weight outside `p`'s support. NaN for no samples.
#[expect(clippy::cast_precision_loss, reason = "a sample count")]
fn sampled_tvd(
    engine: &CpuMpsEngine,
    state: &CanonicalMps,
    order: &[usize],
    exact: &Dense,
    samples: usize,
    rng: &mut Rng,
) -> Res<f64> {
    if samples == 0 {
        return Ok(f64::NAN);
    }
    let mut cumulative = Vec::with_capacity(exact.psi.len());
    let mut total = 0.0;
    for v in &exact.psi {
        total += v.norm_sqr();
        cumulative.push(total);
    }
    let (mut deviation, mut ratio) = (0.0, 0.0);
    for _ in 0..samples {
        let u = rng.uniform() * total;
        let x = cumulative
            .partition_point(|&c| c <= u)
            .min(exact.psi.len() - 1);
        let p = exact.psi[x].norm_sqr() / total;
        let mut fork = engine.fork(state)?;
        let mut q = 1.0;
        for (qubit, &site) in order.iter().enumerate() {
            let one = x >> qubit & 1 == 1;
            let p_one = engine.probability_of_one(&fork, site)?;
            let value = if one { p_one } else { 1.0 - p_one };
            if value <= 1e-15 {
                q = 0.0;
                break;
            }
            q *= value;
            engine.project(&mut fork, site, one)?;
        }
        deviation += (1.0 - q / p).abs();
        ratio += q / p;
    }
    let n = samples as f64;
    Ok(0.5 * (deviation / n + (1.0 - ratio / n).max(0.0)))
}

/// The distribution of `sites` read together, outcome `x` with `sites[0]`
/// as its most significant bit, from the expectations of every Z string.
pub fn readout_distribution(
    engine: &CpuMpsEngine,
    state: &CanonicalMps,
    sites: &[usize],
) -> Res<Vec<f64>> {
    let k = sites.len();
    let mut z = vec![0.0; 1 << k];
    z[0] = 1.0;
    for (subset, slot) in z.iter_mut().enumerate().skip(1) {
        let factors: Vec<(usize, Pauli)> = (0..k)
            .filter(|bit| subset >> (k - 1 - bit) & 1 == 1)
            .map(|bit| (sites[bit], Pauli::Z))
            .collect();
        let observable = PauliSum::new(c(0.0, 0.0), vec![PauliTerm::new(c(1.0, 0.0), factors)?])?;
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

/// This process's peak resident memory, in kB (Linux only; 0 elsewhere).
pub fn peak_memory_kb() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kb| kb.parse().ok())
        })
        .unwrap_or(0)
}

/// The value of `--flag value` in `args`.
pub fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The order `--order a,b,c,…` (the site of each qubit; brackets and
/// spaces allowed), else the identity.
pub fn order(args: &[String], qubits: usize) -> Res<Vec<usize>> {
    let Some(text) = flag(args, "--order") else {
        return Ok((0..qubits).collect());
    };
    let order: Vec<usize> = text
        .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
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
