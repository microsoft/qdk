// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use super::*;
use std::f64::consts::PI;

fn almost_equal(a: f64, b: f64) -> bool {
    a.max(b) - b.min(a) <= 1e-10
}

// Test that basic allocation and release of qubits doesn't fail.
#[test]
fn test_alloc_release() {
    let sim = &mut QuantumSim::default();
    for i in 0..16 {
        assert_eq!(sim.allocate(), i);
    }
    sim.release(4);
    sim.release(7);
    sim.release(12);
    assert_eq!(sim.allocate(), 4);
    for i in 0..7 {
        sim.release(i);
    }
    for i in 8..12 {
        sim.release(i);
    }
    for i in 13..16 {
        sim.release(i);
    }
}

/// Verifies that application of gates to a qubit results in the correct probabilities.
#[test]
fn test_probability() {
    let mut sim = QuantumSim::default();
    let q = sim.allocate();
    let extra = sim.allocate();
    assert!(almost_equal(0.0, sim.joint_probability(&[q])));
    sim.x(q);
    assert!(almost_equal(1.0, sim.joint_probability(&[q])));
    sim.x(q);
    assert!(almost_equal(0.0, sim.joint_probability(&[q])));
    sim.h(q);
    assert!(almost_equal(0.5, sim.joint_probability(&[q])));
    sim.h(q);
    assert!(almost_equal(0.0, sim.joint_probability(&[q])));
    sim.x(q);
    sim.h(q);
    sim.s(q);
    assert!(almost_equal(0.5, sim.joint_probability(&[q])));
    sim.sadj(q);
    sim.h(q);
    sim.x(q);
    assert!(almost_equal(0.0, sim.joint_probability(&[q])));
    sim.release(extra);
    sim.release(q);
}

/// Verify that a qubit in superposition has probability corresponding the measured value and
/// can be operationally reset back into the ground state.
#[test]
fn test_measure() {
    let mut sim = QuantumSim::default();
    let q = sim.allocate();
    let extra = sim.allocate();
    assert!(!sim.measure(q));
    sim.x(q);
    assert!(sim.measure(q));
    let mut res = false;
    while !res {
        sim.h(q);
        res = sim.measure(q);
        assert!(almost_equal(
            sim.joint_probability(&[q]),
            if res { 1.0 } else { 0.0 }
        ));
        if res {
            sim.x(q);
        }
    }
    assert!(almost_equal(sim.joint_probability(&[q]), 0.0));
    sim.release(extra);
    sim.release(q);
}

// Verify that out of order release of non-zero qubits behaves as expected, namely qubits that
// are not released are still in the expected states, newly allocated qubits use the available spot
// and start in a zero state.
#[test]
fn test_out_of_order_release() {
    let sim = &mut QuantumSim::default();
    for i in 0..5 {
        assert_eq!(sim.allocate(), i);
        sim.x(i);
    }

    // Release out of order.
    sim.release(3);

    // Remaining qubits should all still be in one.
    assert_eq!(sim.state.len(), 1);
    assert!(!sim.joint_probability(&[0]).is_nearly_zero());
    assert!(!sim.joint_probability(&[1]).is_nearly_zero());
    assert!(!sim.joint_probability(&[2]).is_nearly_zero());
    assert!(!sim.joint_probability(&[4]).is_nearly_zero());

    // Cheat and peak at the released location to make sure it has been zeroed out.
    assert!(sim.check_joint_probability(&[3]).is_nearly_zero());

    // Next allocation should be the empty spot, and it should be in zero state.
    assert_eq!(sim.allocate(), 3);
    assert!(sim.joint_probability(&[3]).is_nearly_zero());

    for i in 0..5 {
        sim.release(i);
    }
    assert_eq!(sim.state.len(), 1);
}

