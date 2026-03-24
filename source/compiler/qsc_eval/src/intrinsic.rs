// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod utils;

#[cfg(test)]
mod tests;

use crate::{
    Error, Rc,
    backend::{Backend, TracingBackend},
    debug::Frame,
    error::PackageSpan,
    output::Receiver,
    val::{self, Value, unwrap_tuple},
};
use num_bigint::BigInt;
use rand::{Rng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use std::convert::TryFrom;

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn call<B: Backend>(
    name: &str,
    name_span: PackageSpan,
    arg: Value,
    arg_span: PackageSpan,
    call_stack: &[Frame],
    sim: &mut TracingBackend<'_, B>,
    rng: &mut StdRng,
    out: &mut dyn Receiver,
) -> Result<Value, Error> {
    match name {
        "Length" => match arg.unwrap_array().len().try_into() {
            Ok(len) => Ok(Value::Int(len)),
            Err(_) => Err(Error::ArrayTooLarge(arg_span)),
        },
        #[allow(clippy::cast_precision_loss)]
        "IntAsDouble" => Ok(Value::Double(arg.unwrap_int() as f64)),
        "IntAsBigInt" => Ok(Value::BigInt(BigInt::from(arg.unwrap_int()))),
        "DoubleAsStringWithPrecision" => {
            let [input, prec_val] = unwrap_tuple(arg);
            let prec_int = prec_val.unwrap_int();
            if prec_int < 0 {
                Err(Error::InvalidNegativeInt(prec_int, arg_span))
            } else {
                let precision = usize::try_from(prec_int).expect("integer value");
                let is_zero = if precision == 0 { "." } else { "" };
                Ok(Value::String(Rc::from(format!(
                    "{:.*}{}",
                    precision,
                    input.unwrap_double(),
                    is_zero
                ))))
            }
        }
        "DumpMachine" => {
            let (state, qubit_count) = sim.capture_quantum_state();
            match out.state(state, qubit_count) {
                Ok(()) => Ok(Value::unit()),
                Err(_) => Err(Error::OutputFail(name_span)),
            }
        }
        "DumpRegister" => {
            let qubits = arg.unwrap_array();
            let qubits_len = qubits.len();
            let qubits = qubits
                .iter()
                .filter_map(|q| q.clone().unwrap_qubit().try_deref().map(|q| q.0))
                .collect::<Vec<_>>();
            if qubits.len() != qubits_len {
                return Err(Error::QubitUsedAfterRelease(arg_span));
            }
            if qubits.len() != qubits.iter().collect::<FxHashSet<_>>().len() {
                return Err(Error::QubitUniqueness(arg_span));
            }
            let (state, qubit_count) = sim.capture_quantum_state();
            let state = utils::split_state(&qubits, &state, qubit_count)
                .map_err(|()| Error::QubitsNotSeparable(arg_span))?;
            match out.state(state, qubits.len()) {
                Ok(()) => Ok(Value::unit()),
                Err(_) => Err(Error::OutputFail(name_span)),
            }
        }
        "DumpMatrix" => {
            let qubits = arg.unwrap_array();
            let qubits_len = qubits.len();
            let qubits = qubits
                .iter()
                .filter_map(|q| q.clone().unwrap_qubit().try_deref().map(|q| q.0))
                .collect::<Vec<_>>();
            if qubits.len() != qubits_len {
                return Err(Error::QubitUsedAfterRelease(arg_span));
            }
            if qubits.len() != qubits.iter().collect::<FxHashSet<_>>().len() {
                return Err(Error::QubitUniqueness(arg_span));
            }
            let (state, qubit_count) = sim.capture_quantum_state();
            let state = utils::split_state(&qubits, &state, qubit_count)
                .map_err(|()| Error::QubitsNotSeparable(arg_span))?;
            let matrix = utils::state_to_matrix(state, qubits.len() / 2);
            match out.matrix(matrix) {
                Ok(()) => Ok(Value::unit()),
                Err(_) => Err(Error::OutputFail(name_span)),
            }
        }
        "PermuteLabels" => qubit_relabel(arg, arg_span, |q0, q1| {
            sim.qubit_swap_id(q0, q1, call_stack);
        }),
        "Message" => match out.message(&arg.unwrap_string()) {
            Ok(()) => Ok(Value::unit()),
            Err(_) => Err(Error::OutputFail(name_span)),
        },
        "CheckZero" => Ok(Value::Bool(
            sim.qubit_is_zero(
                arg.unwrap_qubit()
                    .try_deref()
                    .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                    .0,
            ),
        )),
        "ArcCos" => Ok(Value::Double(arg.unwrap_double().acos())),
        "ArcSin" => Ok(Value::Double(arg.unwrap_double().asin())),
        "ArcTan" => Ok(Value::Double(arg.unwrap_double().atan())),
        "ArcTan2" => {
            let [x, y] = unwrap_tuple(arg);
            Ok(Value::Double(x.unwrap_double().atan2(y.unwrap_double())))
        }
        "Cos" => Ok(Value::Double(arg.unwrap_double().cos())),
        "Cosh" => Ok(Value::Double(arg.unwrap_double().cosh())),
        "Sin" => Ok(Value::Double(arg.unwrap_double().sin())),
        "Sinh" => Ok(Value::Double(arg.unwrap_double().sinh())),
        "Tan" => Ok(Value::Double(arg.unwrap_double().tan())),
        "Tanh" => Ok(Value::Double(arg.unwrap_double().tanh())),
        "Sqrt" => Ok(Value::Double(arg.unwrap_double().sqrt())),
        "Log" => Ok(Value::Double(arg.unwrap_double().ln())),
        "DrawRandomInt" => {
            let [lo, hi] = unwrap_tuple(arg);
            let lo = lo.unwrap_int();
            let hi = hi.unwrap_int();
            if lo > hi {
                Err(Error::EmptyRange(arg_span))
            } else {
                Ok(Value::Int(rng.gen_range(lo..=hi)))
            }
        }
        "DrawRandomDouble" => {
            let [lo, hi] = unwrap_tuple(arg);
            let lo = lo.unwrap_double();
            let hi = hi.unwrap_double();
            if lo > hi {
                Err(Error::EmptyRange(arg_span))
            } else {
                Ok(Value::Double(rng.gen_range(lo..=hi)))
            }
        }
        "DrawRandomBool" => {
            let p = arg.unwrap_double();
            Ok(Value::Bool(rng.gen_bool(p)))
        }
        #[allow(clippy::cast_possible_truncation)]
        "Truncate" => Ok(Value::Int(arg.unwrap_double() as i64)),
        "__quantum__qis__ccx__body" => three_qubit_gate(
            |ctl0, ctl1, q| sim.ccx(ctl0, ctl1, q, call_stack),
            arg,
            arg_span,
        ),
        "__quantum__qis__cx__body" => {
            two_qubit_gate(|ctl, q| sim.cx(ctl, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__cy__body" => {
            two_qubit_gate(|ctl, q| sim.cy(ctl, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__cz__body" => {
            two_qubit_gate(|ctl, q| sim.cz(ctl, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__rx__body" => {
            one_qubit_rotation(|theta, q| sim.rx(theta, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__rxx__body" => two_qubit_rotation(
            |theta, q0, q1| sim.rxx(theta, q0, q1, call_stack),
            arg,
            arg_span,
        ),
        "__quantum__qis__ry__body" => {
            one_qubit_rotation(|theta, q| sim.ry(theta, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__ryy__body" => two_qubit_rotation(
            |theta, q0, q1| sim.ryy(theta, q0, q1, call_stack),
            arg,
            arg_span,
        ),
        "__quantum__qis__rz__body" => {
            one_qubit_rotation(|theta, q| sim.rz(theta, q, call_stack), arg, arg_span)
        }
        "__quantum__qis__rzz__body" => two_qubit_rotation(
            |theta, q0, q1| sim.rzz(theta, q0, q1, call_stack),
            arg,
            arg_span,
        ),
        "__quantum__qis__h__body" => one_qubit_gate(|q| sim.h(q, call_stack), arg, arg_span),
        "__quantum__qis__s__body" => one_qubit_gate(|q| sim.s(q, call_stack), arg, arg_span),
        "__quantum__qis__s__adj" => one_qubit_gate(|q| sim.sadj(q, call_stack), arg, arg_span),
        "__quantum__qis__sx__body" => one_qubit_gate(|q| sim.sx(q, call_stack), arg, arg_span),
        "__quantum__qis__t__body" => one_qubit_gate(|q| sim.t(q, call_stack), arg, arg_span),
        "__quantum__qis__t__adj" => one_qubit_gate(|q| sim.tadj(q, call_stack), arg, arg_span),
        "__quantum__qis__x__body" => one_qubit_gate(|q| sim.x(q, call_stack), arg, arg_span),
        "__quantum__qis__y__body" => one_qubit_gate(|q| sim.y(q, call_stack), arg, arg_span),
        "__quantum__qis__z__body" => one_qubit_gate(|q| sim.z(q, call_stack), arg, arg_span),
        "__quantum__qis__swap__body" => {
            two_qubit_gate(|q0, q1| sim.swap(q0, q1, call_stack), arg, arg_span)
        }
        "__quantum__qis__reset__body" => {
            one_qubit_gate(|q| sim.reset(q, call_stack), arg, arg_span)
        }
        "__quantum__qis__m__body" => Ok(Value::Result(
            sim.m(
                arg.unwrap_qubit()
                    .try_deref()
                    .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                    .0,
                call_stack,
            ),
        )),
        "__quantum__qis__mresetz__body" => Ok(Value::Result(
            sim.mresetz(
                arg.unwrap_qubit()
                    .try_deref()
                    .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                    .0,
                call_stack,
            ),
        )),
        "__quantum__rt__read_loss" => Ok(Value::Bool(arg == Value::Result(val::Result::Loss))),
        _ => {
            let qubits = arg.qubits();
            let qubits_len = qubits.len();
            let qubits = qubits
                .iter()
                .filter_map(|q| q.try_deref().map(|q| q.0))
                .collect::<Vec<_>>();
            if qubits.len() != qubits_len {
                return Err(Error::QubitUsedAfterRelease(arg_span));
            }
            if let Some(result) = sim.custom_intrinsic(name, arg, call_stack) {
                match result {
                    Ok(value) => Ok(value),
                    Err(message) => Err(Error::IntrinsicFail(name.to_string(), message, name_span)),
                }
            } else {
                Err(Error::UnknownIntrinsic(name.to_string(), name_span))
            }
        }
    }
}

fn one_qubit_gate(
    mut gate: impl FnMut(usize),
    arg: Value,
    arg_span: PackageSpan,
) -> Result<Value, Error> {
    gate(
        arg.unwrap_qubit()
            .try_deref()
            .ok_or(Error::QubitUsedAfterRelease(arg_span))?
            .0,
    );
    Ok(Value::unit())
}

fn two_qubit_gate(
    mut gate: impl FnMut(usize, usize),
    arg: Value,
    arg_span: PackageSpan,
) -> Result<Value, Error> {
    let [x, y] = unwrap_tuple(arg);
    if x == y {
        Err(Error::QubitUniqueness(arg_span))
    } else {
        gate(
            x.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
            y.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
        );
        Ok(Value::unit())
    }
}

fn one_qubit_rotation(
    mut gate: impl FnMut(f64, usize),
    arg: Value,
    arg_span: PackageSpan,
) -> Result<Value, Error> {
    let [x, y] = unwrap_tuple(arg);
    let angle = x.unwrap_double();
    if angle.is_nan() || angle.is_infinite() {
        Err(Error::InvalidRotationAngle(angle, arg_span))
    } else {
        gate(
            angle,
            y.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
        );
        Ok(Value::unit())
    }
}

fn three_qubit_gate(
    mut gate: impl FnMut(usize, usize, usize),
    arg: Value,
    arg_span: PackageSpan,
) -> Result<Value, Error> {
    let [x, y, z] = unwrap_tuple(arg);
    if x == y || y == z || x == z {
        Err(Error::QubitUniqueness(arg_span))
    } else {
        gate(
            x.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
            y.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
            z.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
        );
        Ok(Value::unit())
    }
}

fn two_qubit_rotation(
    mut gate: impl FnMut(f64, usize, usize),
    arg: Value,
    arg_span: PackageSpan,
) -> Result<Value, Error> {
    let [x, y, z] = unwrap_tuple(arg);
    let angle = x.unwrap_double();
    if y == z {
        Err(Error::QubitUniqueness(arg_span))
    } else if angle.is_nan() || angle.is_infinite() {
        Err(Error::InvalidRotationAngle(angle, arg_span))
    } else {
        gate(
            angle,
            y.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
            z.unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitUsedAfterRelease(arg_span))?
                .0,
        );
        Ok(Value::unit())
    }
}

/// Performs relabeling of qubits from the a given left array to the corresponding right array.
/// The function will swap qubits with the given function to match the new relabeling, returning an error
/// if the qubits are not unique or if the relabeling is not a valid permutation.
pub fn qubit_relabel(
    arg: Value,
    arg_span: PackageSpan,
    mut swap: impl FnMut(usize, usize),
) -> Result<Value, Error> {
    let [left, right] = unwrap_tuple(arg);
    let left = left.unwrap_array();
    let left_len = left.len();
    let left = left
        .iter()
        .filter_map(|q| q.clone().unwrap_qubit().try_deref().map(|q| q.0))
        .collect::<Vec<_>>();
    if left.len() != left_len {
        return Err(Error::QubitUsedAfterRelease(arg_span));
    }
    let right = right.unwrap_array();
    let right_len = right.len();
    let right = right
        .iter()
        .filter_map(|q| q.clone().unwrap_qubit().try_deref().map(|q| q.0))
        .collect::<Vec<_>>();
    if right.len() != right_len {
        return Err(Error::QubitUsedAfterRelease(arg_span));
    }
    let left_set = left.iter().collect::<FxHashSet<_>>();
    let right_set = right.iter().collect::<FxHashSet<_>>();
    if left.len() != left_set.len() || right.len() != right_set.len() {
        return Err(Error::QubitUniqueness(arg_span));
    }
    if left_set != right_set {
        return Err(Error::RelabelingMismatch(arg_span));
    }

    // Start with a mapping of each qubit to itself.
    let mut mappings: FxHashMap<usize, usize> =
        left.iter().copied().zip(left.iter().copied()).collect();
    for (l, r) in left.into_iter().zip(right.into_iter()) {
        // Trivial case where the qubit is already mapped to itself in the relabel, which can be short circuited.
        if l == r {
            continue;
        }

        // Check what each label currently maps to.
        let mapped_l = *mappings.get(&l).expect("mapped qubit should be present");
        let mapped_r = *mappings.get(&r).expect("mapped qubit should be present");

        // We only need to swap if the label is not pointing to the correct qubit.
        if mapped_l != r && mapped_r != l {
            // Do a reverse lookup to find which label is currently mapped to the desired right qubit.
            // This tells us which label to use in the swap, which we will use in the update of the mappings too.
            let label_r = *mappings
                .keys()
                .find(|k| mappings[*k] == r)
                .expect("mapped qubit should be present as both key and value");
            swap(l, label_r);
            mappings.insert(label_r, mapped_l);
            mappings.insert(l, mapped_r);
        }
    }

    Ok(Value::unit())
}
