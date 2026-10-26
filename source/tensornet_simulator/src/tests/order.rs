// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order ([`QubitOrder`]): an order is a permutation, refused
//! otherwise before anything is prepared or run; the identity order is the
//! terminal without one, bit for bit; any order draws the same values with
//! the same probabilities, reads the same runs and answers the same
//! queries, since it holds the same state; the engine is handed each
//! qubit's site for every operator, P(One), projection and reset; and an
//! order that keeps the pairs a circuit entangles together needs smaller
//! bonds, which is what it is for. And, on H₂'s bytecode (`#[ignore]`d),
//! reverse Cuthill–McKee in Rust gives `order.py`'s order and spans.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use executor_protocol::{Close as _, PerShot, ShotSet, SubmissionWrapper, open};
use num_complex::Complex64;
use qdk_simulators::{
    OutputRecord, QubitID,
    execution::{PreparedAdaptiveProgram, RegionInstruction, RuntimeAdapter, UnitaryOperation},
};
use rand::{SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};
use tensornet::{
    MpsEngine, MpsShape, SampleSeed,
    reference::{ExactReference, ReferenceError, ReferenceState},
};

use super::{
    TestDoubleEngine,
    cases::{STRINGS, expectations, scrambled},
    crossing_pairs, cx,
    fusion::{random_adaptive_program, walk},
    h, measure, measure_reset, pauli, region, run_batch,
};
use crate::{
    GateFusion, MeasurementRun, MpsStack, QubitOrder, QubitOrderError, ReadoutDistribution,
    TensorNetShots, TensorNetTerminal, mps_capacity,
};

/// A random order of `qubits` qubits, drawn from `seed`.
fn shuffled(qubits: usize, seed: u64) -> QubitOrder {
    let mut sites: Vec<usize> = (0..qubits).collect();
    sites.shuffle(&mut StdRng::seed_from_u64(seed));
    QubitOrder::new(sites).expect("a permutation")
}

fn per_shot(
    qubits: usize,
    fusion: GateFusion,
    readout: ReadoutDistribution,
    order: Option<QubitOrder>,
) -> PerShot<TensorNetShots<ExactReference>> {
    let per_shot = MpsStack::new(ExactReference::new())
        .with_gate_fusion(fusion)
        .with_readout_distribution(readout)
        .per_shot(qubits)
        .expect("a chain");
    match order {
        Some(order) => PerShot(per_shot.0.with_qubit_order(order).expect("fits")),
        None => per_shot,
    }
}

/// Shot `j` of `program` through the walk over `per_shot`, opened with
/// `seed`: its records, ln p, and its measurement runs, read before the
/// session closes.
fn walk_runs(
    per_shot: &PerShot<TensorNetShots<ExactReference>>,
    program: &PreparedAdaptiveProgram<u64>,
    seed: u64,
    j: u32,
) -> (Vec<OutputRecord>, f64, Vec<MeasurementRun>) {
    let session = open(per_shot, seed, &ShotSet::one(j)).expect("opens");
    let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
    let mut records = Vec::new();
    adapter.run(program, &mut records).expect("runs");
    let mut wrapper = adapter.into_inner();
    let runs = wrapper
        .inner()
        .shot(j)
        .expect("held")
        .measurement_runs()
        .to_vec();
    let report = wrapper.close();
    let log_p = report.log_p.get(j).expect("known");
    (records, log_p, runs)
}

#[test]
fn an_order_is_a_permutation_of_the_qubits() {
    let order = QubitOrder::new(vec![2, 0, 1]).expect("a permutation");
    assert_eq!(order.sites(), [2, 0, 1]);
    assert_eq!((order.site(0), order.site(1), order.site(2)), (2, 0, 1));
    assert_eq!(order.sites_of(&[1, 0]), [0, 2]);
    assert_eq!(order.len(), 3);
    assert!(!order.is_identity());
    assert!(QubitOrder::identity(3).is_identity());
    assert_eq!(QubitOrder::new(vec![0, 1, 2]), Ok(QubitOrder::identity(3)));
    assert!(QubitOrder::identity(0).is_empty());
    assert_eq!(
        QubitOrder::new(vec![0, 0, 1]),
        Err(QubitOrderError::SharedSite {
            first: 0,
            second: 1,
            site: 0
        })
    );
    assert_eq!(
        QubitOrder::new(vec![0, 3, 1]),
        Err(QubitOrderError::SiteBeyond {
            qubit: 1,
            site: 3,
            qubits: 3
        })
    );
}