/// Verify joint probability works as expected, namely that it corresponds to the parity of the
/// qubits.
#[test]
fn test_joint_probability() {
    let mut sim = QuantumSim::default();
    let q0 = sim.allocate();
    let q1 = sim.allocate();
    assert!(almost_equal(0.0, sim.joint_probability(&[q0, q1])));
    sim.x(q0);
    assert!(almost_equal(1.0, sim.joint_probability(&[q0, q1])));
    sim.x(q1);
    assert!(almost_equal(0.0, sim.joint_probability(&[q0, q1])));
    assert!(almost_equal(1.0, sim.joint_probability(&[q0])));
    assert!(almost_equal(1.0, sim.joint_probability(&[q1])));
    sim.h(q0);
    assert!(almost_equal(0.5, sim.joint_probability(&[q0, q1])));
    sim.release(q1);
    sim.release(q0);
}

/// Verify joint measurement works as expected, namely that it corresponds to the parity of the
/// qubits.
#[test]
fn test_joint_measurement() {
    let mut sim = QuantumSim::default();
    let q0 = sim.allocate();
    let q1 = sim.allocate();
    assert!(!sim.joint_measure(&[q0, q1]));
    sim.x(q0);
    assert!(sim.joint_measure(&[q0, q1]));
    sim.x(q1);
    assert!(!sim.joint_measure(&[q0, q1]));
    assert!(sim.joint_measure(&[q0]));
    assert!(sim.joint_measure(&[q1]));
    sim.h(q0);
    let res = sim.joint_measure(&[q0, q1]);
    assert!(almost_equal(
        if res { 1.0 } else { 0.0 },
        sim.joint_probability(&[q0, q1])
    ));
    sim.release(q1);
    sim.release(q0);
}

#[test]
fn test_force_collapse() {
    let mut sim = QuantumSim::default();
    let q0 = sim.allocate();
    let q1 = sim.allocate();
    sim.h(q0);
    sim.mcx(&[q0], q1);
    assert!(almost_equal(0.5, sim.joint_probability(&[q0])));
    assert!(almost_equal(0.5, sim.joint_probability(&[q1])));
    sim.force_collapse(false, q0);
    assert!(almost_equal(0.0, sim.joint_probability(&[q0])));
    assert!(almost_equal(0.0, sim.joint_probability(&[q1])));
    sim.release(q1);
    sim.release(q0);
}

#[test]
fn test_force_collapse_to_non_existent_state() {
    let mut sim = QuantumSim::default();
    let q0 = sim.allocate();
    sim.x(q0);
    assert!(almost_equal(1.0, sim.joint_probability(&[q0])));
    assert!(almost_equal(0.0, sim.force_collapse(false, q0)));
    // The qubit should still be in the |1> state since the requested collapse state was not present, so the probability of measuring |1> should still be 1.
    assert!(almost_equal(1.0, sim.joint_probability(&[q0])));
    sim.release(q0);
}

/// Test multiple controls.
#[test]
fn test_multiple_controls() {
    let mut sim = QuantumSim::default();
    let q0 = sim.allocate();
    let q1 = sim.allocate();
    let q2 = sim.allocate();
    assert!(almost_equal(0.0, sim.joint_probability(&[q0])));
    sim.h(q0);
    assert!(almost_equal(0.5, sim.joint_probability(&[q0])));
    sim.h(q0);
    assert!(almost_equal(0.0, sim.joint_probability(&[q0])));
    sim.mch(&[q1], q0);
    assert!(almost_equal(0.0, sim.joint_probability(&[q0])));
    sim.x(q1);
    sim.mch(&[q1], q0);
    assert!(almost_equal(0.5, sim.joint_probability(&[q0])));
    sim.mch(&[q2, q1], q0);
    assert!(almost_equal(0.5, sim.joint_probability(&[q0])));
    sim.x(q2);
    sim.mch(&[q2, q1], q0);
    assert!(almost_equal(0.0, sim.joint_probability(&[q0])));
    sim.x(q0);
    sim.x(q1);
    sim.release(q2);
    sim.release(q1);
    sim.release(q0);
}

/// Verify that targets cannot be duplicated.
#[test]
#[should_panic(expected = "Duplicate qubit id '0' found in application.")]
fn test_duplicate_target() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    sim.mcx(&[q], q);
    let _ = sim.dump();
}

