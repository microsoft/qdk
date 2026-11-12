// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only: the bound on a state's bonds
//! (`routing/bond_bound.rs`), the bond guard's segments and policies, and
//! the length threshold (`routing.rs`).

use num_complex::Complex64;
use qdk_cpu_mps::CpuMpsEngine;
use tensornet::{MpsEngine, MpsShape};

use crate::bytecode::encoding::OPID_MZ;
use crate::mps_simulator::resumer::lowering::{Operator, Step};
use crate::mps_simulator::resumer::routing::{
    BondBound, Placement, operator_schmidt_rank, reset_routing_trace, routing_trace,
};
use crate::mps_simulator::{BoundCheck, GuardPolicy, MpsResumer, Restore, RouteTuning, Routing};
use crate::qir::{OperatorMatrix, QirGate, QirShotId};

use super::super::super::programs::{gate2, measure, program_with, ret};
use super::super::run;
use super::{a_branch_after_distant_gates, cx, distant_gates_with_draws_between, swaps};

/// A chain of `n` qubits whose every bond is `bond`.
fn chain(n: usize, bond: usize) -> MpsShape {
    shape(&vec![bond; n - 1])
}

/// A chain of qubits with the bonds `bonds`, cut by cut.
fn shape(bonds: &[usize]) -> MpsShape {
    let n = bonds.len() + 1;
    MpsShape::new(
        (0..n)
            .map(|site| {
                let left = site.checked_sub(1).map(|cut| bonds[cut]);
                let right = bonds.get(site).copied();
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect(),
    )
    .expect("a valid chain")
}

/// `gate` on the chain positions `a` and `b`, at `angle` if a rotation.
fn op(gate: QirGate, angle: f64, a: usize, b: usize) -> Operator {
    Operator {
        site_slots: [a, b],
        matrix: gate.matrix_at(matches!(gate, QirGate::Rxx | QirGate::Rzz).then_some(angle)),
    }
}

/// X ⊗ Z on the chain positions `a` and `b`: a product, of rank 1.
fn product(a: usize, b: usize) -> Operator {
    let (zero, one) = (Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0));
    let pauli_x = [[zero, one], [one, zero]];
    let pauli_z = [[one, zero], [zero, -one]];
    let mut matrix = [zero; 16];
    for (index, entry) in matrix.iter_mut().enumerate() {
        let (out, input) = (index / 4, index % 4);
        *entry = pauli_x[out >> 1][input >> 1] * pauli_z[out & 1][input & 1];
    }
    Operator {
        site_slots: [a, b],
        matrix: OperatorMatrix::Two(matrix),
    }
}

/// The rank across the cut is the fewest products the operator is a sum
/// of: 1 for a product, 2 for a CNOT or a ZZ rotation, 4 for a SWAP.
#[test]
fn an_operators_schmidt_rank_counts_its_products() {
    let rank = |operator: Operator| operator_schmidt_rank(operator.matrix.row_major());
    assert_eq!(rank(product(0, 1)), 1);
    assert_eq!(rank(op(QirGate::Cx, 0.0, 0, 1)), 2);
    assert_eq!(rank(op(QirGate::Rzz, 0.3, 0, 1)), 2);
    assert_eq!(rank(op(QirGate::Rzz, 0.0, 0, 1)), 1, "the identity");
    assert_eq!(rank(op(QirGate::Swap, 0.0, 0, 1)), 4);
}

/// The bound at a cut after a two-site operator there is the least of
/// twice each neighbouring cut's bound, the operator's rank times the
/// cut's, and the largest bond a state has there: each term binds alone
/// on a bound read to suit it.
#[test]
fn a_two_site_operator_bounds_its_cut_by_the_least_term() {
    let capacity = chain(6, 64);
    let walked = |bonds: &[usize], operator: Operator, cut: usize| {
        let mut bound = BondBound::prepared(&capacity);
        bound.read(&shape(bonds));
        bound.walk(&operator);
        bound.at(cut)
    };
    let cx = |cut| op(QirGate::Cx, 0.0, cut, cut + 1);
    let swap = |cut| op(QirGate::Swap, 0.0, cut, cut + 1);
    assert_eq!(walked(&[2, 4, 1, 4, 2], cx(2), 2), 2, "the rank term");
    assert_eq!(walked(&[2, 1, 4, 4, 2], swap(2), 2), 2, "the left term");
    assert_eq!(walked(&[2, 4, 4, 1, 2], swap(2), 2), 2, "the right term");
    assert_eq!(walked(&[8, 8, 8, 8, 8], swap(2), 2), 8, "the exact term");
    assert_eq!(
        walked(&[8, 8, 8, 8, 8], swap(0), 0),
        2,
        "past the chain's end"
    );
    assert_eq!(
        walked(&[8, 8, 8, 8, 8], swap(1), 2),
        8,
        "another cut is left as it was"
    );
}

/// A two-site operator the engine routes leaves every cut it crosses at
/// the capacity's bond, and is never at risk.
#[test]
fn a_routed_operator_leaves_the_cuts_it_crosses_at_the_capacity() {
    let mut bound = BondBound::prepared(&chain(6, 3));
    assert!(!bound.walk(&op(QirGate::Cx, 0.0, 1, 4)));
    assert_eq!(
        (0..5).map(|cut| bound.at(cut)).collect::<Vec<_>>(),
        [1, 3, 3, 3, 1]
    );
}

/// Right after an engine's route, the next operator on the pair of its
/// last return SWAP may fuse with it, so its rank term is not used; an
/// operator elsewhere first splits it, and the rank term applies again.
#[test]
fn the_pair_a_route_ends_on_drops_its_rank_term_once() {
    let capacity = chain(6, 1);
    let mut bound = BondBound::prepared(&capacity);
    bound.walk(&op(QirGate::Cx, 0.0, 0, 3));
    bound.walk(&product(2, 3));
    assert_eq!(bound.at(2), 2, "twice the neighbours' bound");
    let mut bound = BondBound::prepared(&capacity);
    bound.walk(&op(QirGate::Cx, 0.0, 0, 3));
    bound.walk(&product(4, 5));
    bound.walk(&product(2, 3));
    assert_eq!(bound.at(2), 1, "the product's rank times the capacity");
}

/// The operator is at risk exactly when its bound passes the capacity's
/// bond at its cut.
#[test]
fn an_operator_is_at_risk_when_its_bound_passes_the_capacity() {
    let mut bound = BondBound::prepared(&chain(6, 2));
    assert!(!bound.walk(&op(QirGate::Cx, 0.0, 2, 3)), "2 is not above 2");
    assert!(!bound.walk(&op(QirGate::Cx, 0.0, 1, 2)), "2 is not above 2");
    assert!(!bound.walk(&op(QirGate::Cx, 0.0, 3, 4)), "2 is not above 2");
    assert!(bound.walk(&op(QirGate::Cx, 0.0, 2, 3)), "4 is above 2");
}

/// splitmix64.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).expect("n fits")).expect("below n")
    }

    fn angle(&mut self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "53 bits")]
        let unit = (self.next() >> 11) as f64 / (1_u64 << 53) as f64;
        0.1 + 2.8 * unit
    }
}

