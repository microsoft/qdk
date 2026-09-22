use super::*;
use crate::simulation::contraction::adapter::{
    CuTensorNetOptimizer, CuTensorNetOptimizerSettings, CuTensorNetPlanningReport,
    WorkspaceBudgetSource, import_plan,
};
use qdk_simulators::execution::{
    ContractionOptimizer, CostEstimate, EstimateKind, PlanningConstraints,
};
use tensornet::{ContractionPlan, ContractionStep, Operand, PlanError};

fn optimizer_settings() -> CuTensorNetOptimizerSettings {
    let native = settings();
    CuTensorNetOptimizerSettings {
        hyper_samples: native.hyper_samples,
        threads: native.threads,
        seed: native.seed,
        reconfiguration_iterations: native.reconfiguration_iterations,
        disable_rank_simplification: native.disable_rank_simplification,
    }
}

fn constraints(bytes: u64) -> PlanningConstraints {
    PlanningConstraints {
        workspace_bytes: Some(bytes),
    }
}

fn axes(ids: &[u32]) -> Indices {
    let network = chain();
    Indices::new(
        ids.iter()
            .map(|id| {
                *network
                    .nodes()
                    .iter()
                    .flat_map(Indices::as_slice)
                    .find(|axis| axis.id() == *id)
                    .expect("fixture mode")
            })
            .collect(),
    )
    .expect("consistent fixture axes")
}

fn supplied_plan(query: &ContractionQuery<'_>) -> ContractionPlan {
    ContractionPlan::new(
        query,
        vec![
            ContractionStep::new(vec![Operand::Input(1), Operand::Input(2)], axes(&[53, 23])),
            ContractionStep::new(vec![Operand::Input(0), Operand::Result(0)], axes(&[53, 11])),
            ContractionStep::new(
                vec![Operand::Input(3), Operand::Result(1)],
                query.keep().clone(),
            ),
        ],
    )
    .expect("valid supplied plan with noncanonical intermediate axes")
}

fn optimize(
    api: Arc<TestDoubleContractionApi>,
    constraints: PlanningConstraints,
) -> Result<(ContractionPlan, CuTensorNetPlanningReport), SimulationError> {
    let mut session = SessionResources::new(api, 0)?;
    let network = chain();
    let result = CuTensorNetOptimizer::new(&mut session).optimize(
        &query(&network),
        constraints,
        optimizer_settings(),
    );
    combine_execution_and_cleanup(result, session.close())
}

fn import(api: Arc<TestDoubleContractionApi>) -> Result<(), SimulationError> {
    let mut session = SessionResources::new(api, 0)?;
    let network = chain();
    let query = query(&network);
    let result = import_plan(&mut session, &query, &supplied_plan(&query))
        .and_then(ContractionResources::close);
    combine_execution_and_cleanup(result, session.close())
}

fn assert_no_numerical_work(api: &TestDoubleContractionApi) {
    for event in [
        "allocate",
        "copy_to_device",
        "create_workspace",
        "workspace_memory_size",
        "set_workspace_memory",
        "prepare_contraction",
        "contract",
    ] {
        assert!(!api.events().contains(&event), "unexpected {event}");
    }
}

