// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde_json::{Map, Value, json};

use crate::{
    estimates::{
        ErrorBudget, ErrorBudgetStrategy, ErrorCorrection, Factory, FactoryBuilder, FactoryPart,
        Overhead, PhysicalResourceEstimation, PhysicalResourceEstimationResult,
    },
    system::modeling::{floquet_code, surface_code_gate_based},
};

use super::{
    constants::MAX_DISTILLATION_ROUNDS, estimate_physical_resources,
    modeling::TFactoryDistillationUnitTemplate,
};

use crate::system::{
    Error,
    data::{ErrorBudgetSpecification, JobParams, LogicalResourceCounts},
    error::IO,
    modeling::GateBasedPhysicalQubit,
    modeling::{PhysicalQubit, Protocol, TFactory},
    optimization::TFactoryBuilder,
};

use std::{borrow::Cow, rc::Rc};

#[test]
fn estimate_single() {
    let logical_resources = LogicalResourceCounts {
        num_qubits: 100,
        t_count: 0,
        rotation_count: 112_110,
        rotation_depth: 2001,
        ccz_count: 0,
        ccix_count: 0,
        measurement_count: 0,
        num_compute_qubits: None,
        read_from_memory_count: None,
        write_to_memory_count: None,
    };

    let params: &str = "[{}]";
    let result = estimate_physical_resources(logical_resources, params);

    let json_value: Vec<Value> =
        serde_json::from_str(&result.expect("result is err")).expect("Failed to parse JSON");
    assert_eq!(json_value.len(), 1);

    let map = json_value[0].as_object().expect("Failed build map");
    assert!(!map.contains_key("frontierEntries"));
    assert!(map.contains_key("logicalQubit"));
    assert!(map.contains_key("physicalCounts"));
    assert!(map.contains_key("physicalCountsFormatted"));
}

#[test]
fn estimate_frontier() {
    let logical_resources = LogicalResourceCounts {
        num_qubits: 100,
        t_count: 0,
        rotation_count: 112_110,
        rotation_depth: 2001,
        ccz_count: 0,
        ccix_count: 0,
        measurement_count: 0,
        num_compute_qubits: None,
        read_from_memory_count: None,
        write_to_memory_count: None,
    };

    let params: &str = r#"[{
        "estimateType": "frontier"
    }]"#;

    let result = estimate_physical_resources(logical_resources, params);

    let json_value: Vec<Value> =
        serde_json::from_str(&result.expect("result is err")).expect("Failed to parse JSON");
    assert_eq!(json_value.len(), 1);

    let map = json_value[0].as_object().expect("Failed build map");
    assert!(map.contains_key("frontierEntries"));
    assert!(!map.contains_key("logicalQubit"));
    assert!(!map.contains_key("physicalCounts"));
    assert!(!map.contains_key("physicalCountsFormatted"));
}

#[test]
fn physical_estimates_crash() {
    let result = estimate_physical_resources(
        LogicalResourceCounts {
            num_qubits: 9,
            t_count: 160,
            rotation_count: 0,
            rotation_depth: 0,
            ccz_count: 8,
            ccix_count: 0,
            measurement_count: 5,
            num_compute_qubits: None,
            read_from_memory_count: None,
            write_to_memory_count: None,
        },
        r#"[{"qubitParams": {"name": "qubit_maj_ns_e6"},
            "qecScheme": {"name": "floquet_code"},
            "errorBudget": 0.075}]"#,
    );

    assert!(
        result
            .expect("estimation should succeed")
            .contains(r#""status":"success"#)
    );
}

#[derive(Clone)]
struct TestLayoutOverhead {
    num_qubits: u64,
    logical_depth: u64,
    num_tstates: u64,
}

impl TestLayoutOverhead {
    pub fn new(num_qubits: u64, logical_depth: u64, num_tstates: u64) -> Self {
        Self {
            num_qubits,
            logical_depth,
            num_tstates,
        }
    }
}

impl Overhead for TestLayoutOverhead {
    fn logical_qubits(&self) -> Result<u64, String> {
        Ok(self.num_qubits)
    }

    fn logical_depth(&self, _: &ErrorBudget) -> Result<u64, String> {
        Ok(self.logical_depth)
    }

    fn num_magic_states(&self, _: &ErrorBudget, _: usize) -> Result<u64, String> {
        Ok(self.num_tstates)
    }
}

