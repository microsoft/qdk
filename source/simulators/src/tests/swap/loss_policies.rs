//! SWAP's loss policy determines the surviving state and loss flags when operands
//! are lost. `Skip` preserves the survivor, `Propagate` loses and resets it, and
//! `Degrade` is rejected when one lost operand requires a policy decision.
//!
//! `ApplyAnyway` exchanges loss flags and restores them after two SWAPs.
//! `ResidualSDagger` applies S-adjoint to the relocated survivor; those phase checks
//! disable idle noise to isolate the policy's effect. Configured gate faults are
//! tested separately in `configured_faults`.

use super::{setup_one_lost_swap_input, setup_swap_contract_loss};
use crate::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::FullStateSimulator,
    noise_config::{CumulativeNoiseConfig, LossPolicy, Sampler},
    stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

// Skip retains the survivor at its original slot. MOV probes whether SWAP
// accounted for idle history without relocating it; a later interval still acts.
#[test]
fn full_state_swap_skip_preserves_the_surviving_qubit() {
    for lost in [0, 1] {
        // Preparation: a lost slot and a live |+>; Skip must leave their roles unchanged.
        let survivor = 1 - lost;
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Skip, FullStateSimulator::step);
        let mut expected =
            FullStateSimulator::new(2, 2, 7, Arc::new(CumulativeNoiseConfig::default()));
        expected.h(survivor);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect both loss flags and compare the unchanged joint state.
        simulator.peek_loss(lost, 0);
        simulator.peek_loss(survivor, 1);
        // Assert: the survivor remains live in its original slot.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::One, MeasurementResult::Zero]
        );
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "lost={lost}: Skip must preserve the survivor at its original slot"
        );

        // Probe: request idle accounting at the same time.
        simulator.mov(survivor);
        // Assert: Skip left no duplicated idle interval.
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "lost={lost}: Skip must not leave duplicated idle history"
        );
        // Probe: advance time before accounting for idle noise again.
        simulator.step();
        simulator.mov(survivor);
        expected.s(survivor);
        // Assert: skipping relocation did not disable subsequent idle evolution.
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "lost={lost}: new idle time must still act on the survivor"
        );
    }
}

#[test]
fn stabilizer_swap_skip_preserves_the_surviving_qubit() {
    for lost in [0, 1] {
        // Preparation: a lost slot and a live |+> under Skip.
        let survivor = 1 - lost;
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Skip, StabilizerSimulator::step);
        let observable = [paulimer::core::x(survivor)].into();

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect loss status and X probability at the original survivor slot.
        simulator.peek_loss(lost, 0);
        simulator.peek_loss(survivor, 1);
        // Assert: neither loss status nor the survivor's state was exchanged.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::One, MeasurementResult::Zero]
        );
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "lost={lost}: Skip must preserve |+> at the original slot"
        );

        // Probe: request idle accounting without advancing time.
        simulator.mov(survivor);
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        // Assert: Skip preserved the accounted history as well as the immediate state.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "lost={lost}: Skip must not leave duplicated idle history"
        );
        // Probe: advance time and observe the survivor's Y probability after idle S.
        simulator.step();
        simulator.mov(survivor);
        let observable = [paulimer::core::y(survivor)].into();
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        // Assert: a genuinely new interval still changes the survivor to |+i>.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "lost={lost}: new idle time must still act on the survivor"
        );
    }
}

// Propagate is deliberately not a relocation contract. Losing the survivor
// resets its underlying state; no physical measurement outcome is asserted.
#[test]
fn full_state_swap_propagate_loses_and_resets_the_surviving_qubit() {
    for lost in [0, 1] {
        // Preparation: one lost operand and a live superposition under Propagate.
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Propagate, FullStateSimulator::step);
        let expected = FullStateSimulator::new(2, 2, 7, Arc::new(CumulativeNoiseConfig::default()));

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect both flags and the post-loss joint state directly.
        for target in 0..2 {
            simulator.peek_loss(target, target);
        }
        // Assert: both slots are lost and their underlying state is reset.
        assert_eq!(simulator.measurements(), &[MeasurementResult::One; 2]);
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "lost={lost}: Propagate must reset the survivor when losing it"
        );
    }
}

