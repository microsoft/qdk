// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only: the resumer's routing. A run routed by
//! `MeetHalfway` records what the engine's routing records, on programs
//! with distant gates, draws between them, and forks; a placement routes,
//! translates and falls back as `resumer/routing.rs` says.

use crate::OutputRecord;
use crate::bytecode::encoding::{OPID_CX, OPID_CZ, OPID_H, OPID_MZ, OPID_RY, OPID_X};
use crate::mps_simulator::resumer::lowering::{Operator, Step};
use crate::mps_simulator::resumer::routing::Placement;
use crate::mps_simulator::{GateFusion, MpsResumer, Restore, RouteTuning, Routing};
use crate::qir::{QirGate, QirProgram, QirShotId};
use tensornet::reference::ExactReference;

use super::super::programs::{
    branch, gate, gate1, gate2, jump, measure, program_with, read_result, reset, ret,
};
use super::super::reference_spy::ReferenceSpy;
use super::run;

const ROUTED: [Routing; 2] = [Routing::MeetHalfway, Routing::MeetHalfwayAfterFusion];
const FUSIONS: [GateFusion; 2] = [GateFusion::Unfused, GateFusion::Pairs];

/// The records of the first 64 shots of `program`, seed 7, on the exact
/// reference, routed and fused as given.
fn records(program: &QirProgram, routing: Routing, fusion: GateFusion) -> Vec<Vec<OutputRecord>> {
    let mut resumer = MpsResumer::new(ExactReference::new(), 7)
        .with_routing(routing)
        .with_gate_fusion(fusion);
    run(&mut resumer, program, &QirShotId::first(64)).expect("the program runs")
}

/// The operators the engine applies on the first 64 shots of `program`,
/// unfused, routed as given.
fn applies(program: &QirProgram, routing: Routing) -> usize {
    let mut resumer = MpsResumer::new(ReferenceSpy::new(), 7)
        .with_routing(routing)
        .with_gate_fusion(GateFusion::Unfused);
    run(&mut resumer, program, &QirShotId::first(64)).expect("the program runs");
    resumer.engine().applies()
}

/// Six qubits, qubit 0 meeting four others (so no order puts every gate's
/// qubits side by side), with a measurement, a reset and gates after them
/// in one batch, then every qubit read.
fn distant_gates_with_draws_between() -> QirProgram {
    program_with(
        6,
        7,
        &[],
        &[&[
            gate(OPID_RY, 0, 0, 0.7),
            gate(OPID_RY, 1, 0, 1.1),
            gate1(OPID_H, 2),
            gate2(OPID_CX, 0, 3),
            gate2(OPID_CX, 1, 4),
            gate2(OPID_CX, 0, 5),
            gate2(OPID_CZ, 2, 5),
            gate(OPID_RY, 4, 0, 0.4),
            gate2(OPID_CX, 2, 4),
            gate2(OPID_CX, 0, 2),
            measure(OPID_MZ, 3, 6),
            reset(4),
            gate(OPID_RY, 4, 0, 0.9),
            gate2(OPID_CX, 4, 0),
            gate2(OPID_CX, 5, 1),
            gate2(OPID_CX, 3, 2),
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            measure(OPID_MZ, 2, 2),
            measure(OPID_MZ, 3, 3),
            measure(OPID_MZ, 4, 4),
            measure(OPID_MZ, 5, 5),
            ret(),
        ]],
    )
}

/// Distant gates, then a coin (qubit 1) read and branched on: the shots
/// split, a forked state goes on, and each part runs its next batch, of
/// distant gates, from where the first batch left its qubits.
fn a_branch_after_distant_gates() -> QirProgram {
    let (flip, join) = (1, 2);
    program_with(
        6,
        7,
        &[],
        &[
            &[
                gate(OPID_RY, 0, 0, 0.7),
                gate1(OPID_H, 2),
                gate2(OPID_CX, 0, 3),
                gate2(OPID_CX, 0, 5),
                gate2(OPID_CX, 2, 4),
                gate2(OPID_CX, 0, 4),
                gate1(OPID_H, 1),
                measure(OPID_MZ, 1, 6),
                read_result(0, 6),
                branch(0, flip, join),
            ],
            &[gate1(OPID_X, 4), gate2(OPID_CX, 4, 1), jump(join)],
            &[
                gate2(OPID_CX, 3, 5),
                gate2(OPID_CX, 0, 2),
                gate2(OPID_CZ, 1, 4),
                gate(OPID_RY, 3, 0, 0.5),
                gate2(OPID_CX, 5, 1),
                measure(OPID_MZ, 0, 0),
                measure(OPID_MZ, 1, 1),
                measure(OPID_MZ, 2, 2),
                measure(OPID_MZ, 3, 3),
                measure(OPID_MZ, 4, 4),
                measure(OPID_MZ, 5, 5),
                ret(),
            ],
        ],
    )
}

