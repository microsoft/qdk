// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_hir::{
    hir::{Item, ItemKind, Pat, PatKind},
    ty::{Prim, Ty},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("expression does not evaluate to an operation that takes qubit parameters")]
    #[diagnostic(code("Qsc.Circuit.NoCircuitForOperation"))]
    #[diagnostic(help(
        "provide the name of a callable or a lambda expression that only takes qubits as parameters"
    ))]
    NoQubitParameters,
    #[error("cannot generate circuit for controlled invocation")]
    #[diagnostic(code("Qsc.Circuit.ControlledUnsupported"))]
    #[diagnostic(help(
        "controlled invocations are not currently supported. consider wrapping the invocation in a lambda expression"
    ))]
    ControlledUnsupported,
}

pub struct QubitParam {
    /// The number of array dimensions of the qubit input parameter.
    /// `Qubit` is 0, `Qubit[]` is 1, `Qubit[][]` is 2, etc.
    pub(crate) dimensions: u32,
    /// The source offset of the parameter in the operation declaration.
    pub(crate) source_offset: u32,
}

impl QubitParam {
    /// The total number of qubit array elements for this input parameter.
    #[must_use]
    pub fn num_qubits(&self) -> u32 {
        NUM_QUBITS.pow(self.dimensions)
    }
}

/// If the item is a callable, returns the information that would
/// be needed to generate a circuit for it.
///
/// If the item is not a callable, returns `None`.
/// If the callable takes any non-qubit parameters, returns `None`.
///
/// If the callable only takes qubit parameters (including qubit arrays) or no parameters,
/// returns the qubit parameter information.
#[must_use]
pub fn qubit_param_info(item: &Item) -> Option<Vec<QubitParam>> {
    if let ItemKind::Callable(decl) = &item.kind {
        if decl.input.ty == Ty::UNIT {
            // Support no parameters by allocating 0 qubits.
            return Some(vec![]);
        }

        let param_info = get_qubit_param_info(&decl.input);

        if !param_info.is_empty() {
            return Some(param_info);
        }
    }
    None
}

/// Returns an entry expression to directly invoke the operation
/// for the purposes of generating a circuit for it.
///
/// `operation_expr` is the source for the expression that refers to the operation,
/// e.g. "Test.Foo" or "qs => H(qs[0])".
///
/// If the item is not a callable, returns `None`.
/// If the callable takes any non-qubit parameters, returns `None`.
pub fn entry_expr_for_qubit_operation(
    item: &Item,
    functor_app: qsc_data_structures::functors::FunctorApp,
    operation_expr: &str,
) -> Result<String, Error> {
    if functor_app.controlled > 0 {
        return Err(Error::ControlledUnsupported);
    }

    if let Some(param_info) = qubit_param_info(item) {
        return Ok(operation_circuit_entry_expr(operation_expr, &param_info));
    }

    Err(Error::NoQubitParameters)
}

/// Generates the entry expression to call the operation described by `params`.
/// The expression allocates qubits and invokes the operation.
#[must_use]
fn operation_circuit_entry_expr(operation_expr: &str, qubit_params: &[QubitParam]) -> String {
    let alloc_qubits = format!(
        "use qs = Qubit[{}];",
        qubit_params.iter().map(QubitParam::num_qubits).sum::<u32>()
    );

    let mut qs_start = 0;
    let mut call_args = vec![];
    for q in qubit_params {
        // Q# ranges are end-inclusive
        let qs_end = qs_start + q.num_qubits() - 1;
        if q.dimensions == 0 {
            call_args.push(format!("qs[{qs_start}]"));
        } else {
            // Array argument - use a range to index
            let mut call_arg = format!("qs[{qs_start}..{qs_end}]");
            for _ in 1..q.dimensions {
                // Chunk the array for multi-dimensional array arguments
                call_arg = format!("Microsoft.Quantum.Arrays.Chunks({NUM_QUBITS}, {call_arg})");
            }
            call_args.push(call_arg);
        }
        qs_start = qs_end + 1;
    }

    let call_args = call_args.join(", ");

    // We don't reset the qubits since we don't want reset gates
    // included in circuit output.
    // We also don't measure the qubits but we have to return a result
    // array to satisfy Base Profile.
    format!(
        r#"{{
            {alloc_qubits}
            ({operation_expr})({call_args});
            let r: Result[] = [];
            r
        }}"#
    )
}

/// The number of qubits to allocate for each qubit array
/// in the operation arguments.
const NUM_QUBITS: u32 = 2;

fn get_qubit_param_info(input: &Pat) -> Vec<QubitParam> {
    match &input.ty {
        Ty::Prim(Prim::Qubit) => {
            return vec![QubitParam {
                dimensions: 0,
                source_offset: input.span.lo,
            }];
        }
        Ty::Array(ty) => {
            if let Some(element_dim) = get_array_dimension(ty) {
                let dim = element_dim + 1;
                return vec![QubitParam {
                    dimensions: dim,
                    source_offset: input.span.lo,
                }];
            }
        }
        Ty::Tuple(tys) => {
            let params = if let PatKind::Tuple(pats) = &input.kind {
                pats.iter()
                    .map(|p| {
                        get_array_dimension(&p.ty).map(|dimension| QubitParam {
                            dimensions: dimension,
                            source_offset: p.span.lo,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                tys.iter()
                    .map(|ty| {
                        get_array_dimension(ty).map(|dimension| QubitParam {
                            dimensions: dimension,
                            source_offset: input.span.lo,
                        })
                    })
                    .collect::<Vec<_>>()
            };

            if params.iter().all(Option::is_some) {
                return params.into_iter().map(Option::unwrap).fold(
                    vec![],
                    |mut param_info, param| {
                        param_info.push(param);
                        param_info
                    },
                );
            }
        }
        _ => {}
    }
    vec![]
}

/// If `Ty` is a qubit or a qubit array, returns the number of dimensions of the array.
/// A qubit is considered to be a 0-dimensional array.
/// For example, for a `Qubit` it returns `Some(0)`, for a `Qubit[][]` it returns `Some(2)`.
/// For a non-qubit type, returns `None`.
fn get_array_dimension(input: &Ty) -> Option<u32> {
    match input {
        Ty::Prim(Prim::Qubit) => Some(0),
        Ty::Array(ty) => get_array_dimension(ty).map(|d| d + 1),
        _ => None,
    }
}
