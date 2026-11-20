// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rustc_hash::FxHashSet;
use std::rc::Rc;

use crate::estimates::{DistillationUnit, LogicalPatch};

use super::super::super::{
    data::{
        TFactoryDistillationUnitSpecification,
        TFactoryProtocolSpecificDistillationUnitSpecification,
    },
    modeling::{
        PhysicalQubit, Protocol, TFactoryDistillationUnit, TFactoryDistillationUnitTemplate,
        TFactoryDistillationUnitType, floquet_code, surface_code_gate_based,
    },
};

use super::DistillationUnitsMap;

fn create_default_units_for_distance<'a>(
    distillation_units_map: &'a DistillationUnitsMap<'a>,
    position: usize,
    distance: u64,
) -> Vec<&'a TFactoryDistillationUnit<'a>> {
    if position == 0 && distance == 1 {
        (0..distillation_units_map.num_physical_distillation_units)
            .map(|idx| {
                distillation_units_map.get(
                    position,
                    distance,
                    idx + distillation_units_map.num_logical_distillation_units,
                )
            })
            .collect::<Vec<_>>()
    } else {
        (0..distillation_units_map.num_logical_distillation_units)
            .map(|idx| distillation_units_map.get(position, distance, idx))
            .collect::<Vec<_>>()
    }
}

fn create_default_map<'a>(
    physical_qubit: PhysicalQubit,
    ftp: &'a Protocol,
    templates: &'a [TFactoryDistillationUnitTemplate],
) -> DistillationUnitsMap<'a> {
    let qubit = Rc::new(physical_qubit);

    let min_code_distance = 1;
    let max_code_distance = 11;
    let distances: Vec<_> = (min_code_distance..=max_code_distance).step_by(2).collect();
    let mut qubits = vec![None; max_code_distance as usize + 1];
    for &distance in &distances {
        qubits[distance as usize] = LogicalPatch::new(ftp, distance, qubit.clone())
            .ok()
            .map(Rc::new);
    }

    DistillationUnitsMap::create(qubit.as_ref(), &qubits, distances, templates)
}

fn create_and_test(
    qubit: PhysicalQubit,
    ftp: &Protocol,
    position: usize,
    distance: u64,
    expected: &str,
    not_expected: &str,
) {
    let distillation_unit_templates =
        &TFactoryDistillationUnitTemplate::default_distillation_unit_templates();
    let distillation_units_map = create_default_map(qubit, ftp, distillation_unit_templates);
    let units = create_default_units_for_distance(&distillation_units_map, position, distance);
    for unit in &units {
        assert!(unit.name.contains(expected));
        assert!(!unit.name.contains(not_expected));
    }
}

fn create_distillation_unit_template(
    name: &str,
    unit_type: TFactoryDistillationUnitType,
) -> TFactoryDistillationUnitTemplate {
    let (
        physical_qubit_specification,
        logical_qubit_specification,
        logical_qubit_specification_first_round_override,
    ) = match unit_type {
        TFactoryDistillationUnitType::Physical => (
            Some(create_default_t_factory_protocol_specific_distillation_unit_specification()),
            None,
            None,
        ),
        TFactoryDistillationUnitType::Logical => (
            None,
            Some(create_default_t_factory_protocol_specific_distillation_unit_specification()),
            None, // or create_default_t_factory_protocol_specific_distillation_unit_specification(),
        ),
        TFactoryDistillationUnitType::Combined => (
            Some(create_default_t_factory_protocol_specific_distillation_unit_specification()),
            Some(create_default_t_factory_protocol_specific_distillation_unit_specification()),
            None, // or create_default_t_factory_protocol_specific_distillation_unit_specification(),
        ),
    };

    let specification = TFactoryDistillationUnitSpecification::Custom {
        display_name: name.to_owned(),
        num_input_ts: 1,
        num_output_ts: 1,
        failure_probability_formula: "0.5 * inputErrorRate".to_owned(),
        output_error_rate_formula: "0.5 * inputErrorRate".to_owned(),
        physical_qubit_specification,
        logical_qubit_specification,
        logical_qubit_specification_first_round_override,
    };

    TFactoryDistillationUnitTemplate::try_from(&specification)
        .expect("TFactory from specification should succeed")
}

