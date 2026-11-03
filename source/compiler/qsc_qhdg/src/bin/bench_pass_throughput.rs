// Pass-throughput benchmark for graph rewriting and storage layouts.
//
// Answers two questions with measurement rather than reasoning:
//
//   1. Where does driver time actually go: node storage, edge scanning,
//      re-matching per application, or canonical digesting?
//   2. Would a denser storage layout help, given that any replacement must
//      preserve the deterministic ordered iteration the current types provide?
//
// Driver cost decomposition uses the real crate. Storage layout comparison
// uses the real matcher's access pattern, holding determinism constant.

// Identifiers and counts are indices and rates here, not values whose exact
// width matters, and this harness only ever runs on a 64-bit host.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use qsc_qhdg::{
    Direction, DischargeContext, Edge, EdgeKind, EffectDomain, EffectEvent, EffectMode,
    EffectResource, GlobalPhasePolicy, Graph, GraphBuilder, GraphRewriteDriver,
    GraphRewriteOutcome, InverseVocabulary, Observation, OperationClass, OperationFacts, PortKind,
    RegionKind, ResourceId, RewriteContext, Stage, Type, Usage, graph_digest, graph_rewrite_rules,
    verify,
};

/// The workload builds pairs of `unitary:h`, so the driver has to be told that
/// name undoes itself before it will cancel one.
fn bench_discharge() -> DischargeContext {
    DischargeContext::new(InverseVocabulary::self_inverse(["unitary:h"]))
}

fn best_of<F: FnMut() -> T, T>(samples: usize, mut f: F) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..samples {
        let start = Instant::now();
        let value = f();
        let elapsed = start.elapsed().as_secs_f64();
        drop(value);
        best = best.min(elapsed);
    }
    best
}

fn mean_of<F: FnMut() -> T, T>(repetitions: usize, mut f: F) -> f64 {
    let mut total = 0.0;
    for _ in 0..repetitions {
        let start = Instant::now();
        let value = f();
        total += start.elapsed().as_secs_f64();
        drop(value);
    }
    total / repetitions as f64
}

fn digest_samples(
    graph: &Graph,
    samples: usize,
    best: bool,
) -> Result<f64, qsc_qhdg::SnapshotError> {
    let mut elapsed = if best { f64::MAX } else { 0.0 };
    for _ in 0..samples {
        let start = Instant::now();
        let digest = graph_digest(graph)?;
        let sample = start.elapsed().as_secs_f64();
        drop(digest);
        elapsed = if best {
            elapsed.min(sample)
        } else {
            elapsed + sample
        };
    }
    Ok(if best {
        elapsed
    } else {
        elapsed / samples as f64
    })
}

fn quantum_effect() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode: EffectMode::Write,
        resource: EffectResource::Named("q".to_string()),
        observable: false,
    }
}