/// Verify that controls cannot be duplicated.
#[test]
#[should_panic(expected = "Duplicate qubit id '1' found in application.")]
fn test_duplicate_control() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    let c = sim.allocate();
    sim.mcx(&[c, c], q);
    let _ = sim.dump();
}

/// Verify that targets aren't in controls.
#[test]
#[should_panic(expected = "Duplicate qubit id '0' found in application.")]
fn test_target_in_control() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    let c = sim.allocate();
    sim.mcx(&[c, q], q);
    let _ = sim.dump();
}

/// Large, entangled state handling.
#[test]
fn test_large_state() {
    let mut sim = QuantumSim::new(None);
    let ctl = sim.allocate();
    sim.h(ctl);
    for _ in 0..4999 {
        let q = sim.allocate();
        sim.mcx(&[ctl], q);
    }
    let _ = sim.measure(ctl);
    for i in 0..5000 {
        sim.release(i);
    }
}

/// Verify seeded RNG is predictable.
#[test]
fn test_seeded_rng() {
    let mut sim = QuantumSim::new(None);
    sim.set_rng_seed(42);
    let q = sim.allocate();
    let mut val1 = 0_u64;
    for i in 0..64 {
        sim.h(q);
        if sim.measure(q) {
            val1 += 1 << i;
        }
    }
    let mut sim = QuantumSim::new(None);
    sim.set_rng_seed(42);
    let q = sim.allocate();
    let mut val2 = 0_u64;
    for i in 0..64 {
        sim.h(q);
        if sim.measure(q) {
            val2 += 1 << i;
        }
    }
    assert_eq!(val1, val2);
}

/// Verify that dump after swap on released qubits doesn't crash.
#[test]
fn test_swap_dump() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    let inner_q = sim.allocate();
    sim.swap_qubit_ids(q, inner_q);
    sim.release(inner_q);
    println!("{}", sim.dump());
}

/// Verify that swap preserves queued rotations.
#[test]
fn test_swap_rotations() {
    let mut sim = QuantumSim::new(None);
    let (q1, q2) = (sim.allocate(), sim.allocate());
    sim.rx(PI / 7.0, q1);
    sim.ry(PI / 7.0, q2);
    sim.swap_qubit_ids(q1, q2);
    sim.rx(-PI / 7.0, q2);
    sim.ry(-PI / 7.0, q1);
    assert!(sim.joint_probability(&[q1]).is_nearly_zero());
    assert!(sim.joint_probability(&[q2]).is_nearly_zero());
}

/// Verify that two queued Rx rotations that sum to zero are treated as
/// a no-op.
#[test]
fn test_rx_queue_nearly_zero() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    sim.rx(PI / 4.0, q);
    assert_eq!(sim.state.len(), 1);
    sim.rx(-PI / 4.0, q);
    assert_eq!(sim.state.len(), 1);
    assert!(sim.joint_probability(&[q]).is_nearly_zero());
}

/// Verify that two queued Ry rotations that sum to zero are treated as
/// a no-op.
#[test]
fn test_ry_queue_nearly_zero() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    sim.ry(PI / 4.0, q);
    assert_eq!(sim.state.len(), 1);
    sim.ry(-PI / 4.0, q);
    assert_eq!(sim.state.len(), 1);
    assert!(sim.joint_probability(&[q]).is_nearly_zero());
}

/// Verifies that an Rx rotation by PI, which becomes an X gate, is correctly flushed.
#[test]
fn test_rx_pi_flushed() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    sim.rx(PI, q);
    assert!(almost_equal(
        sim.joint_probability(&[q]),
        sim.joint_probability(&[q])
    ));
    assert!(!sim.joint_probability(&[q]).is_nearly_zero());
}

/// Verifies that an Ry rotation by PI, which becomes an Y gate, is correctly flushed.
#[test]
fn test_ry_pi_flushed() {
    let mut sim = QuantumSim::new(None);
    let q = sim.allocate();
    sim.ry(PI, q);
    assert!(almost_equal(
        sim.joint_probability(&[q]),
        sim.joint_probability(&[q])
    ));
    assert!(!sim.joint_probability(&[q]).is_nearly_zero());
}

