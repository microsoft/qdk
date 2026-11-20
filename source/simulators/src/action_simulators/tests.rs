// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    FullStateActionSimulator, StabilizerActionSimulator, full_state::synthesize, paulis::letter,
};
use paulimer::{Clifford as _, CliffordUnitary, pauli::Pauli as _};
use qodec_runtime_interface::{
    ActionOp, Backend, Generator, Guard,
    pauli::{Clifford, DensePauli, Pauli, PauliOp},
};
use rand::{SeedableRng as _, rngs::StdRng};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

type Engine = Box<dyn Backend<Vec<ActionOp>, Result<Vec<bool>, String>>>;

/// Both simulators, seeded with `seed`, for `qubits` qubits at most.
fn engines(qubits: usize, seed: u32) -> [(&'static str, Engine); 2] {
    [
        (
            "stabilizer",
            Box::new(StabilizerActionSimulator::new(seed.into())),
        ),
        (
            "full state",
            Box::new(FullStateActionSimulator::new(qubits, seed)),
        ),
    ]
}

fn p(text: &str) -> Pauli {
    Pauli::parse(text).expect("a Pauli")
}

/// The `clifford` step of `gate`, its qubit `i` on `targets[i]`.
fn clifford(gate: &Clifford, targets: &[usize]) -> ActionOp {
    let place = |d: &DensePauli| {
        let local = signed(d);
        let terms = local
            .terms()
            .iter()
            .map(|&(q, op)| (targets[q], op))
            .collect();
        Pauli::new(local.negative, terms).expect("a Pauli")
    };
    let images = targets
        .iter()
        .zip(gate.x_images.iter().zip(&gate.z_images))
        .flat_map(|(&q, (x, z))| [(Generator::X(q), place(x)), (Generator::Z(q), place(z))])
        .collect();
    ActionOp::Clifford {
        images,
        guard: None,
    }
}

fn named(name: &str, targets: &[usize]) -> ActionOp {
    clifford(
        &Clifford::named(name, targets.len()).expect("a named Clifford"),
        targets,
    )
}

fn rotate(pauli: &str, angle: f64) -> ActionOp {
    ActionOp::Rotate {
        pauli: p(pauli),
        angle,
        guard: None,
    }
}

fn observe(paulis: &[&str]) -> ActionOp {
    ActionOp::Observe(paulis.iter().map(|t| p(t)).collect())
}

fn allocate(n: usize) -> Vec<ActionOp> {
    (0..n).map(ActionOp::Allocate).collect()
}

/// `ops`, run by each simulator with seeds `0..seeds`: the outcomes must be
/// `expected` every time.
fn always(qubits: usize, seeds: u32, ops: &[ActionOp], expected: &[bool]) {
    for seed in 0..seeds {
        for (name, mut engine) in engines(qubits, seed) {
            assert_eq!(
                engine.execute(ops.to_vec()).as_deref(),
                Ok(expected),
                "{name}, seed {seed}"
            );
        }
    }
}

// ── Cliffords ───────────────────────────────────────────────────────────────

#[test]
fn cliffords_decompose_into_gates() {
    let mut rng = StdRng::seed_from_u64(7);
    for n in 1..=5 {
        for _ in 0..40 {
            let c = CliffordUnitary::random(n, &mut rng);
            let mut product = CliffordUnitary::identity(n);
            for g in synthesize(&c) {
                g.on(&mut product);
            }
            assert_eq!(product, c);
        }
    }
}

/// `c` as the runtime's Clifford.
fn interface(c: &CliffordUnitary) -> Clifford {
    let n = c.num_qubits();
    let dense = |p: &paulimer::DensePauli| DensePauli {
        x: (0..n).map(|q| letter(p, q).0).collect(),
        z: (0..n).map(|q| letter(p, q).1).collect(),
        phase: p.xz_phase_exponent(),
    };
    Clifford {
        x_images: (0..n).map(|q| dense(&c.image_x(q))).collect(),
        z_images: (0..n).map(|q| dense(&c.image_z(q))).collect(),
    }
}

/// The Hermitian `p`, `i^phase X^x Z^z`, as a signed Pauli.
fn signed(p: &DensePauli) -> Pauli {
    let terms: Vec<(usize, PauliOp)> = (0..p.width())
        .filter_map(|q| match (p.x[q], p.z[q]) {
            (true, false) => Some((q, PauliOp::X)),
            (true, true) => Some((q, PauliOp::Y)),
            (false, true) => Some((q, PauliOp::Z)),
            (false, false) => None,
        })
        .collect();
    let ys = terms.iter().filter(|(_, o)| *o == PauliOp::Y).count();
    let negative = (usize::from(p.phase) + 4 - ys % 4) % 4 == 2;
    Pauli::new(negative, terms).expect("a Pauli")
}

#[test]
fn both_simulators_prepare_the_stabilizer_states_of_cliffords() {
    // `C|0…0⟩` is stabilized by every `C Z_q C†`.
    let mut rng = StdRng::seed_from_u64(11);
    let n = 4;
    for round in 0..30 {
        let c = interface(&CliffordUnitary::random(n, &mut rng));
        let mut ops = allocate(n);
        ops.push(clifford(&c, &(0..n).collect::<Vec<_>>()));
        ops.push(ActionOp::Observe(c.z_images.iter().map(signed).collect()));
        for (name, mut engine) in engines(n, round) {
            assert_eq!(
                engine.execute(ops.clone()),
                Ok(vec![false; n]),
                "{name}, round {round}"
            );
        }
    }
}

#[test]
fn a_bell_pair_measures_alike_and_randomly() {
    let ops = [
        allocate(2),
        vec![
            named("h", &[0]),
            named("cx", &[0, 1]),
            observe(&["X_0 X_1", "Z_0", "Z_1"]),
        ],
    ]
    .concat();
    let mut seen = [false; 2];
    for seed in 0..20 {
        for (name, mut engine) in engines(2, seed) {
            let out = engine.execute(ops.clone()).expect("runs");
            assert!(!out[0], "{name}");
            assert_eq!(out[1], out[2], "{name}");
            seen[usize::from(out[1])] = true;
        }
    }
    assert_eq!(seen, [true, true]);
}

// ── Steps ───────────────────────────────────────────────────────────────────

#[test]
fn rotations_turn_the_right_way() {
    // `exp(-iπ/4 Z)|+⟩ = |+i⟩`, and the other way about `-Z`.
    let plus = [allocate(1), vec![named("h", &[0])]].concat();
    let then = |ops: Vec<ActionOp>| [plus.clone(), ops].concat();
    always(
        1,
        5,
        &then(vec![rotate("Z_0", FRAC_PI_2), observe(&["Y_0"])]),
        &[false],
    );
    always(
        1,
        5,
        &then(vec![rotate("-Z_0", FRAC_PI_2), observe(&["Y_0"])]),
        &[true],
    );
    always(
        1,
        5,
        &then(vec![rotate("Z_0", 3.0 * FRAC_PI_2), observe(&["Y_0"])]),
        &[true],
    );
    // A half turn about Y flips |0⟩, on two qubits too.
    let half = [
        allocate(2),
        vec![rotate("Y_0 Y_1", PI), observe(&["Z_0", "Z_1"])],
    ]
    .concat();
    always(2, 5, &half, &[true, true]);
    // Only the full state turns by π/4; twice is a quarter turn.
    let eighth = then(vec![rotate("Z_0", FRAC_PI_4)]);
    let err = StabilizerActionSimulator::new(0)
        .execute(eighth.clone())
        .expect_err("not a Clifford");
    assert!(err.contains("only by multiples of π/2"), "{err}");
    let twice = [eighth, vec![rotate("Z_0", FRAC_PI_4), observe(&["Y_0"])]].concat();
    assert_eq!(
        FullStateActionSimulator::new(1, 0).execute(twice),
        Ok(vec![false])
    );
}

#[test]
fn stabilize_prepares_the_state_of_every_pauli() {
    let ghz = ["X_0 X_1 X_2", "Z_0 Z_1", "-Z_1 Z_2"];
    let ops = [
        allocate(3),
        vec![
            named("h", &[1]),
            ActionOp::Stabilize {
                paulis: ghz.iter().map(|t| p(t)).collect(),
                guard: None,
            },
            observe(&ghz),
        ],
    ]
    .concat();
    always(3, 10, &ops, &[false, false, false]);
}

#[test]
fn guards_read_earlier_outcomes() {
    // Measure X on |0⟩, then flip a `-1` back: |+⟩ every time.
    let ops = [
        allocate(1),
        vec![
            observe(&["X_0"]),
            ActionOp::Pauli {
                pauli: p("Z_0"),
                guard: Some(Guard {
                    outcomes: vec![0],
                    invert: false,
                }),
            },
            observe(&["X_0"]),
        ],
    ]
    .concat();
    for seed in 0..10 {
        for (name, mut engine) in engines(1, seed) {
            let out = engine.execute(ops.clone()).expect("runs");
            assert!(!out[1], "{name}, seed {seed}");
        }
    }
}

#[test]
fn released_qubits_come_back_in_zero() {
    let ops = [
        allocate(2),
        vec![
            named("h", &[0]),
            named("cx", &[0, 1]),
            ActionOp::Release(0),
            ActionOp::Allocate(0),
            observe(&["Z_0"]),
        ],
    ]
    .concat();
    always(2, 10, &ops, &[false]);
}

#[test]
fn qubits_must_be_in_use() {
    for (name, mut engine) in engines(2, 0) {
        let err = engine
            .execute(vec![observe(&["Z_1"])])
            .expect_err("not allocated");
        assert!(err.contains("qubit 1 is not in use"), "{name}: {err}");
        let err = engine
            .execute(vec![ActionOp::Allocate(0), ActionOp::Allocate(0)])
            .expect_err("twice");
        assert!(err.contains("already in use"), "{name}: {err}");
    }
    let err = FullStateActionSimulator::new(2, 0)
        .execute(vec![ActionOp::Allocate(2)])
        .expect_err("too many");
    assert!(err.contains("past the 2 qubits"), "{err}");
    // A tableau whose images do not commute as they should is no Clifford.
    let mut broken = Clifford::named("h", 1).expect("h");
    broken.z_images[0] = broken.x_images[0].clone();
    for (name, mut engine) in engines(1, 0) {
        let ops = [allocate(1), vec![clifford(&broken, &[0])]].concat();
        assert!(engine.execute(ops).is_err(), "{name}");
    }
}