#[test]
pub fn test_no_tstates() {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::default());

    let partitioning = ErrorBudget::new(1e-3, 0.0, 0.0);
    let layout_overhead = TestLayoutOverhead::new(12, 0, 0);

    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    assert!(estimation.estimate(&partitioning).is_err());
}

#[test]
pub fn single_tstate() -> Result<(), Error> {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::default());

    let partitioning = ErrorBudget::new(0.5e-4, 0.5e-4, 0.0);
    let layout_overhead = TestLayoutOverhead::new(4, 1, 1);

    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    estimation.estimate(&partitioning)?;

    Ok(())
}

#[test]
pub fn perfect_tstate() -> Result<(), Error> {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::GateBased(GateBasedPhysicalQubit {
        t_gate_error_rate: 0.5e-4,
        ..GateBasedPhysicalQubit::default()
    }));

    let partitioning = ErrorBudget::new(0.5e-4, 0.5e-4, 0.0);
    let layout_overhead = TestLayoutOverhead::new(4, 1, 1);

    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    estimation.estimate(&partitioning)?;

    Ok(())
}

fn hubbard_overhead_and_partitioning() -> Result<(LogicalResourceCounts, ErrorBudget), Error> {
    let logical_counts =
        serde_json::from_str(include_str!("counts.json")).map_err(IO::CannotParseJSON)?;
    let partitioning = ErrorBudgetSpecification::Total(1e-3)
        .partitioning(&logical_counts)
        .expect("partitioning should succeed");

    Ok((logical_counts, partitioning))
}

fn validate_result_invariants(result: &PhysicalResourceEstimationResult<Protocol, TFactory>) {
    let part = get_factory(result);

    assert_eq!(
        result.physical_qubits(),
        result.physical_qubits_for_factories() + result.physical_qubits_for_algorithm()
    );
    assert_eq!(
        result.physical_qubits_for_factories(),
        part.factory().physical_qubits() * part.copies()
    );

    assert!(result.logical_patch().logical_error_rate() <= result.required_logical_error_rate());

    assert!(part.factory().duration() * part.runs() <= result.runtime());
}

