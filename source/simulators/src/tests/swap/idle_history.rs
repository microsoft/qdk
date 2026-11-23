//! SWAP preserves already-accounted idle history: a same-time MOV must not apply
//! idle noise again, while a newly elapsed interval must still affect the state.
//!
//! Cases cover one-lost relocation under `ApplyAnyway` and `ResidualSDagger`, plus
//! both-live exchange involving an entangled partner. Phase-sensitive state and
//! correlation checks expose duplicated idle noise; MOV only probes the history
//! left by SWAP.

use super::setup_one_lost_swap_input;
use crate::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::FullStateSimulator,
    noise_config::{CumulativeNoiseConfig, IdleNoiseParams, LossPolicy},
    stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

// The setup returns a live |+> at time 1 with its idle history accounted for.
// MOV has no configured gate faults and time does not advance, so the state
// must remain |+>. A stale destination timestamp instead counts one idle step;
// probability 1 guarantees an extra S, producing |+i> regardless of seed.
// Compare with a noiseless reference: Z-basis probabilities alone would miss
// this phase change, and disabling idle noise would hide the bookkeeping bug.
#[test]
fn full_state_swap_preserves_accounted_idle_history_with_apply_anyway() {
    for target in [0, 1] {
        // Preparation: one lost slot and a live |+> with idle time already accounted for.
        let mut simulator =
            setup_one_lost_swap_input(target, LossPolicy::ApplyAnyway, FullStateSimulator::step);
        let mut reference =
            FullStateSimulator::new(2, 2, 7, Arc::new(CumulativeNoiseConfig::default()));
        reference.h(target);
        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect the destination and source without further evolution.
        simulator.peek_loss(target, 0);
        simulator.peek_loss(1 - target, 1);
        // Assert: the destination is live in |+>; the source is now lost.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero, MeasurementResult::One]
        );
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must be |+> after SWAP"
        );

        // Probe: MOV at the same time exposes any idle interval duplicated by SWAP.
        simulator.mov(target);
        // Assert: relocation preserves the survivor without adding an idle fault.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must remain |+> after MOV"
        );

        // Probe: advance one step, then request idle accounting at the destination.
        simulator.step();
        simulator.mov(target);
        reference.s(target);
        // Assert: SWAP preserved history without disabling future idle noise.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must receive idle S after advancing to time 2"
        );
    }
}

// The setup returns a live |+> at time 1 with its idle history accounted for.
// A same-time MOV with no configured gate faults must preserve that state.
// A stale destination timestamp would trigger a certain idle S, yielding
// |+i>. Check the probability of the +1 X outcome: it must remain 1, but the
// extra S makes it 1/2. This checks the phase error without sampling outcomes;
// probability 1 for idle noise makes the error independent of the seed.
#[test]
fn stabilizer_swap_preserves_accounted_idle_history_with_apply_anyway() {
    for target in [0, 1] {
        // Preparation: one lost slot and a live |+> with idle time already accounted for.
        let mut simulator =
            setup_one_lost_swap_input(target, LossPolicy::ApplyAnyway, StabilizerSimulator::step);
        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect loss status and the destination's X probability.
        simulator.peek_loss(target, 0);
        simulator.peek_loss(1 - target, 1);
        // Assert: loss follows relocation and the destination is certainly |+>.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero, MeasurementResult::One]
        );
        let observable = [paulimer::core::x(target)].into();
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must be |+> after SWAP, probability={probability}"
        );

        // Probe: same-time MOV makes stale destination history observable as a phase.
        simulator.mov(target);
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        // Assert: SWAP did not leave an idle interval to be applied twice.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must remain |+> after MOV, probability={probability}"
        );

        // Probe: account for a genuinely new idle interval at the destination.
        simulator.step();
        simulator.mov(target);
        let observable = [paulimer::core::y(target)].into();
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        // Assert: new idle noise still acts, producing the expected |+i> state.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must receive idle S after advancing to time 2, probability={probability}"
        );
    }
}