/// One threaded qubit per pair: allocate, two self-inverse gates, release. Each
/// pair is an independent cancellation opportunity, so match count scales.
fn build_graph(pairs: u64) -> (Graph, RewriteContext) {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut gates = Vec::new();
    for _ in 0..pairs {
        let source = builder
            .add_node(region, "quantum.allocate", vec![])
            .expect("source");
        let source_out = builder
            .add_port(
                source,
                Direction::Output,
                PortKind::Capability,
                Type::Qubit,
                Stage::Generation,
                Usage::Affine,
            )
            .expect("port");
        let mut previous = source_out;
        let mut pair = Vec::new();
        for _ in 0..2 {
            let gate = builder
                .add_node(region, "unitary:h", vec![quantum_effect()])
                .expect("gate");
            let input = builder
                .add_port(
                    gate,
                    Direction::Input,
                    PortKind::Capability,
                    Type::Qubit,
                    Stage::Generation,
                    Usage::Affine,
                )
                .expect("port");
            let output = builder
                .add_port(
                    gate,
                    Direction::Output,
                    PortKind::Capability,
                    Type::Qubit,
                    Stage::Generation,
                    Usage::Affine,
                )
                .expect("port");
            builder
                .connect(Edge {
                    source: previous,
                    target: input,
                    kind: EdgeKind::AffineCapability,
                })
                .expect("edge");
            previous = output;
            pair.push(gate);
        }
        let sink = builder
            .add_node(region, "quantum.release", vec![])
            .expect("sink");
        let sink_in = builder
            .add_port(
                sink,
                Direction::Input,
                PortKind::Capability,
                Type::Qubit,
                Stage::Generation,
                Usage::Affine,
            )
            .expect("port");
        builder
            .connect(Edge {
                source: previous,
                target: sink_in,
                kind: EdgeKind::AffineCapability,
            })
            .expect("edge");
        gates.push(pair);
    }

    let graph = builder.finish();
    // Every observation is in force, which is the strictest thing the workload
    // can say and the setting the built-in pipeline uses.
    let mut context = RewriteContext {
        observations: Observation::all(),
        ..RewriteContext::default()
    };
    for (index, pair) in gates.iter().enumerate() {
        for gate in pair {
            let stable = graph.stable_id(*gate).expect("stable id");
            context.operation_facts.insert(
                stable,
                OperationFacts {
                    class: OperationClass::Unitary,
                    resources: BTreeSet::from([ResourceId(index as u64)]),
                    inverse_of: Some("unitary:h".to_string()),
                    // Without this the pair does not match, because a global
                    // phase nobody proved is a difference the rule will not
                    // discard. Leaving it at the default made this workload
                    // measure a scan that applies nothing, which is not a
                    // throughput measurement.
                    global_phase: GlobalPhasePolicy::Exact,
                    ..OperationFacts::default()
                },
            );
        }
    }
    (graph, context)
}

