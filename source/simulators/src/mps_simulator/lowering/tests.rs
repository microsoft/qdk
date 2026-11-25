// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering's rules ([`lower`]): a batch's calls become steps, in order,
//! on the sites the qubit order gives their qubits; a gate's step is its
//! matrix at its angle, sign included; a measurement reads the qubit in
//! its basis and leaves it in the state it read; and a call the MPS
//! simulation method refuses fails, naming what it refuses. Each test
//! lowers calls with the qubits placed by [`placed`] and checks the steps.

use std::convert::Infallible;
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2};

use num_complex::Complex64;
use qsc_data_structures::measurement::MeasurementBasis;

use crate::mps_simulator::MpsResumerError;
use crate::mps_simulator::lowering::{SiteGate, SiteStep, lower};
use crate::mps_simulator::qubit_order::QubitOrder;
use crate::qir::{OperatorMatrix, QirCall, QirGate, QirInstruction};

/// A batch's calls become steps, in order, each on the sites the qubit
/// order gives its qubits. With qubits 0 to 3 on sites 2, 0, 3 and 1
/// ([`placed`]), each row is the calls and their steps. A two-qubit gate
/// keeps its operand order, so `cx(0, 3)` acts on sites 2 then 1, and
/// `cx(3, 0)` on sites 1 then 2; a `move` changes no state, so it has no
/// step.
#[test]
fn each_call_acts_on_the_sites_the_order_gives_its_qubits() {
    let (h, cx) = (&QirGate::H.matrix_at(None), &QirGate::Cx.matrix_at(None));
    let rows = [
        ("a gate", vec![one(QirGate::H, 1)], vec![gate_on(h, &[0])]),
        (
            "a two-qubit gate, in its operand order",
            vec![two(QirGate::Cx, 0, 3), two(QirGate::Cx, 3, 0)],
            vec![gate_on(cx, &[2, 1]), gate_on(cx, &[1, 2])],
        ),
        (
            "a measurement",
            vec![measure(MeasurementBasis::Z, 2, 5)],
            vec![SiteStep::Measure {
                site: 3,
                result: 5,
                reset: false,
            }],
        ),
        (
            "a measure-reset",
            vec![QirCall::MeasureReset {
                qubit: 3,
                result: 1,
            }],
            vec![SiteStep::Measure {
                site: 1,
                result: 1,
                reset: true,
            }],
        ),
        (
            "a reset",
            vec![QirCall::Reset { qubit: 0 }],
            vec![SiteStep::Reset { site: 2 }],
        ),
        ("a move", vec![QirCall::Move { qubit: 2 }], vec![]),
        (
            "a batch's calls, in order",
            vec![
                QirCall::Reset { qubit: 0 },
                QirCall::Move { qubit: 2 },
                one(QirGate::H, 1),
            ],
            vec![SiteStep::Reset { site: 2 }, gate_on(h, &[0])],
        ),
    ];
    for (label, calls, steps) in rows {
        assert_eq!(lowered(&calls), steps, "{label}");
    }
}

/// A gate's step is its matrix at its angle ([`QirGate::matrix_at`]): the
/// angle is passed on unchanged, sign included. Which way an angle turns
/// is QIR's rule, tested in `qir/gate/tests.rs`. Each row is `ry` on
/// qubit 0, on site 2, by π/2 or −π/2, whose matrices differ.
#[test]
fn a_gates_step_is_its_matrix_at_its_angle_sign_included() {
    for angle in [FRAC_PI_2, -FRAC_PI_2] {
        let ry = QirCall::Gate {
            gate: QirGate::Ry,
            qubits: (0, None),
            angle: Some(angle),
        };
        assert_eq!(
            lowered(&[ry]),
            [gate_on(&QirGate::Ry.matrix_at(Some(angle)), &[2])],
            "ry by {angle}"
        );
    }
}

/// A measurement reads the qubit in its basis: the gates before its draw
/// in Z turn each state of the basis into the Z state of the value it
/// reads, up to a phase. So `mx` reads |+⟩ as 0 and |−⟩ as 1, and `my`
/// reads |+i⟩ as 0 and |−i⟩ as 1. Each row is a state and the value its
/// basis's measurement reads ([`basis_states`]).
#[test]
fn a_measurement_reads_the_qubit_in_its_basis() {
    for (label, basis, state, value) in basis_states() {
        let (before, _) = around_its_draw(basis);
        assert!(
            same_up_to_phase(applied(&before, state), z_state(value)),
            "{label}"
        );
    }
}

