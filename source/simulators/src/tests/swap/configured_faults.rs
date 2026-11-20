//! Configured SWAP faults act after loss-policy handling, on the resulting state
//! and live slots. Faults targeting lost slots are suppressed, including when
//! both operands are lost; uninvolved slots remain unchanged.
//!
//! Deterministic X faults distinguish policy-then-fault ordering from the reverse
//! and check suppression without relying on sampled outcomes. Idle noise is
//! disabled so these tests isolate configured gate faults.

use super::setup_swap_contract_loss;
use crate::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::FullStateSimulator,
    noise_config::{CumulativeNoiseConfig, Fault, FaultTerm, LossPolicy, Sampler, uq1_63},
    stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

// Both-lost handling precedes policy dispatch, including Degrade rejection.
// Certain X faults cannot act on lost slots. The third qubit remains live in
// |+>, so the assertions also detect accidental changes outside SWAP's operands.
#[test]
fn full_state_swap_preserves_lost_qubits_under_configured_faults() {
    for policy in [
        LossPolicy::Skip,
        LossPolicy::Propagate,
        LossPolicy::ApplyAnyway,
        LossPolicy::ResidualSDagger,
        LossPolicy::Degrade,
    ] {
        // Preparation: two lost operands, certain X faults, and an untouched live |+>.
        let mut simulator = setup_swap_contract_loss::<FullStateSimulator>(
            [true, true, false],
            policy,
            Sampler::new([Fault(vec![FaultTerm::X, FaultTerm::X])], [uq1_63::ONE]),
        );
        simulator.h(2);
        let mut expected =
            FullStateSimulator::new(3, 3, 7, Arc::new(CumulativeNoiseConfig::default()));
        expected.h(2);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect all loss flags and the joint state without further gates.
        for target in 0..3 {
            simulator.peek_loss(target, target);
        }
        // Assert: both operands stay lost; faults do not alter them or the third qubit.
        assert_eq!(
            simulator.measurements(),
            &[
                MeasurementResult::One,
                MeasurementResult::One,
                MeasurementResult::Zero
            ]
        );
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "both-lost SWAP must preserve the joint state despite configured X faults"
        );
    }
}

#[test]
fn stabilizer_swap_preserves_lost_qubits_under_configured_faults() {
    for policy in [
        LossPolicy::Skip,
        LossPolicy::Propagate,
        LossPolicy::ApplyAnyway,
        LossPolicy::ResidualSDagger,
        LossPolicy::Degrade,
    ] {
        // Preparation: two lost operands, certain X faults, and a live |+> spectator.
        let mut simulator = setup_swap_contract_loss::<StabilizerSimulator>(
            [true, true, false],
            policy,
            Sampler::new([Fault(vec![FaultTerm::X, FaultTerm::X])], [uq1_63::ONE]),
        );
        simulator.h(2);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect loss status without reloading either operand.
        for target in 0..3 {
            simulator.peek_loss(target, target);
        }
        // Assert: both operands remain lost and the spectator remains live.
        assert_eq!(
            simulator.measurements(),
            &[
                MeasurementResult::One,
                MeasurementResult::One,
                MeasurementResult::Zero
            ]
        );
        // Probe: observe the lost slots' reset states and the spectator's X probability.
        for observable in [
            [paulimer::core::z(0)].into(),
            [paulimer::core::z(1)].into(),
            [paulimer::core::x(2)].into(),
        ] {
            let probability = simulator
                .state_dump()
                .outcome_probability(&observable, false);
            // Assert: configured faults on lost operands leave the joint state unchanged.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "both-lost SWAP must preserve the joint state despite configured X faults"
            );
        }
    }
}

