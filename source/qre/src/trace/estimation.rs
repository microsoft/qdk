// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    iter::repeat_with,
    sync::{Arc, RwLock, atomic::AtomicUsize},
};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{EstimationCollection, ISA, ProvenanceGraph, ResultSummary, Trace};

/// Estimates all (trace, ISA) combinations in parallel, returning only the
/// successful results collected into an [`EstimationCollection`].
///
/// This uses a shared atomic counter as a lock-free work queue.  Each worker
/// thread atomically claims the next job index, maps it to a `(trace, isa)`
/// pair, and runs the estimation.  This keeps all available cores busy until
/// the last job completes.
///
/// # Work distribution
///
/// Jobs are numbered `0 .. traces.len() * isas.len()`.  For job index `j`:
///   - `trace_idx = j / isas.len()`
///   - `isa_idx   = j % isas.len()`
///
/// Each worker accumulates results locally and sends them back over a bounded
/// channel once it runs out of work, avoiding contention on the shared
/// collection.
#[must_use]
pub fn estimate_parallel<'a>(
    traces: &[&'a Trace],
    isas: &[&'a ISA],
    max_error: Option<f64>,
    post_process: bool,
) -> EstimationCollection {
    let total_jobs = traces.len() * isas.len();
    let num_isas = isas.len();

    // Shared atomic counter acts as a lock-free work queue.  Workers call
    // fetch_add to claim the next job index.
    let next_job = AtomicUsize::new(0);

    let mut collection = EstimationCollection::new();
    collection.set_total_jobs(total_jobs);

    std::thread::scope(|scope| {
        let num_threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        // Bounded channel so each worker can send its batch of results back
        // to the main thread without unbounded buffering.
        let (tx, rx) = std::sync::mpsc::sync_channel(num_threads);

        for _ in 0..num_threads {
            let tx = tx.clone();
            let next_job = &next_job;
            scope.spawn(move || {
                let mut local_results = Vec::new();
                loop {
                    // Atomically claim the next job.  Relaxed ordering is
                    // sufficient because there is no dependent data between
                    // jobs — each (trace, isa) pair is independent.
                    let job = next_job.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if job >= total_jobs {
                        break;
                    }

                    // Map the flat job index to a (trace, ISA) pair.
                    let trace_idx = job / num_isas;
                    let isa_idx = job % num_isas;

                    if let Ok(mut estimation) = traces[trace_idx].estimate(isas[isa_idx], max_error)
                    {
                        estimation.set_isa_index(isa_idx);
                        estimation.set_trace_index(trace_idx);

                        local_results.push(estimation);
                    }
                }
                // Send all results from this worker in one batch.
                let _ = tx.send(local_results);
            });
        }
        // Drop the cloned sender so the receiver iterator terminates once all
        // workers have finished.
        drop(tx);

        // Collect results from all workers into the shared collection.
        let mut successful = 0;
        for local_results in rx {
            if post_process {
                for result in &local_results {
                    collection.push_summary(ResultSummary {
                        trace_index: result.trace_index().unwrap_or(0),
                        isa_index: result.isa_index().unwrap_or(0),
                        qubits: result.qubits(),
                        runtime: result.runtime(),
                    });
                }
            }
            successful += local_results.len();
            collection.extend(local_results.into_iter());
        }
        collection.set_successful_estimates(successful);
    });

    // Attach ISAs only to Pareto-surviving results, avoiding O(M) HashMap
    // clones for discarded results.
    for result in collection.iter_mut() {
        if let Some(idx) = result.isa_index() {
            result.set_isa(isas[idx].clone());
        }
    }

    collection
}

/// A node in the provenance graph along with pre-computed (space, time) values
/// for pruning.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct NodeProfile {
    node_index: usize,
    space: u64,
    time: u64,
}

/// A single entry in a combination of instruction choices for estimation.
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
struct CombinationEntry {
    instruction_id: u64,
    node: NodeProfile,
}

/// Per-slot pruning witnesses: maps a context hash to the `(space, time)`
/// pairs observed in successful estimations.
type SlotWitnesses = RwLock<FxHashMap<u64, Vec<(u64, u64)>>>;