/// A measurement leaves the qubit in the state it read: the gates after
/// its draw in Z turn the Z state the draw left back into the basis's
/// state of that value, up to a phase. So after `mx` reads 0, the qubit is
/// at |+⟩. Each row is a state and the value its basis's measurement reads
/// ([`basis_states`]).
#[test]
fn a_measurement_leaves_the_state_it_read() {
    for (label, basis, state, value) in basis_states() {
        let (_, after) = around_its_draw(basis);
        assert!(
            same_up_to_phase(applied(&after, z_state(value)), state),
            "{label}"
        );
    }
}

/// A call the MPS simulation method refuses fails, naming what it
/// refuses: the instruction, for a call the method does not run, and the
/// gate and the qubit, for a gate on one qubit twice. Each row is the
/// calls after an `h`, which the method runs: readout noise, correlated
/// noise, a loss check, `cx` on qubit 1 twice, and readout noise then a
/// loss check, which fails on the first.
#[test]
fn a_refused_call_fails_naming_what_it_refuses() {
    let readout_noise = QirCall::ReadoutNoise {
        zero_as_one: 0.1,
        one_as_zero: 0.1,
        result: 0,
    };
    let peek_loss = QirCall::PeekLoss {
        qubit: 0,
        result: 0,
    };
    let unsupported = |instruction| Refusal {
        instruction,
        qubit: None,
    };
    let rows = [
        (
            "readout noise",
            vec![readout_noise.clone()],
            unsupported(QirInstruction::ReadoutNoise),
        ),
        (
            "correlated noise",
            vec![QirCall::CorrelatedNoise {
                qubits: [0].into_iter().collect(),
                table: 0,
            }],
            unsupported(QirInstruction::CorrelatedNoise),
        ),
        (
            "a loss check",
            vec![peek_loss.clone()],
            unsupported(QirInstruction::PeekLoss),
        ),
        (
            "a gate on one qubit twice",
            vec![two(QirGate::Cx, 1, 1)],
            Refusal {
                instruction: QirInstruction::Gate(QirGate::Cx),
                qubit: Some(1),
            },
        ),
        (
            "two refused calls",
            vec![readout_noise, peek_loss],
            unsupported(QirInstruction::ReadoutNoise),
        ),
    ];
    for (label, refused, refusal) in rows {
        let calls = [&[one(QirGate::H, 0)][..], &refused].concat();
        let error = lower::<Infallible>(&calls, &placed()).expect_err(label);
        assert_eq!(Refusal::of(&error), Some(refusal), "{label}: {error:?}");
    }
}

/// Qubits 0 to 3 on sites 2, 0, 3 and 1: no qubit on the site of its own
/// index, so a step on a qubit's index, not its site, shows.
fn placed() -> QubitOrder {
    QubitOrder::from_sites(vec![2, 0, 3, 1])
}

/// The steps of `calls`, with the qubits [`placed`].
fn lowered(calls: &[QirCall]) -> Vec<SiteStep> {
    lower::<Infallible>(calls, &placed()).expect("the method runs every call")
}

/// The call of the one-qubit `gate` on `qubit`.
fn one(gate: QirGate, qubit: u32) -> QirCall {
    QirCall::Gate {
        gate,
        qubits: (qubit, None),
        angle: None,
    }
}

/// The call of the two-qubit `gate` on `first` and `second`, in that
/// order.
fn two(gate: QirGate, first: u32, second: u32) -> QirCall {
    QirCall::Gate {
        gate,
        qubits: (first, Some(second)),
        angle: None,
    }
}

/// The call measuring `qubit` in `basis` into `result`.
fn measure(basis: MeasurementBasis, qubit: u32, result: u32) -> QirCall {
    QirCall::Measure {
        basis,
        qubit,
        result,
    }
}

/// The step applying `matrix` on `sites`.
fn gate_on(matrix: &OperatorMatrix, sites: &[usize]) -> SiteStep {
    SiteStep::Apply(SiteGate::new(matrix, sites))
}