#[test]
fn an_order_of_other_qubits_or_after_a_batch_is_refused_before_anything_runs() {
    let engine = Rc::new(TestDoubleEngine::counting());
    let shots = TensorNetShots::new(Rc::clone(&engine), 3, 4).expect("a chain");
    assert_eq!(
        shots.with_qubit_order(QubitOrder::identity(4)).err(),
        Some(QubitOrderError::Qubits {
            order: 4,
            qubits: 3
        })
    );
    assert_eq!(engine.prepared(), 0);
    let capacity = mps_capacity(3, 4).expect("a chain");
    let terminal = TensorNetTerminal::new(Rc::clone(&engine), capacity.clone(), 1).expect("ok");
    assert_eq!(
        terminal.with_qubit_order(QubitOrder::identity(2)).err(),
        Some(QubitOrderError::Qubits {
            order: 2,
            qubits: 3
        })
    );
    let mut terminal = TensorNetTerminal::new(Rc::clone(&engine), capacity.clone(), 1).expect("ok");
    let (_, end) = run_batch(&mut terminal, &[region([h(0)])]);
    end.expect("ran");
    let order = || QubitOrder::new(vec![1, 2, 0]).expect("a permutation");
    assert_eq!(
        terminal.with_qubit_order(order()).err(),
        Some(QubitOrderError::Started)
    );
    let mut terminal = TensorNetTerminal::new(Rc::clone(&engine), capacity, 1).expect("ok");
    let (_, end) = run_batch(&mut terminal, &[region([h(0)])]);
    end.expect("ran");
    assert_eq!(terminal.close().batches, 1);
    assert_eq!(
        terminal.with_qubit_order(order()).err(),
        Some(QubitOrderError::Started),
        "closing clears the report, not the state's placement"
    );
}

/// The identity order, set, is the terminal without one: the same records
/// and ln p, bit for bit, on random Adaptive programs (feedback, resets,
/// `MResetZ`), unfused and in pairs, with and without the readout
/// distribution.
#[test]
fn the_identity_order_is_the_default_bit_for_bit() {
    for seed in 0..20 {
        let width = 2 + (seed % 4) as u32;
        let qubits = width as usize;
        let program = random_adaptive_program(seed, width, 6);
        for fusion in [GateFusion::Unfused, GateFusion::Pairs] {
            for readout in [ReadoutDistribution::Off, ReadoutDistribution::Exact] {
                let default = per_shot(qubits, fusion, readout, None);
                assert!(default.0.qubit_order().is_identity());
                let identity =
                    per_shot(qubits, fusion, readout, Some(QubitOrder::identity(qubits)));
                for j in 1..=3 {
                    let (records, log_p, _) = walk(&default, &program, seed, j);
                    let (same, same_log_p, _) = walk(&identity, &program, seed, j);
                    assert_eq!(records, same, "program {seed}, shot {j}");
                    assert_eq!(log_p.to_bits(), same_log_p.to_bits());
                }
            }
        }
    }
}

/// Any order holds the same state on other sites, so a shot draws the
/// same values with ln p equal to rounding, and reads runs of the same
/// qubits, tags and values with distributions equal to rounding, each
/// measurement read on its qubit's site; unfused and in pairs (whose
/// blocks, ascending in qubits, may be descending in sites).
#[test]
fn any_order_draws_the_same_values_with_the_same_probabilities() {
    let mut placed = 0;
    for seed in 0..40 {
        let width = 3 + (seed % 4) as u32;
        let qubits = width as usize;
        let program = random_adaptive_program(seed, width, 6);
        let order = shuffled(qubits, 1000 + seed);
        placed += usize::from(!order.is_identity());
        for fusion in [GateFusion::Unfused, GateFusion::Pairs] {
            for readout in [ReadoutDistribution::Off, ReadoutDistribution::Exact] {
                let identity = per_shot(qubits, fusion, readout, None);
                let ordered = per_shot(qubits, fusion, readout, Some(order.clone()));
                for j in 1..=3 {
                    let at = format!("program {seed}, {fusion:?}, {readout:?}, shot {j}");
                    let (records, log_p, runs) = walk_runs(&identity, &program, seed, j);
                    let (same, same_log_p, same_runs) = walk_runs(&ordered, &program, seed, j);
                    assert_eq!(records, same, "{at}");
                    assert!((log_p - same_log_p).abs() < 1e-9, "{at}");
                    assert_eq!(runs.len(), same_runs.len(), "{at}");
                    for (run, same) in runs.iter().zip(&same_runs) {
                        assert_eq!(run.qubits(), same.qubits(), "{at}");
                        assert_eq!(run.tags(), same.tags(), "{at}");
                        assert_eq!(run.values(), same.values(), "{at}");
                        assert_eq!(run.sites(), run.qubits(), "{at}");
                        assert_eq!(same.sites(), order.sites_of(same.qubits()), "{at}");
                        let distance = run
                            .distribution()
                            .iter()
                            .zip(same.distribution())
                            .map(|(a, b)| (a - b).abs())
                            .fold(0.0, f64::max);
                        assert!(distance < 1e-12, "{at}: {distance}");
                    }
                }
            }
        }
    }
    assert!(placed >= 30, "{placed} of 40 orders are not the identity");
}