/// Computes a hash of the combination context (all slots except the excluded
/// one).  Two combinations that agree on every slot except `exclude_idx`
/// produce the same context hash.
fn combination_context_hash(combination: &[CombinationEntry], exclude_idx: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (i, entry) in combination.iter().enumerate() {
        if i != exclude_idx {
            entry.instruction_id.hash(&mut hasher);
            entry.node.node_index.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Checks whether a combination is dominated by a previously successful one.
///
/// A combination is prunable if, for any instruction slot, there exists a
/// successful combination with the same instructions in all other slots and
/// an instruction at that slot with `space <=` and `time <=`.
///
/// However, for instruction slots that affect the algorithm runtime (gate
/// instructions) when the trace has factory instructions (resource states),
/// time-based dominance is **unsound**.  A faster gate instruction reduces
/// the algorithm runtime, which reduces the number of factory runs per copy,
/// requiring more factory copies and potentially more total qubits.  The
/// "dominated" combination (with a slower gate) can therefore produce a
/// Pareto-optimal result with fewer qubits but more runtime.
///
/// When `runtime_affecting_ids` is non-empty, slots whose instruction ID
/// appears in that set are skipped entirely during the dominance check.
fn is_dominated(
    combination: &[CombinationEntry],
    trace_pruning: &[SlotWitnesses],
    runtime_affecting_ids: &FxHashSet<u64>,
) -> bool {
    for (slot_idx, entry) in combination.iter().enumerate() {
        // Skip dominance check for runtime-affecting slots when factories
        // exist, because shorter gate time can increase factory overhead.
        if runtime_affecting_ids.contains(&entry.instruction_id) {
            continue;
        }
        let ctx_hash = combination_context_hash(combination, slot_idx);
        let map = trace_pruning[slot_idx]
            .read()
            .expect("Pruning lock poisoned");
        if map.get(&ctx_hash).is_some_and(|w| {
            w.iter()
                .any(|&(ws, wt)| ws <= entry.node.space && wt <= entry.node.time)
        }) {
            return true;
        }
    }
    false
}

/// Records a successful estimation as a pruning witness for future
/// combinations.
fn record_success(combination: &[CombinationEntry], trace_pruning: &[SlotWitnesses]) {
    for (slot_idx, entry) in combination.iter().enumerate() {
        let ctx_hash = combination_context_hash(combination, slot_idx);
        let mut map = trace_pruning[slot_idx]
            .write()
            .expect("Pruning lock poisoned");
        map.entry(ctx_hash)
            .or_default()
            .push((entry.node.space, entry.node.time));
    }
}

#[derive(Default)]
struct ISAIndex {
    index: FxHashMap<Vec<CombinationEntry>, usize>,
    isas: Vec<ISA>,
}

impl From<ISAIndex> for Vec<ISA> {
    fn from(value: ISAIndex) -> Self {
        value.isas
    }
}

impl ISAIndex {
    pub fn push(&mut self, combination: &Vec<CombinationEntry>, isa: &ISA) -> usize {
        if let Some(&idx) = self.index.get(combination) {
            idx
        } else {
            let idx = self.isas.len();
            self.isas.push(isa.clone());
            self.index.insert(combination.clone(), idx);
            idx
        }
    }
}

/// Generates the cartesian product of `id_and_nodes` and pushes each
/// combination directly into `jobs`, avoiding intermediate allocations.
///
/// The cartesian product is enumerated using mixed-radix indexing.  Given
/// dimensions with sizes `[n0, n1, n2, …]`, the total number of combinations
/// is `n0 * n1 * n2 * …`.  Each combination index `i` in `0..total` uniquely
/// identifies one element from every dimension: the index into dimension `d` is
/// `(i / (n0 * n1 * … * n(d-1))) % nd`, which we compute incrementally by
/// repeatedly taking `i % nd` and then dividing `i` by `nd`.  This is
/// analogous to extracting digits from a number in a mixed-radix system.
fn push_cartesian_product(
    id_and_nodes: &[(u64, Vec<NodeProfile>)],
    trace_idx: usize,
    jobs: &mut Vec<(usize, Vec<CombinationEntry>)>,
    max_slots: &mut usize,
) {
    // The product of all dimension sizes gives the total number of
    // combinations.  If any dimension is empty the product is zero and there
    // are no valid combinations to generate.
    let total: usize = id_and_nodes.iter().map(|(_, nodes)| nodes.len()).product();
    if total == 0 {
        return;
    }

    *max_slots = (*max_slots).max(id_and_nodes.len());
    jobs.reserve(total);

    // Enumerate every combination by treating the combination index `i` as a
    // mixed-radix number.  The inner loop "peels off" one digit per dimension:
    //   node_idx = i % nodes.len()   — selects this dimension's element
    //   i       /= nodes.len()       — shifts to the next dimension's digit
    // After processing all dimensions, `i` is exhausted (becomes 0), and
    // `combo` contains exactly one entry per instruction id.
    for mut i in 0..total {
        let mut combo = Vec::with_capacity(id_and_nodes.len());
        for (id, nodes) in id_and_nodes {
            let node_idx = i % nodes.len();
            i /= nodes.len();
            let profile = nodes[node_idx];
            combo.push(CombinationEntry {
                instruction_id: *id,
                node: profile,
            });
        }
        jobs.push((trace_idx, combo));
    }
}

#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn estimate_with_graph(
    traces: &[&Trace],
    graph: &Arc<RwLock<ProvenanceGraph>>,
    max_error: Option<f64>,
    post_process: bool,
) -> EstimationCollection {
    let max_error = max_error.unwrap_or(1.0);

    // Phase 1: Pre-compute all (trace_index, combination) jobs sequentially.
    // This reads the provenance graph once per trace and generates the
    // cartesian product of Pareto-filtered nodes.  Each node carries
    // pre-computed (space, time) values for dominance pruning in Phase 2.
    let mut jobs: Vec<(usize, Vec<CombinationEntry>)> = Vec::new();

    // Use the maximum number of instruction slots across all combinations to
    // size the pruning witness structure.  This will updated while we generate
    // jobs.
    let mut max_slots = 0;

    // For each trace, collect the set of instruction IDs that affect the
    // algorithm runtime (gate instructions from the trace block structure).
    // When a trace also has resource states (factories), dominance pruning
    // on these slots is unsound because shorter gate time can increase
    // factory overhead (see `is_dominated` documentation).
    let runtime_affecting_ids: Vec<FxHashSet<u64>> = traces
        .iter()
        .map(|trace| {
            let has_factories = trace.get_resource_states().is_some_and(|rs| !rs.is_empty());
            if has_factories {
                trace.deep_iter().map(|(gate, _)| gate.id).collect()
            } else {
                FxHashSet::default()
            }
        })
        .collect();

    for (trace_idx, trace) in traces.iter().enumerate() {
        if trace.base_error() > max_error {
            continue;
        }

        let required = trace.required_instruction_ids(Some(max_error));

        let graph_lock = graph.read().expect("Graph lock poisoned");
        let id_and_nodes: Vec<_> = required
            .constraints()
            .iter()
            .filter_map(|constraint| {
                graph_lock.pareto_nodes(constraint.id()).map(|nodes| {
                    (
                        constraint.id(),
                        nodes
                            .iter()
                            .filter(|&&node| {
                                // Filter out nodes that don't meet the constraint bounds.
                                let instruction = graph_lock.instruction(node);
                                constraint.error_rate().is_none_or(|c| {
                                    c.evaluate(&instruction.error_rate(Some(1)).unwrap_or(0.0))
                                })
                            })
                            .map(|&node| {
                                let instruction = graph_lock.instruction(node);
                                let space = instruction.space(Some(1)).unwrap_or(0);
                                let time = instruction.time(Some(1)).unwrap_or(0);
                                NodeProfile {
                                    node_index: node,
                                    space,
                                    time,
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .collect();
        drop(graph_lock);

        if id_and_nodes.len() != required.len() {
            // If any required instruction is missing from the graph, we can't
            // run any estimation for this trace.
            continue;
        }

        push_cartesian_product(&id_and_nodes, trace_idx, &mut jobs, &mut max_slots);
    }

    // Sort jobs so that combinations with smaller total (space + time) are
    // processed first.  This maximises the effectiveness of dominance pruning
    // because successful "cheap" combinations establish witnesses that let us
    // skip more expensive ones.
    jobs.sort_by_key(|(_, combo)| {
        combo
            .iter()
            .map(|entry| entry.node.space + entry.node.time)
            .sum::<u64>()
    });

    let total_jobs = jobs.len();

    // Phase 2: Run estimations in parallel with dominance-based pruning.
    //
    // For each instruction slot in a combination, we track (space, time)
    // witnesses from successful estimations keyed by the "context", which is a
    // hash of the node indices in all *other* slots.  Before running an
    // estimation, we check every slot: if a witness with space ≤ and time ≤
    // exists for that context, the combination is dominated and skipped.
    let next_job = AtomicUsize::new(0);

    let pruning_witnesses: Vec<Vec<_>> = repeat_with(|| {
        repeat_with(|| RwLock::new(FxHashMap::default()))
            .take(max_slots)
            .collect()
    })
    .take(traces.len())
    .collect();

    // There are no explicit ISAs in this estimation function, as we create them
    // on the fly from the graph nodes.  For successful jobs, we will attach the
    // ISAs to the results collection in a vector with the ISA index addressing
    // that vector.  In order to avoid storing duplicate ISAs we hash the ISA
    // index.
    let isa_index = Arc::new(RwLock::new(ISAIndex::default()));

    let mut collection = EstimationCollection::new();
    collection.set_total_jobs(total_jobs);

    std::thread::scope(|scope| {
        let num_threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        let (tx, rx) = std::sync::mpsc::sync_channel(num_threads);

        for _ in 0..num_threads {
            let tx = tx.clone();
            let next_job = &next_job;
            let jobs = &jobs;
            let pruning_witnesses = &pruning_witnesses;
            let runtime_affecting_ids = &runtime_affecting_ids;
            let isa_index = Arc::clone(&isa_index);
            scope.spawn(move || {
                let mut local_results = Vec::new();
                loop {
                    let job_idx = next_job.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if job_idx >= total_jobs {
                        break;
                    }

                    let (trace_idx, combination) = &jobs[job_idx];

                    // Dominance pruning: skip if a cheaper instruction at any
                    // slot already succeeded with the same surrounding context.
                    if is_dominated(
                        combination,
                        &pruning_witnesses[*trace_idx],
                        &runtime_affecting_ids[*trace_idx],
                    ) {
                        continue;
                    }

                    let mut isa = ISA::with_graph(graph.clone());
                    for entry in combination {
                        isa.add_node(entry.instruction_id, entry.node.node_index);
                    }

                    if let Ok(mut result) = traces[*trace_idx].estimate(&isa, Some(max_error)) {
                        let isa_idx = isa_index
                            .write()
                            .expect("RwLock should not be poisoned")
                            .push(combination, &isa);
                        result.set_isa_index(isa_idx);

                        result.set_trace_index(*trace_idx);

                        local_results.push(result);
                        record_success(combination, &pruning_witnesses[*trace_idx]);
                    }
                }
                let _ = tx.send(local_results);
            });
        }
        drop(tx);

        let mut successful = 0;
        for local_results in rx {
            if post_process {
                for result in &local_results {
                    collection.push_summary(ResultSummary {
                        trace_index: result.trace_index().unwrap_or(0),
                        isa_index: result.isa_index().unwrap_or(0),
                        qubits: result.qubits(),
                        runtime: result.runtime(),
                    });
                }
            }
            successful += local_results.len();
            collection.extend(local_results.into_iter());
        }
        collection.set_successful_estimates(successful);
    });

    let isa_index = Arc::try_unwrap(isa_index)
        .ok()
        .expect("all threads joined; Arc refcount should be 1")
        .into_inner()
        .expect("RwLock should not be poisoned");

    // Attach ISAs only to Pareto-surviving results, avoiding O(M) HashMap
    // clones for discarded results.
    for result in collection.iter_mut() {
        if let Some(idx) = result.isa_index() {
            result.set_isa(isa_index.isas[idx].clone());
        }
    }

    collection.set_isas(isa_index.into());

    collection
}