fn create_distillation_unit_template_with_override() -> TFactoryDistillationUnitTemplate {
    let specification = TFactoryDistillationUnitSpecification::Custom {
        display_name: "combined with override".to_owned(),
        num_input_ts: 1,
        num_output_ts: 1,
        failure_probability_formula: "0.5 * inputErrorRate".to_owned(),
        output_error_rate_formula: "0.5 * inputErrorRate".to_owned(),
        physical_qubit_specification: Some(TFactoryProtocolSpecificDistillationUnitSpecification {
            num_unit_qubits: 1,
            duration_in_qubit_cycle_time: 2,
        }),
        logical_qubit_specification: Some(TFactoryProtocolSpecificDistillationUnitSpecification {
            num_unit_qubits: 3,
            duration_in_qubit_cycle_time: 4,
        }),
        logical_qubit_specification_first_round_override: Some(
            TFactoryProtocolSpecificDistillationUnitSpecification {
                num_unit_qubits: 5,
                duration_in_qubit_cycle_time: 6,
            },
        ),
    };

    TFactoryDistillationUnitTemplate::try_from(&specification)
        .expect("TFactory from specification should succeed")
}

fn create_distillation_unit_template_without_override() -> TFactoryDistillationUnitTemplate {
    let specification = TFactoryDistillationUnitSpecification::Custom {
        display_name: "combined without override".to_owned(),
        num_input_ts: 1,
        num_output_ts: 1,
        failure_probability_formula: "0.5 * inputErrorRate".to_owned(),
        output_error_rate_formula: "0.5 * inputErrorRate".to_owned(),
        physical_qubit_specification: Some(TFactoryProtocolSpecificDistillationUnitSpecification {
            num_unit_qubits: 1,
            duration_in_qubit_cycle_time: 2,
        }),
        logical_qubit_specification: Some(TFactoryProtocolSpecificDistillationUnitSpecification {
            num_unit_qubits: 3,
            duration_in_qubit_cycle_time: 4,
        }),
        logical_qubit_specification_first_round_override: None,
    };

    TFactoryDistillationUnitTemplate::try_from(&specification)
        .expect("TFactory from specification should succeed")
}

fn create_default_t_factory_protocol_specific_distillation_unit_specification()
-> TFactoryProtocolSpecificDistillationUnitSpecification {
    TFactoryProtocolSpecificDistillationUnitSpecification {
        num_unit_qubits: 1,
        duration_in_qubit_cycle_time: 1,
    }
}

#[test]
fn units_for_distance_test_position_0_distance_1() {
    create_and_test(
        PhysicalQubit::qubit_maj_ns_e4(),
        &floquet_code(),
        0,
        1,
        "physical",
        "logical",
    );
}

#[test]
fn units_for_distance_test_position_0_distance_3() {
    create_and_test(
        PhysicalQubit::default(),
        &surface_code_gate_based(),
        0,
        3,
        "logical",
        "physical",
    );
}

#[test]
fn units_for_distance_test_position_1_distance_1() {
    create_and_test(
        PhysicalQubit::default(),
        &surface_code_gate_based(),
        1,
        1,
        "logical",
        "physical",
    );
}

#[test]
fn units_for_distance_test_position_1_distance_5() {
    create_and_test(
        PhysicalQubit::default(),
        &surface_code_gate_based(),
        1,
        5,
        "logical",
        "physical",
    );
}

fn create_templates_list_2_2_2() -> Vec<TFactoryDistillationUnitTemplate> {
    let combined_template1 =
        create_distillation_unit_template("combined1", TFactoryDistillationUnitType::Combined);

    let logical_template1 =
        create_distillation_unit_template("logical1", TFactoryDistillationUnitType::Logical);

    let physical_template1 =
        create_distillation_unit_template("physical1", TFactoryDistillationUnitType::Physical);

    let combined_template2 =
        create_distillation_unit_template("combined2", TFactoryDistillationUnitType::Combined);

    let logical_template2 =
        create_distillation_unit_template("logical2", TFactoryDistillationUnitType::Logical);

    let physical_template2 =
        create_distillation_unit_template("physical2", TFactoryDistillationUnitType::Physical);

    Vec::from([
        combined_template1,
        logical_template1,
        physical_template1,
        combined_template2,
        logical_template2,
        physical_template2,
    ])
}