#[allow(clippy::too_many_lines)]
#[test]
pub fn test_hubbard_e2e() -> Result<(), Error> {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::default());
    let (layout_overhead, partitioning) = hubbard_overhead_and_partitioning()?;
    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit.clone(),
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    let result = estimation.estimate(&partitioning)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();
    let tfactory = part.factory();

    assert_eq!(logical_qubit.code_parameter(), &17);
    assert_eq!(logical_qubit.logical_cycle_time(), 6800);

    assert_eq!(result.layout_overhead().logical_qubits(), 72);
    assert_eq!(result.algorithmic_logical_depth(), 22623);
    assert_eq!(part.copies(), 14);
    assert_eq!(part.runs(), 1667);
    assert_eq!(result.physical_qubits_for_factories(), 252_000);
    assert_eq!(result.physical_qubits_for_algorithm(), 41616);
    assert_eq!(result.physical_qubits(), 293_616);
    assert_eq!(result.runtime(), 153_836_400);

    assert_eq!(tfactory.physical_qubits(), 18000);
    assert_eq!(tfactory.duration(), 92000);
    assert_eq!(tfactory.num_rounds(), 2);
    assert_eq!(tfactory.num_units_per_round(), vec![18, 1]);
    assert_eq!(
        tfactory.unit_names(),
        vec![
            String::from("15-to-1 space efficient"),
            String::from("15-to-1 RM prep")
        ]
    );

    validate_result_invariants(&result);

    let same_ftp = surface_code_gate_based();
    let output_t_error_rate = part.required_output_error_rate();
    let builder = create_factory_builder();
    let tfactories = builder
        .find_factories(
            &same_ftp,
            &qubit,
            0,
            output_t_error_rate,
            same_ftp
                .max_code_distance()
                .expect("code has max code distance"),
        )
        .expect("can compute factories");

    assert_eq!(tfactories.len(), 2);
    if let Some(factory1) = find_factory(&tfactories, 88000, 27900) {
        assert_eq!(factory1.num_rounds(), 2);
        assert_eq!(factory1.num_units_per_round(), vec![18, 1]);
        assert_eq!(
            factory1.unit_names(),
            vec![
                String::from("15-to-1 RM prep"),
                String::from("15-to-1 RM prep")
            ]
        );
        assert_eq!(
            factory1.code_parameter_per_round(),
            vec![Some(&5), Some(&15)]
        );
    }

    if let Some(factory2) = find_factory(&tfactories, 92000, 18000) {
        assert_eq!(factory2.num_rounds(), 2);
        assert_eq!(factory2.num_units_per_round(), vec![18, 1]);
        assert_eq!(
            factory2.unit_names(),
            vec![
                String::from("15-to-1 space efficient"),
                String::from("15-to-1 RM prep")
            ]
        );
        assert_eq!(
            factory2.code_parameter_per_round(),
            vec![Some(&5), Some(&15)]
        );
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
#[test]
pub fn test_hubbard_e2e_measurement_based() -> Result<(), Error> {
    let ftp = floquet_code();
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());
    let (layout_overhead, partitioning) = hubbard_overhead_and_partitioning()?;
    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit.clone(),
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    let result = estimation.estimate(&partitioning)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();
    let tfactory = part.factory();

    assert_eq!(logical_qubit.code_parameter(), &5);
    assert_eq!(logical_qubit.logical_cycle_time(), 1500);

    assert_eq!(result.layout_overhead().logical_qubits(), 72);
    assert_eq!(result.algorithmic_logical_depth(), 22623);
    assert_eq!(part.copies(), 10);
    assert_eq!(result.physical_qubits_for_factories(), 10400);
    assert_eq!(result.physical_qubits_for_algorithm(), 9504);
    assert_eq!(result.physical_qubits(), 19904);
    assert_eq!(result.runtime(), 33_934_500);

    assert_eq!(tfactory.physical_qubits(), 1040);
    assert_eq!(tfactory.num_rounds(), 2);
    assert_eq!(tfactory.num_units_per_round(), vec![23, 1]);
    assert_eq!(
        tfactory.unit_names(),
        vec![
            String::from("15-to-1 RM prep"),
            String::from("15-to-1 space efficient")
        ]
    );

    validate_result_invariants(&result);

    let output_t_error_rate = part.required_output_error_rate();
    let same_ftp = floquet_code();
    let builder = create_factory_builder();
    let tfactories = builder
        .find_factories(
            &same_ftp,
            &qubit,
            0,
            output_t_error_rate,
            same_ftp
                .max_code_distance()
                .expect("code has max code distance"),
        )
        .expect("can compute factories");

    assert_eq!(tfactories.len(), 2);
    if let Some(factory1) = find_factory(&tfactories, 12300, 1612) {
        assert_eq!(factory1.num_rounds(), 2);
        assert_eq!(factory1.num_units_per_round(), vec![23, 1]);
        assert_eq!(
            factory1.unit_names(),
            vec![
                String::from("15-to-1 RM prep"),
                String::from("15-to-1 RM prep")
            ]
        );
        assert_eq!(
            factory1.code_parameter_per_round(),
            vec![Some(&1), Some(&3)]
        );
    }

    if let Some(factory2) = find_factory(&tfactories, 14100, 1040) {
        assert_eq!(factory2.num_rounds(), 2);
        assert_eq!(factory2.num_units_per_round(), vec![23, 1]);
        assert_eq!(
            factory2.unit_names(),
            vec![
                String::from("15-to-1 RM prep"),
                String::from("15-to-1 space efficient")
            ]
        );
        assert_eq!(
            factory2.code_parameter_per_round(),
            vec![Some(&1), Some(&3)]
        );
    }

    Ok(())
}

#[test]
pub fn test_hubbard_e2e_increasing_max_duration() -> Result<(), Error> {
    let ftp = floquet_code();
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());
    let (layout_overhead, partitioning) = hubbard_overhead_and_partitioning()?;
    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    let max_duration_in_nanoseconds1: u64 = 50_000_000_u64;
    let max_duration_in_nanoseconds2: u64 = 500_000_000_u64;

    let result1 =
        estimation.estimate_with_max_duration(&partitioning, max_duration_in_nanoseconds1)?;
    let result2 =
        estimation.estimate_with_max_duration(&partitioning, max_duration_in_nanoseconds2)?;

    assert!(result1.runtime() <= max_duration_in_nanoseconds1);
    assert!(result2.runtime() <= max_duration_in_nanoseconds2);
    assert!(result1.physical_qubits() >= result2.physical_qubits());

    assert_eq!(result1.physical_qubits(), 16784);
    assert_eq!(result2.physical_qubits(), 10544);
    Ok(())
}

