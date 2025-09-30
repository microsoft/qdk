// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module contains the types and functions used to build the
//! data-interop layer between Python and Q#.

use crate::interpreter::QasmError;

use super::{Pauli, Result};
use num_bigint::BigInt;
use pyo3::{
    IntoPyObjectExt,
    conversion::FromPyObjectBound,
    exceptions::PyTypeError,
    prelude::*,
    types::{PyList, PyTuple},
};
use qsc::{
    fir::{self},
    hir::ty::{Prim, Ty},
    interpret::{self, Value},
};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Instances of this enum represent a Q# type. This is used
/// to send the definitions of Q# UDTs defined by the user to Python
/// and creating equivalent Python dataclasses in `qsharp.code.*`.
#[pyclass]
#[derive(Clone)]
pub(super) enum TypeIR {
    Primitive(PrimitiveKind),
    Tuple(Vec<TypeIR>),
    Array(Vec<TypeIR>),
    Udt(UdtIR),
}

#[pymethods]
impl TypeIR {
    fn kind(&self) -> TypeKind {
        match self {
            Self::Primitive(_) => TypeKind::Primitive,
            Self::Tuple(_) => TypeKind::Tuple,
            Self::Array(_) => TypeKind::Array,
            Self::Udt(_) => TypeKind::Udt,
        }
    }

    fn unwrap_primitive(&self) -> PyResult<PrimitiveKind> {
        if let Self::Primitive(ty) = self {
            Ok(*ty)
        } else {
            Err(PyTypeError::new_err("type is not a primitive".to_string()))
        }
    }

    fn unwrap_tuple(&self) -> PyResult<Vec<TypeIR>> {
        if let Self::Tuple(ty) = self {
            Ok(ty.clone())
        } else {
            Err(PyTypeError::new_err("type is not a tuple".to_string()))
        }
    }

    fn unwrap_array(&self) -> PyResult<Vec<TypeIR>> {
        if let Self::Tuple(ty) = self {
            Ok(ty.clone())
        } else {
            Err(PyTypeError::new_err("type is not an array".to_string()))
        }
    }