/// A routed run records what the engine's routing records, shot by shot,
/// on a program whose distant gates have a measurement and a reset between
/// them; routing adds SWAPs there.
#[test]
fn a_routed_run_records_as_the_engines_routing_across_draws() {
    let program = distant_gates_with_draws_between();
    assert!(
        applies(&program, Routing::MeetHalfway) > applies(&program, Routing::Engine),
        "the program has distant gates, which routing moves by SWAPs"
    );
    for fusion in FUSIONS {
        let expected = records(&program, Routing::Engine, fusion);
        for routing in ROUTED {
            assert_eq!(
                records(&program, routing, fusion),
                expected,
                "{routing:?}, {fusion:?}"
            );
        }
    }
}

/// After a fork, each part goes on from where routing left its qubits: a
/// routed run records what the engine's routing records, both values of the
/// coin occurring.
#[test]
fn a_fork_goes_on_from_where_routing_left_its_qubits() {
    let program = a_branch_after_distant_gates();
    assert!(
        applies(&program, Routing::MeetHalfway) > applies(&program, Routing::Engine),
        "the program has distant gates, which routing moves by SWAPs"
    );
    for fusion in FUSIONS {
        let expected = records(&program, Routing::Engine, fusion);
        assert!(
            expected.iter().any(|shot| shot[6] != expected[0][6]),
            "both values of the coin occur, so the shots split"
        );
        for routing in ROUTED {
            assert_eq!(
                records(&program, routing, fusion),
                expected,
                "{routing:?}, {fusion:?}"
            );
        }
    }
}

/// A program whose gates are all on neighbouring sites gets no SWAP: the
/// engine applies as many operators, and the records are the same.
#[test]
fn a_program_of_neighbouring_gates_gets_no_swap() {
    let program = program_with(
        4,
        4,
        &[],
        &[&[
            gate1(OPID_H, 0),
            gate2(OPID_CX, 0, 1),
            gate2(OPID_CX, 1, 2),
            gate2(OPID_CX, 2, 3),
            gate(OPID_RY, 3, 0, 0.8),
            gate2(OPID_CX, 3, 2),
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            measure(OPID_MZ, 2, 2),
            measure(OPID_MZ, 3, 3),
            ret(),
        ]],
    );
    assert_eq!(
        applies(&program, Routing::MeetHalfway),
        applies(&program, Routing::Engine)
    );
    for fusion in FUSIONS {
        assert_eq!(
            records(&program, Routing::MeetHalfway, fusion),
            records(&program, Routing::Engine, fusion)
        );
    }
}

/// A two-site operator on the layout's sites `a` and `b`.
fn cx(a: usize, b: usize) -> Step {
    Step::Apply(Operator {
        site_slots: [a, b],
        matrix: QirGate::Cx.matrix_at(None),
    })
}

/// The SWAPs among `steps`.
fn swaps(steps: &[Step]) -> usize {
    let swap = QirGate::Swap.matrix_at(None);
    steps
        .iter()
        .filter(|step| matches!(step, Step::Apply(operator) if operator.matrix == swap))
        .count()
}

/// The sites of the last operator among `steps`.
fn last_operator_sites(steps: &[Step]) -> Vec<usize> {
    match steps.last() {
        Some(Step::Apply(operator)) => operator.sites().to_vec(),
        other => panic!("the steps end with an operator, not {other:?}"),
    }
}

/// A gate on sites five apart takes four SWAPs and applies on neighbours;
/// a gate that follows on the same two sites takes none.
#[test]
fn a_route_takes_half_the_swaps_and_leaves_the_sites_together() {
    let mut placement = Placement::in_layout_order(6);
    let first = placement.route(vec![cx(0, 5)]);
    assert_eq!(swaps(&first), 4);
    let sites = last_operator_sites(&first);
    assert_eq!(sites[0].abs_diff(sites[1]), 1, "{sites:?}");
    assert_eq!(sites, [placement.position(0), placement.position(5)]);
    let second = placement.route(vec![cx(5, 0)]);
    assert_eq!(swaps(&second), 0);
    assert_eq!(
        last_operator_sites(&second),
        [placement.position(5), placement.position(0)]
    );
}

/// A measurement and a reset after a route go to where their qubits sit
/// then: qubit 0 moved to site 1, qubit 3 to site 2.
#[test]
fn a_draw_goes_to_where_its_qubit_sits() {
    let mut placement = Placement::in_layout_order(4);
    let steps = placement.route(vec![
        cx(0, 3),
        Step::Measure {
            site: 0,
            result: 0,
            reset: false,
        },
        Step::Reset { site: 3 },
    ]);
    assert_eq!(
        steps[steps.len() - 2..],
        [
            Step::Measure {
                site: 1,
                result: 0,
                reset: false
            },
            Step::Reset { site: 2 },
        ]
    );
}