#[test]
pub fn test_hubbard_e2e_increasing_max_num_qubits() -> Result<(), Error> {
    let ftp = floquet_code();
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());
    let (layout_overhead, partitioning) = hubbard_overhead_and_partitioning()?;
    let estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        create_factory_builder(),
        Rc::new(layout_overhead),
    );

    let max_num_qubits1: u64 = 11000;
    let max_num_qubits2: u64 = 20000;

    let result1 = estimation.estimate_with_max_num_qubits(&partitioning, max_num_qubits1)?;
    let result2 = estimation.estimate_with_max_num_qubits(&partitioning, max_num_qubits2)?;

    assert!(result1.physical_qubits() <= max_num_qubits1);
    assert!(result2.physical_qubits() <= max_num_qubits2);
    assert!(result1.runtime() >= result2.runtime());

    assert_eq!(result1.runtime(), 329_010_000_u64);
    assert_eq!(result2.runtime(), 33_934_500_u64);
    Ok(())
}

fn prepare_chemistry_estimation_with_expected_majorana() -> (
    PhysicalResourceEstimation<Protocol, TFactoryBuilder, LogicalResourceCounts>,
    ErrorBudget,
) {
    let ftp = floquet_code();
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());

    let value = r#"{
        "numQubits": 1318,
        "tCount": 96,
        "rotationCount": 11987084,
        "rotationDepth": 11986482,
        "cczCount": 67474931068,
        "ccixCount": 0,
        "measurementCount": 63472407520
    }"#;

    let counts: LogicalResourceCounts = serde_json::from_str(value).expect("json should be valid");

    let partitioning = ErrorBudgetSpecification::Total(0.01)
        .partitioning(&counts)
        .expect("partitioning should succeed");
    (
        PhysicalResourceEstimation::new(ftp, qubit, create_factory_builder(), Rc::new(counts)),
        partitioning,
    )
}

#[test]
pub fn test_chemistry_small_max_duration() {
    let max_duration_in_nanoseconds: u64 = 1_000_000_000_u64;

    let (estimation, budget) = prepare_chemistry_estimation_with_expected_majorana();

    let result = estimation.estimate_with_max_duration(&budget, max_duration_in_nanoseconds);

    match result {
        Err(crate::estimates::Error::MaxDurationTooSmall) => {}
        _ => unreachable!("Expected MaxDurationTooSmall"),
    }
}

#[test]
pub fn test_chemistry_small_max_num_qubits() {
    let max_num_qubits: u64 = 10_000;
    let (estimation, budget) = prepare_chemistry_estimation_with_expected_majorana();

    let result = estimation.estimate_with_max_num_qubits(&budget, max_num_qubits);

    match result {
        Err(crate::estimates::Error::MaxPhysicalQubitsTooSmall) => {}
        _ => unreachable!("Expected MaxNumQubitsTooSmall"),
    }
}

#[test]
pub fn test_chemistry_based_max_duration() -> Result<(), Error> {
    let max_duration_in_nanoseconds: u64 = 365 * 24 * 3600 * 1_000_000_000_u64;

    let (estimation, budget) = prepare_chemistry_estimation_with_expected_majorana();

    let result = estimation.estimate_with_max_duration(&budget, max_duration_in_nanoseconds)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();
    let tfactory = part.factory();

    // constraint is not violated
    assert!(result.runtime() <= max_duration_in_nanoseconds);

    assert_eq!(logical_qubit.code_parameter(), &9);
    assert_eq!(logical_qubit.logical_cycle_time(), 2700);

    assert_eq!(result.layout_overhead().logical_qubits(), 2740);
    assert_eq!(result.algorithmic_logical_depth(), 266_172_890_508);
    assert_eq!(part.copies(), 1);
    assert_eq!(result.physical_qubits_for_factories(), 16_640);
    assert_eq!(result.physical_qubits_for_algorithm(), 1_063_120);
    assert_eq!(result.physical_qubits(), 1_079_760);

    assert_eq!(result.runtime(), 10_050_079_976_035_200);

    assert_eq!(tfactory.physical_qubits(), 16_640);
    assert_eq!(tfactory.num_rounds(), 3);
    assert_eq!(tfactory.num_units_per_round(), vec![303, 16, 1]);
    assert_eq!(
        tfactory.unit_names(),
        vec![
            String::from("15-to-1 RM prep"),
            String::from("15-to-1 space efficient"),
            String::from("15-to-1 RM prep"),
        ]
    );
    assert_eq!(
        tfactory.code_parameter_per_round(),
        vec![Some(&1), Some(&3), Some(&7)]
    );

    assert_eq!(
        result.physical_qubits(),
        result.physical_qubits_for_factories() + result.physical_qubits_for_algorithm()
    );
    assert_eq!(
        result.physical_qubits_for_factories(),
        tfactory.physical_qubits() * part.copies()
    );

    assert!(result.logical_patch().logical_error_rate() <= result.required_logical_error_rate());

    Ok(())
}