fn create_templates_list_0_2_1() -> Vec<TFactoryDistillationUnitTemplate> {
    let logical_template1 =
        create_distillation_unit_template("logical1", TFactoryDistillationUnitType::Logical);

    let physical_template1 =
        create_distillation_unit_template("physical1", TFactoryDistillationUnitType::Physical);

    let logical_template2 =
        create_distillation_unit_template("logical2", TFactoryDistillationUnitType::Logical);

    Vec::from([logical_template1, physical_template1, logical_template2])
}

#[test]
fn test_map_creation_no_purely_physical_templates_filtered_out_by_is_valid_condition() {
    let templates = create_templates_list_2_2_2();
    let ftp = surface_code_gate_based();
    let map = create_default_map(PhysicalQubit::default(), &ftp, &templates);
    assert_eq!(map.num_physical_distillation_units, 0);
    assert_eq!(map.num_logical_distillation_units, 2);
    assert_eq!(map.num_combined_distillation_units, 2);
    assert_eq!(map.get(0, 1, 0).name, "combined1");
    assert_eq!(map.get(0, 1, 1).name, "combined2");

    assert_eq!(map.get(0, 3, 0).name, "combined1");
    assert_eq!(map.get(0, 3, 1).name, "combined2");
    assert_eq!(map.get(0, 3, 2).name, "logical1");
    assert_eq!(map.get(0, 3, 3).name, "logical2");
}

#[test]
fn test_map_creation_with_purely_physical_templates() {
    let templates: Vec<TFactoryDistillationUnitTemplate> = create_templates_list_2_2_2();
    let ftp = floquet_code();
    let map = create_default_map(PhysicalQubit::qubit_maj_ns_e4(), &ftp, &templates);
    assert_eq!(map.num_physical_distillation_units, 2);
    assert_eq!(map.num_logical_distillation_units, 2);
    assert_eq!(map.num_combined_distillation_units, 2);
    assert_eq!(map.get(0, 1, 0).name, "combined1");
    assert_eq!(map.get(0, 1, 1).name, "combined2");
    assert_eq!(map.get(0, 1, 4).name, "physical1");
    assert_eq!(map.get(0, 1, 5).name, "physical2");

    assert_eq!(map.get(0, 3, 0).name, "combined1");
    assert_eq!(map.get(0, 3, 1).name, "combined2");
    assert_eq!(map.get(0, 3, 2).name, "logical1");
    assert_eq!(map.get(0, 3, 3).name, "logical2");
}

#[test]
fn test_map_creation_with_purely_physical_and_no_combined_templates() {
    let templates = create_templates_list_0_2_1();
    let ftp = floquet_code();
    let map = create_default_map(PhysicalQubit::qubit_maj_ns_e4(), &ftp, &templates);
    assert_eq!(map.num_physical_distillation_units, 1);
    assert_eq!(map.num_logical_distillation_units, 2);
    assert_eq!(map.num_combined_distillation_units, 0);
    assert_eq!(map.get(0, 1, 2).name, "physical1");
    assert_eq!(map.get(0, 3, 0).name, "logical1");
    assert_eq!(map.get(0, 3, 1).name, "logical2");
}