    fn unwrap_udt(&self) -> PyResult<UdtIR> {
        if let Self::Udt(ty) = self {
            Ok(ty.clone())
        } else {
            Err(PyTypeError::new_err("type is not a UDT".to_string()))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[pyclass(eq, eq_int, ord)]
pub(super) enum TypeKind {
    Primitive,
    Tuple,
    Array,
    Udt,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[pyclass(eq, eq_int, ord)]
pub(super) enum PrimitiveKind {
    Bool,
    Int,
    Double,
    Complex,
    String,
    Pauli,
    Result,
}

#[pyclass]
#[derive(Clone)]
pub(super) struct UdtIR {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    fields: Vec<(String, TypeIR)>,
}

/// This type is used to send objects from Python to Q#.
/// It is a `HashMap` to make it simple checking that the
/// objects have all the required fields to match the UDTs
/// they represent, without considering the order of the fields.
pub(super) type UdtFields = FxHashMap<String, PyObject>;

/// This type is used to send instances of UDTs from Q# to Python.
/// It is a `Vec` and not a `HashMap` to preserve the order of the fields,
/// since that results in a better user experience when printing the
/// objects in Python.
#[pyclass]
pub(super) struct UdtValue {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    fields: Vec<(String, PyObject)>,
}

#[pyclass]
#[derive(Clone)]
pub(super) enum PrimitiveValue {
    Bool(bool),
    Int(i64),
    BigInt(BigInt),
    Double(f64),
    Complex(num_complex::Complex64),
    String(String),
    Result(Result),
    Pauli(Pauli),
}

/// UDT fields are stored recursively, this function flattens that structure
/// and returns a vector with all the fields. Errors if any of the fields
/// is anonymous.
pub(super) fn collect_udt_fields<'ctx, 'udt_def>(
    udt: &'udt_def qsc::hir::ty::Udt,
) -> PyResult<Vec<(Rc<str>, &'ctx Ty)>>
where
    'udt_def: 'ctx,
{
    let mut fields = Vec::new();
    collect_udt_fields_rec(&udt.name, &udt.definition, &mut fields)?;
    Ok(fields)
}

fn collect_udt_fields_rec<'ctx, 'udt_def>(
    udt_name: &str,
    udt_def: &'udt_def qsc::hir::ty::UdtDef,
    buffer: &mut Vec<(Rc<str>, &'ctx Ty)>,
) -> PyResult<()>
where
    'udt_def: 'ctx,
{
    match &udt_def.kind {
        qsc::hir::ty::UdtDefKind::Field(udt_field) => {
            if let Some(name) = udt_field.name.as_ref() {
                buffer.push((name.clone(), &udt_field.ty));
                Ok(())
            } else {
                Err(PyTypeError::new_err(format!(
                    "structs with anonymous fields are not supported: {udt_name}"
                )))
            }
        }
        qsc::hir::ty::UdtDefKind::Tuple(udt_defs) => {
            for udt_def in udt_defs {
                collect_udt_fields_rec(udt_name, udt_def, buffer)?;
            }
            Ok(())
        }
    }
}

/// Gets the type name of a Python object.
fn obj_type(py: Python, obj: &PyObject) -> PyResult<String> {
    Ok(obj.bind(py).get_type().name()?.to_string())
}

/// A wrapper around the `obj.extract::<T>` functionality that allows to return
/// user friendly errors when casting fails, similar to the Q# ones.
fn extract_obj<'py, 'obj, T>(py: Python<'py>, obj: &'obj PyObject, ty: &Ty) -> PyResult<T>
where
    T: FromPyObjectBound<'obj, 'py>,
    'py: 'obj,
{
    match obj.extract::<T>(py) {
        Ok(val) => Ok(val),
        Err(err) => {
            if err.is_instance_of::<PyTypeError>(py) {
                // If we have a type error, we return a friendly user error.
                Err(PyTypeError::new_err(format!(
                    "expected {}, found {}",
                    ty.display(),
                    obj_type(py, obj)?
                )))
            } else {
                // If we have other kind of errors (e.g.: an overflow error when
                // converting from a python int to a rust i64) we leave it as is.
                Err(err)
            }
        }
    }
}

/// Given a type, convert a Python object into a Q# value of that type. This will recur through tuples and arrays,
/// and will return an error if the type is not supported or the object cannot be converted.
pub(super) fn pyobj_to_value(
    ctx: &interpret::Interpreter,
    py: Python,
    obj: &PyObject,
    ty: &Ty,
) -> PyResult<Value> {
    match ty {
        Ty::Prim(prim_ty) => match prim_ty {
            Prim::Bool => Ok(Value::Bool(extract_obj::<bool>(py, obj, ty)?)),
            Prim::Int => Ok(Value::Int(extract_obj::<i64>(py, obj, ty)?)),
            Prim::BigInt => Ok(Value::BigInt(extract_obj::<BigInt>(py, obj, ty)?)),
            Prim::Double => Ok(Value::Double(extract_obj::<f64>(py, obj, ty)?)),
            Prim::String => Ok(Value::String(extract_obj::<String>(py, obj, ty)?.into())),
            Prim::Result => Ok(Value::Result(extract_obj::<Result>(py, obj, ty)?.into())),
            Prim::Pauli => Ok(Value::Pauli(extract_obj::<Pauli>(py, obj, ty)?.into())),
            Prim::Qubit | Prim::Range | Prim::RangeTo | Prim::RangeFrom | Prim::RangeFull => {
                unimplemented!("primitive input type: {prim_ty:?}")
            }
        },
        Ty::Tuple(tup) => {
            let objs = extract_obj::<Vec<PyObject>>(py, obj, ty)?;

            if tup.len() != objs.len() {
                return Err(PyTypeError::new_err(format!(
                    "mismatched tuple arity: expected {}, found {}",
                    tup.len(),
                    objs.len()
                )));
            }
            if objs.len() == 1 {
                pyobj_to_value(ctx, py, &objs[0], &tup[0])
            } else {
                let mut tuple = Vec::new();
                for (obj, ty) in objs.iter().zip(tup) {
                    tuple.push(pyobj_to_value(ctx, py, obj, ty)?);
                }
                Ok(Value::Tuple(tuple.into(), None))
            }
        }
        Ty::Array(ty) => {
            let objs = extract_obj::<Vec<PyObject>>(py, obj, ty)?;
            let ty = &**ty;
            let mut array = Vec::new();
            for obj in &objs {
                array.push(pyobj_to_value(ctx, py, obj, ty)?);
            }
            Ok(Value::Array(array.into()))
        }
        Ty::Udt(_, res) => {
            let qsc::hir::Res::Item(item_id) = res else {
                panic!("Udt should be an item");
            };
            let (udt, kind) = ctx.udt_ty_from_item_id(item_id);

            match kind {
                interpret::UdtKind::Angle => {
                    let angle = extract_obj::<f64>(py, obj, ty)?;
                    let angle = qsc::qasm::stdlib::angle::Angle::from_f64_maybe_sized(angle, None);
                    let value = i64::try_from(angle.value)
                        .expect("angles built with `None` size have at most 53 bits");
                    let size = i64::from(angle.size);
                    Ok(Value::Tuple(
                        Rc::new([Value::Int(value), Value::Int(size)]),
                        Some(Rc::new(ctx.get_angle_id())),
                    ))
                }
                interpret::UdtKind::Complex => {
                    let val = extract_obj::<num_complex::Complex64>(py, obj, ty)?;
                    Ok(Value::Tuple(
                        Rc::new([Value::Double(val.re), Value::Double(val.im)]),
                        Some(Rc::new(ctx.get_complex_id())),
                    ))
                }
                interpret::UdtKind::Udt => {
                    let udt_fields = extract_obj::<UdtFields>(py, obj, ty)?;

                    let mut tuple = Vec::new();
                    for (name, ty) in collect_udt_fields(udt)? {
                        let Some(value) = udt_fields.get(&*name) else {
                            return Err(PyTypeError::new_err(format!(
                                "missing field {} in {}",
                                name, udt.name,
                            )));
                        };
                        tuple.push(pyobj_to_value(ctx, py, value, ty)?);
                    }
                    Ok(Value::Tuple(tuple.into(), None))
                }
            }
        }
        _ => unimplemented!("input type: {ty}"),
    }
}

pub(super) fn type_ir_from_qsharp_ty(ctx: &interpret::Interpreter, ty: &Ty) -> Option<TypeIR> {
    match ty {
        Ty::Prim(prim) => {
            let prim = match prim {
                Prim::Bool => PrimitiveKind::Bool,
                Prim::Int | Prim::BigInt => PrimitiveKind::Int,
                Prim::Double => PrimitiveKind::Double,
                Prim::String => PrimitiveKind::String,
                Prim::Pauli => PrimitiveKind::Pauli,
                Prim::Result => PrimitiveKind::Result,
                Prim::Qubit | Prim::Range | Prim::RangeTo | Prim::RangeFrom | Prim::RangeFull => {
                    return None;
                }
            };
            Some(TypeIR::Primitive(prim))
        }
        Ty::Array(ty) | Ty::SizedArray(ty, _) => {
            Some(TypeIR::Array(vec![type_ir_from_qsharp_ty(ctx, ty)?]))
        }
        Ty::Tuple(items) => {
            let mut tuple = Vec::new();
            for item in items {
                tuple.push(type_ir_from_qsharp_ty(ctx, item)?);
            }
            Some(TypeIR::Tuple(tuple))
        }
        Ty::Udt(name, res) => {
            let qsc::hir::Res::Item(item_id) = res else {
                panic!("Udt should be an item");
            };
            let (udt, kind) = ctx.udt_ty_from_item_id(item_id);

            match kind {
                interpret::UdtKind::Angle => Some(TypeIR::Primitive(PrimitiveKind::Double)),
                interpret::UdtKind::Complex => Some(TypeIR::Primitive(PrimitiveKind::Complex)),
                interpret::UdtKind::Udt => {
                    let udt_fields = collect_udt_fields(udt).ok()?;
                    let mut fields = Vec::new();

                    for (name, ty) in udt_fields {
                        fields.push((name.to_string(), type_ir_from_qsharp_ty(ctx, ty)?));
                    }

                    Some(TypeIR::Udt(UdtIR {
                        name: name.to_string(),
                        fields,
                    }))
                }
            }
        }
        Ty::Param { .. } | Ty::Infer(..) | Ty::Arrow(..) | Ty::Err => None,
    }
}

pub(crate) fn value_to_pyobj(
    ctx: &interpret::Interpreter,
    py: Python,
    value: &Value,
) -> PyResult<PyObject> {
    match value {
        Value::Int(val) => val.into_py_any(py),
        Value::BigInt(val) => val.into_py_any(py),
        Value::Double(val) => val.into_py_any(py),
        Value::Bool(val) => val.into_py_any(py),
        Value::String(val) => val.into_py_any(py),
        Value::Result(val) => {
            let val = match val {
                qsc::interpret::Result::Id(_) => {
                    panic!("unexpected Result::Id in typed_value_to_value_ir")
                }
                qsc::interpret::Result::Val(true) => Result::One,
                qsc::interpret::Result::Val(false) => Result::Zero,
                qsc::interpret::Result::Loss => Result::Loss,
            };
            val.into_py_any(py)
        }
        Value::Pauli(val) => {
            let val = match val {
                fir::Pauli::I => Pauli::I,
                fir::Pauli::X => Pauli::X,
                fir::Pauli::Y => Pauli::Y,
                fir::Pauli::Z => Pauli::Z,
            };
            val.into_py_any(py)
        }
        Value::Tuple(values, None) => {
            let mut tuple = Vec::new();
            for val in values.iter() {
                tuple.push(value_to_pyobj(ctx, py, val)?);
            }

            // Special case Value::UNIT maps to None.
            if tuple.is_empty() {
                Ok(py.None())
            } else {
                PyTuple::new(py, tuple)?.into_py_any(py)
            }
        }
        Value::Tuple(values, Some(store_item_id)) => {
            let (udt, kind) = ctx.udt_ty_from_store_item_id(**store_item_id);

            match kind {
                interpret::UdtKind::Angle => {
                    let value = values[0].clone().unwrap_int();
                    let size = values[1].clone().unwrap_int();
                    let value = u64::try_from(value).expect("value should fit in u64");
                    let size = u32::try_from(size).expect("size should fit in u32");
                    let angle = qsc::qasm::stdlib::angle::Angle::new(value, size);
                    let angle: f64 = angle
                        .try_into()
                        .map_err(|_| QasmError::new_err("failed to cast angle to 64-bit float"))?;
                    angle.into_py_any(py)
                }
                interpret::UdtKind::Complex => {
                    let re = values[0].clone().unwrap_double();
                    let im = values[1].clone().unwrap_double();
                    let val = num_complex::Complex { re, im };
                    val.into_py_any(py)
                }
                interpret::UdtKind::Udt => {
                    let ty_fields = collect_udt_fields(udt)?;
                    let mut fields = Vec::new();
                    for (value, (name, _)) in values.iter().zip(ty_fields) {
                        fields.push((name.to_string(), value_to_pyobj(ctx, py, value)?));
                    }
                    UdtValue {
                        name: udt.name.to_string(),
                        fields,
                    }
                    .into_py_any(py)
                }
            }
        }
        Value::Array(values) => {
            let mut array = Vec::with_capacity(values.len());
            for val in values.iter() {
                array.push(value_to_pyobj(ctx, py, val)?);
            }
            PyList::new(py, array)?.into_py_any(py)
        }
        Value::Closure(..)
        | Value::Global(..)
        | Value::Qubit(..)
        | Value::Range(..)
        | Value::Var(..) => format!("<{}> {}", value.type_name(), value).into_py_any(py),
    }
}