fn benchmark_driver_costs(samples: usize) -> Result<(), qsc_qhdg::SnapshotError> {
    println!();
    println!("Driver cost decomposition");
    println!();
    println!(
        "{:>8} {:>8} {:>14} {:>14} {:>16} {:>14}",
        "pairs", "nodes", "digest s", "budget=1 s", "budget=1 - 1xdig", "per-node us"
    );

    for pairs in [250u64, 500, 1000, 2000, 4000] {
        let nodes = pairs * 4;
        let (graph, _) = build_graph(pairs);

        let digest = digest_samples(&graph, samples, true)?;

        // Budget 1 performs: entry digest, one match scan, one application,
        // then the budget check. One digest, not two: the budget report used to
        // hash the graph again and no longer does, and the repeat check used to
        // hash it once per application and now compares rule and match instead.
        let single = best_of(samples, || {
            let (mut working, context) = build_graph(pairs);
            GraphRewriteDriver::new(graph_rewrite_rules(), 1)
                .run(&mut working, &context, &mut bench_discharge())
                .expect("rewrite")
        });

        let build_only = best_of(samples, || build_graph(pairs));
        let scan_and_apply = single - build_only - digest;

        println!(
            "{:>8} {:>8} {:>14.6} {:>14.6} {:>16.6} {:>14.2}",
            pairs,
            nodes,
            digest,
            single,
            scan_and_apply.max(0.0),
            (scan_and_apply.max(0.0) / nodes as f64) * 1e6
        );
    }

    println!();
    println!("Fixed application count, growing graph. Flat per-application cost");
    println!("would mean matching is worklist-bounded; growth means it is not.");
    println!();
    println!(
        "{:>8} {:>8} {:>14} {:>18}",
        "pairs", "nodes", "4 applies s", "s per application"
    );
    for pairs in [250u64, 500, 1000, 2000] {
        let nodes = pairs * 4;
        let build_only = best_of(samples, || build_graph(pairs));
        let four = best_of(samples, || {
            let (mut working, context) = build_graph(pairs);
            GraphRewriteDriver::new(graph_rewrite_rules(), 4)
                .run(&mut working, &context, &mut bench_discharge())
                .expect("rewrite")
        });
        let net = (four - build_only).max(0.0);
        println!(
            "{:>8} {:>8} {:>14.6} {:>18.6}",
            pairs,
            nodes,
            net,
            net / 4.0
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Storage layouts under identical deterministic-iteration semantics.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct PlainNode {
    id: u64,
    operation: String,
}

/// Mirrors the ordered-map layout: O(log n) lookup, ordered iteration.
struct TreeStore {
    nodes: BTreeMap<u64, PlainNode>,
    edges: BTreeSet<(u64, u64)>,
}

/// Dense slots indexed by id. Iteration in index order is id order, so
/// determinism matches the ordered map. Tombstones keep order stable
/// across removal, which the generational arena also guarantees.
struct DenseStore {
    nodes: Vec<Option<PlainNode>>,
    edges: BTreeSet<(u64, u64)>,
}

/// Dense slots plus adjacency lists, so neighbour queries avoid scanning
/// the whole edge set. Adjacency lists are kept sorted for determinism.
struct IndexedStore {
    nodes: Vec<Option<PlainNode>>,
    incoming: Vec<Vec<u64>>,
    outgoing: Vec<Vec<u64>>,
}

fn plain_nodes(pairs: u64) -> (Vec<PlainNode>, Vec<(u64, u64)>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for pair in 0..pairs {
        let first = pair * 2;
        nodes.push(PlainNode {
            id: first,
            operation: "unitary:h".to_string(),
        });
        nodes.push(PlainNode {
            id: first + 1,
            operation: "unitary:h".to_string(),
        });
        edges.push((first, first + 1));
    }
    (nodes, edges)
}

/// The access pattern the real matcher performs per seed: count incoming,
/// collect outgoing, hop once, then count the successor's edges.
fn scan_tree(store: &TreeStore) -> usize {
    let mut hits = 0;
    for node in store.nodes.values() {
        let seed = node.id;
        let incoming = store.edges.iter().filter(|(_, t)| *t == seed).count();
        if incoming > 1 {
            continue;
        }
        let outgoing: Vec<_> = store.edges.iter().filter(|(s, _)| *s == seed).collect();
        if outgoing.len() != 1 {
            continue;
        }
        let next = outgoing[0].1;
        let next_incoming = store.edges.iter().filter(|(_, t)| *t == next).count();
        if next_incoming == 1
            && store
                .nodes
                .get(&next)
                .is_some_and(|n| n.operation == node.operation)
        {
            hits += 1;
        }
    }
    hits
}

fn scan_dense(store: &DenseStore) -> usize {
    let mut hits = 0;
    for slot in store.nodes.iter().flatten() {
        let seed = slot.id;
        let incoming = store.edges.iter().filter(|(_, t)| *t == seed).count();
        if incoming > 1 {
            continue;
        }
        let outgoing: Vec<_> = store.edges.iter().filter(|(s, _)| *s == seed).collect();
        if outgoing.len() != 1 {
            continue;
        }
        let next = outgoing[0].1;
        let next_incoming = store.edges.iter().filter(|(_, t)| *t == next).count();
        let matches_next = store
            .nodes
            .get(next as usize)
            .and_then(|s| s.as_ref())
            .is_some_and(|n| n.operation == slot.operation);
        if next_incoming == 1 && matches_next {
            hits += 1;
        }
    }
    hits
}

fn scan_indexed(store: &IndexedStore) -> usize {
    let mut hits = 0;
    for slot in store.nodes.iter().flatten() {
        let seed = slot.id as usize;
        if store.incoming[seed].len() > 1 || store.outgoing[seed].len() != 1 {
            continue;
        }
        let next = store.outgoing[seed][0] as usize;
        let matches_next = store.nodes[next]
            .as_ref()
            .is_some_and(|n| n.operation == slot.operation);
        if store.incoming[next].len() == 1 && matches_next {
            hits += 1;
        }
    }
    hits
}

fn compare_storage_layouts(samples: usize) {
    println!();
    println!("Storage layout under identical deterministic iteration");
    println!();
    println!(
        "{:>8} {:>8} {:>14} {:>14} {:>14} {:>12} {:>12}",
        "pairs", "nodes", "tree s", "dense s", "indexed s", "dense x", "indexed x"
    );

    for pairs in [250u64, 500, 1000, 2000, 4000] {
        let nodes = pairs * 2;
        let (plain, edge_list) = plain_nodes(pairs);

        let tree = TreeStore {
            nodes: plain.iter().map(|n| (n.id, n.clone())).collect(),
            edges: edge_list.iter().copied().collect(),
        };

        let mut slots: Vec<Option<PlainNode>> = vec![None; nodes as usize];
        for node in &plain {
            slots[node.id as usize] = Some(node.clone());
        }
        let dense = DenseStore {
            nodes: slots.clone(),
            edges: edge_list.iter().copied().collect(),
        };

        let mut incoming = vec![Vec::new(); nodes as usize];
        let mut outgoing = vec![Vec::new(); nodes as usize];
        for (source, target) in &edge_list {
            outgoing[*source as usize].push(*target);
            incoming[*target as usize].push(*source);
        }
        for list in incoming.iter_mut().chain(outgoing.iter_mut()) {
            list.sort_unstable();
        }
        let indexed = IndexedStore {
            nodes: slots,
            incoming,
            outgoing,
        };

        let tree_s = best_of(samples, || scan_tree(&tree));
        let dense_s = best_of(samples, || scan_dense(&dense));
        let indexed_s = best_of(samples, || scan_indexed(&indexed));

        assert_eq!(scan_tree(&tree), scan_dense(&dense), "dense must agree");
        assert_eq!(
            scan_tree(&tree),
            scan_indexed(&indexed),
            "indexed must agree"
        );

        println!(
            "{:>8} {:>8} {:>14.6} {:>14.6} {:>14.6} {:>11.2}x {:>11.2}x",
            pairs,
            nodes,
            tree_s,
            dense_s,
            indexed_s,
            tree_s / dense_s,
            tree_s / indexed_s
        );
    }
}

/// Compare mean digest and verification costs against fixed reference timings.
/// The sweep counts nodes rather than gates, and both measurements use the same
/// repetition count so their costs are comparable.
fn compare_digest_and_verification(repetitions: usize) -> Result<(), qsc_qhdg::SnapshotError> {
    // Reference digest and verification timings, in milliseconds.
    let reference_timings: BTreeMap<u64, (f64, f64)> = BTreeMap::from([
        (256, (0.5254, 0.0012)),
        (512, (1.0412, 0.0034)),
        (1024, (2.1045, 0.0068)),
        (2048, (4.2628, 0.0129)),
        (4096, (8.6440, 0.0293)),
    ]);

    println!();
    println!("Digest and verification costs, mean over {repetitions} repetitions per point");
    println!();
    println!(
        "{:>8} {:>14} {:>14} {:>14} {:>14} {:>16}",
        "nodes", "digest ms", "reference ms", "verify ms", "reference ms", "digest/verify"
    );
    for nodes in [256u64, 512, 1024, 2048, 4096] {
        let (graph, _) = build_graph(nodes / 4);
        let digest = digest_samples(&graph, repetitions, false)? * 1e3;
        let checked = mean_of(repetitions, || verify(&graph)) * 1e3;
        let (reference_digest, reference_verify) = reference_timings[&nodes];
        println!(
            "{:>8} {:>14.4} {:>14.4} {:>14.4} {:>14.4} {:>16.2}",
            nodes,
            digest,
            reference_digest,
            checked,
            reference_verify,
            digest / checked
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Dead-code elimination, which the quantum gate workloads do not trigger.
// ---------------------------------------------------------------------------

/// A chain of pure classical nodes that nothing reads.
///
/// The cancelling workload is repeated `unitary:h` and the non-cancelling one
/// alternates two quantum gates, while dead-code elimination matches only pure
/// classical nodes. So neither of them exercises it at all, and a measurement
/// that cannot observe a change is not evidence that there was none.
fn build_dead_cone(depth: u64) -> Graph {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut previous = None;
    for _ in 0..depth {
        let node = builder
            .add_node(region, "pure:add", vec![])
            .expect("cone node");
        if let Some(source) = previous {
            let input = builder
                .add_port(
                    node,
                    Direction::Input,
                    PortKind::Value,
                    Type::Int,
                    Stage::Generation,
                    Usage::Copyable,
                )
                .expect("port");
            builder
                .connect(Edge {
                    source,
                    target: input,
                    kind: EdgeKind::CopyableValue,
                })
                .expect("edge");
        }
        previous = Some(
            builder
                .add_port(
                    node,
                    Direction::Output,
                    PortKind::Value,
                    Type::Int,
                    Stage::Generation,
                    Usage::Copyable,
                )
                .expect("port"),
        );
    }
    builder.finish()
}

fn benchmark_dead_code_elimination(samples: usize) {
    println!();
    println!("Dead-code elimination over a cone nothing reads");
    println!();
    println!(
        "{:>8} {:>14} {:>14} {:>16}",
        "depth", "applications", "run s", "s per node"
    );
    for depth in [64u64, 128, 256, 512] {
        let context = RewriteContext::default();
        let mut applications = 0;
        let elapsed = best_of(samples, || {
            let mut working = build_dead_cone(depth);
            let outcome = GraphRewriteDriver::new(graph_rewrite_rules(), depth as usize + 1)
                .run(&mut working, &context, &mut bench_discharge())
                .expect("rewrite");
            if let GraphRewriteOutcome::Committed {
                applications: a, ..
            } = outcome
            {
                applications = a;
            }
            working
        });
        let build_only = best_of(samples, || build_dead_cone(depth));
        let net = (elapsed - build_only).max(0.0);
        println!(
            "{:>8} {:>14} {:>14.6} {:>16.8}",
            depth,
            applications,
            net,
            net / depth as f64
        );
    }
}

/// What the per-application whole-graph digest used to cost.
///
/// The driver ran the cancelling workload to convergence hashing the whole
/// graph after every application, to decide whether it had seen the resulting
/// state before. That term is gone, and this reconstructs it from two things
/// still measurable here: how many applications convergence takes, and what one
/// digest of that graph costs. Reporting it as a reconstruction rather than as
/// an A/B is the honest form, since the removed code is removed.
///
/// Convergence is one application per pair, so this also answers whether the
/// cancelling workload is still quadratic: doubling the size doubles the
/// applications, and each application still scans the whole graph to match.
/// Sizes are small and samples few because that product is the point.
fn estimate_convergence_digest_cost(samples: usize) -> Result<(), qsc_qhdg::SnapshotError> {
    println!();
    println!("Estimated per-application digest cost at convergence");
    println!();
    println!(
        "{:>8} {:>8} {:>10} {:>14} {:>16} {:>12} {:>14}",
        "pairs", "nodes", "applies", "converge s", "removed digest s", "share", "s per apply"
    );
    for pairs in [125u64, 250, 500, 1000] {
        let nodes = pairs * 4;
        let (graph, _) = build_graph(pairs);
        let digest = digest_samples(&graph, samples, true)?;

        let mut applications = 0;
        let elapsed = best_of(samples, || {
            let (mut working, context) = build_graph(pairs);
            let outcome = GraphRewriteDriver::new(graph_rewrite_rules(), usize::MAX)
                .run(&mut working, &context, &mut bench_discharge())
                .expect("rewrite");
            applications = match outcome {
                GraphRewriteOutcome::Committed {
                    applications: a, ..
                }
                | GraphRewriteOutcome::BudgetExhausted {
                    applications: a, ..
                }
                | GraphRewriteOutcome::Oscillation {
                    applications: a, ..
                } => a,
                GraphRewriteOutcome::Unchanged => 0,
            };
            working
        });
        let build_only = best_of(samples, || build_graph(pairs));
        let net = (elapsed - build_only).max(0.0);
        let removed = applications as f64 * digest;
        println!(
            "{:>8} {:>8} {:>10} {:>14.6} {:>16.6} {:>11.1}x {:>14.6}",
            pairs,
            nodes,
            applications,
            net,
            removed,
            if net > 0.0 { removed / net } else { 0.0 },
            if applications > 0 {
                net / applications as f64
            } else {
                0.0
            }
        );
    }
    Ok(())
}

fn main() -> Result<(), qsc_qhdg::SnapshotError> {
    let samples = 5;
    println!("Pass-throughput comparison, best of {samples} samples per point.");
    benchmark_driver_costs(samples)?;
    compare_storage_layouts(samples);
    compare_digest_and_verification(20)?;
    benchmark_dead_code_elimination(samples);
    // Convergence is the expensive workload here, so it gets fewer samples.
    estimate_convergence_digest_cost(2)?;
    Ok(())
}