/// Verifies that when a controlled Ry(PI) is recognized as equivalent to a
/// controlled -iY (and handed as such), the state vector is not corrupted
#[test]
fn test_mcry_pi() {
    let mut sim = QuantumSim::new(None);
    let q1 = sim.allocate();
    let q2 = sim.allocate();
    sim.h(q1);
    sim.x(q1);
    sim.mcry(&[q1], PI, q2);
    sim.x(q1);
    // Expected result is an equal superposition of |01⟩ and |10⟩
    assert!(almost_equal(sim.joint_probability(&[q1, q2]), 1.0));
}

/// Verifies that when a controlled Ry(2*PI) is recognized as equivalent to a
/// controlled -I (and handed as such), the state vector is not corrupted
#[test]
fn test_mcry_2pi() {
    let mut sim = QuantumSim::new(None);
    let q1 = sim.allocate();
    let q2 = sim.allocate();
    sim.h(q1);
    sim.mcry(&[q1], 2.0 * PI, q2);
    sim.h(q1);
    // Expected result is |10⟩ because CRy(2pi) = Z ⊗ I, so conjugating
    // with Hadamards on the left makes it equivalent to a bit flip X ⊗ I
    assert!(almost_equal(sim.joint_probability(&[q1, q2]), 1.0));
}

/// Utility for testing operation equivalence.
fn assert_operation_equal_referenced<F1, F2>(mut op: F1, mut reference: F2, count: usize)
where
    F1: FnMut(&mut QuantumSim, &[usize]),
    F2: FnMut(&mut QuantumSim, &[usize]),
{
    enum QueuedOp {
        NoOp,
        H,
        Rx,
        Ry,
    }

    for inner_op in [QueuedOp::NoOp, QueuedOp::H, QueuedOp::Rx, QueuedOp::Ry] {
        let mut sim = QuantumSim::default();

        // Allocte the control we use to verify behavior.
        let ctl = sim.allocate();
        sim.h(ctl);

        // Allocate the requested number of targets, entangling the control with them.
        let mut qs = vec![];
        for _ in 0..count {
            let q = sim.allocate();
            sim.mcx(&[ctl], q);
            qs.push(q);
        }

        // To test queuing, try the op after running each of the different intermediate operations that
        // can be queued.
        match inner_op {
            QueuedOp::NoOp => (),
            QueuedOp::H => {
                for &q in &qs {
                    sim.h(q);
                }
            }
            QueuedOp::Rx => {
                for &q in &qs {
                    sim.rx(PI / 7.0, q);
                }
            }
            QueuedOp::Ry => {
                for &q in &qs {
                    sim.ry(PI / 7.0, q);
                }
            }
        }

        op(&mut sim, &qs);

        // Trigger a flush between the op and expected adjoint reference to ensure the reference is
        // run without any queued, commuted operations.
        let _ = sim.joint_probability(&qs);

        reference(&mut sim, &qs);

        // Perform the adjoint of any additional ops. We check the joint probability of the target
        // qubits before and after to force a flush of the operation queue. This helps us verify queuing, as the
        // original operation will have used the queue and commuting while the adjoint perform here will not.
        let _ = sim.joint_probability(&qs);
        match inner_op {
            QueuedOp::NoOp => (),
            QueuedOp::H => {
                for &q in &qs {
                    sim.h(q);
                }
            }
            QueuedOp::Rx => {
                for &q in &qs {
                    sim.rx(PI / -7.0, q);
                }
            }
            QueuedOp::Ry => {
                for &q in &qs {
                    sim.ry(PI / -7.0, q);
                }
            }
        }
        let _ = sim.joint_probability(&qs);

        // Undo the entanglement.
        for q in &qs {
            sim.mcx(&[ctl], *q);
        }
        sim.h(ctl);

        // We know the operations are equal if the qubits are left in the zero state.
        assert!(sim.joint_probability(&[ctl]).is_nearly_zero());
        for q in qs {
            assert!(sim.joint_probability(&[q]).is_nearly_zero());
        }

        // Sparse state vector should have one entry for |0⟩.
        // Dump the state first to force a flush of any queued operations.
        println!("{}", sim.dump());
        assert_eq!(sim.state.len(), 1);
    }
}