#[test]
fn selected_plan_and_report_outlive_optimizer_session_and_query() {
    let source = TestDoubleContractionApi::with_observations(
        Vec::new(),
        Corruption::None,
        NativeObservations {
            estimates: [1234.5, 0.0],
            ..NativeObservations::default()
        },
    );
    let (plan, report) =
        optimize(source.clone(), constraints(67_108_864)).expect("shared optimizer succeeds");
    source.assert_released();
    assert_no_numerical_work(&source);
    assert!(!source.events().contains(&"memory_info"));
    assert_eq!(
        plan.steps()[0].operands(),
        [Operand::Input(1), Operand::Input(2)]
    );
    assert_eq!(
        plan.steps()[1].operands(),
        [Operand::Input(0), Operand::Result(0)]
    );
    assert_eq!(
        plan.steps()[2].operands(),
        [Operand::Input(3), Operand::Result(1)]
    );
    assert_eq!(report.effective_workspace_bytes, 67_108_864);
    assert_eq!(report.workspace_source, WorkspaceBudgetSource::Explicit);
    let shared = report.as_ref();
    assert_eq!(shared.optimizer, "cuTensorNet");
    assert_eq!(shared.search_seconds, None);
    assert_eq!(shared.accepted_constraints, constraints(67_108_864));
    assert_eq!(
        shared.estimates,
        [
            CostEstimate {
                provider: "cuTensorNet",
                quantity: EstimateKind::FlopCount,
                value: 1234.5,
            },
            CostEstimate {
                provider: "cuTensorNet",
                quantity: EstimateKind::LargestIntermediateElements,
                value: 0.0,
            },
        ]
    );
    assert!(
        source
            .state
            .lock()
            .expect("state")
            .settings
            .contains(&(OptimizerSetting::DisableSlicing, 1))
    );
    drop(source);

    for _ in 0..2 {
        let destination = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        let mut session = SessionResources::new(destination.clone(), 0).expect("session");
        let resources = {
            let network = chain();
            let query = query(&network);
            let local_plan = plan.clone();
            import_plan(&mut session, &query, &local_plan).expect("fresh import")
        };
        assert_eq!(resources.tensor_ids(), [90, 7, 400, 12]);
        resources.close().expect("close imported topology");
        session.close().expect("close destination session");
        assert!(!destination.events().contains(&"optimize"));
        assert!(!destination.events().contains(&"create_optimizer_config"));
        assert!(!destination.events().contains(&"estimate"));
        assert_no_numerical_work(&destination);
        destination.assert_released();
    }
}

#[test]
fn reversed_pairs_and_ordered_output_do_not_depend_on_native_mode_order() {
    let selected = NativeMetadata {
        path: vec![[2, 1], [2, 0], [1, 0]],
        ..metadata()
    };
    let api = TestDoubleContractionApi::with_observations(
        Vec::new(),
        Corruption::None,
        NativeObservations {
            selected: selected.clone(),
            ..NativeObservations::default()
        },
    );
    let network = chain();
    let query = ContractionQuery::new(&network, axes(&[71, 11])).expect("reordered output");
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let (plan, _) = CuTensorNetOptimizer::new(&mut session)
        .optimize(&query, constraints(17), optimizer_settings())
        .expect("optimize reversed pairs");
    assert_eq!(
        plan.steps()[0].operands(),
        [Operand::Input(2), Operand::Input(1)]
    );
    assert_eq!(
        plan.steps()[1].operands(),
        [Operand::Result(0), Operand::Input(0)]
    );
    assert_eq!(
        plan.steps()[2].operands(),
        [Operand::Result(1), Operand::Input(3)]
    );
    assert_eq!(plan.steps()[0].result_axes(), &axes(&[53, 23]));
    assert_eq!(plan.steps()[1].result_axes(), &axes(&[53, 11]));
    assert_eq!(plan.steps()[2].result_axes(), query.keep());
    let mut imported = import_plan(&mut session, &query, &plan).expect("import selected path");
    assert_eq!(imported.export().expect("owned metadata"), selected);
    imported.close().expect("close import");
    session.close().expect("close session");
    assert_eq!(api.state.lock().expect("state").output, Some(vec![71, 11]));
    assert_eq!(
        api.events()
            .iter()
            .filter(|&&event| event == "optimize")
            .count(),
        1
    );
    api.assert_released();
}

