// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ops::Deref;

use pyo3::{
    Bound, IntoPyObject, PyAny, PyErr, PyResult, Python,
    exceptions::PyTypeError,
    types::{
        PyAnyMethods, PyBool, PyDict, PyDictMethods, PyFloat, PyInt, PyList, PyListMethods, PyNone,
        PyString, PyTypeMethods,
    },
};
use serde::{Serialize, ser::SerializeMap};
use serde_json::{Map, Value, json};

/// Converts a JSON value to a Python object, handling various types such as
/// `null`, `number`, `string`, `boolean`, `array`, and `object`.
fn json_value_to_python_object<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(PyNone::get(py).to_owned().into_any()),
        Value::Number(n) => {
            if let Some(int) = n.as_i64() {
                Ok(int.into_pyobject(py)?.into_any())
            } else if let Some(float) = n.as_f64() {
                Ok(float.into_pyobject(py)?.into_any())
            } else {
                Err(PyErr::new::<PyTypeError, _>(format!("cannot convert {n}")))
            }
        }
        Value::String(s) => Ok(PyString::new(py, s).into_any()),
        &Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any()),
        Value::Array(elements) => {
            let list = PyList::empty(py);

            for element in elements {
                list.append(json_value_to_python_object(py, element)?)?;
            }

            Ok(list.into_any())
        }
        Value::Object(map) => Ok(json_map_to_python_dict(py, map)?.into_any()),
    }
}

/// Converts a JSON map to a Python dictionary.
pub(crate) fn json_map_to_python_dict<'py>(
    py: Python<'py>,
    map: &Map<String, Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (key, value) in map {
        dict.set_item(key, json_value_to_python_object(py, value)?)?;
    }
    Ok(dict)
}

/// Converts a Python object to a JSON value, handling various types such as
/// `None`, `int`, `float`, `bool`, `str`, `list`, and `dict`.
pub(crate) fn python_object_to_json_value(value: &Bound<'_, PyAny>) -> Option<Value> {
    if value.is_none() {
        Some(Value::Null)
    } else if let Ok(n) = value.downcast_exact::<PyInt>() {
        Some(json!(n.extract::<i64>().expect("n is PyInt")))
    } else if let Ok(n) = value.downcast_exact::<PyFloat>() {
        Some(json!(n.extract::<f64>().expect("n is PyFloat")))
    } else if let Ok(b) = value.downcast_exact::<PyBool>() {
        Some(json!(b.extract::<bool>().expect("b is PyBool")))
    } else if let Ok(s) = value.downcast_exact::<PyString>() {
        Some(json!(s.extract::<String>().expect("s is PyString")))
    } else if let Ok(l) = value.downcast_exact::<PyList>() {
        let values: Vec<_> = l
            .iter()
            .map(|v| python_object_to_json_value(&v))
            .collect::<Option<_>>()?;
        Some(Value::Array(values))
    } else if let Ok(d) = value.downcast_exact::<PyDict>() {
        Some(Value::Object(python_dict_to_json_map(d)?))
    } else {
        None
    }
}

/// Converts a Python dictionary to a JSON map.
pub fn python_dict_to_json_map(value: &Bound<'_, PyDict>) -> Option<Map<String, Value>> {
    let mut map: Map<String, Value> = Map::new();
    for (key, value) in value.iter() {
        map.insert(
            key.extract::<String>().ok()?,
            python_object_to_json_value(&value)?,
        );
    }
    Some(map)
}

/// A wrapper around a Python instance that can be serialized in order to embed
/// its value into the returned resource estimates.
#[derive(Clone, Debug)]
pub struct SerializableBound<'py>(pub Bound<'py, PyAny>);

impl<'py> Deref for SerializableBound<'py> {
    type Target = Bound<'py, PyAny>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for SerializableBound<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Ok(dict) = self.downcast::<PyDict>() {
            let mut map = serializer.serialize_map(Some(dict.len()))?;
            for (key, value) in dict.iter() {
                map.serialize_key(&SerializableBound(key))?;
                map.serialize_value(&SerializableBound(value))?;
            }
            map.end()
        } else if let Ok(number) = self.downcast::<PyInt>() {
            serializer.serialize_i64(number.extract().expect("number is PyInt"))
        } else {
            serializer.serialize_str(&self.to_string())
        }
    }
}

/// Extracts a method from a Python instance and checks if it is callable
pub fn extract_and_check_method<'py>(
    instance: &Bound<'py, PyAny>,
    method_name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let member = instance.getattr(method_name)?;
    if !member.is_callable() {
        return Err(PyTypeError::new_err(format!(
            "Method '{}' is not callable on the instance of type '{}'",
            method_name,
            instance.get_type().name()?
        )));
    }
    Ok(member)
}

/// Attempts to extract a method from a Python instance and checks if it is
/// callable
pub fn maybe_extract_and_check_method<'py>(
    instance: &Bound<'py, PyAny>,
    method_name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if !instance.hasattr(method_name)? {
        return Ok(None);
    }

    let member = instance.getattr(method_name)?;
    if !member.is_callable() {
        return Err(PyTypeError::new_err(format!(
            "Method '{}' is not callable on the instance of type '{}'",
            method_name,
            instance.get_type().name()?
        )));
    }
    Ok(Some(member))
}