#[test]
pub fn test_chemistry_based_max_num_qubits() -> Result<(), Error> {
    let max_num_qubits: u64 = 4_923_120;

    let (estimation, budget) = prepare_chemistry_estimation_with_expected_majorana();

    let result = estimation.estimate_with_max_num_qubits(&budget, max_num_qubits)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();
    let tfactory = part.factory();

    // constraint is not violated
    assert!(result.physical_qubits() <= max_num_qubits);

    assert_eq!(logical_qubit.code_parameter(), &9);
    assert_eq!(logical_qubit.logical_cycle_time(), 2700);

    assert_eq!(result.layout_overhead().logical_qubits(), 2740);
    assert_eq!(result.algorithmic_logical_depth(), 266_172_890_508);
    assert_eq!(part.copies(), 231);
    assert_eq!(result.physical_qubits_for_factories(), 3_843_840);
    assert_eq!(result.physical_qubits_for_algorithm(), 1_063_120);
    assert_eq!(result.physical_qubits(), 4_906_960);
    assert_eq!(result.runtime(), 718_666_804_371_600);

    assert_eq!(tfactory.physical_qubits(), 16_640);
    assert_eq!(tfactory.num_rounds(), 3);
    assert_eq!(tfactory.num_units_per_round(), vec![303, 16, 1]);
    assert_eq!(
        tfactory.unit_names(),
        vec![
            String::from("15-to-1 RM prep"),
            String::from("15-to-1 space efficient"),
            String::from("15-to-1 RM prep"),
        ]
    );
    assert_eq!(
        tfactory.code_parameter_per_round(),
        vec![Some(&1), Some(&3), Some(&7)]
    );

    assert_eq!(
        result.physical_qubits(),
        result.physical_qubits_for_factories() + result.physical_qubits_for_algorithm()
    );
    assert_eq!(
        result.physical_qubits_for_factories(),
        tfactory.physical_qubits() * part.copies()
    );

    assert!(result.logical_patch().logical_error_rate() <= result.required_logical_error_rate());

    Ok(())
}

fn prepare_factorization_estimation_with_optimistic_majorana() -> (
    PhysicalResourceEstimation<Protocol, TFactoryBuilder, LogicalResourceCounts>,
    ErrorBudget,
) {
    let ftp = floquet_code();
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());

    let value = r#"{
        "numQubits": 12581,
        "tCount": 12,
        "rotationCount": 12,
        "rotationDepth": 12,
        "cczCount": 3731607428,
        "ccixCount": 0,
        "measurementCount": 1078154040
    }"#;

    let counts: LogicalResourceCounts = serde_json::from_str(value).expect("json should be valid");

    let partitioning = ErrorBudgetSpecification::Total(1e-3)
        .partitioning(&counts)
        .expect("partitioning should succeed");
    (
        PhysicalResourceEstimation::new(ftp, qubit, create_factory_builder(), Rc::new(counts)),
        partitioning,
    )
}