/// A route that leaves the drift above one half (here 2/3, on four sites)
/// restores the layout's order and falls back for good: a later distant
/// gate goes to the engine as it is.
#[test]
fn a_drift_past_one_half_restores_the_order_and_falls_back() {
    let mut placement = Placement::in_layout_order(4);
    let below = placement.route(vec![cx(0, 2), cx(0, 3)]);
    assert!(!placement.fell_back(), "{below:?}");
    placement.route(vec![cx(2, 3)]);
    assert!(placement.fell_back());
    assert!((0..4).all(|site| placement.position(site) == site));
    let after = placement.route(vec![cx(0, 3)]);
    assert_eq!(swaps(&after), 0);
    assert_eq!(last_operator_sites(&after), [0, 3]);
}

/// A bond read that reaches the max bond dimension makes the next batch
/// restore the layout's order first, and fall back for good.
#[test]
fn a_bond_at_the_capacity_restores_the_order_at_the_next_batch() {
    let mut placement = Placement::in_layout_order(4);
    placement.route(vec![cx(0, 3)]);
    placement.capacity_reached();
    let next = placement.route(vec![cx(1, 3)]);
    assert!(placement.fell_back());
    assert!((0..4).all(|site| placement.position(site) == site));
    assert_eq!(swaps(&next), 2, "the restore's SWAPs, and no route");
    assert_eq!(last_operator_sites(&next), [1, 3]);
}

/// On a run, a bond that reaches the max bond dimension (2, a Bell pair's)
/// makes routing fall back, and the records stay the engine routing's.
#[test]
fn a_run_whose_bond_reaches_the_capacity_records_as_the_engines_routing() {
    let program = a_branch_after_distant_gates();
    let chi = std::num::NonZeroUsize::new(2).expect("not zero");
    let run_with = |routing| {
        let mut resumer = MpsResumer::new(ExactReference::new(), 7)
            .with_routing(routing)
            .with_max_bond_dimension(chi);
        run(&mut resumer, &program, &QirShotId::first(64)).expect("the program runs")
    };
    assert_eq!(run_with(Routing::MeetHalfway), run_with(Routing::Engine));
}

/// Development only: the tuned routings, each restoring the layout's order
/// at batch ends, before draws, or past a drift budget, before and after
/// fusion.
fn tuned_routings() -> Vec<Routing> {
    let mut routings = Vec::new();
    for after_fusion in [false, true] {
        for (restore, drift_budget_permille) in [
            (Restore::BatchEnd, 1000),
            (Restore::Draws, 1000),
            (Restore::Never, 50),
        ] {
            routings.push(Routing::Tuned(RouteTuning {
                after_fusion,
                restore,
                drift_budget_permille,
                ..RouteTuning::default()
            }));
        }
    }
    routings
}

/// Development only: a run routed by a tuned routing records what the
/// engine's routing records, across draws and forks.
#[test]
fn a_tuned_routing_records_as_the_engines_routing() {
    for program in [
        distant_gates_with_draws_between(),
        a_branch_after_distant_gates(),
    ] {
        for fusion in FUSIONS {
            let expected = records(&program, Routing::Engine, fusion);
            for routing in tuned_routings() {
                assert_eq!(
                    records(&program, routing, fusion),
                    expected,
                    "{routing:?}, {fusion:?}"
                );
            }
        }
    }
}

/// Development only: restoring at batch ends, a batch leaves the layout's
/// order, by as many SWAPs as its route made inversions, and routing goes
/// on at the next batch.
#[test]
fn a_batch_end_restore_leaves_the_layouts_order_and_routing_goes_on() {
    let tuning = RouteTuning {
        restore: Restore::BatchEnd,
        ..RouteTuning::default()
    };
    let mut placement = Placement::tuned(6, tuning);
    let first = placement.route(vec![cx(0, 5)]);
    assert_eq!(swaps(&first), 8, "four to route, four to restore");
    assert!((0..6).all(|site| placement.position(site) == site));
    let second = placement.route(vec![cx(0, 5)]);
    assert!(!placement.fell_back());
    assert_eq!(swaps(&second), 8, "routed again from the layout's order");
}

/// Development only: past the drift budget, a route restores the layout's
/// order at once, without falling back.
#[test]
fn a_drift_past_the_budget_restores_the_order_without_falling_back() {
    let tuning = RouteTuning {
        drift_budget_permille: 100,
        ..RouteTuning::default()
    };
    let mut placement = Placement::tuned(6, tuning);
    let steps = placement.route(vec![cx(0, 5), cx(1, 2)]);
    assert!(!placement.fell_back());
    assert_eq!(swaps(&steps), 8, "four to route, four to restore");
    assert_eq!(last_operator_sites(&steps), [1, 2], "on the layout's order");
}
