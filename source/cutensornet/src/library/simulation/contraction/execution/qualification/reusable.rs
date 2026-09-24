//! Tiny analytical supplied-plan cases, independent of the frozen unit-norm oracles.

use num_complex::Complex64;
use qdk_simulators::execution::InputMutability;
use tensornet::{
    ContractionPlan, ContractionQuery, ContractionStep, Index, Indices, Operand, TensorNetwork,
};

const LIMIT: f64 = 1e-12;

struct Input {
    dimensions: Vec<usize>,
    values: Vec<Complex64>,
    mutability: InputMutability,
}

struct Selection {
    label: &'static str,
    inputs: Vec<usize>,
    replacement: Option<(usize, Vec<Complex64>)>,
    expected: Vec<Complex64>,
    squared_norm: f64,
}

struct Fixture {
    name: &'static str,
    network: TensorNetwork,
    keep: Indices,
    steps: Vec<ContractionStep>,
    inputs: Vec<Input>,
    selections: Vec<Selection>,
}

impl Fixture {
    fn query(&self) -> ContractionQuery<'_> {
        ContractionQuery::new(&self.network, self.keep.clone()).expect("analytical query")
    }

    fn plan(&self) -> ContractionPlan {
        ContractionPlan::new(&self.query(), self.steps.clone()).expect("supplied analytical plan")
    }
}

fn axes(ids: &[u32]) -> Indices {
    Indices::new(
        ids.iter()
            .map(|&id| Index::new(id, 2).expect("positive dimension"))
            .collect(),
    )
    .expect("consistent axes")
}

fn values(entries: &[(f64, f64)]) -> Vec<Complex64> {
    entries
        .iter()
        .map(|&(real, imaginary)| Complex64::new(real, imaginary))
        .collect()
}

fn chain() -> Fixture {
    let input = |entries: &[(f64, f64)], mutability| Input {
        dimensions: vec![2, 2],
        values: values(entries),
        mutability,
    };
    let selection = |label, inputs, expected: &[(f64, f64)], squared_norm| Selection {
        label,
        inputs,
        replacement: None,
        expected: values(expected),
        squared_norm,
    };
    let alpha_squared = [(1., 0.), (0., 3.), (0., 0.), (4., 0.)];
    let alpha_beta = [(2., 1.), (1., 0.), (2., 0.), (0., -2.)];
    let replacement = values(&[(-1., 0.), (3., 0.), (0., 2.), (0.5, 0.)]);
    Fixture {
        name: "reusable_chain",
        network: TensorNetwork::new(vec![axes(&[11, 23]), axes(&[23, 37]), axes(&[37, 53])])
            .expect("three-matrix chain"),
        keep: axes(&[53, 11]),
        steps: vec![
            ContractionStep::new(vec![Operand::Input(1), Operand::Input(2)], axes(&[53, 23])),
            ContractionStep::new(vec![Operand::Input(0), Operand::Result(0)], axes(&[53, 11])),
        ],
        inputs: vec![
            input(
                &[(1., 0.), (0., 0.), (0., 0.), (1., 0.)],
                InputMutability::Immutable,
            ),
            input(
                &[(1., 0.), (0., 0.), (0., 1.), (2., 0.)],
                InputMutability::Immutable,
            ),
            input(
                &[(2., 0.), (1., 0.), (0., 0.), (0., -1.)],
                InputMutability::Immutable,
            ),
            input(
                &[(2., 0.), (1., 0.), (0., 0.), (0., -1.)],
                InputMutability::Mutable,
            ),
            input(
                &[(1., 0.), (0., 0.), (0., 0.), (0., 1.)],
                InputMutability::Immutable,
            ),
            input(
                &[(0., 0.), (0., 0.), (1., 0.), (0., 0.)],
                InputMutability::Immutable,
            ),
            input(&[(0.25, -0.5); 4], InputMutability::Immutable),
        ],
        selections: vec![
            selection("A-shared", vec![1, 1, 0], &alpha_squared, 26.),
            selection("B-diverged", vec![1, 2, 0], &alpha_beta, 14.),
            selection(
                "B-rejoined",
                vec![2, 2, 0],
                &[(4., 0.), (0., 0.), (2., -1.), (-1., 0.)],
                22.,
            ),
            selection("A-restored", vec![1, 1, 0], &alpha_squared, 26.),
            selection("mutable-before", vec![1, 3, 0], &alpha_beta, 14.),
            Selection {
                replacement: Some((3, replacement)),
                ..selection(
                    "C-replaced",
                    vec![1, 3, 0],
                    &[(-1., 3.), (0., 2.5), (6., 0.), (1., 0.)],
                    53.25,
                )
            },
            selection(
                "C-shared",
                vec![3, 3, 0],
                &[(1., 6.), (0., -1.), (-1.5, 0.), (0.25, 6.)],
                76.3125,
            ),
            selection(
                "S",
                vec![4, 0, 0],
                &[(1., 0.), (0., 0.), (0., 0.), (0., 1.)],
                2.,
            ),
            selection(
                "S-warm",
                vec![4, 0, 0],
                &[(1., 0.), (0., 0.), (0., 0.), (0., 1.)],
                2.,
            ),
            selection(
                "reset-branch",
                vec![5, 0, 0],
                &[(0., 0.), (1., 0.), (0., 0.), (0., 0.)],
                1.,
            ),
        ],
    }
}

