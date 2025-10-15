// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{borrow::Cow, marker::PhantomData};

use pyo3::{
    Bound, PyAny, PyResult,
    exceptions::PyLookupError,
    types::{PyAnyMethods, PyDict, PyList},
};
use resource_estimator::estimates::{
    DistillationRound, Factory, FactoryBuilder, PhysicalQubitCalculation, RoundBasedFactory,
};
use round_based::{OrderedBFSControl, ordered_bfs};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::generic_estimator::utils::maybe_extract_and_check_method;

use super::{
    code::PythonQEC,
    utils::{SerializableBound, python_dict_to_json_map},
};

mod dispatch;
pub use dispatch::PythonFactoryBuilderDispatch;
pub(crate) mod round_based;
pub use round_based::PythonDistillationUnit;

enum FactoryImplementation<'py> {
    Generic(Bound<'py, PyAny>),
    RoundBased {
        distillation_units: Bound<'py, PyAny>,
        trivial_distillation_unit: Option<Bound<'py, PyAny>>,
    },
}

/// A wrapper around a Python instance to compute magic state factories.
///
/// There are two ways to model magic state factories: 1) create factories
/// explicitly, which are modeled by means of their size, runtime, and number of
/// output states, or 2) return distillation units which can then be composed in
/// multiple rounds to create a factory.
///
/// ## Explicitly creating factories
///
/// To create magic state factories explicitly, the class must implement the
/// function `find_factories`, which takes as input the code that is used for
/// algorithm qubits (it must not be used by the factory builder, and other
/// codes could be constructed), the qubit parameters specified by the user, and
/// the target error rate required for magic states in the current estimation.
/// The function either returns `None`, if, e.g., no factories can be found that
/// satisfy the target error rate, or it returns a list of factory objects.
///
/// ```python
///     def find_factories(self, code, qubit, target_error_rate):
///         # e.g., return [
///         #   {
///         #     "physical_qubits": 42,
///         #     "duration": 655321,  # in ns
///         #     "num_output_states": 1
///         #   }
///         # ]
///         ...
/// ```
///
/// A factory object is simply a Python dictionary, which _must_ contain
/// entries for `"physical_qubits"` and `"duration"` (in nano seconds).  It may
/// also contain `"num_output_states"`, which defaults to 1 if not specified. It
/// may contain other entries, which will be included in the serialized resource
/// estimates.
///
/// ## Creating factories based on multiple rounds of distillation
///
/// To create factories based on multiple rounds of distillation, the class must
/// implement the function `distillation_units`, which returns a list of
/// distillation unit objects (described below). Further, such a class may
/// provide local variables `gate_error`, `max_rounds` and `max_extra_rounds`
/// (by default set to `"gate_error"`, 3 and 5, respectively). The variable
/// `gate_error` is a string that is used to index the `qubit` parameter for the
/// magic gate error rate.  The variable `max_rounds` controls how many rounds
/// of distillation should always be explored (even, if a factory is found with
/// fewer than `max_rounds` rounds). And the variable `max_extra_rounds`
/// controls by how many extra rounds the search should be extended, if no
/// factory can be found within `max_rounds` rounds.
///
/// ```python
///     def distillation_units(self, code, qubit, max_code_parameter):
///         # e.g., return [
///         #   {
///         #     "name": "unit",
///         #     "code_parameter": 5,  # must be same type as max_code_parameter
///         #     "num_input_states": 42,
///         #     "num_output_states": 2,
///         #     "physical_qubits": lambda round: 100,  # round is 0-based index to when unit is used in factory
///         #     "duration": lambda round: 30,  # duration in ns, and round is as above
///         #     "output_error_rate": lambda input_error_rate: ...,  # some formula to compute output_error_rate
///         #     "failure_probability": lambda input_error_rate: ...  # some formula to compute failure_probability
///         #   }
///         # ]
/// ```
///
/// A distillation unit object is simply a Python dictionary, which _must_
/// contain entries for `"num_input_states"`, `"physical_qubits"`, `"duration"`,
/// `"output_error_rate"`, and `"failure_probability"`.  The fields `"name"`,
/// `"code_parameter"`, and `"num_output_states"` are optional and default to
/// `"distillation-unit"`, `None`, and `1`.  The fields for `"physical_qubits"`
/// and `"runtime"` must be callable (e.g., using a Python lambda) with the
/// 0-based round index as only parameter.  The fields for `"output_error_rate"`
/// and `"failure_probability"` also must be callable with the input error rate
/// (of the previous round or initial gate error rate) as the only parameter.
pub struct PythonFactoryBuilder<'py> {
    builder: Bound<'py, PyAny>,
    implementation: FactoryImplementation<'py>,
    num_magic_state_types: usize,
}