// ResidualSDagger maps the incoming |+> to |-i>. A same-time MOV must not
// cancel that phase; only a new idle interval supplies S and restores |+>.
// The noiseless reference checks the joint state, including the lost source.
#[test]
fn full_state_swap_preserves_accounted_idle_history_with_residual_s_dagger() {
    for target in [0, 1] {
        // Preparation: a live |+>, a lost destination, and the policy's |-i> reference.
        let mut simulator = setup_one_lost_swap_input(
            target,
            LossPolicy::ResidualSDagger,
            FullStateSimulator::step,
        );
        let mut reference =
            FullStateSimulator::new(2, 2, 7, Arc::new(CumulativeNoiseConfig::default()));
        reference.h(target);
        reference.s_adj(target);
        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect the joint state and both loss flags immediately.
        simulator.peek_loss(target, 0);
        simulator.peek_loss(1 - target, 1);
        // Assert: relocation exchanges loss status and applies the residual phase.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero, MeasurementResult::One]
        );
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must be |-i> after residual SWAP"
        );

        // Probe: request idle accounting again without advancing time.
        simulator.mov(target);
        // Assert: no duplicated interval cancels SWAP's residual phase.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must remain |-i> after same-time MOV"
        );

        // Probe: advance time and account for a new idle interval.
        simulator.step();
        simulator.mov(target);
        reference.s(target);
        // Assert: only the new idle S cancels the residual phase, restoring |+>.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "target={target} must become |+> after a new idle S"
        );
    }
}

// |-i> has Y eigenvalue -1: outcome_probability(..., true) checks that sign
// without sampling. Z probabilities cannot distinguish it from |+>.
// Same-time MOV preserves Y=-1; a new idle S changes the state to X=+1.
#[test]
fn stabilizer_swap_preserves_accounted_idle_history_with_residual_s_dagger() {
    for target in [0, 1] {
        // Preparation: one lost destination and a live |+> under ResidualSDagger.
        let mut simulator = setup_one_lost_swap_input(
            target,
            LossPolicy::ResidualSDagger,
            StabilizerSimulator::step,
        );
        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect loss flags and the negative-Y probability at the destination.
        simulator.peek_loss(target, 0);
        simulator.peek_loss(1 - target, 1);
        // Assert: the survivor relocates with the required |-i> phase.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero, MeasurementResult::One]
        );
        let observable = [paulimer::core::y(target)].into();
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, true);
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must be |-i> after residual SWAP, probability={probability}"
        );

        // Probe: same-time idle accounting must not introduce another S.
        simulator.mov(target);
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, true);
        // Assert: SWAP's residual phase survives the continuation.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must remain |-i> after same-time MOV, probability={probability}"
        );

        // Probe: advance time before the next idle-accounting request.
        simulator.step();
        simulator.mov(target);
        let observable = [paulimer::core::x(target)].into();
        let probability = simulator
            .state_dump()
            .outcome_probability(&observable, false);
        // Assert: a new idle S, not relocation itself, restores |+>.
        assert!(
            (probability - 1.0).abs() < 1e-10,
            "target={target} must become |+> after a new idle S, probability={probability}"
        );
    }
}

// Prepare all three slots live, with source and partner in a Bell pair.
//
//                    time 0      [--------- time 1 ---------]
//                            step()
// source:             |0> ------ [idle S] -- H --- CX ---+
//                                                 |    | Bell pair
// partner (qubit 2):   |0> ------ [idle S] -------- X ---+
// destination:        |0> ----------------------------- |0>
//
// The incidental idle S gates act on |0>, before entanglement is created.
// On return, source and partner have accounted for time 1; destination has
// not. SWAP is left to each test. No gate faults are configured, and the
// certain one-step idle S makes any duplicated interval phase-detectable.
fn setup_entangled_qubit_and_live_destination<S>(destination: usize, step: impl FnOnce(&mut S)) -> S
where
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
{
    let config = CumulativeNoiseConfig {
        idle: IdleNoiseParams { s_probability: 1.0 },
        ..CumulativeNoiseConfig::default()
    };
    let mut simulator = S::new(3, 3, 7, Arc::new(config));
    step(&mut simulator);
    simulator.h(1 - destination);
    simulator.cx(1 - destination, 2);
    simulator
}