fn joint() -> Fixture {
    let mut filter = vec![Complex64::new(0., 0.); 16];
    for (index, value) in
        [0, 5, 10, 15]
            .into_iter()
            .zip(values(&[(0.5, 0.), (0., 0.25), (-1., 0.), (2., 0.)]))
    {
        filter[index] = value;
    }
    filter[12] = Complex64::new(1., 1.);
    let mut joint = vec![Complex64::new(0., 0.); 16];
    for (column, value) in values(&[(1., 0.), (0., 1.), (-0.5, 0.), (2., -1.)])
        .into_iter()
        .enumerate()
    {
        joint[4 * column + (column ^ 3)] = value;
    }
    let selection = |label, operator, expected: &[(f64, f64)], squared_norm| Selection {
        label,
        inputs: vec![operator, 2],
        replacement: None,
        expected: values(expected),
        squared_norm,
    };
    let filtered = [(2.5, 0.), (0.5, 0.), (-0.5, 0.), (2., -2.)];
    let replaced_joint = [(1., -0.5), (0., 1.), (0., 1.), (0., 1.)];
    Fixture {
        name: "reusable_joint",
        network: TensorNetwork::new(vec![axes(&[0, 1, 2, 3]), axes(&[2, 3])])
            .expect("two-qubit operator and state"),
        keep: axes(&[1, 0]),
        steps: vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1)],
            axes(&[1, 0]),
        )],
        inputs: vec![
            Input {
                dimensions: vec![2; 4],
                values: filter.clone(),
                mutability: InputMutability::Immutable,
            },
            Input {
                dimensions: vec![2; 4],
                values: joint,
                mutability: InputMutability::Immutable,
            },
            Input {
                dimensions: vec![2; 2],
                values: values(&[(1., 0.), (0., 2.), (-0.5, 0.), (1., -1.)]),
                mutability: InputMutability::Mutable,
            },
            Input {
                dimensions: vec![2; 4],
                values: filter,
                mutability: InputMutability::Immutable,
            },
        ],
        selections: vec![
            selection("filter", 0, &filtered, 14.75),
            selection(
                "joint",
                1,
                &[(1., -3.), (-2., 0.), (0.25, 0.), (1., 0.)],
                15.0625,
            ),
            selection("filter-restored", 0, &filtered, 14.75),
            Selection {
                replacement: Some((2, values(&[(0., 1.), (1., 0.), (0., -2.), (0.5, 0.)]))),
                ..selection(
                    "state-replaced",
                    0,
                    &[(0.5, 1.), (0., 2.), (0., 0.25), (1., 0.)],
                    6.3125,
                )
            },
            selection("joint-replaced", 1, &replaced_joint, 4.25),
            selection("joint-warm", 1, &replaced_joint, 4.25),
        ],
    }
}

