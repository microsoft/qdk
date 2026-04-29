// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cmp::Ordering;

use pyo3::{
    Bound, PyResult,
    types::{PyAnyMethods, PyDict},
};
use resource_estimator::estimates::DistillationUnit;

use super::super::utils::SerializableBound;

/// A wrapper to model a distillation unit for round-based factory builders.
pub struct PythonDistillationUnit<'py> {
    dict: Bound<'py, PyDict>,
    name: String,
    code_parameter: Option<SerializableBound<'py>>,
}

impl<'py> PythonDistillationUnit<'py> {
    pub fn new(dict: Bound<'py, PyDict>) -> PyResult<Self> {
        let name = dict.get_item("name").map_or_else(
            |_| Ok(String::from("distillation-unit")),
            |field| -> PyResult<String> { Ok(field.to_string()) },
        )?;

        let code_parameter = dict.get_item("code_parameter").map_or(
            Ok(None),
            |field| -> PyResult<Option<SerializableBound<'py>>> {
                if field.is_none() {
                    Ok(None)
                } else {
                    Ok(Some(SerializableBound(field)))
                }
            },
        )?;

        Ok(Self {
            dict,
            name,
            code_parameter,
        })
    }
}

impl<'py> DistillationUnit<SerializableBound<'py>> for PythonDistillationUnit<'py> {
    fn num_output_states(&self) -> u64 {
        let Ok(field) = self.dict.get_item("num_output_states") else {
            return 1;
        };

        field
            .extract()
            .expect("can extract u64 value from field num_output_states")
    }

    fn num_input_states(&self) -> u64 {
        self.dict
            .get_item("num_input_states")
            .expect("has field num_input_states")
            .extract()
            .expect("can extract u64 value from field num_input_states")
    }

    fn duration(&self, position: usize) -> u64 {
        self.dict
            .get_item("duration")
            .expect("has field duration")
            .call1((position,))
            .expect("can call lambda duration")
            .extract()
            .expect("can extract u64 value from lambda duration")
    }

    fn physical_qubits(&self, position: usize) -> u64 {
        self.dict
            .get_item("physical_qubits")
            .expect("has field physical_qubits")
            .call1((position,))
            .expect("can call lambda physical_qubits")
            .extract()
            .expect("can extract u64 value from lambda physical_qubits")
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn code_parameter(&self) -> Option<&SerializableBound<'py>> {
        self.code_parameter.as_ref()
    }

    fn output_error_rate(&self, input_error_rate: f64) -> f64 {
        self.dict
            .get_item("output_error_rate")
            .expect("has field output_error_rate")
            .call1((input_error_rate,))
            .expect("can call lambda output_error_rate")
            .extract()
            .expect("can extract u64 value from lambda output_error_rate")
    }

    fn failure_probability(&self, input_error_rate: f64) -> f64 {
        self.dict
            .get_item("failure_probability")
            .expect("has field failure_probability")
            .call1((input_error_rate,))
            .expect("can call lambda failure_probability")
            .extract()
            .expect("can extract u64 value from lambda failure_probability")
    }
}

impl PartialEq for PythonDistillationUnit<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self.code_parameter(), other.code_parameter()) {
            (Some(lhs), Some(rhs)) => {
                let Ok(eq_fun) = lhs.getattr("__eq__") else {
                    return false;
                };
                let Ok(eq_result) = eq_fun.call1((&**rhs,)) else {
                    return false;
                };

                eq_result.extract().unwrap_or(false)
            }
            (None, None) => true,
            _ => false,
        }
    }
}

impl PartialOrd for PythonDistillationUnit<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let lhs = self.code_parameter()?;
        let rhs = other.code_parameter()?;

        let eq_fun = lhs.getattr("__eq__").ok()?;
        if eq_fun.call1((&**rhs,)).ok()?.extract().ok()? {
            return Some(Ordering::Equal);
        }

        let lt_fun = lhs.getattr("__lt__").ok()?;
        if lt_fun.call1((&**rhs,)).ok()?.extract().ok()? {
            Some(Ordering::Less)
        } else {
            Some(Ordering::Greater)
        }
    }
}

pub enum OrderedBFSControl {
    Continue,
    #[allow(dead_code)]
    Cutoff,
    Terminate,
}

/// Performs a breadth-first search (BFS) over the elements with up to
/// `max_depth` repetitions.  It is guaranteed that the elements remain in the
/// same order as initially provided.  The function `visit` is called on each
/// tuple.
pub fn ordered_bfs<T: PartialOrd>(
    elements: &[T],
    max_depth: usize,
    mut visit: impl FnMut(Vec<&T>) -> Result<OrderedBFSControl, String>,
) -> Result<(), String> {
    let mut prev_prefixes = vec![vec![]];

    for _ in 1..=max_depth {
        let mut prefixes = vec![];

        for stem in &prev_prefixes {
            for (idx, element) in elements.iter().enumerate() {
                if stem.last().is_some_and(|last| *element < elements[*last]) {
                    continue;
                }

                let mut new_tuple = stem.clone();
                new_tuple.push(idx);

                match visit(new_tuple.iter().copied().map(|i| &elements[i]).collect())? {
                    OrderedBFSControl::Continue => {
                        prefixes.push(new_tuple);
                    }
                    OrderedBFSControl::Cutoff => {}
                    OrderedBFSControl::Terminate => {
                        return Ok(());
                    }
                }
            }
        }

        prev_prefixes = prefixes;
    }

    Ok(())
}