impl<'py> PythonFactoryBuilder<'py> {
    pub fn from_bound(builder: Bound<'py, PyAny>) -> PyResult<Self> {
        let implementation = if let Ok(find_factories) = builder.getattr("find_factories") {
            FactoryImplementation::Generic(find_factories)
        } else if let Ok(distillation_units) = builder.getattr("distillation_units") {
            let trivial_distillation_unit =
                maybe_extract_and_check_method(&builder, "trivial_distillation_unit")?;
            FactoryImplementation::RoundBased {
                distillation_units,
                trivial_distillation_unit,
            }
        } else {
            return Err(PyLookupError::new_err(
                "FactoryBuilder must have either find_factories or distillation_units method",
            ));
        };

        let num_magic_state_types = if let Ok(method) = builder.getattr("num_magic_state_types") {
            method.call0()?.extract()?
        } else {
            1
        };

        Ok(Self {
            builder,
            implementation,
            num_magic_state_types,
        })
    }

    fn create_generic_factories<'a>(
        code: &PythonQEC<'py>,
        qubit: &Bound<'py, PyDict>,
        output_error_rate: f64,
        find_factories: &Bound<'py, PyAny>,
    ) -> Result<Vec<std::borrow::Cow<'a, PythonFactory<'py>>>, String> {
        let result = find_factories
            .call1((code.bound(), qubit.as_ref(), output_error_rate))
            .map_err(|e| e.to_string())?;

        if result.is_none() {
            Ok(vec![])
        } else {
            let factories = result.downcast::<PyList>().map_err(|e| e.to_string())?;
            let mut converted = vec![];

            for element in factories {
                let dict = element.downcast::<PyDict>().map_err(|e| e.to_string())?;
                let factory = PythonFactory::from_py_dict(dict).ok_or(format!(
                            "Failed to convert factory from Python dict: {dict:?}, does the dictionary contain entries for 'physical_qubits' and 'duration'?",
                        ))?;
                converted.push(Cow::Owned(factory));
            }

            Ok(converted)
        }
    }

    fn create_round_based_factories(
        &self,
        code: &PythonQEC<'py>,
        qubit: &Bound<'py, PyDict>,
        output_error_rate: f64,
        max_code_parameter: &SerializableBound<'py>,
        distillation_units: &Bound<'py, PyAny>,
        trivial_distillation_unit: Option<&Bound<'py, PyAny>>,
    ) -> Result<Vec<std::borrow::Cow<PythonFactory<'py>>>, String> {
        // input error rate
        let qubit_key = self
            .builder
            .getattr("gate_error")
            .and_then(|a| a.extract())
            .unwrap_or(String::from("gate_error"));
        let initial_input_error_rate = qubit
            .get_item(&qubit_key)
            .map_err(|e| e.to_string())?
            .extract()
            .map_err(|e| e.to_string())?;

        if initial_input_error_rate <= output_error_rate
            && let Some(trivial_distillation_unit) = trivial_distillation_unit
        {
            return Self::create_trivial_distillation_unit(
                code,
                qubit,
                max_code_parameter,
                trivial_distillation_unit,
                initial_input_error_rate,
            );
        }

        let use_max_qubits_per_round = self
            .builder
            .getattr("use_max_qubits_per_round")
            .and_then(|a| a.extract())
            .unwrap_or(false);
        let max_rounds = self
            .builder
            .getattr("max_rounds")
            .and_then(|a| a.extract())
            .unwrap_or(3);
        let max_extra_rounds = self
            .builder
            .getattr("max_extra_rounds")
            .and_then(|a| a.extract())
            .unwrap_or(5);

        let return_value = distillation_units
            .call1((code.bound(), qubit.as_ref(), &**max_code_parameter))
            .map_err(|e| e.to_string())?;

        let units: Vec<_> = return_value
            .try_iter()
            .map_err(|e| {
                format!("{e} (check the return value of the 'distillation_units' method)",)
            })?
            .map(|bound| PythonDistillationUnit::new(bound?.downcast_into::<PyDict>()?))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;

        let mut factories: Vec<Cow<PythonFactory>> = vec![];

        ordered_bfs(&units, max_extra_rounds, |selected_units| {
            if selected_units.len() > max_rounds && !factories.is_empty() {
                return Ok(OrderedBFSControl::Terminate);
            }

            if let Ok(mut factory) =
                RoundBasedFactory::build(&selected_units, initial_input_error_rate, 0.01)
            {
                if factory.output_error_rate() <= output_error_rate {
                    factory.set_physical_qubit_calculation(if use_max_qubits_per_round {
                        PhysicalQubitCalculation::Max
                    } else {
                        PhysicalQubitCalculation::Sum
                    });
                    factories.push(Cow::Owned(PythonFactory::try_from(factory)?));

                    if selected_units.len() > max_rounds {
                        return Ok(OrderedBFSControl::Terminate);
                    }
                }
            }

            Ok(OrderedBFSControl::Continue)
        })?;

        if factories.is_empty() {
            return Err(format!(
                "No factories found for output error rate: {output_error_rate}, try increasing the 'max_rounds' parameter."
            ));
        }

        factories.sort_unstable_by(|f1, f2| {
            f1.normalized_qubits()
                .total_cmp(&f2.normalized_qubits())
                .then(f1.duration().cmp(&f2.duration()))
        });

        Ok(factories)
    }

    fn create_trivial_distillation_unit<'a>(
        code: &PythonQEC<'py>,
        qubit: &Bound<'py, PyDict>,
        max_code_parameter: &SerializableBound<'py>,
        trivial_distillation_unit: &Bound<'py, PyAny>,
        input_error_rate: f64,
    ) -> Result<Vec<std::borrow::Cow<'a, PythonFactory<'py>>>, String> {
        let result = trivial_distillation_unit
            .call1((code.bound(), qubit.as_ref(), &**max_code_parameter))
            .map_err(|e| e.to_string())?;

        let unit = PythonDistillationUnit::new(
            result
                .downcast_into::<PyDict>()
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let round = DistillationRound::new(&unit, 0.0, 0);

        let factory =
            RoundBasedFactory::new(1, 0.0, vec![round], vec![input_error_rate; 2], vec![0.0; 2]);

        Ok(vec![Cow::Owned(PythonFactory::try_from(factory)?)])
    }
}

