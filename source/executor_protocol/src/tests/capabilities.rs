// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Capabilities, Describe, Outcomes, ProgramInput, executor_fn};

/// A component that states nothing states no limit: any number of qubits
/// and states, feedback, resets and replayable shots; and no probabilities,
/// which a component states only when it computes them.
#[test]
fn default_capabilities_state_no_limit() {
    let offered = Capabilities::default();
    assert_eq!(
        (
            offered.qubits,
            offered.states,
            offered.feedback,
            offered.resets,
            offered.replayable,
            offered.probabilities
        ),
        (None, None, true, true, true, false)
    );
}

/// Two runs of one simulator, given other seeds: the run's seed is not a
/// capability, so they offer the same, and show the same.
#[test]
fn the_runs_seed_is_not_a_capability() {
    let simulator = |seed| {
        executor_fn(
            Capabilities::default().with_qubits(4),
            |_| Ok(()),
            |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
        )
        .seed(seed)
        .capabilities()
    };
    let (first, second) = (simulator(1), simulator(2));
    assert_eq!(first, second);
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}
