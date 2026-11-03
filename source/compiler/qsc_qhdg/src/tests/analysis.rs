use std::collections::BTreeSet;

use crate::{
    Analysis, AnalysisCache, AnalysisContext, AnalysisError, AnalysisKey, AnalysisKind,
    AnalysisManager, AnalysisScope, AnalysisScopeKind, AnalysisValue, CachedAnalysis,
};

#[test]
fn deterministic_analysis_cache_rejects_stale_entries_and_invalidates_dependents() {
    let graph = crate::Graph {
        revision: 3,
        ..crate::Graph::default()
    };
    let demand = AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Node(crate::StableNodeId(1)),
    };
    let effects = AnalysisKey {
        kind: AnalysisKind::Effects,
        scope: AnalysisScope::Graph,
    };
    let mut cache = AnalysisCache::default();
    cache.insert(
        &graph,
        demand.clone(),
        CachedAnalysis {
            revision: 3,
            dependencies: BTreeSet::new(),
            value: AnalysisValue::Nodes(BTreeSet::from([crate::StableNodeId(1)])),
        },
    );
    cache.insert(
        &graph,
        effects.clone(),
        CachedAnalysis {
            revision: 3,
            dependencies: BTreeSet::from([demand.clone()]),
            value: AnalysisValue::Effects(BTreeSet::from(["memory".to_string()])),
        },
    );
    let mut stale = graph.clone();
    stale.revision = 2;
    assert!(cache.get(&stale, &demand).is_none());
    assert!(cache.get(&graph, &demand).is_some());
    assert!(
        cache
            .get(
                &crate::Graph {
                    revision: 3,
                    ..crate::Graph::default()
                },
                &demand
            )
            .is_none()
    );

    let footprint = crate::GraphFootprint {
        nodes: BTreeSet::from([crate::StableNodeId(1)]),
        invalidations: BTreeSet::from(["demand".to_string()]),
        ..crate::GraphFootprint::default()
    };
    assert_eq!(
        cache.invalidate(&footprint),
        BTreeSet::from([demand, effects])
    );
}

#[test]
fn ownership_analysis_cache_uses_typed_footprint_invalidations() {
    let graph = crate::Graph {
        revision: 1,
        ..crate::Graph::default()
    };
    let key = AnalysisKey {
        kind: AnalysisKind::Ownership,
        scope: AnalysisScope::Graph,
    };
    let mut cache = AnalysisCache::default();
    cache.insert(
        &graph,
        key.clone(),
        CachedAnalysis {
            revision: 1,
            dependencies: BTreeSet::new(),
            value: AnalysisValue::Ownership(BTreeSet::from([crate::OwnerPath::root(1)])),
        },
    );
    let footprint = crate::GraphFootprint {
        owner_paths: BTreeSet::from([crate::OwnerPath::root(1)]),
        invalidations: BTreeSet::from(["ownership".to_string()]),
        ..crate::GraphFootprint::default()
    };
    assert_eq!(cache.invalidate(&footprint), BTreeSet::from([key]));
}

#[test]
fn a_scoped_entry_survives_an_edit_confined_to_another_scope() {
    let graph = crate::Graph {
        revision: 1,
        ..crate::Graph::default()
    };
    // The invalidation names say which analyses an edit disturbs, not where.
    // Applying them to a scoped entry would invalidate every region on every
    // edit anywhere, which would leave the scope doing no work at all.
    let here = AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Region(1),
    };
    let elsewhere = AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Region(2),
    };
    let whole = AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Graph,
    };
    let entry = CachedAnalysis {
        revision: 1,
        dependencies: BTreeSet::new(),
        value: AnalysisValue::Nodes(BTreeSet::new()),
    };
    let mut cache = AnalysisCache::default();
    for key in [&here, &elsewhere, &whole] {
        cache.insert(&graph, key.clone(), entry.clone());
    }

    let footprint = crate::GraphFootprint {
        regions: BTreeSet::from([crate::RegionId(1)]),
        nodes: BTreeSet::from([crate::StableNodeId(7)]),
        invalidations: BTreeSet::from(["demand".to_string()]),
        ..crate::GraphFootprint::default()
    };
    assert_eq!(
        cache.invalidate(&footprint),
        BTreeSet::from([here, whole]),
        "an edit in region 1 leaves region 2 alone and still takes the whole-graph entry"
    );
}

#[test]
fn an_empty_footprint_invalidates_nothing() {
    let graph = crate::Graph {
        revision: 1,
        ..crate::Graph::default()
    };
    let key = AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Graph,
    };
    let mut cache = AnalysisCache::default();
    cache.insert(
        &graph,
        key.clone(),
        CachedAnalysis {
            revision: 1,
            dependencies: BTreeSet::new(),
            value: AnalysisValue::Nodes(BTreeSet::new()),
        },
    );
    assert!(
        cache
            .invalidate(&crate::GraphFootprint::default())
            .is_empty()
    );
    assert!(cache.get(&graph, &key).is_some());
}

