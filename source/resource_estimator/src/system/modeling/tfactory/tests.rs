// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use probability::distribution::Binomial;
use probability::prelude::Inverse;

use super::*;
use std::rc::Rc;

use crate::estimates::LogicalPatch;
use crate::system::modeling::surface_code_gate_based;

use super::super::super::modeling::{PhysicalQubit, Protocol};
use super::super::super::{Result, constants::FLOAT_COMPARISON_EPSILON};

#[test]
fn test_ppf() {
    let p = 0.7;
    let q = 0.1;

    // This returns the same values like scipy.stats.binom.ppf(q, n, p)
    let ks: Vec<_> = (0..20).map(|n| Binomial::new(n, p).inverse(q)).collect();
    assert_eq!(
        ks,
        vec![0, 0, 1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6, 7, 8, 8, 9, 9, 10, 11]
    );
}

#[test]
fn former_endless_inverse1_fixed() {
    let p = 1.0 - 0.018_979_537_135_266_43;
    let n = 3666;
    let q = 1.0 - 0.996_666_666_666_666_7;

    let dist = Binomial::new(n, p);

    dist.inverse(q);
}

#[test]
fn former_endless_inverse2_fixed() {
    let p = 0.99;
    let n = 1024;
    let q = 1.0 - 0.999_974_32;

    let dist = Binomial::new(n, p);

    dist.inverse(q);
}

#[test]
fn former_endless_inverse3_fixed() {
    let p = 0.984_893_2;
    let n = 3601;
    let q = 0.002_499_999_999_999_946_7;

    let dist = Binomial::new(n, p);

    dist.inverse(q);
}

#[test]
fn former_endless_inverse4_fixed() {
    let p = 0.999_475;
    let n = 3796;
    let q = 0.002;

    let dist = Binomial::new(n, p);

    dist.inverse(q);
}

#[test]
fn single_physical_qubit() {
    let qubit = PhysicalQubit::qubit_maj_ns_e6();
    let template =
        TFactoryDistillationUnitTemplate::create_distillation_unit_15_to_1_rm_prep_template();
    let unit = TFactoryDistillationUnit::by_template(&template, &TFactoryQubit::Physical(&qubit));

    assert!((unit.output_error_rate(qubit.t_gate_error_rate()) - 4.21e-5).abs() <= f64::EPSILON);
    assert!(
        (unit.failure_probability(qubit.t_gate_error_rate()) - 0.150_356).abs() <= f64::EPSILON
    );
    assert_eq!(unit.duration(0), 2400);
    assert_eq!(unit.physical_qubits(0), 31);
}

fn create_logical_qubit_with_distance(
    code_distance: u64,
) -> core::result::Result<LogicalPatch<Protocol>, crate::estimates::Error> {
    let ftp = surface_code_gate_based();
    let qubit = Rc::new(PhysicalQubit::default());

    LogicalPatch::new(&ftp, code_distance, qubit)
}

#[test]
fn single_logical_unit() -> Result<()> {
    let qubit = create_logical_qubit_with_distance(9)?;
    let template =
        TFactoryDistillationUnitTemplate::create_distillation_unit_15_to_1_rm_prep_template();
    let unit = TFactoryDistillationUnit::by_template(&template, &TFactoryQubit::Logical(&qubit));

    assert!(unit.failure_probability(1e-1) >= 1.0);

    Ok(())
}

type DistanceAndUnitTemplate<'a> = (u64, &'a TFactoryDistillationUnitTemplate);
type TestOutputTuple = (u64, u64, f64, f64, u64, u64);
fn create_pipeline_with_distance(
    unit_specifications: &[DistanceAndUnitTemplate],
) -> Option<TestOutputTuple> {
    let units: Vec<_> = unit_specifications
        .iter()
        .map(|(code_distance, specification)| {
            TFactoryDistillationUnit::by_template(
                specification,
                &TFactoryQubit::Logical(
                    &create_logical_qubit_with_distance(*code_distance)
                        .expect("code distance calculation should succeed"),
                ),
            )
        })
        .collect();
    let unit_refs = units.iter().collect::<Vec<_>>();
    let failure_probability_requirement = 0.01;
    if let Ok(pipeline) = TFactory::build(
        &unit_refs,
        unit_refs[0].qubit_t_error_rate(),
        failure_probability_requirement,
    ) {
        Some((
            pipeline.physical_qubits(),
            pipeline.duration(),
            pipeline.input_error_rate(),
            pipeline.output_error_rate(),
            pipeline.num_input_states(),
            pipeline.num_output_states(),
        ))
    } else {
        None
    }
}