#[test]
fn supplied_logical_axes_are_not_rewritten_to_native_order() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    let network = chain();
    let query = query(&network);
    let plan = supplied_plan(&query);
    let original = plan.clone();
    let mut resources = import_plan(&mut session, &query, &plan).expect("supported logical order");
    assert_eq!(resources.export().expect("native metadata"), metadata());
    assert_eq!(plan, original);
    resources.close().expect("close import");
    session.close().expect("close session");
    assert!(!api.events().contains(&"optimize"));
    api.assert_released();
}

#[test]
fn scalar_results_diagonals_and_hyperedges_keep_query_semantics() {
    let scalar_network =
        TensorNetwork::new(vec![axes(&[11, 23]), axes(&[11, 23])]).expect("scalar network");
    let hyperedge_network =
        TensorNetwork::new(vec![axes(&[11, 11, 23]), axes(&[23, 37]), axes(&[23, 37])])
            .expect("diagonal and hyperedge network");
    for (network, keep, path, modes) in [
        (scalar_network, axes(&[]), vec![[1, 0]], vec![vec![]]),
        (
            hyperedge_network,
            axes(&[11]),
            vec![[0, 1], [0, 1]],
            vec![vec![37, 11, 23], vec![11]],
        ),
    ] {
        let selected = NativeMetadata {
            path,
            slicing: Vec::new(),
            num_slices: 1,
        };
        let api = TestDoubleContractionApi::with_observations(
            Vec::new(),
            Corruption::None,
            NativeObservations {
                selected: selected.clone(),
                modes,
                ..NativeObservations::default()
            },
        );
        let query = ContractionQuery::new(&network, keep).expect("query");
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let (plan, _) = CuTensorNetOptimizer::new(&mut session)
            .optimize(&query, constraints(1024), optimizer_settings())
            .expect("native plan satisfies shared semantics");
        assert_eq!(
            plan.steps().last().expect("final step").result_axes(),
            query.keep()
        );
        let mut imported = import_plan(&mut session, &query, &plan).expect("fresh import");
        assert_eq!(imported.export().expect("imported metadata"), selected);
        imported.close().expect("close import");
        session.close().expect("close session");
        api.assert_released();
    }
}

#[test]
fn automatic_budget_is_half_of_current_free_memory_each_time() {
    let api = TestDoubleContractionApi::with_observations(
        Vec::new(),
        Corruption::None,
        NativeObservations {
            memory: vec![(1025, 4096), (259, 4096), (2, 4096)],
            ..NativeObservations::default()
        },
    );
    let network = chain();
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    {
        let mut optimizer = CuTensorNetOptimizer::new(&mut session);
        for expected in [512, 129, 1] {
            let (_, report) = optimizer
                .optimize(
                    &query(&network),
                    PlanningConstraints::default(),
                    optimizer_settings(),
                )
                .expect("automatic budget");
            assert_eq!(report.effective_workspace_bytes, expected);
            assert_eq!(report.workspace_source, WorkspaceBudgetSource::Automatic);
            assert_eq!(report.as_ref().accepted_constraints.workspace_bytes, None);
        }
    }
    session.close().expect("close session");
    assert_eq!(
        api.state.lock().expect("state").workspace_constraints,
        [512, 129, 1]
    );
    let events = api.events();
    for (index, event) in events.iter().enumerate() {
        if *event == "memory_info" {
            assert_eq!(events[index - 1], "set_device");
        }
    }
    assert_no_numerical_work(&api);
    api.assert_released();
}

#[test]
fn explicit_budget_is_forwarded_without_a_memory_probe() {
    for bytes in [1, u64::MAX] {
        let api = TestDoubleContractionApi::new(vec![("memory_info", 1)], Corruption::None);
        let (_, report) = optimize(api.clone(), constraints(bytes)).expect("explicit budget");
        assert_eq!(report.effective_workspace_bytes, bytes);
        assert_eq!(
            api.state.lock().expect("state").workspace_constraints,
            [bytes]
        );
        assert!(!api.events().contains(&"memory_info"));
        api.assert_released();
    }
}