/// An analysis that answers with every node in the graph. The answer is
/// uninteresting on purpose: what these tests are about is whether it is
/// computed once or twice, which the manager counts.
struct EveryNode;

impl Analysis for EveryNode {
    type Output = BTreeSet<crate::StableNodeId>;

    const KIND: AnalysisKind = AnalysisKind::Demand;
    const SCOPE: AnalysisScopeKind = AnalysisScopeKind::Graph;

    fn dependencies() -> &'static [AnalysisKey] {
        &[]
    }

    fn compute(
        context: &AnalysisContext<'_>,
        _scope: &AnalysisScope,
    ) -> Result<Self::Output, AnalysisError> {
        Ok(context
            .graph()
            .nodes()
            .filter_map(|node| context.graph().stable_id(node.id))
            .collect())
    }
}

fn one_node_graph() -> crate::Graph {
    let mut builder = crate::GraphBuilder::new();
    let region = builder.add_region(None, crate::RegionKind::ModuleOmega);
    builder
        .add_node(region, "constant:1", Vec::new())
        .expect("node");
    builder.finish()
}

fn demand_graph(demanded: Option<usize>) -> crate::Graph {
    let mut builder = crate::GraphBuilder::new();
    let region = builder.add_region(None, crate::RegionKind::ModuleOmega);
    for index in 0..2 {
        let node = builder
            .add_node(region, format!("constant:{index}"), Vec::new())
            .expect("node");
        if demanded == Some(index) {
            builder.demand(node);
        }
    }
    builder.finish()
}

fn read_demand(
    manager: &mut AnalysisManager,
    graph: &crate::Graph,
) -> BTreeSet<crate::StableNodeId> {
    manager
        .get::<crate::Demand>(
            &AnalysisContext::new(graph, &crate::Adjacency::new(graph)),
            AnalysisScope::Graph,
        )
        .expect("demand computes")
}

#[test]
fn independent_equal_revision_graphs_compute_their_own_demand() {
    let first = demand_graph(Some(0));
    let second = demand_graph(Some(1));
    let mut manager = AnalysisManager::default();
    assert_eq!(first.revision(), second.revision());
    assert_eq!(
        read_demand(&mut manager, &first),
        BTreeSet::from([crate::StableNodeId(0)])
    );
    assert_eq!(
        read_demand(&mut manager, &second),
        BTreeSet::from([crate::StableNodeId(1)])
    );
    assert_eq!(
        read_demand(&mut manager, &second),
        BTreeSet::from([crate::StableNodeId(1)])
    );
    assert_eq!(manager.computations(), 2);
}

#[test]
fn divergent_equal_revision_clones_compute_their_own_demand() {
    let mut first = demand_graph(None);
    let mut second = first.clone();
    let mut edit = first.transaction();
    edit.demand(crate::NodeId(0));
    edit.commit().expect("first fork");
    let mut edit = second.transaction();
    edit.demand(crate::NodeId(1));
    edit.commit().expect("second fork");
    let mut manager = AnalysisManager::default();
    assert_eq!(first.revision(), second.revision());
    assert_eq!(
        read_demand(&mut manager, &first),
        BTreeSet::from([crate::StableNodeId(0)])
    );
    assert_eq!(
        read_demand(&mut manager, &second),
        BTreeSet::from([crate::StableNodeId(1)])
    );
    assert_eq!(manager.computations(), 2);
}

#[test]
fn transaction_shadow_reads_never_poison_rolled_back_graph_analyses() {
    let mut graph = demand_graph(None);
    let mut manager = AnalysisManager::default();
    assert!(read_demand(&mut manager, &graph).is_empty());
    {
        let mut edit = graph.transaction();
        edit.demand(crate::NodeId(0));
        assert_eq!(
            read_demand(&mut manager, edit.graph()),
            BTreeSet::from([crate::StableNodeId(0)])
        );
        edit.demand(crate::NodeId(1));
        assert_eq!(
            read_demand(&mut manager, edit.graph()),
            BTreeSet::from([crate::StableNodeId(0), crate::StableNodeId(1)])
        );
    }
    assert!(read_demand(&mut manager, &graph).is_empty());
    assert_eq!(graph.revision(), 0);
    assert_eq!(manager.computations(), 4);
}