#[derive(Debug)]
struct Comparison {
    amplitude_error: f64,
    squared_norm_error: f64,
}

fn compare(
    actual: &[Complex64],
    expected: &[Complex64],
    expected_squared_norm: f64,
    limit: f64,
) -> Result<Comparison, String> {
    if actual.is_empty()
        || actual.len() != expected.len()
        || !limit.is_finite()
        || limit < 0.
        || !expected_squared_norm.is_finite()
        || expected_squared_norm < 0.
        || actual
            .iter()
            .chain(expected)
            .any(|value| !value.re.is_finite() || !value.im.is_finite())
    {
        return Err("invalid general-tensor shape, amplitude, norm or tolerance".into());
    }
    let expected_norm: f64 = expected.iter().map(Complex64::norm_sqr).sum();
    let actual_norm: f64 = actual.iter().map(Complex64::norm_sqr).sum();
    let report = Comparison {
        amplitude_error: actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (*a - *b).norm())
            .fold(0., f64::max),
        squared_norm_error: (actual_norm - expected_squared_norm).abs(),
    };
    if !expected_norm.is_finite()
        || !actual_norm.is_finite()
        || (expected_norm - expected_squared_norm).abs() > limit
        || report.amplitude_error > limit
        || report.squared_norm_error > limit
    {
        return Err(format!(
            "general-tensor mismatch: {report:?}; expected_squared_norm={expected_squared_norm}; limit={limit}; no normalization or phase alignment"
        ));
    }
    Ok(report)
}

#[test]
fn analytical_values_match_small_matrix_products_and_ordered_outputs() {
    for fixture in [chain(), joint()] {
        let query = fixture.query();
        assert_eq!(query.keep().element_count(), Some(4));
        assert_eq!(
            fixture
                .plan()
                .steps()
                .last()
                .expect("final step")
                .result_axes(),
            query.keep()
        );
        let mut inputs: Vec<_> = fixture
            .inputs
            .iter()
            .map(|input| input.values.clone())
            .collect();
        for step in &fixture.selections {
            if let Some((input, replacement)) = &step.replacement {
                assert_eq!(fixture.inputs[*input].mutability, InputMutability::Mutable);
                assert_eq!(inputs[*input].len(), replacement.len());
                inputs[*input] = replacement.clone();
            }
            let mut result = vec![Complex64::new(0., 0.); 4];
            if step.inputs.len() == 3 {
                let [a, b, c] = [
                    &inputs[step.inputs[0]],
                    &inputs[step.inputs[1]],
                    &inputs[step.inputs[2]],
                ];
                for row in 0..2 {
                    for column in 0..2 {
                        for j in 0..2 {
                            for k in 0..2 {
                                result[2 * row + column] +=
                                    a[row + 2 * j] * b[j + 2 * k] * c[k + 2 * column];
                            }
                        }
                    }
                }
            } else {
                let operator = &inputs[step.inputs[0]];
                let state = &inputs[step.inputs[1]];
                for row in 0..4 {
                    let swapped = (row % 2) * 2 + row / 2;
                    for column in 0..4 {
                        result[swapped] += operator[row + 4 * column] * state[column];
                    }
                }
            }
            assert_eq!(result, step.expected, "{}", step.label);
            compare(&result, &step.expected, step.squared_norm, LIMIT).expect("analytical norm");
        }
    }
}