impl<'py> FactoryBuilder<PythonQEC<'py>> for PythonFactoryBuilder<'py> {
    type Factory = PythonFactory<'py>;

    fn find_factories(
        &self,
        code: &PythonQEC<'py>,
        qubit: &std::rc::Rc<Bound<'py, PyDict>>,
        _magic_state_type: usize,
        output_error_rate: f64,
        max_code_parameter: &SerializableBound<'py>,
    ) -> Result<Vec<std::borrow::Cow<Self::Factory>>, String> {
        match &self.implementation {
            FactoryImplementation::Generic(find_factories) => {
                Self::create_generic_factories(code, qubit, output_error_rate, find_factories)
            }
            FactoryImplementation::RoundBased {
                distillation_units,
                trivial_distillation_unit,
            } => self.create_round_based_factories(
                code,
                qubit,
                output_error_rate,
                max_code_parameter,
                distillation_units,
                trivial_distillation_unit.as_ref(),
            ),
        }
    }

    fn num_magic_state_types(&self) -> usize {
        self.num_magic_state_types
    }
}

#[derive(Clone, Serialize)]
pub struct PythonFactory<'py> {
    #[serde(flatten)]
    values: Map<String, Value>,
    #[serde(skip)]
    physical_qubits: u64,
    #[serde(skip)]
    duration: u64,
    #[serde(skip)]
    num_output_states: u64,
    #[serde(skip)]
    phantom: PhantomData<&'py ()>,
}

impl<'py> PythonFactory<'py> {
    fn new(values: Map<String, Value>) -> Option<Self> {
        let physical_qubits = values
            .get("physical_qubits")
            .and_then(serde_json::Value::as_u64)?;
        let duration = values.get("duration").and_then(serde_json::Value::as_u64)?;
        let num_output_states = values
            .get("num_output_states")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);

        Some(Self {
            values,
            physical_qubits,
            duration,
            num_output_states,
            phantom: PhantomData,
        })
    }

    fn from_py_dict(dict: &Bound<'py, PyDict>) -> Option<Self> {
        Self::new(python_dict_to_json_map(dict)?)
    }

    #[allow(clippy::cast_precision_loss)] // Relevant numbers in arithmetic are small enough
    pub fn normalized_qubits(&self) -> f64 {
        self.physical_qubits() as f64 / self.num_output_states() as f64
    }
}

impl<'py> TryFrom<RoundBasedFactory<SerializableBound<'py>>> for PythonFactory<'py> {
    type Error = String;

    fn try_from(value: RoundBasedFactory<SerializableBound<'py>>) -> Result<Self, Self::Error> {
        let values = json! {{
            "physical_qubits": value.physical_qubits(),
            "duration": value.duration(),
            "num_rounds": value.num_rounds(),
            "num_output_states": value.num_output_states(),
            "num_units_per_round": value.num_units_per_round(),
            "num_input_states": value.num_input_states(),
            "physical_qubits_per_round": value.physical_qubits_per_round(),
            "duration_per_round": value.duration_per_round(),
            "unit_name_per_round": value.unit_names(),
            "code_parameter_per_round": value
                .code_parameter_per_round()
                .iter()
                .map(|p| p.map_or(Value::Null, |p| json!(p)))
                .collect::<Vec<_>>(),
            "logical_error_rate": value.output_error_rate()
        }};

        Self::new(values.as_object().expect("values is a JSON object").clone()).ok_or(String::from(
            "Failed to construct factory instance from round-based factory information",
        ))
    }
}

impl<'py> Factory for PythonFactory<'py> {
    type Parameter = SerializableBound<'py>;

    fn physical_qubits(&self) -> u64 {
        self.physical_qubits
    }

    fn duration(&self) -> u64 {
        self.duration
    }

    fn num_output_states(&self) -> u64 {
        self.num_output_states
    }

    fn max_code_parameter(&self) -> Option<Cow<Self::Parameter>> {
        None
    }
}