#[test]
fn builder_snapshot_forks_and_direct_edits_do_not_reuse_demand() {
    let mut builder = crate::GraphBuilder::new();
    let region = builder.add_region(None, crate::RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "constant:1", Vec::new())
        .expect("node");
    let snapshot = builder.clone().finish();
    let mut manager = AnalysisManager::default();
    assert!(read_demand(&mut manager, &snapshot).is_empty());
    builder.demand(node);
    let mut graph = builder.finish();
    assert_eq!(
        read_demand(&mut manager, &graph),
        BTreeSet::from([crate::StableNodeId(0)])
    );
    let extra = graph
        .add_node(region, "constant:2", Vec::new())
        .expect("node");
    graph.demand(extra);
    assert_eq!(
        read_demand(&mut manager, &graph),
        BTreeSet::from([crate::StableNodeId(0), crate::StableNodeId(1)])
    );
    assert_eq!(graph.revision(), 0);
    assert_eq!(manager.computations(), 3);
}

#[test]
fn deserialized_replacement_cannot_reuse_a_dropped_graphs_analysis() {
    let mut manager = AnalysisManager::default();
    {
        let graph = demand_graph(Some(0));
        assert_eq!(
            read_demand(&mut manager, &graph),
            BTreeSet::from([crate::StableNodeId(0)])
        );
    }
    let replacement: crate::Graph =
        serde_json::from_str(&serde_json::to_string(&demand_graph(Some(1))).expect("serialize"))
            .expect("deserialize");
    assert_eq!(
        read_demand(&mut manager, &replacement),
        BTreeSet::from([crate::StableNodeId(1)])
    );
    assert_eq!(manager.computations(), 2);
}

#[test]
fn a_registered_analysis_is_computed_once_and_served_afterwards() {
    let graph = one_node_graph();
    let adjacency = crate::Adjacency::new(&graph);
    let context = AnalysisContext::new(&graph, &adjacency);
    let mut manager = AnalysisManager::default();

    let first = manager
        .get::<EveryNode>(&context, AnalysisScope::Graph)
        .expect("first read computes");
    let second = manager
        .get::<EveryNode>(&context, AnalysisScope::Graph)
        .expect("second read is served");

    assert_eq!(first, second);
    assert_eq!(
        manager.computations(),
        1,
        "the second read must come from the cache"
    );
}

#[test]
fn an_edit_to_the_analysed_scope_forces_a_recomputation() {
    let mut graph = one_node_graph();
    let region = graph.regions().next().expect("region").id;
    let adjacency = crate::Adjacency::new(&graph);
    let mut manager = AnalysisManager::default();
    manager
        .get::<EveryNode>(
            &AnalysisContext::new(&graph, &adjacency),
            AnalysisScope::Graph,
        )
        .expect("first read computes");

    let mut transaction = graph.transaction();
    transaction
        .add_node(region, "constant:2", Vec::new())
        .expect("second node");
    let footprint = transaction.commit().expect("commit");
    manager.invalidate(&footprint);

    let adjacency = crate::Adjacency::new(&graph);
    let after_edit = manager
        .get::<EveryNode>(
            &AnalysisContext::new(&graph, &adjacency),
            AnalysisScope::Graph,
        )
        .expect("second read recomputes");

    assert_eq!(after_edit.len(), 2, "the recomputation sees the new node");
    assert_eq!(manager.computations(), 2);
}

#[test]
fn an_analysis_read_at_the_wrong_kind_of_scope_is_refused() {
    let graph = one_node_graph();
    let adjacency = crate::Adjacency::new(&graph);
    let context = AnalysisContext::new(&graph, &adjacency);
    let mut manager = AnalysisManager::default();
    let scope = AnalysisScope::Node(crate::StableNodeId(1));

    assert_eq!(
        manager.get::<EveryNode>(&context, scope.clone()),
        Err(AnalysisError::ScopeMismatch(scope))
    );
    assert_eq!(manager.computations(), 0);
}

#[test]
fn a_cached_entry_holding_another_analysis_value_is_refused_rather_than_coerced() {
    let graph = one_node_graph();
    let adjacency = crate::Adjacency::new(&graph);
    let context = AnalysisContext::new(&graph, &adjacency);
    let mut cache = AnalysisCache::default();
    cache.insert(
        &graph,
        AnalysisKey {
            kind: AnalysisKind::Demand,
            scope: AnalysisScope::Graph,
        },
        CachedAnalysis {
            revision: graph.revision(),
            dependencies: BTreeSet::new(),
            value: AnalysisValue::Stage("not a node set".to_string()),
        },
    );
    let mut manager = AnalysisManager::new(cache);

    assert_eq!(
        manager.get::<EveryNode>(&context, AnalysisScope::Graph),
        Err(AnalysisError::ValueMismatch)
    );
}