#[test]
fn invalid_or_unavailable_budgets_fail_without_native_topology() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let error = optimize(api.clone(), constraints(0)).expect_err("zero is not automatic");
    assert!(matches!(
        error,
        SimulationError::InvalidContractionConfiguration { .. }
    ));
    assert!(error.to_string().contains("path-search"));
    assert!(!api.events().contains(&"memory_info"));
    assert!(!api.events().contains(&"create_network"));
    api.assert_released();

    for memory in [(0, 4096), (1, 4096), (4097, 4096)] {
        let api = TestDoubleContractionApi::with_observations(
            Vec::new(),
            Corruption::None,
            NativeObservations {
                memory: vec![memory],
                ..NativeObservations::default()
            },
        );
        let error =
            optimize(api.clone(), PlanningConstraints::default()).expect_err("invalid budget");
        if memory.0 > memory.1 {
            assert!(matches!(error, SimulationError::InvalidNativeResult { .. }));
        } else {
            assert!(error.to_string().contains("automatic workspace budget"));
        }
        assert!(!api.events().contains(&"create_network"));
        api.assert_released();
    }
    for event in [("memory_info", 1), ("set_device", 2)] {
        let api = TestDoubleContractionApi::new(vec![event], Corruption::None);
        assert!(optimize(api.clone(), PlanningConstraints::default()).is_err());
        assert!(!api.events().contains(&"create_network"));
        api.assert_released();
    }
}

#[test]
fn invalid_settings_fail_without_native_topology() {
    for settings in [
        CuTensorNetOptimizerSettings {
            hyper_samples: -1,
            ..optimizer_settings()
        },
        CuTensorNetOptimizerSettings {
            threads: 0,
            ..optimizer_settings()
        },
        CuTensorNetOptimizerSettings {
            seed: -1,
            ..optimizer_settings()
        },
        CuTensorNetOptimizerSettings {
            reconfiguration_iterations: -1,
            ..optimizer_settings()
        },
    ] {
        let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = chain();
        assert!(matches!(
            CuTensorNetOptimizer::new(&mut session).optimize(
                &query(&network),
                constraints(1),
                settings
            ),
            Err(SimulationError::InvalidContractionConfiguration { .. })
        ));
        session.close().expect("close session");
        assert!(!api.events().contains(&"create_network"));
        api.assert_released();
    }
}

#[test]
fn invalid_plan_and_unsupported_capabilities_are_distinct() {
    let network = chain();
    let original_query = query(&network);
    let plan = supplied_plan(&original_query);
    let reordered_query = ContractionQuery::new(&network, axes(&[71, 11])).expect("query");
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let mut session = SessionResources::new(api.clone(), 0).expect("session");
    assert!(matches!(
        import_plan(&mut session, &reordered_query, &plan),
        Err(SimulationError::InvalidContractionPlan {
            error: PlanError::WrongOutputAxes { .. }
        })
    ));
    let nary = ContractionPlan::new(
        &original_query,
        vec![ContractionStep::new(
            (0..4).map(Operand::Input).collect(),
            original_query.keep().clone(),
        )],
    )
    .expect("model permits nonpairwise steps");
    assert!(matches!(
        import_plan(&mut session, &original_query, &nary),
        Err(SimulationError::UnsupportedContraction { .. })
    ));
    let single = TensorNetwork::new(vec![axes(&[11, 71])]).expect("single input");
    let single_query = query(&single);
    let trivial =
        ContractionPlan::new(&single_query, Vec::new()).expect("model permits trivial plan");
    assert!(matches!(
        import_plan(&mut session, &single_query, &trivial),
        Err(SimulationError::UnsupportedContraction { .. })
    ));
    assert!(matches!(
        CuTensorNetOptimizer::new(&mut session).optimize(
            &single_query,
            constraints(1),
            optimizer_settings(),
        ),
        Err(SimulationError::UnsupportedContraction { .. })
    ));
    session.close().expect("close session");
    assert!(!api.events().contains(&"create_network"));
    api.assert_released();
}