/// The state queries on shots with an order place the program's qubits so
/// too: the same expectations as without one.
#[test]
fn queries_on_ordered_shots_read_the_same_expectations() {
    let observables: Vec<_> = STRINGS
        .iter()
        .map(|labels| pauli(1.0, labels, &[0, 1, 2, 3]))
        .collect();
    for seed in 0..10 {
        let program = scrambled(4, seed).measure_all(&[0, 1, 2, 3]).build();
        let identity = TensorNetShots::new(Rc::new(ExactReference::new()), 4, 16).expect("ok");
        let ordered = TensorNetShots::new(Rc::new(ExactReference::new()), 4, 16)
            .expect("ok")
            .with_qubit_order(shuffled(4, 2000 + seed))
            .expect("fits");
        let expected = expectations(&identity, &program, &observables);
        let actual = expectations(&ordered, &program, &observables);
        for ((a, e), labels) in actual.iter().zip(&expected).zip(STRINGS) {
            assert!(
                (a - e).norm() < 1e-12,
                "seed {seed}, ⟨{labels}⟩: {a} vs {e}"
            );
        }
    }
}

/// What the terminal asks of the engine, by site.
#[derive(Clone, Debug, PartialEq)]
enum Call {
    Apply(Vec<usize>),
    ProbabilityOfOne(usize),
    Conditional(usize, Vec<usize>),
    Project(usize),
}

/// [`ExactReference`], recording the sites of each call.
#[derive(Default)]
struct TestDoubleRecordingEngine {
    calls: RefCell<Vec<Call>>,
}

impl TestDoubleRecordingEngine {
    fn take(&self) -> Vec<Call> {
        self.calls.take()
    }
}

impl MpsEngine for TestDoubleRecordingEngine {
    type State = ReferenceState;
    type Error = ReferenceError;

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceState, ReferenceError> {
        ExactReference.prepare(capacity)
    }

    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceError> {
        self.calls.borrow_mut().push(Call::Apply(sites.to_vec()));
        ExactReference.apply(state, sites, matrix)
    }

    fn probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
    ) -> Result<f64, ReferenceError> {
        self.calls.borrow_mut().push(Call::ProbabilityOfOne(site));
        ExactReference.probability_of_one(state, site)
    }

    fn conditional_probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, ReferenceError> {
        self.calls.borrow_mut().push(Call::Conditional(
            site,
            given.iter().map(|&(at, _)| at).collect(),
        ));
        ExactReference.conditional_probability_of_one(state, site, given)
    }

    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), ReferenceError> {
        self.calls.borrow_mut().push(Call::Project(site));
        ExactReference.project(state, site, one, probability)
    }

    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, ReferenceError> {
        ExactReference.fork(state)
    }

    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, ReferenceError> {
        ExactReference.sample(state, sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &tensornet::PauliSum,
    ) -> Result<Complex64, ReferenceError> {
        ExactReference.expectation(state, observable)
    }

    fn shape(&self, state: &ReferenceState) -> Result<MpsShape, ReferenceError> {
        ExactReference.shape(state)
    }

    fn retained_weight(&self, state: &ReferenceState) -> Result<f64, ReferenceError> {
        ExactReference.retained_weight(state)
    }
}