#[test]
fn general_comparator_keeps_nonunit_norm_and_phase_sensitive_inclusive_limits() {
    let expected = values(&[(2., 0.), (0., 1.)]);
    compare(&expected, &expected, 5., LIMIT).expect("nonunit norm is intentional");
    let zero = values(&[(0., 0.)]);
    compare(&zero, &zero, 0., LIMIT).expect("zero tensor needs no normalization");
    for actual in [
        values(&[(0., 2.), (-1., 0.)]),
        values(&[(0., 1.), (2., 0.)]),
        values(&[(2., 0.)]),
        values(&[(f64::NAN, 0.), (0., 1.)]),
    ] {
        assert!(compare(&actual, &expected, 5., LIMIT).is_err());
    }
    assert!(compare(&expected, &expected, 1., LIMIT).is_err());
    for limit in [f64::NAN, f64::INFINITY, -1.] {
        assert!(compare(&expected, &expected, 5., limit).is_err());
    }
    let one = values(&[(1., 0.)]);
    let two = values(&[(2., 0.)]);
    compare(&two, &one, 1., 3.).expect("inclusive squared-norm boundary");
    assert!(compare(&two, &one, 1., 3_f64.next_down()).is_err());
    let negative_two = values(&[(-2., 0.)]);
    compare(&negative_two, &two, 4., 4.).expect("inclusive amplitude boundary");
    assert!(compare(&negative_two, &two, 4., 4_f64.next_down()).is_err());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native {
    use super::super::native::{preparation_error, save_output};
    use super::*;
    use crate::{
        library::CuTensorNetApi,
        simulation::{
            SimulationError,
            contraction::{
                execution::{CuTensorNetExecutableContraction, TensorInput},
                unexpected,
            },
            error::combine_execution_and_cleanup,
            resources::SessionResources,
        },
    };
    use qdk_simulators::execution::{ContractionContext, ExecutableContraction, ExecutionLimits};
    use std::{collections::BTreeSet, sync::Arc};

    const LIMITS: ExecutionLimits = ExecutionLimits {
        device_scratch_bytes: Some(67_108_864),
        host_scratch_bytes: Some(1_048_576),
    };

    fn resources(
        execution: &CuTensorNetExecutableContraction<'_, CuTensorNetApi>,
        fixture: &Fixture,
        registered: usize,
        selected: Option<&[usize]>,
        phase: &str,
    ) -> Result<(), SimulationError> {
        println!(
            "{}: phase={phase}; resources={:?}",
            fixture.name,
            execution.resources()
        );
        let report = execution.resources().as_ref();
        let resident_bytes: usize = fixture.inputs[..registered]
            .iter()
            .map(|input| input.values.len() * 16)
            .sum();
        let selected: Option<BTreeSet<_>> = selected.map(|inputs| inputs.iter().copied().collect());
        let selected_count = selected.as_ref().map(BTreeSet::len);
        let selected_bytes = selected.as_ref().map(|inputs| {
            inputs
                .iter()
                .map(|&index| fixture.inputs[index].values.len() * 16)
                .sum()
        });
        let scratch = report
            .device_scratch_allocated
            .ok_or_else(|| unexpected("prepared device scratch allocation is unknown"))?;
        if report.resident_input_count != Some(registered)
            || report.resident_input_bytes != Some(resident_bytes)
            || report.selected_input_count != selected_count
            || report.selected_input_bytes != selected_bytes
            || report.output_bytes != Some(64)
            || report.owned_device_bytes != Some(scratch + 64 + resident_bytes)
        {
            return Err(unexpected(format!(
                "{}: resource accounting mismatch at {phase}",
                fixture.name
            )));
        }
        Ok(())
    }

    fn check_retained(
        fixture: &Fixture,
        outputs: &[Vec<Complex64>],
    ) -> Result<(), SimulationError> {
        for (actual, step) in outputs.iter().zip(fixture.selections.iter().cycle()) {
            compare(actual, &step.expected, step.squared_norm, LIMIT).map_err(unexpected)?;
        }
        Ok(())
    }

    fn lifecycle(
        session: &mut SessionResources<CuTensorNetApi>,
        fixture: &Fixture,
        repetition: usize,
        outputs: &mut Vec<Vec<Complex64>>,
    ) -> Result<(), SimulationError> {
        let plan = fixture.plan();
        let mut execution = session
            .prepare(&fixture.query(), &plan, LIMITS)
            .map_err(preparation_error)?;
        let result = (|| {
            println!(
                "{}: repetition={repetition}; supplied_plan={plan:?}; limits={LIMITS:?}",
                fixture.name
            );
            resources(&execution, fixture, 0, None, "prepared")?;
            let metadata = execution.metadata()?;
            println!(
                "{}: metadata={metadata:?}; intermediate_modes={:?}",
                fixture.name,
                execution.intermediate_modes()?
            );
            let mut ids = Vec::new();
            for (index, input) in fixture.inputs.iter().enumerate() {
                println!(
                    "{}: register={index}; dimensions={:?}; values={:?}; mutability={:?}",
                    fixture.name, input.dimensions, input.values, input.mutability
                );
                ids.push(execution.register_input(
                    TensorInput {
                        dimensions: &input.dimensions,
                        values: &input.values,
                    },
                    input.mutability,
                )?);
                resources(&execution, fixture, ids.len(), None, "registered")?;
            }
            for (index, step) in fixture.selections.iter().enumerate() {
                if let Some((input, replacement)) = &step.replacement {
                    let before = execution.resources().clone();
                    println!("{}: replace={input}; values={replacement:?}", fixture.name);
                    execution.replace_input(
                        ids[*input],
                        TensorInput {
                            dimensions: &fixture.inputs[*input].dimensions,
                            values: replacement,
                        },
                    )?;
                    println!(
                        "{}: phase=replaced; resources={:?}",
                        fixture.name,
                        execution.resources()
                    );
                    if execution.resources() != &before {
                        return Err(unexpected(
                            "same-shape replacement changed resource accounting",
                        ));
                    }
                }
                println!(
                    "{}: selection={}; inputs={:?}; expected={:?}; expected_squared_norm={}",
                    fixture.name, step.label, step.inputs, step.expected, step.squared_norm
                );
                let selected: Vec<_> = step.inputs.iter().map(|&index| ids[index]).collect();
                let output = execution.execute(&selected)?;
                save_output(
                    fixture.name,
                    repetition * fixture.selections.len() + index,
                    &output,
                )?;
                let comparison = compare(&output, &step.expected, step.squared_norm, LIMIT)
                    .map_err(unexpected)?;
                println!(
                    "{}: selection={}; comparison={comparison:?}; limit={LIMIT}",
                    fixture.name, step.label
                );
                resources(
                    &execution,
                    fixture,
                    ids.len(),
                    Some(&step.inputs),
                    step.label,
                )?;
                outputs.push(output);
                check_retained(fixture, outputs)?;
            }
            if execution.plan() != &plan || execution.metadata()? != metadata {
                return Err(unexpected(
                    "supplied plan or metadata changed during input reuse",
                ));
            }
            Ok(())
        })();
        println!(
            "{}: final_resources={:?}",
            fixture.name,
            execution.resources()
        );
        let cleanup = execution.close();
        println!(
            "{}: repetition={repetition}; execution_cleanup={cleanup:?}",
            fixture.name
        );
        combine_execution_and_cleanup(result, cleanup)?;
        check_retained(fixture, outputs)
    }

    fn execute(fixture: &Fixture) -> Result<(), SimulationError> {
        let availability = crate::discover().expect("audited native libraries required");
        println!(
            "{}: versions={:?}; ordered_inputs={:?}; output_axes={:?}",
            fixture.name,
            availability.report(),
            fixture.network.nodes(),
            fixture.keep
        );
        println!(
            "{}: supplied plans only; no optimizer called by this test; native allocation/upload call counts not measured",
            fixture.name
        );
        let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
        let mut outputs = Vec::new();
        let result = (|| {
            for repetition in 0..2 {
                lifecycle(&mut session, fixture, repetition, &mut outputs)?;
            }
            Ok(())
        })();
        let cleanup = session.close();
        println!("{}: session_cleanup={cleanup:?}", fixture.name);
        combine_execution_and_cleanup(result, cleanup)?;
        check_retained(fixture, &outputs)?;
        println!(
            "{}: retained_outputs_after_close={}",
            fixture.name,
            outputs.len()
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires audited cuTensorNet/CUDA and GPU; separate reusable-input qualification"]
    fn a_supplied_plan_candidate_reuse() {
        execute(&chain()).expect("native candidate reuse, analytical outputs and cleanup");
    }

    #[test]
    #[ignore = "requires audited cuTensorNet/CUDA and GPU; separate reusable-input qualification"]
    fn b_supplied_plan_joint_operators() {
        execute(&joint()).expect("native two-qubit inputs, ordered outputs and cleanup");
    }
}