/// What a lowering fails with when it reaches a refused call: the
/// instruction it names, and the qubit it names, for a gate on one qubit
/// twice.
#[derive(Debug, PartialEq)]
struct Refusal {
    /// The instruction refused.
    instruction: QirInstruction,
    /// The qubit named twice; `None` for a call the method does not run.
    qubit: Option<u32>,
}

impl Refusal {
    /// The refusal `error` is; `None` when it is none.
    fn of<E>(error: &MpsResumerError<E>) -> Option<Self> {
        match *error {
            MpsResumerError::Unsupported { instruction } => Some(Self {
                instruction,
                qubit: None,
            }),
            MpsResumerError::RepeatedQubit { instruction, qubit } => Some(Self {
                instruction,
                qubit: Some(qubit),
            }),
            _ => None,
        }
    }
}

/// A qubit's state: its amplitudes on |0⟩ and |1⟩.
type State = [Complex64; 2];

/// A one-qubit operator, row-major `M[out][in]`, as
/// [`OperatorMatrix::One`] holds it.
type Operator = [Complex64; 4];

/// Each state of each basis, and the value a measurement in its basis
/// reads it as.
fn basis_states() -> [(&'static str, MeasurementBasis, State, u8); 6] {
    let r = Complex64::new(FRAC_1_SQRT_2, 0.0);
    let i_r = Complex64::new(0.0, FRAC_1_SQRT_2);
    [
        ("|0⟩ by mz", MeasurementBasis::Z, z_state(0), 0),
        ("|1⟩ by mz", MeasurementBasis::Z, z_state(1), 1),
        ("|+⟩ by mx", MeasurementBasis::X, [r, r], 0),
        ("|−⟩ by mx", MeasurementBasis::X, [r, -r], 1),
        ("|+i⟩ by my", MeasurementBasis::Y, [r, i_r], 0),
        ("|−i⟩ by my", MeasurementBasis::Y, [r, -i_r], 1),
    ]
}

/// The steps of a measurement of qubit 0, on site 2, in `basis`, around
/// its draw: the operator of the gates before it, and of those after it,
/// each in the order they run. Checks that there is one draw, a
/// measurement of site 2 into its result, and that every gate acts on
/// site 2.
fn around_its_draw(basis: MeasurementBasis) -> (Operator, Operator) {
    let (zero, one) = (Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0));
    let identity = [one, zero, zero, one];
    let (mut before, mut after, mut draws) = (identity, identity, 0);
    for step in lowered(&[measure(basis, 0, 4)]) {
        match step {
            SiteStep::Apply(gate) => {
                assert_eq!(gate.sites(), [2], "{basis:?}: a gate on the qubit's site");
                let OperatorMatrix::One(matrix) = gate.matrix else {
                    panic!("{basis:?}: a one-qubit gate");
                };
                let operator = if draws == 0 { &mut before } else { &mut after };
                *operator = product(&matrix, operator);
            }
            SiteStep::Measure {
                site: 2,
                result: 4,
                reset: false,
            } => draws += 1,
            other => panic!("{basis:?}: a step that is no draw of the qubit: {other:?}"),
        }
    }
    assert_eq!(draws, 1, "{basis:?}: one draw");
    (before, after)
}

/// The Z state of `value`: |0⟩ or |1⟩.
fn z_state(value: u8) -> State {
    let mut state = [Complex64::new(0.0, 0.0); 2];
    state[usize::from(value)] = Complex64::new(1.0, 0.0);
    state
}

/// The operator of `second` run after `first`.
fn product(second: &Operator, first: &Operator) -> Operator {
    let entry = |row: usize, column: usize| {
        second[row * 2] * first[column] + second[row * 2 + 1] * first[2 + column]
    };
    [entry(0, 0), entry(0, 1), entry(1, 0), entry(1, 1)]
}

/// `operator` applied to `state`.
fn applied(operator: &Operator, state: State) -> State {
    [
        operator[0] * state[0] + operator[1] * state[1],
        operator[2] * state[0] + operator[3] * state[1],
    ]
}

/// Whether the unit states `a` and `b` are the same state, up to a
/// global phase and to rounding: their overlap has modulus 1.
fn same_up_to_phase(a: State, b: State) -> bool {
    let overlap = a[0].conj() * b[0] + a[1].conj() * b[1];
    (overlap.norm() - 1.0).abs() < 1e-12
}