#[test]
fn native_width_capabilities_are_checked_before_creating_resources() {
    let mut unsupported = vec![(u32::MAX, 2)];
    if let Ok(extent) = usize::try_from(1_u64 << 63) {
        unsupported.push((11, extent));
    }
    for (id, dim) in unsupported {
        let node =
            Indices::new(vec![Index::new(id, dim).expect("model axis")]).expect("model node");
        let network = TensorNetwork::new(vec![node.clone(), node]).expect("model network");
        let query = ContractionQuery::new(&network, axes(&[])).expect("scalar query");
        let plan = ContractionPlan::new(
            &query,
            vec![ContractionStep::new(
                vec![Operand::Input(0), Operand::Input(1)],
                axes(&[]),
            )],
        )
        .expect("model-valid plan");
        let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        assert!(matches!(
            import_plan(&mut session, &query, &plan),
            Err(SimulationError::UnsupportedContraction { .. })
        ));
        assert!(matches!(
            CuTensorNetOptimizer::new(&mut session).optimize(
                &query,
                constraints(1),
                optimizer_settings()
            ),
            Err(SimulationError::UnsupportedContraction { .. })
        ));
        session.close().expect("close session");
        assert!(!api.events().contains(&"create_network"));
        api.assert_released();
    }
}

#[test]
fn sliced_native_selection_is_not_silently_exported_as_unsliced() {
    let api = TestDoubleContractionApi::with_observations(
        Vec::new(),
        Corruption::None,
        NativeObservations {
            selected: NativeMetadata {
                slicing: vec![SlicedMode {
                    mode: 23,
                    extent: 1,
                }],
                num_slices: 3,
                ..metadata()
            },
            ..NativeObservations::default()
        },
    );
    assert!(matches!(
        optimize(api.clone(), constraints(1024)),
        Err(SimulationError::UnsupportedContraction { .. })
    ));
    api.assert_released();
}

