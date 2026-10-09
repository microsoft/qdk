//! Expected SWAP behavior across concrete input scenarios and loss policies.
//!
//! Each test prepares an input quantum state and the relevant loss, noise, and
//! timing conditions, applies SWAP, optionally performs operations that expose
//! its effects (such as MOV to probe idle history), and asserts the expected
//! behavior named by the test. Probe operations serve to observe SWAP's behavior;
//! their own contracts are not the subject of these tests.
//!
//! Backend pairs are kept together within each behavioral group:
//! - `state_exchange`: exchange quantum states without losing relative phases or
//!   correlations, and restore the original state after two SWAPs.
//! - `loss_policies`: preserve, propagate, or exchange loss as the policy requires,
//!   apply the required residual phase, and reject unsupported one-lost `Degrade`.
//! - `idle_history`: preserve already-accounted idle history through SWAP while
//!   allowing newly elapsed time to produce idle noise.
//! - `configured_faults`: apply configured SWAP faults after policy handling and
//!   suppress faults on slots that remain lost.
//!
//! This parent module holds input-preparation helpers shared across groups:
//! `setup_one_lost_swap_input` prepares a survivor with accounted idle history;
//! `setup_swap_contract_loss` prepares loss flags and SWAP faults with idle noise
//! disabled. Group-specific preparation and observation helpers stay local.
//! SWAP actions and expected-behavior assertions remain explicit in each test.

mod configured_faults;
mod idle_history;
mod loss_policies;
mod state_exchange;

use crate::{
    Simulator,
    noise_config::{
        CumulativeNoiseConfig, CumulativeNoiseTable, Fault, FaultTerm, IdleNoiseParams, LossPolicy,
        Sampler, uq1_63,
    },
};
use std::sync::Arc;

// Prepare one live |+> and one lost slot before SWAP.
//
//                    [ time 0 ] [----------- time 1 -----------]
//                              ^
//                              |
//                            step()
// survivor:          |0> ------ [idle S] -- H ------ |+>
// lost destination:  LOSS ------------------------- lost
//
// step() advances global time for BOTH slots; gates do not advance it, and
// idle timestamps record accounted history rather than separate clocks.
// H first accounts for the survivor's idle interval: the incidental [idle S]
// leaves |0> unchanged, then H prepares |+>, making a later extra S detectable.
// The lost slot receives no operation between LOSS and SWAP.
//
// Immediately before SWAP:       survivor    lost slot
// global time:                      1            1
// last-accounted idle timestamp:    1            0
//
// The setup does not perform SWAP. Each test owns the policy expectation and
// verifies the resulting state and loss status before probing idle history.
// Idle noise remains certain for one elapsed step, independently of the seed.
fn setup_one_lost_swap_input<S>(
    lost_qubit: usize,
    on_loss: LossPolicy,
    step: impl FnOnce(&mut S),
) -> S
where
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
{
    let remaining_qubit = 1 - lost_qubit;
    let config = CumulativeNoiseConfig {
        idle: IdleNoiseParams { s_probability: 1.0 },
        swap: CumulativeNoiseTable {
            on_loss,
            ..CumulativeNoiseTable::default()
        },
        intrinsics: [(
            0,
            Sampler::new([Fault(vec![FaultTerm::Loss])], [uq1_63::ONE]),
        )]
        .into_iter()
        .collect(),
        ..CumulativeNoiseConfig::default()
    };
    let mut simulator = S::new(2, 2, 7, Arc::new(config));

    simulator.correlated_noise_intrinsic(0, &[lost_qubit]);
    step(&mut simulator);
    simulator.h(remaining_qubit);
    simulator
}

// Loss is deterministic and separate from the quantum state, which stays
// |000>. The caller selects the policy and SWAP faults; idle noise is disabled.
// peek_loss can observe the resulting flags without reloading the slots.
fn setup_swap_contract_loss<S>(lost: [bool; 3], on_loss: LossPolicy, sampler: Sampler) -> S
where
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
{
    let config = CumulativeNoiseConfig {
        swap: CumulativeNoiseTable { on_loss, sampler },
        intrinsics: [(
            0,
            Sampler::new([Fault(vec![FaultTerm::Loss])], [uq1_63::ONE]),
        )]
        .into_iter()
        .collect(),
        ..CumulativeNoiseConfig::default()
    };
    let mut simulator = S::new(3, 3, 7, Arc::new(config));
    for (target, is_lost) in lost.into_iter().enumerate() {
        if is_lost {
            simulator.correlated_noise_intrinsic(0, &[target]);
        }
    }
    simulator
}