#[test]
pub fn test_factorization_2048_max_duration_matches_regular_estimate() -> Result<(), Error> {
    let (estimation, budget) = prepare_factorization_estimation_with_optimistic_majorana();

    let result_no_max_duration = estimation.estimate_without_restrictions(&budget)?;
    let part_no_max_duration = get_factory(&result_no_max_duration);

    let logical_qubit_no_max_duration = result_no_max_duration.logical_patch();

    let max_duration_in_nanoseconds: u64 = result_no_max_duration.runtime();
    let result = estimation.estimate_with_max_duration(&budget, max_duration_in_nanoseconds)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();

    assert_eq!(
        logical_qubit_no_max_duration.code_parameter(),
        logical_qubit.code_parameter()
    );

    assert_eq!(
        result_no_max_duration.layout_overhead().logical_qubits(),
        result.layout_overhead().logical_qubits()
    );

    assert_eq!(
        result_no_max_duration.algorithmic_logical_depth(),
        result.algorithmic_logical_depth()
    );

    assert_eq!(part_no_max_duration.copies(), part.copies());

    assert_eq!(
        result_no_max_duration.physical_qubits_for_factories(),
        result.physical_qubits_for_factories()
    );

    assert_eq!(
        result_no_max_duration.physical_qubits_for_algorithm(),
        result.physical_qubits_for_algorithm()
    );

    assert_eq!(
        result_no_max_duration.physical_qubits(),
        result.physical_qubits()
    );

    assert_eq!(result_no_max_duration.runtime(), result.runtime());

    Ok(())
}

#[test]
pub fn test_factorization_2048_max_num_qubits_matches_regular_estimate() -> Result<(), Error> {
    let (estimation, budget) = prepare_factorization_estimation_with_optimistic_majorana();

    let result_no_max_num_qubits = estimation.estimate_without_restrictions(&budget)?;
    let part_no_max_num_qubits = get_factory(&result_no_max_num_qubits);

    let logical_qubit_no_max_num_qubits = result_no_max_num_qubits.logical_patch();

    let max_num_qubits = result_no_max_num_qubits.physical_qubits();
    let result = estimation.estimate_with_max_num_qubits(&budget, max_num_qubits)?;
    let part = get_factory(&result);

    let logical_qubit = result.logical_patch();

    assert_eq!(
        logical_qubit_no_max_num_qubits.code_parameter(),
        logical_qubit.code_parameter()
    );

    assert_eq!(
        result_no_max_num_qubits.layout_overhead().logical_qubits(),
        result.layout_overhead().logical_qubits()
    );

    assert_eq!(
        result_no_max_num_qubits.algorithmic_logical_depth(),
        result.algorithmic_logical_depth()
    );

    assert_eq!(part_no_max_num_qubits.copies(), part.copies());

    assert_eq!(
        result_no_max_num_qubits.physical_qubits_for_factories(),
        result.physical_qubits_for_factories()
    );

    assert_eq!(
        result_no_max_num_qubits.physical_qubits_for_algorithm(),
        result.physical_qubits_for_algorithm()
    );

    assert_eq!(
        result_no_max_num_qubits.physical_qubits(),
        result.physical_qubits()
    );

    assert_eq!(result_no_max_num_qubits.runtime(), result.runtime());

    Ok(())
}

fn prepare_ising20x20_estimation_with_pessimistic_gate_based() -> (
    PhysicalResourceEstimation<Protocol, TFactoryBuilder, LogicalResourceCounts>,
    ErrorBudget,
) {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::qubit_gate_us_e3());

    let value = r#"{
        "numQubits": 100,
        "tCount": 0,
        "rotationCount": 112110,
        "rotationDepth": 2001,
        "cczCount": 0,
        "ccixCount": 0,
        "measurementCount": 0
    }"#;

    let counts: LogicalResourceCounts = serde_json::from_str(value).expect("cannot parse json");

    let partitioning = ErrorBudgetSpecification::Total(1e-3)
        .partitioning(&counts)
        .expect("cannot setup error budget partitioning");
    (
        PhysicalResourceEstimation::new(ftp, qubit, create_factory_builder(), Rc::new(counts)),
        partitioning,
    )
}

#[test]
fn test_chemistry_based_max_factories() {
    for max_factories in 1..=14 {
        let (mut estimation, budget) = prepare_chemistry_estimation_with_expected_majorana();
        estimation.set_max_factories(max_factories);

        let result = estimation.estimate(&budget).expect("failed to estimate");
        let actual_factories = result.factory_parts()[0]
            .as_ref()
            .expect("has factories")
            .copies();

        assert!(
            actual_factories <= max_factories,
            "failed for {max_factories} maximum factories"
        );
    }
}