// The input is |010>, with qubit 0 optionally lost. A certain X on slot 0
// distinguishes a fault after relocation (|000>) from one before it (|110>).
// Under Skip, that same fault is suppressed only when slot 0 remains lost.
#[test]
fn full_state_swap_applies_configured_faults_after_policy_decision() {
    for (lost_first, policy, fault_target, expected_bits, expected_loss) in [
        (
            false,
            LossPolicy::Skip,
            0,
            [false, false, false],
            [false, false, false],
        ),
        (
            true,
            LossPolicy::Skip,
            0,
            [false, true, false],
            [true, false, false],
        ),
        (
            true,
            LossPolicy::Skip,
            1,
            [false, false, false],
            [true, false, false],
        ),
        (
            true,
            LossPolicy::ApplyAnyway,
            0,
            [false, false, false],
            [false, true, false],
        ),
        (
            true,
            LossPolicy::ResidualSDagger,
            0,
            [false, false, false],
            [false, true, false],
        ),
        (
            true,
            LossPolicy::Propagate,
            0,
            [false, false, false],
            [true, true, false],
        ),
    ] {
        // Preparation: |010>, the selected loss policy, and a certain fault on one slot.
        let mut fault = vec![FaultTerm::I; 2];
        fault[fault_target] = FaultTerm::X;
        let mut simulator = setup_swap_contract_loss::<FullStateSimulator>(
            [lost_first, false, false],
            policy,
            Sampler::new([Fault(fault)], [uq1_63::ONE]),
        );
        simulator.x(1);
        let mut expected =
            FullStateSimulator::new(3, 3, 7, Arc::new(CumulativeNoiseConfig::default()));
        for (target, is_one) in expected_bits.into_iter().enumerate() {
            if is_one {
                expected.x(target);
            }
        }

        // SWAP
        simulator.swap(0, 1);
        // Probe: compare the state directly with the policy-then-fault reference.
        // Assert: faults acted on the state and live slots resulting from the policy.
        assert!(
            simulator.state_dump() == expected.state_dump(),
            "SWAP faults must act after the policy decision"
        );
        // Probe: inspect loss flags separately, including the unrelated slot.
        for (target, is_lost) in expected_loss.into_iter().enumerate() {
            simulator.peek_loss(target, target);
            // Assert: fault application preserves the policy's expected loss pattern.
            assert_eq!(
                simulator.measurements()[target],
                if is_lost {
                    MeasurementResult::One
                } else {
                    MeasurementResult::Zero
                },
                "target={target}: incorrect loss status after noisy SWAP"
            );
        }
    }
}

#[test]
fn stabilizer_swap_applies_configured_faults_after_policy_decision() {
    for (lost_first, policy, fault_target, expected_bits, expected_loss) in [
        (
            false,
            LossPolicy::Skip,
            0,
            [false, false, false],
            [false, false, false],
        ),
        (
            true,
            LossPolicy::Skip,
            0,
            [false, true, false],
            [true, false, false],
        ),
        (
            true,
            LossPolicy::Skip,
            1,
            [false, false, false],
            [true, false, false],
        ),
        (
            true,
            LossPolicy::ApplyAnyway,
            0,
            [false, false, false],
            [false, true, false],
        ),
        (
            true,
            LossPolicy::ResidualSDagger,
            0,
            [false, false, false],
            [false, true, false],
        ),
        (
            true,
            LossPolicy::Propagate,
            0,
            [false, false, false],
            [true, true, false],
        ),
    ] {
        // Preparation: |010>, the selected loss policy, and one deterministic X fault.
        let mut fault = vec![FaultTerm::I; 2];
        fault[fault_target] = FaultTerm::X;
        let mut simulator = setup_swap_contract_loss::<StabilizerSimulator>(
            [lost_first, false, false],
            policy,
            Sampler::new([Fault(fault)], [uq1_63::ONE]),
        );
        simulator.x(1);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect probabilities for the policy-then-fault output bit pattern.
        for (target, is_one) in expected_bits.into_iter().enumerate() {
            let observable = [paulimer::core::z(target)].into();
            let probability = simulator
                .state_dump()
                .outcome_probability(&observable, is_one);
            // Assert: each output bit agrees with faults acting after the policy decision.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "target={target}: SWAP faults must act after the policy decision"
            );
        }
        // Probe: inspect loss independently of the quantum-state probabilities.
        for (target, is_lost) in expected_loss.into_iter().enumerate() {
            simulator.peek_loss(target, target);
            // Assert: the selected policy determines which slots remain live.
            assert_eq!(
                simulator.measurements()[target],
                if is_lost {
                    MeasurementResult::One
                } else {
                    MeasurementResult::Zero
                },
                "target={target}: incorrect loss status after noisy SWAP"
            );
        }
    }
}