#[test]
fn native_mode_sets_are_validated_not_just_mode_membership() {
    for modes in [
        vec![vec![23], vec![11, 53], vec![11, 71]],
        vec![vec![23, 37], vec![11, 53], vec![11, 71]],
        vec![vec![23, 71], vec![11, 53], vec![11, 71]],
        vec![vec![23, 53], vec![11, 53], vec![11, 53]],
    ] {
        let api = TestDoubleContractionApi::with_observations(
            Vec::new(),
            Corruption::None,
            NativeObservations {
                modes,
                ..NativeObservations::default()
            },
        );
        assert!(matches!(
            optimize(api.clone(), constraints(1024)),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
fn optimizer_native_failures_and_missing_estimates_propagate_with_cleanup() {
    for failure in [
        ("create_network", 1),
        ("append_tensor", 2),
        ("set_output", 1),
        ("set_compute_f64", 1),
        ("create_optimizer_info", 1),
        ("create_optimizer_config", 1),
        ("configure_optimizer", 4),
        ("optimize", 1),
        ("read_path", 1),
        ("num_sliced_modes", 1),
        ("read_slicing", 1),
        ("num_slices", 1),
        ("intermediate_mode_counts", 1),
        ("intermediate_modes", 1),
        ("estimate", 1),
        ("estimate", 2),
    ] {
        let api = TestDoubleContractionApi::new(vec![failure], Corruption::None);
        let error = optimize(api.clone(), constraints(1024)).expect_err("native failure");
        assert!(error.to_string().contains(failure.0), "{error}");
        api.assert_released();
    }
    for values in [[f64::NAN, 1.0], [1.0, f64::INFINITY], [1.0, -1.0]] {
        let api = TestDoubleContractionApi::with_observations(
            Vec::new(),
            Corruption::None,
            NativeObservations {
                estimates: values,
                ..NativeObservations::default()
            },
        );
        assert!(matches!(
            optimize(api.clone(), constraints(1024)),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
fn malformed_native_metadata_does_not_escape_the_shared_adapter() {
    for corruption in [
        Corruption::DuplicateId,
        Corruption::PathCount,
        Corruption::PathOperand,
        Corruption::NegativeSlicedCount,
        Corruption::ExcessSlicedCount,
        Corruption::ChangedSlicedCount,
        Corruption::SliceCount,
        Corruption::NegativeRank,
        Corruption::ExcessRank,
        Corruption::UnknownMode,
        Corruption::DuplicateMode,
    ] {
        let api = TestDoubleContractionApi::new(Vec::new(), corruption);
        assert!(matches!(
            optimize(api.clone(), constraints(1024)),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
        let api = TestDoubleContractionApi::new(Vec::new(), corruption);
        assert!(matches!(
            import(api.clone()),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
fn import_failures_close_fresh_owners_without_search() {
    for failure in [
        ("create_network", 1),
        ("append_tensor", 2),
        ("set_output", 1),
        ("set_compute_f64", 1),
        ("create_optimizer_info", 1),
        ("set_path", 1),
        ("set_slicing", 1),
        ("attach_optimizer_info", 1),
        ("read_path", 1),
        ("num_sliced_modes", 1),
        ("read_slicing", 1),
        ("num_slices", 1),
        ("intermediate_mode_counts", 1),
        ("intermediate_modes", 1),
    ] {
        let api = TestDoubleContractionApi::new(vec![failure], Corruption::None);
        let error = import(api.clone()).expect_err("failed import");
        assert!(error.to_string().contains(failure.0), "{error}");
        assert!(!api.events().contains(&"optimize"));
        assert!(!api.events().contains(&"create_optimizer_config"));
        api.assert_released();
    }
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::ChangedPath);
    assert!(matches!(
        import(api.clone()),
        Err(SimulationError::InvalidNativeResult { .. })
    ));
    api.assert_released();
}

#[test]
fn primary_conversion_and_cleanup_failures_are_both_observable() {
    for failures in [
        vec![("set_output", 1), ("destroy_network", 1)],
        vec![
            ("estimate", 1),
            ("destroy_optimizer_info", 1),
            ("destroy_optimizer_config", 1),
            ("destroy_network", 1),
        ],
    ] {
        let api = TestDoubleContractionApi::new(failures.clone(), Corruption::None);
        let error = optimize(api.clone(), constraints(1024)).expect_err("combined failure");
        assert!(matches!(
            error,
            SimulationError::ExecutionAndCleanupFailed { .. }
        ));
        for (event, _) in failures {
            assert!(error.to_string().contains(event), "{error}");
            assert_eq!(api.events().iter().filter(|&&e| e == event).count(), 1);
        }
        api.assert_released();
    }
    let api = TestDoubleContractionApi::new(
        vec![("destroy_optimizer_info", 1), ("destroy_network", 1)],
        Corruption::ChangedPath,
    );
    let error = import(api.clone()).expect_err("conversion and cleanup failure");
    assert!(matches!(
        error,
        SimulationError::ExecutionAndCleanupFailed { .. }
    ));
    for message in [
        "changed the supplied path",
        "destroy_optimizer_info",
        "destroy_network",
    ] {
        assert!(error.to_string().contains(message), "{error}");
    }
    api.assert_released();
}

#[test]
fn successful_planning_does_not_hide_cleanup_failure() {
    for event in [
        "destroy_optimizer_info",
        "destroy_optimizer_config",
        "destroy_network",
    ] {
        let api = TestDoubleContractionApi::new(vec![(event, 1)], Corruption::None);
        let error = optimize(api.clone(), constraints(1024)).expect_err("cleanup failure");
        assert!(error.to_string().contains(event), "{error}");
        assert_eq!(api.events().iter().filter(|&&e| e == event).count(), 1);
        api.assert_released();
    }
}