/// Qubit 0 on site 2, qubit 1 on site 0, qubit 2 on site 1: each operator
/// goes to its qubits' sites in operand order (a pair block, ascending in
/// qubits, in the sites of those qubits), and each P(One), projection and
/// reset flip to the measured qubit's site; with the readout distribution,
/// each condition is on the site of the qubit it was drawn for.
#[test]
fn the_engine_is_handed_each_qubits_site() {
    let order = QubitOrder::new(vec![2, 0, 1]).expect("a permutation");
    let terminal = |engine: &Rc<TestDoubleRecordingEngine>, fusion, readout| {
        TensorNetTerminal::new(Rc::clone(engine), mps_capacity(3, 8).expect("a chain"), 3)
            .expect("prepares")
            .with_gate_fusion(fusion)
            .with_readout_distribution(readout)
            .with_qubit_order(order.clone())
            .expect("fits")
    };
    let engine = Rc::new(TestDoubleRecordingEngine::default());
    let mut unfused = terminal(&engine, GateFusion::Unfused, ReadoutDistribution::Off);
    // X 2 makes the reset of qubit 2 draw One, so it flips it back.
    let batch = [
        region([h(0), cx(0, 1), UnitaryOperation::X { target: 2 }]),
        measure(1, 0),
        RegionInstruction::Reset { qubit: 2 },
    ];
    let (_, end) = run_batch(&mut unfused, &batch);
    end.expect("ran");
    assert_eq!(
        engine.take(),
        [
            Call::Apply(vec![2]),
            Call::Apply(vec![2, 0]),
            Call::Apply(vec![1]),
            Call::ProbabilityOfOne(0),
            Call::Project(0),
            Call::ProbabilityOfOne(1),
            Call::Project(1),
            Call::Apply(vec![1]),
        ]
    );
    // In pairs, CX(1, 0) is the block on qubits [0, 1]: sites [2, 0].
    let mut pairs = terminal(&engine, GateFusion::Pairs, ReadoutDistribution::Exact);
    let batch = [region([h(1), cx(1, 0)]), measure(0, 0), measure_reset(1, 1)];
    let (_, end) = run_batch(&mut pairs, &batch);
    end.expect("ran");
    let _ = pairs.close();
    let calls = engine.take();
    assert_eq!(calls[0], Call::Apply(vec![2, 0]));
    assert_eq!(calls[1], Call::ProbabilityOfOne(2));
    assert!(
        calls[2..].iter().all(
            |call| matches!(call, Call::Conditional(0, given) if *given == [2])
                || matches!(call, Call::Project(2 | 0))
                || *call == Call::Apply(vec![0])
        ),
        "{calls:?}"
    );
    assert!(calls.contains(&Call::Conditional(0, vec![2])), "{calls:?}");
    let projected: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, Call::Project(_)))
        .collect();
    assert_eq!(projected, [&Call::Project(2), &Call::Project(0)]);
    let runs = pairs.measurement_runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].qubits(), [0, 1]);
    assert_eq!(runs[0].sites(), [2, 0]);
}

/// Two Bell pairs, (0, 2) and (1, 3), cross the middle of four qubits on
/// their own sites (bond 4 there); an order that places each pair on
/// neighbouring sites holds the same state with no bond above 2.
#[test]
fn an_order_that_keeps_each_pair_together_needs_smaller_bonds() {
    let bond = |order: QubitOrder| {
        let mut terminal = TensorNetTerminal::new(
            Rc::new(ExactReference::new()),
            mps_capacity(4, 16).expect("a chain"),
            1,
        )
        .expect("prepares")
        .with_qubit_order(order)
        .expect("fits");
        let (_, end) = run_batch(&mut terminal, &[region(crossing_pairs())]);
        end.expect("ran");
        terminal.close().max_bond
    };
    assert_eq!(bond(QubitOrder::identity(4)), 4);
    let together = QubitOrder::new(vec![0, 2, 1, 3]).expect("a permutation");
    assert_eq!(bond(together), 2);
}

/// The weight of each pair of qubits (ascending): how many of `pairs` are
/// on it.
fn pair_weights(pairs: impl Iterator<Item = (QubitID, QubitID)>) -> BTreeMap<(usize, usize), u64> {
    let mut weights = BTreeMap::new();
    for (a, b) in pairs {
        *weights.entry((a.min(b), a.max(b))).or_default() += 1;
    }
    weights
}

/// The sum over `weights` of weight · |site(a) − site(b)| under `order`.
fn weighted_span(order: &QubitOrder, weights: &BTreeMap<(usize, usize), u64>) -> u64 {
    weights
        .iter()
        .map(|(&(a, b), &w)| w * order.site(a).abs_diff(order.site(b)) as u64)
        .sum()
}