// Build the expected Bell pair directly at the destination, without SWAP,
// so the reference does not rely on the operation under test being correct.
#[test]
fn full_state_swap_preserves_accounted_idle_history_for_entangled_qubits() {
    for destination in [0, 1] {
        // Preparation: a Bell pair, a live destination, and an independently mapped reference.
        let mut simulator =
            setup_entangled_qubit_and_live_destination(destination, FullStateSimulator::step);
        let mut reference =
            FullStateSimulator::new(3, 3, 7, Arc::new(CumulativeNoiseConfig::default()));
        reference.h(destination);
        reference.cx(destination, 2);

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect the joint state directly, without a continuation.
        // Assert: correlations now connect the destination to the untouched partner.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "destination={destination} must share the Bell pair with qubit 2 after SWAP"
        );
        // Probe: observe all loss flags without collapsing the Bell pair.
        for target in 0..3 {
            simulator.peek_loss(target, target);
        }
        // Assert: exchanging live operands did not lose any qubit.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero; 3],
            "destination={destination}: SWAP must leave all three slots live"
        );

        // Probe: request idle accounting on every slot at the same global time.
        for target in 0..3 {
            simulator.mov(target);
        }
        // Assert: SWAP preserved both entanglement and accounted idle history.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "destination={destination} must preserve the Bell pair after same-time MOV"
        );

        // Probe: account for a fresh interval at the destination only.
        simulator.step();
        simulator.mov(destination);
        reference.s(destination);
        // Assert: the destination still receives new idle noise after relocation.
        assert!(
            simulator.state_dump() == reference.state_dump(),
            "destination={destination} must receive idle S only for the new interval"
        );
    }
}

// Z(source), XX(destination, partner), and ZZ(destination, partner), all +1,
// uniquely specify |0> at the source and the expected Bell pair. Individual
// qubit probabilities would not establish that entanglement was transferred.
// A later S on the destination changes the XX correlation to YX.
#[test]
fn stabilizer_swap_preserves_accounted_idle_history_for_entangled_qubits() {
    for destination in [0, 1] {
        // Preparation: a Bell pair and the joint observables expected after relocation.
        let mut simulator =
            setup_entangled_qubit_and_live_destination(destination, StabilizerSimulator::step);
        let observables = [
            [paulimer::core::z(1 - destination)].into(),
            [paulimer::core::x(destination), paulimer::core::x(2)].into(),
            [paulimer::core::z(destination), paulimer::core::z(2)].into(),
        ];

        // SWAP
        simulator.swap(0, 1);
        // Probe: inspect joint probabilities, not individual sampled outcomes.
        for observable in &observables {
            let probability = simulator
                .state_dump()
                .outcome_probability(observable, false);
            // Assert: the source is reset and the destination shares the Bell pair.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "destination={destination} must share the Bell pair with qubit 2 and leave the source in |0>, probability={probability}"
            );
        }
        // Probe: inspect loss status separately from quantum correlations.
        for target in 0..3 {
            simulator.peek_loss(target, target);
        }
        // Assert: all three slots remain live.
        assert_eq!(
            simulator.measurements(),
            &[MeasurementResult::Zero; 3],
            "destination={destination}: SWAP must leave all three slots live"
        );

        // Probe: ask all slots to account for idle time without advancing it.
        for target in 0..3 {
            simulator.mov(target);
        }
        for observable in &observables {
            let probability = simulator
                .state_dump()
                .outcome_probability(observable, false);
            // Assert: SWAP left no extra interval that changes the Bell pair.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "destination={destination} must preserve the Bell pair after same-time MOV, probability={probability}"
            );
        }

        // Probe: advance time and apply the new idle interval at the destination.
        simulator.step();
        simulator.mov(destination);
        let observables = [
            [paulimer::core::z(1 - destination)].into(),
            [paulimer::core::y(destination), paulimer::core::x(2)].into(),
            [paulimer::core::z(destination), paulimer::core::z(2)].into(),
        ];
        for observable in &observables {
            let probability = simulator
                .state_dump()
                .outcome_probability(observable, false);
            // Assert: new idle evolution changes XX to YX, preserving the other correlations.
            assert!(
                (probability - 1.0).abs() < 1e-10,
                "destination={destination} must receive idle S only for the new interval, probability={probability}"
            );
        }
    }
}