#[test]
fn test_first_round_overrides_applied() {
    let mut templates = create_templates_list_2_2_2();
    templates.push(create_distillation_unit_template_with_override());
    templates.push(create_distillation_unit_template_without_override());
    let ftp = floquet_code();
    let map = create_default_map(PhysicalQubit::qubit_maj_ns_e4(), &ftp, &templates);

    assert_eq!(map.num_physical_distillation_units, 2);
    assert_eq!(map.num_logical_distillation_units, 2);
    assert_eq!(map.num_combined_distillation_units, 4);

    // physical
    let unit1_1 = map.get(0, 1, 2);
    assert_eq!(unit1_1.name, "combined with override");
    assert_eq!(unit1_1.physical_qubits(0), 1);
    assert_eq!(unit1_1.duration(0), 200);

    // physical
    let unit1_2 = map.get(0, 1, 3);
    assert_eq!(unit1_2.name, "combined without override");
    assert_eq!(unit1_2.physical_qubits(0), 1);
    assert_eq!(unit1_2.duration(0), 200);

    // logical at subsequent round
    let unit2_1 = map.get(1, 1, 2);
    assert_eq!(unit2_1.name, "combined with override");
    assert_eq!(unit2_1.physical_qubits(1), 12);
    assert_eq!(unit2_1.duration(1), 1200);

    // logical at subsequent round
    let unit2_2 = map.get(1, 1, 3);
    assert_eq!(unit2_2.name, "combined without override");
    assert_eq!(unit2_2.physical_qubits(1), 12);
    assert_eq!(unit2_2.duration(1), 1200);

    // logical at first round. override
    let unit3_1 = map.get(0, 3, 2);
    assert_eq!(unit3_1.name, "combined with override");
    assert_eq!(unit3_1.physical_qubits(0), 260);
    assert_eq!(unit3_1.duration(0), 5400);

    // logical at first round. no override
    let unit3_2 = map.get(0, 3, 3);
    assert_eq!(unit3_2.name, "combined without override");
    assert_eq!(unit3_2.physical_qubits(0), 156);
    assert_eq!(unit3_2.duration(0), 3600);

    // logical at subsequent round
    let unit4_1 = map.get(1, 3, 2);
    assert_eq!(unit4_1.name, "combined with override");
    assert_eq!(unit4_1.physical_qubits(1), 156);
    assert_eq!(unit4_1.duration(1), 3600);

    // logical at subsequent round
    let unit4_2 = map.get(1, 3, 3);
    assert_eq!(unit4_2.name, "combined without override");
    assert_eq!(unit4_2.physical_qubits(1), 156);
    assert_eq!(unit4_2.duration(1), 3600);
}

#[test]
fn iterate_for_all_distillation_units_test() {
    let templates = create_templates_list_2_2_2();
    let ftp = floquet_code();
    let map = create_default_map(PhysicalQubit::qubit_maj_ns_e4(), &ftp, &templates);

    let mut hashmap = FxHashSet::default();
    let mut callback = |indexes: &[usize]| {
        hashmap.insert(indexes[0]);
    };
    map.iterate_for_all_distillation_units(1, &mut callback);
    assert_eq!(hashmap.len(), 6);

    let mut hashmap = FxHashSet::default();
    let mut callback = |indexes: &[usize]| {
        hashmap.insert(indexes[0] * 10 + indexes[1]);
    };
    map.iterate_for_all_distillation_units(2, &mut callback);
    // 6 on the first position, 4 on the second (not purely physical)
    assert_eq!(hashmap.len(), 24);

    let mut hashmap = FxHashSet::default();
    let mut callback = |indexes: &[usize]| {
        hashmap.insert(indexes[0] * 100 + indexes[1] * 10 + indexes[2]);
    };
    map.iterate_for_all_distillation_units(3, &mut callback);
    // 6 on the first position, 4 on the second (not purely physical), 4 on the third (not purely physical)
    assert_eq!(hashmap.len(), 96);
}

#[test]
fn iterate_for_all_distillation_units_0_2_1_test() {
    let templates = create_templates_list_0_2_1();
    let ftp = floquet_code();
    let map = create_default_map(PhysicalQubit::qubit_maj_ns_e4(), &ftp, &templates);
    let mut hashmap = FxHashSet::default();
    let mut callback = |indexes: &[usize]| {
        hashmap.insert(indexes[0]);
    };
    map.iterate_for_all_distillation_units(1, &mut callback);
    assert_eq!(hashmap.len(), 3);

    let mut hashmap = FxHashSet::default();
    let mut callback = |indexes: &[usize]| {
        hashmap.insert(indexes[0] * 10 + indexes[1]);
    };
    map.iterate_for_all_distillation_units(2, &mut callback);
    // 3 on the first position, 2 on the second (not purely physical)
    assert_eq!(hashmap.len(), 6);
}