/// Reverse Cuthill–McKee on the interaction graph of `qubits` qubits whose
/// edges are `weights`, as `scratch/m1-run/fusion/order.py` computes it:
/// each component from its unseen qubit of least weighted degree (then
/// least index), breadth first, neighbours by weight down, degree up, index
/// up; the visit order reversed is the chain.
fn reverse_cuthill_mckee(qubits: usize, weights: &BTreeMap<(usize, usize), u64>) -> QubitOrder {
    let mut neighbours: Vec<BTreeMap<usize, u64>> = vec![BTreeMap::new(); qubits];
    for (&(a, b), &w) in weights {
        *neighbours[a].entry(b).or_default() += w;
        *neighbours[b].entry(a).or_default() += w;
    }
    let degree: Vec<u64> = neighbours.iter().map(|n| n.values().sum()).collect();
    let mut seen = vec![false; qubits];
    let mut visits = Vec::with_capacity(qubits);
    while visits.len() < qubits {
        let start = (0..qubits)
            .filter(|&q| !seen[q])
            .min_by_key(|&q| (degree[q], q))
            .expect("an unseen qubit");
        seen[start] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(q) = queue.pop_front() {
            visits.push(q);
            let mut next: Vec<(usize, u64)> = neighbours[q].iter().map(|(&r, &w)| (r, w)).collect();
            next.sort_by_key(|&(r, w)| (std::cmp::Reverse(w), degree[r], r));
            for (r, _) in next {
                if !seen[r] {
                    seen[r] = true;
                    queue.push_back(r);
                }
            }
        }
    }
    let mut sites = vec![0; qubits];
    for (site, &qubit) in visits.iter().rev().enumerate() {
        sites[qubit] = site;
    }
    QubitOrder::new(sites).expect("a permutation")
}

/// `order.py`'s figures for H₂ (`scratch/m1-run/fusion/order.out`): the
/// site of each qubit by RCM and by its local search, and the sum of the
/// spans over the two-qubit gates and over the two-site operators of
/// `GateFusion::Pairs` for today's order, RCM and the local search.
const ORDER_PY_RCM: [usize; 29] = [
    25, 24, 26, 23, 21, 1, 8, 3, 2, 19, 17, 14, 15, 4, 5, 28, 27, 22, 20, 18, 11, 7, 6, 13, 12, 16,
    10, 9, 0,
];
const ORDER_PY_LOCAL_SEARCH: [usize; 29] = [
    26, 25, 27, 24, 21, 6, 8, 4, 3, 18, 17, 15, 16, 1, 5, 28, 23, 22, 20, 19, 10, 2, 0, 11, 12, 14,
    13, 9, 7,
];
const ORDER_PY_SPANS: [(&str, u64, u64); 3] = [
    ("identity", 628_526, 602_217),
    ("rcm", 315_223, 307_925),
    ("local search", 216_950, 211_226),
];

/// RCM in Rust, from H₂'s two-qubit gates over the regions `count.py` cuts
/// (`fusion::static_regions`), gives `order.py`'s order, and the spans of
/// today's order, RCM and the local search are `order.py`'s, over the gates
/// and over the operators of `GateFusion::Pairs`. `QDK_H2_BYTECODE` names
/// the JSON.
#[test]
#[ignore = "needs H₂'s bytecode in QDK_H2_BYTECODE"]
fn h2_reverse_cuthill_mckee_is_order_pys() {
    let path = std::env::var("QDK_H2_BYTECODE")
        .expect("QDK_H2_BYTECODE should name the JSON of H₂'s AdaptiveProgram.as_dict()");
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let program = super::adaptive_program_json::adaptive_program_from_json(&json);
    let qubits = usize::try_from(program.num_qubits).expect("fits");
    let regions = super::fusion::static_regions(&program);
    let gates =
        pair_weights(
            regions
                .iter()
                .flatten()
                .filter_map(|operation| match operation.qubits() {
                    (a, Some(b)) => Some((a, b)),
                    (_, None) => None,
                }),
        );
    let pairs = pair_weights(
        regions
            .iter()
            .flat_map(|region| GateFusion::Pairs.operators(region))
            .filter_map(|operator| match *operator.qubits() {
                [a, b] => Some((a, b)),
                _ => None,
            }),
    );
    let rcm = reverse_cuthill_mckee(qubits, &gates);
    println!("h2 rcm: the site of each qubit {:?}", rcm.sites());
    assert_eq!(rcm.sites(), ORDER_PY_RCM);
    let orders = [
        QubitOrder::identity(qubits),
        rcm,
        QubitOrder::new(ORDER_PY_LOCAL_SEARCH.to_vec()).expect("a permutation"),
    ];
    let total = |weights: &BTreeMap<_, u64>| weights.values().sum::<u64>();
    for (order, (name, over_gates, over_pairs)) in orders.iter().zip(ORDER_PY_SPANS) {
        let spans = (weighted_span(order, &gates), weighted_span(order, &pairs));
        println!(
            "h2 {name}: span {} over {} two-qubit gates, {} over {} Pairs operators",
            spans.0,
            total(&gates),
            spans.1,
            total(&pairs)
        );
        assert_eq!(spans, (over_gates, over_pairs), "{name}");
    }
}