#[test]
fn stabilizer_swap_propagate_loses_and_resets_the_surviving_qubit() {
    for lost in [0, 1] {
        // Preparation: one lost operand and a live superposition under Propagate.
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Propagate, StabilizerSimulator::step);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect loss flags and the probability of |0> in each slot.
        for target in 0..2 {
            simulator.peek_loss(target, target);
            let observable = [paulimer::core::z(target)].into();
            let probability = simulator
                .state_dump()
                .outcome_probability(&observable, false);
            // Assert: loss propagation resets the underlying state.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "lost={lost}: Propagate must leave target={target} reset"
            );
        }
        // Assert: the original survivor has also become lost.
        assert_eq!(simulator.measurements(), &[MeasurementResult::One; 2]);
    }
}

// Degrade is rejected when the one-lost path needs a policy decision. Catch
// each panic separately so both operand orientations must reject it.
#[test]
fn full_state_swap_reports_unsupported_degrade_for_a_lost_operand() {
    for lost in [0, 1] {
        // Preparation: one lost operand forces a decision under unsupported Degrade.
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Degrade, FullStateSimulator::step);
        // SWAP / Probe: capture rejection instead of allowing the panic to end the test.
        let failure =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| simulator.swap(0, 1)))
                .expect_err("one-lost SWAP must reject Degrade");
        // Assert: failure reports the unsupported SWAP policy, not an unrelated error.
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        assert!(
            message.contains("the `swap` gate does not support the Degrade loss policy"),
            "lost={lost}: unexpected panic: {message}"
        );
    }
}

#[test]
fn stabilizer_swap_reports_unsupported_degrade_for_a_lost_operand() {
    for lost in [0, 1] {
        // Preparation: one lost operand forces a decision under unsupported Degrade.
        let mut simulator =
            setup_one_lost_swap_input(lost, LossPolicy::Degrade, StabilizerSimulator::step);
        // SWAP / Probe: capture rejection for each lost-operand orientation.
        let failure =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| simulator.swap(0, 1)))
                .expect_err("one-lost SWAP must reject Degrade");
        // Assert: the rejected policy, rather than an unrelated panic, caused failure.
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        assert!(
            message.contains("the `swap` gate does not support the Degrade loss policy"),
            "lost={lost}: unexpected panic: {message}"
        );
    }
}

#[test]
fn full_state_swap_apply_anyway_exchanges_loss_status() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for loss_mask in 0..8 {
                // Preparation: a loss pattern and its expected exchange, with noise disabled.
                let lost = std::array::from_fn(|target| loss_mask & (1 << target) != 0);
                let original = lost.map(|is_lost| {
                    if is_lost {
                        MeasurementResult::One
                    } else {
                        MeasurementResult::Zero
                    }
                });
                let mut simulator = setup_swap_contract_loss::<FullStateSimulator>(
                    lost,
                    LossPolicy::ApplyAnyway,
                    Sampler::default(),
                );
                let mut expected = original;
                expected.swap(first, second);

                // SWAP
                simulator.swap(first, second);
                // Probe: peek at every slot, including the one outside the exchanged pair.
                for target in 0..3 {
                    simulator.peek_loss(target, target);
                }
                // Assert: selected loss statuses exchange; unrelated status stays unchanged.
                assert_eq!(
                    simulator.measurements(),
                    &expected,
                    "SWAP({first}, {second}) must exchange only the selected loss flags: loss_mask={loss_mask}"
                );
            }
        }
    }
}

#[test]
fn stabilizer_swap_apply_anyway_exchanges_loss_status() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for loss_mask in 0..8 {
                // Preparation: a loss pattern and its expected exchange, with noise disabled.
                let lost = std::array::from_fn(|target| loss_mask & (1 << target) != 0);
                let original = lost.map(|is_lost| {
                    if is_lost {
                        MeasurementResult::One
                    } else {
                        MeasurementResult::Zero
                    }
                });
                let mut simulator = setup_swap_contract_loss::<StabilizerSimulator>(
                    lost,
                    LossPolicy::ApplyAnyway,
                    Sampler::default(),
                );
                let mut expected = original;
                expected.swap(first, second);

                // SWAP
                simulator.swap(first, second);
                // Probe: inspect loss status without measuring or reloading qubits.
                for target in 0..3 {
                    simulator.peek_loss(target, target);
                }
                // Assert: only the selected slots exchange status.
                assert_eq!(
                    simulator.measurements(),
                    &expected,
                    "SWAP({first}, {second}) must exchange only the selected loss flags: loss_mask={loss_mask}"
                );
            }
        }
    }
}