/// A random operator on `n` qubits: one-site, on neighbours (of each
/// rank), or on sites further apart.
fn random_operator(random: &mut Random, n: usize) -> Operator {
    let gates = [QirGate::Rzz, QirGate::Rxx, QirGate::Cx, QirGate::Swap];
    match random.below(10) {
        0 | 1 => Operator {
            site_slots: [random.below(n), 0],
            matrix: QirGate::Ry.matrix_at(Some(random.angle())),
        },
        2 | 3 => {
            let a = random.below(n - 3);
            let b = a + 2 + random.below(n - a - 2);
            op(QirGate::Rxx, random.angle(), b, a)
        }
        _ => {
            let cut = random.below(n - 1);
            let gate = gates[random.below(gates.len())];
            if random.below(2) == 0 {
                op(gate, random.angle(), cut, cut + 1)
            } else {
                op(gate, random.angle(), cut + 1, cut)
            }
        }
    }
}

/// The CPU engine's bonds, read every `every` operators, never pass the
/// bound walked on them; read after every operator, neither does the
/// need of a neighbours' operator's split (the bond the cutoff alone
/// would keep). Random circuits on 8 and 10 qubits, at bonds 2, 4 and 8,
/// whose operators fuse until a read.
#[test]
fn the_cpu_engines_bonds_and_needs_stay_within_the_bound() {
    let engine = CpuMpsEngine::default();
    let mut random = Random(17);
    let mut checked = 0;
    for (n, chi) in [(8, 2), (8, 4), (10, 4), (10, 8)] {
        for every in [1, 3, 7] {
            let capacity = MpsShape::qubit_chain(n, chi).expect("a chain");
            let mut state = engine.prepare(&capacity).expect("prepared");
            let mut bound = BondBound::prepared(&capacity);
            qdk_cpu_mps::dev_trace::start_split_log();
            for step in 1..=300 {
                let operator = random_operator(&mut random, n);
                bound.walk(&operator);
                engine
                    .apply(&mut state, operator.sites(), operator.matrix.row_major())
                    .expect("applied");
                if step % every == 0 {
                    let read = engine.shape(&state).expect("a shape");
                    assert_eq!(bound.exceeded_by(&read), 0, "n {n}, χ {chi}, step {step}");
                    let splits = qdk_cpu_mps::dev_trace::take_split_log();
                    if every == 1
                        && let &[a, b] = operator.sites()
                        && a.abs_diff(b) == 1
                    {
                        for split in splits.iter().filter(|split| !split.transient) {
                            assert!(
                                split.need <= bound.at(split.cut),
                                "n {n}, χ {chi}, step {step}: {split:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
            qdk_cpu_mps::dev_trace::stop_split_log();
        }
    }
    assert!(checked > 100, "{checked} needs checked");
}

/// A placement routed by the bond guard at bond 2 on six sites, restoring
/// at batch ends.
fn guarded(policy: GuardPolicy) -> Placement {
    let tuning = RouteTuning {
        restore: Restore::BatchEnd,
        guard: Some(policy),
        ..RouteTuning::default()
    };
    Placement::tuned(6, tuning).bounded(&chain(6, 2))
}

/// A route at risk on a bound walked since the last read ends the segment
/// before it, and asks for a read: the first route of (0, 5) leaves every
/// bound at 2, and (1, 4)'s second SWAP would pass it.
#[test]
fn an_operator_at_risk_on_a_walked_bound_ends_the_segment_for_a_read() {
    for policy in [GuardPolicy::Cut, GuardPolicy::HandOver] {
        let mut placement = guarded(policy);
        let segment = placement.route_segment(&[cx(0, 5), cx(1, 4)]);
        assert!(segment.needs_read, "{policy:?}");
        assert_eq!(segment.routed, 1, "{policy:?}");
        assert_eq!(swaps(&segment.steps), 4, "{policy:?}: (0, 5)'s route");
    }
}

/// Read, the bonds may leave the operator safe: it is routed as `rb`
/// routes it, with no restore.
#[test]
fn an_operator_safe_on_the_bonds_read_is_routed() {
    for policy in [GuardPolicy::Cut, GuardPolicy::HandOver] {
        reset_routing_trace();
        let mut placement = guarded(policy);
        placement.route_segment(&[cx(0, 5)]);
        placement.bonds_read(&chain(6, 1));
        let segment = placement.route_segment(&[cx(1, 4)]);
        assert!(!segment.needs_read);
        assert_eq!(swaps(&segment.steps), 4, "{policy:?}: (1, 4)'s route alone");
        assert_eq!(routing_trace().guard_at_risk, 0, "{policy:?}");
    }
}

/// Still at risk on the bonds read, policy `Cut` restores the layout's
/// order, then routes the operator from it.
#[test]
fn cut_restores_the_order_then_routes_the_operator() {
    reset_routing_trace();
    let mut placement = guarded(GuardPolicy::Cut);
    placement.route_segment(&[cx(0, 5)]);
    placement.bonds_read(&chain(6, 2));
    let segment = placement.route_segment(&[cx(1, 4)]);
    assert!(!segment.needs_read);
    assert_eq!(swaps(&segment.steps), 6, "four to restore, two to route");
    assert_eq!(
        (placement.position(1), placement.position(4)),
        (2, 3),
        "(1, 4) routed from the layout's order"
    );
    let trace = routing_trace();
    assert_eq!((trace.guard_at_risk, trace.guard_cuts), (1, 1));
}

/// Still at risk on the bonds read, policy `HandOver` restores the
/// layout's order and sends the rest of the batch to the engine as it is;
/// the next batch is routed again.
#[test]
fn hand_over_restores_the_order_and_sends_the_rest_of_the_batch_as_is() {
    reset_routing_trace();
    let mut placement = guarded(GuardPolicy::HandOver);
    placement.route_segment(&[cx(0, 5)]);
    placement.bonds_read(&chain(6, 2));
    let segment = placement.route_segment(&[cx(1, 4), cx(0, 5)]);
    assert_eq!(swaps(&segment.steps), 4, "the restore's SWAPs alone");
    let sites: Vec<Vec<usize>> = segment.steps[4..]
        .iter()
        .map(|step| match step {
            Step::Apply(operator) => operator.sites().to_vec(),
            other => panic!("an operator, not {other:?}"),
        })
        .collect();
    assert_eq!(sites, [vec![1, 4], vec![0, 5]]);
    assert_eq!(routing_trace().guard_handovers, 1);
    let mut next = Vec::new();
    placement.start_batch(&mut next);
    placement.bonds_read(&chain(6, 1));
    let segment = placement.route_segment(&[cx(0, 5)]);
    assert_eq!(swaps(&segment.steps), 4, "routed again");
}

/// Policy `HandOverUnread` hands over at risk on a bound walked since the
/// last read, without asking for a read.
#[test]
fn hand_over_unread_hands_over_on_a_walked_bound_without_a_read() {
    reset_routing_trace();
    let mut placement = guarded(GuardPolicy::HandOverUnread);
    let segment = placement.route_segment(&[cx(0, 5), cx(1, 4)]);
    assert!(!segment.needs_read);
    assert_eq!(segment.routed, 2);
    assert_eq!(
        swaps(&segment.steps),
        8,
        "(0, 5)'s route, then the restore's SWAPs"
    );
    let trace = routing_trace();
    assert_eq!((trace.guard_at_risk, trace.guard_handovers), (1, 1));
}

/// Under `Cut`, an operator at risk in the layout's order has nothing to
/// restore: it is routed without a read.
#[test]
fn cut_in_the_layouts_order_needs_no_read() {
    let tuning = RouteTuning {
        guard: Some(GuardPolicy::Cut),
        ..RouteTuning::default()
    };
    reset_routing_trace();
    let mut placement = Placement::tuned(6, tuning).bounded(&chain(6, 1));
    let segment = placement.route_segment(&[
        Step::Apply(op(QirGate::Cx, 0.0, 2, 3)),
        Step::Apply(op(QirGate::Cx, 0.0, 2, 3)),
    ]);
    assert!(!segment.needs_read);
    assert_eq!(segment.routed, 2);
    assert_eq!(routing_trace().guard_at_risk, 2, "read, then walked");
}

/// A segment ends at a draw, included, where the group reads the bonds.
#[test]
fn a_segment_ends_at_a_draw() {
    let mut placement = guarded(GuardPolicy::Cut);
    let measure = Step::Measure {
        site: 0,
        result: 0,
        reset: false,
    };
    let segment = placement.route_segment(&[cx(0, 1), measure, cx(0, 1)]);
    assert!(segment.at_draw && !segment.needs_read);
    assert_eq!(segment.routed, 2);
}

/// The length threshold restores the layout's order after its number of
/// routed operators, and a draw between starts the count again.
#[test]
fn the_length_threshold_restores_after_its_routes_unless_a_draw_comes_between() {
    let tuning = RouteTuning {
        cut_after_routed: 2,
        ..RouteTuning::default()
    };
    let measure = Step::Measure {
        site: 2,
        result: 0,
        reset: false,
    };
    let cuts = |batch: Vec<Step>| {
        reset_routing_trace();
        Placement::tuned(6, tuning).route(batch);
        routing_trace().threshold_cuts
    };
    assert_eq!(cuts(vec![cx(0, 5), cx(1, 4)]), 1);
    assert_eq!(cuts(vec![cx(0, 5), measure, cx(1, 4)]), 0);
}

/// Development only: the routings of the bond guard, the threshold and
/// the checks, before fusion.
fn guarded_routings() -> Vec<Routing> {
    let tuned = |tuning: RouteTuning| {
        Routing::Tuned(RouteTuning {
            restore: Restore::BatchEnd,
            ..tuning
        })
    };
    vec![
        tuned(RouteTuning {
            guard: Some(GuardPolicy::Cut),
            ..RouteTuning::default()
        }),
        tuned(RouteTuning {
            guard: Some(GuardPolicy::HandOver),
            ..RouteTuning::default()
        }),
        tuned(RouteTuning {
            guard: Some(GuardPolicy::HandOverUnread),
            ..RouteTuning::default()
        }),
        tuned(RouteTuning {
            guard: Some(GuardPolicy::Cut),
            check_bound: BoundCheck::EveryApply,
            ..RouteTuning::default()
        }),
        tuned(RouteTuning {
            cut_after_routed: 1,
            ..RouteTuning::default()
        }),
        tuned(RouteTuning {
            check_bound: BoundCheck::Reads,
            ..RouteTuning::default()
        }),
    ]
}

/// Development only: on the exact reference at bond 2, where the guard
/// re-reads and acts, a guarded run records what the engine's routing
/// records, across draws and forks.
#[test]
fn a_guarded_run_records_as_the_engines_routing() {
    let chi = std::num::NonZeroUsize::new(2).expect("not zero");
    for program in [
        distant_gates_with_draws_between(),
        a_branch_after_distant_gates(),
    ] {
        let records = |routing| {
            let mut resumer = MpsResumer::new(tensornet::reference::ExactReference::new(), 7)
                .with_routing(routing)
                .with_max_bond_dimension(chi);
            run(&mut resumer, &program, &QirShotId::first(64)).expect("the program runs")
        };
        let expected = records(Routing::Engine);
        for routing in guarded_routings() {
            reset_routing_trace();
            assert_eq!(records(routing), expected, "{routing:?}");
        }
    }
}

/// Development only: on the CPU engine at bond 2, a guarded run's every
/// read, and every operator's bonds, stay within the bound, and the
/// guard acts.
#[test]
fn a_guarded_run_on_the_cpu_engine_stays_within_the_bound() {
    let chi = std::num::NonZeroUsize::new(2).expect("not zero");
    let wide = program_with(
        8,
        8,
        &[],
        &[&[
            gate2(crate::bytecode::encoding::OPID_CX, 0, 7),
            gate2(crate::bytecode::encoding::OPID_CX, 1, 6),
            gate2(crate::bytecode::encoding::OPID_CX, 2, 5),
            gate2(crate::bytecode::encoding::OPID_CX, 0, 4),
            gate2(crate::bytecode::encoding::OPID_CX, 3, 7),
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            measure(OPID_MZ, 2, 2),
            measure(OPID_MZ, 3, 3),
            measure(OPID_MZ, 4, 4),
            measure(OPID_MZ, 5, 5),
            measure(OPID_MZ, 6, 6),
            measure(OPID_MZ, 7, 7),
            ret(),
        ]],
    );
    let mut acted = 0;
    for program in [
        distant_gates_with_draws_between(),
        a_branch_after_distant_gates(),
        wide,
    ] {
        for routing in guarded_routings().into_iter().filter(|routing| {
            routing.tuning().is_some_and(|tuning| {
                tuning.guard.is_some() || tuning.check_bound != BoundCheck::Off
            })
        }) {
            reset_routing_trace();
            let mut resumer = MpsResumer::new(CpuMpsEngine::default(), 7)
                .with_routing(routing)
                .with_max_bond_dimension(chi);
            run(&mut resumer, &program, &QirShotId::first(16)).expect("the program runs");
            let trace = routing_trace();
            assert!(trace.bound_checks > 0, "{routing:?}");
            assert_eq!(trace.bound_violations, 0, "{routing:?}");
            acted += trace.guard_at_risk + trace.guard_rereads;
        }
    }
    assert!(acted > 0, "the guard re-read or acted");
}