#[test]
fn test_budget_pruning() {
    let (mut estimation, budget) = prepare_ising20x20_estimation_with_pessimistic_gate_based();
    estimation.set_error_budget_strategy(ErrorBudgetStrategy::Static);
    let result1 = estimation.estimate(&budget).expect("failed to estimate");

    estimation.set_error_budget_strategy(ErrorBudgetStrategy::PruneLogicalAndRotations);
    let result2 = estimation.estimate(&budget).expect("failed to estimate");

    assert_eq!(result1.physical_qubits(), 2_938_540);
    assert_eq!(result1.runtime(), 1_734_282_000_000);

    assert_eq!(result2.physical_qubits(), 2_154_380);
    assert_eq!(result2.runtime(), 1_734_282_000_000);

    assert!(result1.error_budget().logical() >= result2.error_budget().logical());
    assert!(result1.error_budget().rotations() >= result2.error_budget().rotations());
    assert!(result1.error_budget().magic_states() <= result2.error_budget().magic_states());
}

#[test]
fn build_frontier_test() {
    let (estimation, budget) = prepare_ising20x20_estimation_with_pessimistic_gate_based();

    let frontier_result = estimation.build_frontier(&budget);

    let points = frontier_result.expect("failed to estimate");
    assert_eq!(points.len(), 189);

    for i in 0..points.len() - 1 {
        assert!(points[i].runtime() <= points[i + 1].runtime());
        assert!(points[i].physical_qubits() >= points[i + 1].physical_qubits());
        assert!(get_factory(&points[i]).copies() >= get_factory(&points[i + 1]).copies());
        assert!(
            points[i].logical_patch().code_parameter()
                <= points[i + 1].logical_patch().code_parameter()
        );
    }

    let shortest_runtime_result = estimation.estimate(&budget).expect("failed to estimate");
    assert_eq!(points[0].runtime(), shortest_runtime_result.runtime());
    assert_eq!(
        points[0].physical_qubits(),
        shortest_runtime_result.physical_qubits()
    );
    assert_eq!(
        get_factory(&points[0]).copies(),
        get_factory(&shortest_runtime_result).copies()
    );
    assert_eq!(
        points[0].logical_patch().code_parameter(),
        shortest_runtime_result.logical_patch().code_parameter()
    );

    let mut max_duration = shortest_runtime_result.runtime();
    let num_iterations = 100;
    let coefficient = 1.05;
    for _ in 0..num_iterations {
        max_duration = (max_duration as f64 * coefficient) as u64;
        let result = estimation
            .estimate_with_max_duration(&budget, max_duration)
            .expect("failed to estimate");

        assert!(
            points
                .iter()
                .filter(|point| point.runtime() <= result.runtime()
                    && point.physical_qubits() == result.physical_qubits())
                .count()
                == 1
        );
    }

    let mut max_num_qubits = shortest_runtime_result.physical_qubits();
    let num_iterations = 100;
    let coefficient = 1.10;
    for _ in 0..num_iterations {
        max_num_qubits = (max_num_qubits as f64 / coefficient) as u64;
        let result = estimation.estimate_with_max_num_qubits(&budget, max_num_qubits);

        if let Ok(result) = result {
            assert!(
                points
                    .iter()
                    .filter(|point| point.runtime() <= result.runtime()
                        && point.physical_qubits() == result.physical_qubits())
                    .count()
                    == 1
            );
        }
    }
}

fn prepare_bit_flip_code_resources_and_majorana_n6_qubit() -> (
    PhysicalResourceEstimation<Protocol, TFactoryBuilder, LogicalResourceCounts>,
    ErrorBudget,
) {
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());
    let ftp = floquet_code();

    let value = r#"{
        "numQubits": 5,
        "tCount": 0,
        "rotationCount": 1,
        "rotationDepth": 1,
        "cczCount": 0,
        "ccixCount": 0,
        "measurementCount": 3
    }"#;

    let counts: LogicalResourceCounts = serde_json::from_str(value).expect("cannot parse json");

    let partitioning = ErrorBudgetSpecification::Total(1e-3)
        .partitioning(&counts)
        .expect("cannot setup error budget partitioning");
    (
        PhysicalResourceEstimation::new(ftp, qubit, create_factory_builder(), Rc::new(counts)),
        partitioning,
    )
}