fn assert_numeric_tuples_eq(a: Option<TestOutputTuple>, b: Option<TestOutputTuple>) {
    if let (Some(a), Some(b)) = (a, b) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
        assert!((a.2 - b.2).abs() < FLOAT_COMPARISON_EPSILON);
        assert!((a.3 - b.3).abs() < FLOAT_COMPARISON_EPSILON);
        assert_eq!(a.4, b.4);
        assert_eq!(a.5, b.5);
    } else {
        assert_eq!(a, b);
    }
}

#[test]
fn test_single_unit_15_to_1_rm_prep() {
    let template =
        &TFactoryDistillationUnitTemplate::create_distillation_unit_15_to_1_rm_prep_template();

    for code_distance in (1..5).skip(2) {
        assert_eq!(
            create_pipeline_with_distance(&[(code_distance, template)],),
            None
        );
    }
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(7, template)]),
        Some((6076, 30800, 0.001, 2.1335e-5, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(9, template)]),
        Some((10044, 39600, 0.001, 2.165_000_000_000_000_4e-6, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(11, template)]),
        Some((15004, 48400, 0.001, 2.480_000_000_000_000_5e-7, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(13, template)]),
        Some((20956, 57200, 0.001, 5.630_000_000_000_001e-8, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(15, template)]),
        Some((27900, 66000, 0.001, 3.713e-8, 30, 1)),
    );
}

#[test]
fn test_single_unit_15_to_1_space_efficient() {
    let template = &TFactoryDistillationUnitTemplate ::create_distillation_unit_15_to_1_rm_space_efficient_template();

    for code_distance in (1..5).skip(2) {
        assert_eq!(
            create_pipeline_with_distance(&[(code_distance, template)],),
            None
        );
    }
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(7, template)]),
        Some((3920, 36400, 0.001, 2.1335e-5, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(9, template)]),
        Some((6480, 46800, 0.001, 2.165_000_000_000_000_4e-6, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(11, template)]),
        Some((9680, 57200, 0.001, 2.480_000_000_000_000_5e-7, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(13, template)]),
        Some((13520, 67600, 0.001, 5.630_000_000_000_001e-8, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(13, template)]),
        Some((13520, 67600, 0.001, 5.630_000_000_000_001e-8, 30, 1)),
    );
    assert_numeric_tuples_eq(
        create_pipeline_with_distance(&[(15, template)]),
        Some((18000, 78000, 0.001, 3.713e-8, 30, 1)),
    );
}

#[test]
fn expression_by_formula1() {
    let expression =
        "15.0 * inputErrorRate + 356.0 * cliffordErrorRate + 22.0 * readoutErrorRate ^2";

    let function = TFactoryFormula::from(
        CompiledExpression::from_string(expression, "test_expression_1")
            .expect("expression compilation should succeed"),
    );

    // arguments order: inputErrorRate, cliffordErrorRate, readoutErrorRate
    assert!(function(0.0, 0.0, 0.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.0, 1.0) - 22.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 1.0, 0.0) - 356.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 1.0, 1.0) - 378.0).abs() <= f64::EPSILON);
    assert!((function(1.0, 0.0, 0.0) - 15.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.0, 0.5) - 5.5).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.5, 0.0) - 178.0).abs() <= f64::EPSILON);
}

#[test]
fn expression_by_formula2() {
    let expression = "15.0 * z + 356.0 * c + 22.0 * r ^2";

    let function = TFactoryFormula::from(
        CompiledExpression::from_string(expression, "test_expression_2")
            .expect("expression compilation should succeed"),
    );

    // arguments order: z, c, r
    assert!(function(0.0, 0.0, 0.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.0, 1.0) - 22.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 1.0, 0.0) - 356.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 1.0, 1.0) - 378.0).abs() <= f64::EPSILON);
    assert!((function(1.0, 0.0, 0.0) - 15.0).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.0, 0.5) - 5.5).abs() <= f64::EPSILON);
    assert!((function(0.0, 0.5, 0.0) - 178.0).abs() <= f64::EPSILON);
}

#[test]
fn test_default_t_factory() {
    let physical_qubit = PhysicalQubit::default();
    let ftp = surface_code_gate_based();
    let logical_qubit = LogicalPatch::new(&ftp, 15, Rc::new(physical_qubit))
        .expect("logical qubit construction should succeed");
    let tfactory = default_t_factory(&logical_qubit);

    assert_eq!(tfactory.num_rounds(), 1);
    assert_eq!(tfactory.num_units_per_round(), vec![1]);
    assert_eq!(tfactory.code_parameter_per_round(), vec![Some(&15)]);
    assert_eq!(tfactory.physical_qubits_per_round(), vec![450]);
    assert_eq!(tfactory.duration_per_round(), vec![6000]);
}