#[test]
fn full_state_swap_apply_anyway_twice_restores_loss_status() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for loss_mask in 0..8 {
                // Preparation: each three-slot loss pattern with SWAP faults disabled.
                let lost = std::array::from_fn(|target| loss_mask & (1 << target) != 0);
                let mut simulator = setup_swap_contract_loss::<FullStateSimulator>(
                    lost,
                    LossPolicy::ApplyAnyway,
                    Sampler::default(),
                );

                // SWAP
                simulator.swap(first, second);
                // Probe: exchange again, then inspect every slot without reloading it.
                simulator.swap(first, second);
                for target in 0..3 {
                    simulator.peek_loss(target, target);
                }
                // Assert: the original loss pattern is restored, including the uninvolved slot.
                let expected = lost.map(|is_lost| {
                    if is_lost {
                        MeasurementResult::One
                    } else {
                        MeasurementResult::Zero
                    }
                });
                assert_eq!(
                    simulator.measurements(),
                    &expected,
                    "two SWAP({first}, {second}) operations must restore loss status: loss_mask={loss_mask}"
                );
            }
        }
    }
}

#[test]
fn stabilizer_swap_apply_anyway_twice_restores_loss_status() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for loss_mask in 0..8 {
                // Preparation: each three-slot loss pattern with SWAP faults disabled.
                let lost = std::array::from_fn(|target| loss_mask & (1 << target) != 0);
                let mut simulator = setup_swap_contract_loss::<StabilizerSimulator>(
                    lost,
                    LossPolicy::ApplyAnyway,
                    Sampler::default(),
                );

                // SWAP
                simulator.swap(first, second);
                // Probe: exchange again and observe all loss flags without reloading.
                simulator.swap(first, second);
                for target in 0..3 {
                    simulator.peek_loss(target, target);
                }
                // Assert: every slot has the same loss status as before either SWAP.
                let expected = lost.map(|is_lost| {
                    if is_lost {
                        MeasurementResult::One
                    } else {
                        MeasurementResult::Zero
                    }
                });
                assert_eq!(
                    simulator.measurements(),
                    &expected,
                    "two SWAP({first}, {second}) operations must restore loss status: loss_mask={loss_mask}"
                );
            }
        }
    }
}

// Isolate the policy's phase from elapsed-time behavior: these cases have no
// idle noise. X/Y eigenstates distinguish S-adjoint from identity and S.
#[test]
fn full_state_swap_residual_s_dagger_applies_the_required_phase() {
    for destination in [0, 1] {
        for imaginary in [false, true] {
            // Preparation: a live |+> or |+i>, one lost slot, and an independent reference.
            let lost = std::array::from_fn(|target| target == destination);
            let mut simulator = setup_swap_contract_loss::<FullStateSimulator>(
                lost,
                LossPolicy::ResidualSDagger,
                Sampler::default(),
            );
            let mut expected =
                FullStateSimulator::new(3, 3, 7, Arc::new(CumulativeNoiseConfig::default()));
            simulator.h(1 - destination);
            expected.h(destination);
            if imaginary {
                simulator.s(1 - destination);
                expected.s(destination);
            }
            expected.s_adj(destination);

            // SWAP
            simulator.swap(0, 1);
            // Probe: inspect the complete output state, with no idle-time continuation.
            // Assert: relocation applies precisely S-adjoint to the incoming survivor.
            assert!(
                simulator.state_dump() == expected.state_dump(),
                "destination={destination}, imaginary={imaginary}: SWAP must apply the required residual phase"
            );
        }
    }
}

#[test]
fn stabilizer_swap_residual_s_dagger_applies_the_required_phase() {
    for destination in [0, 1] {
        for imaginary in [false, true] {
            // Preparation: a live X/Y eigenstate, with idle and gate faults disabled.
            let lost = std::array::from_fn(|target| target == destination);
            let mut simulator = setup_swap_contract_loss::<StabilizerSimulator>(
                lost,
                LossPolicy::ResidualSDagger,
                Sampler::default(),
            );
            simulator.h(1 - destination);
            if imaginary {
                simulator.s(1 - destination);
            }

            // SWAP
            simulator.swap(0, 1);
            // Probe: observe X=+1 for input |+i>, or Y=-1 for input |+>.
            let observable = [if imaginary {
                paulimer::core::x(destination)
            } else {
                paulimer::core::y(destination)
            }]
            .into();
            let probability = simulator
                .state_dump()
                .outcome_probability(&observable, !imaginary);
            // Assert: the residual phase is S-adjoint, independently of idle history.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "destination={destination}, imaginary={imaginary}: SWAP must apply the required residual phase, probability={probability}"
            );
        }
    }
}