#[test]
fn build_frontier_bit_flip_code_test() {
    let (estimation, budget) = prepare_bit_flip_code_resources_and_majorana_n6_qubit();

    let frontier_result = estimation.build_frontier(&budget);

    let points = frontier_result.expect("failed to estimate");
    assert_eq!(points.len(), 10);
    let part0 = get_factory(&points[0]);

    let shortest_runtime_result = estimation.estimate(&budget).expect("failed to estimate");
    let part_shortest_runtime = get_factory(&shortest_runtime_result);

    assert_eq!(points[0].runtime(), shortest_runtime_result.runtime());
    assert_eq!(
        part0.factory().duration(),
        part_shortest_runtime.factory().duration()
    );

    assert_eq!(
        part0.factory().physical_qubits(),
        part_shortest_runtime.factory().physical_qubits()
    );

    assert_eq!(part0.copies(), part_shortest_runtime.copies());

    assert_eq!(
        points[0].physical_qubits(),
        shortest_runtime_result.physical_qubits()
    );
}

#[test]
fn code_distance_tests() {
    let params = JobParams::default();

    let ftp = surface_code_gate_based();

    for logical_qubits in (50..=1000).step_by(50) {
        for num_cycles in (50_000..=500_000).step_by(50_000) {
            for exp in 1..=15 {
                let budget_logical = 10.0_f64.powi(-exp);

                let required_logical_qubit_error_rate =
                    budget_logical / (logical_qubits * num_cycles) as f64;

                let qubit = params.qubit_params().clone();
                let code_distance = ftp
                    .compute_code_parameter(&qubit, required_logical_qubit_error_rate)
                    .expect("code distance can be computed");

                assert!(
                    code_distance <= *ftp.max_code_distance().expect("code has max code distance")
                );
            }
        }
    }
}

#[test]
fn test_report() {
    let logical_resources = LogicalResourceCounts {
        num_qubits: 100,
        t_count: 0,
        rotation_count: 112_110,
        rotation_depth: 2001,
        ccz_count: 0,
        ccix_count: 0,
        measurement_count: 0,
        num_compute_qubits: None,
        read_from_memory_count: None,
        write_to_memory_count: None,
    };

    let params: &str = "[{}]";
    let result = estimate_physical_resources(logical_resources, params);

    let json_value: Vec<Value> =
        serde_json::from_str(&result.expect("result is err")).expect("Failed to parse JSON");
    assert_eq!(json_value.len(), 1);
    assert_eq!(
        strip_numbers(&json_value[0]),
        strip_numbers(
            &serde_json::from_str::<Value>(include_str!("test_report.json"))
                .expect("Failed to parse JSON")
        )
    );
}

fn create_factory_builder() -> TFactoryBuilder {
    TFactoryBuilder::new(
        TFactoryDistillationUnitTemplate::default_distillation_unit_templates(),
        MAX_DISTILLATION_ROUNDS,
    )
}

fn find_factory<'a>(
    tfactories: &[Cow<'a, TFactory>],
    duration: u64,
    physical_qubits: u64,
) -> Option<Cow<'a, TFactory>> {
    let tfactory = tfactories.iter().find(|tfactory| {
        tfactory.duration() == duration && tfactory.physical_qubits() == physical_qubits
    });
    assert!(tfactory.is_some());
    tfactory.cloned()
}

// In order to avoid small numerical imprecision when comparing JSON values, we
// can use this function to replace any numerical values by 0.
fn strip_numbers(value: &Value) -> Value {
    match value {
        Value::Number(_) => json!(0),
        Value::Null | Value::Bool(_) | Value::String(_) => value.clone(),
        Value::Array(entries) => Value::Array(entries.iter().map(strip_numbers).collect()),
        Value::Object(entries) => {
            let mut map = Map::new();

            for (key, value) in entries {
                map.insert(key.clone(), strip_numbers(value));
            }

            Value::Object(map)
        }
    }
}

// In this system, there is only one magic state type, T states, and therefore
// one factory part in the result with information on the factory.
fn get_factory(
    result: &PhysicalResourceEstimationResult<Protocol, TFactory>,
) -> &FactoryPart<TFactory> {
    result.factory_parts()[0]
        .as_ref()
        .expect("result has a T-factory")
}