#[test]
fn test_h() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.h(qs[0]);
        },
        |sim, qs| {
            sim.h(qs[0]);
        },
        1,
    );
}

#[test]
fn test_x() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.x(qs[0]);
        },
        |sim, qs| {
            sim.x(qs[0]);
        },
        1,
    );
}

#[test]
fn test_y() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.y(qs[0]);
        },
        |sim, qs| {
            sim.y(qs[0]);
        },
        1,
    );
}

#[test]
fn test_z() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.z(qs[0]);
        },
        |sim, qs| {
            sim.z(qs[0]);
        },
        1,
    );
}

#[test]
fn test_s() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.s(qs[0]);
        },
        |sim, qs| {
            sim.sadj(qs[0]);
        },
        1,
    );
}

#[test]
fn test_sadj() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.sadj(qs[0]);
        },
        |sim, qs| {
            sim.s(qs[0]);
        },
        1,
    );
}

#[test]
fn test_cx() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcx(&[qs[0]], qs[1]);
        },
        |sim, qs| {
            sim.mcx(&[qs[0]], qs[1]);
        },
        2,
    );
}

#[test]
fn test_cz() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcz(&[qs[0]], qs[1]);
        },
        |sim, qs| {
            sim.mcz(&[qs[0]], qs[1]);
        },
        2,
    );
}

#[test]
fn test_swap() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.swap_qubit_ids(qs[0], qs[1]);
        },
        |sim, qs| {
            sim.swap_qubit_ids(qs[0], qs[1]);
        },
        2,
    );
}

#[test]
fn test_rz() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rz(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.rz(-PI / 7.0, qs[0]);
        },
        1,
    );
}

#[test]
fn test_rx() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.rx(-PI / 7.0, qs[0]);
        },
        1,
    );
}

#[test]
fn test_ry() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.ry(-PI / 7.0, qs[0]);
        },
        1,
    );
}

#[test]
fn test_mcri() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcphase(
                &qs[2..3],
                Complex64::exp(Complex64::new(0.0, -(PI / 7.0) / 2.0)),
                qs[1],
            );
        },
        |sim, qs| {
            sim.mcphase(
                &qs[2..3],
                Complex64::exp(Complex64::new(0.0, (PI / 7.0) / 2.0)),
                qs[1],
            );
        },
        3,
    );
}

#[test]
fn test_op_queue_flushes_at_limit() {
    let mut sim = QuantumSim::default();
    let q = sim.allocate();
    for _ in 0..10_002 {
        sim.x(q);
    }
    assert_eq!(sim.op_queue.len(), 2);
    assert_eq!(sim.state.len(), 1);
}

#[test]
fn test_cx_after_h_ry_executes_queued_operations_in_order() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.h(qs[0]);
            sim.ry(PI, qs[0]);
            sim.h(qs[1]);
            sim.mcx(&[qs[1]], qs[0]);
        },
        |sim, qs| {
            sim.mcx(&[qs[1]], qs[0]);
            sim.h(qs[1]);
            sim.ry(-PI, qs[0]);
            sim.h(qs[0]);
        },
        2,
    );
}

#[test]
fn test_global_phase_dropped_when_all_qubits_released() {
    let mut sim = QuantumSim::default();
    let q = sim.allocate();
    sim.x(q);
    sim.z(q);
    sim.release(q);
    let _ = sim.allocate();
    let (state, count) = sim.get_state();
    assert_eq!(count, 1);
    assert_eq!(state.len(), 1);
    let (index, value) = state.first().expect("state should have at least one entry");
    assert_eq!(index, &BigUint::zero());
    assert_eq!(value, &Complex64::one());
}
