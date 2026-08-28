// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_hir::{
    hir::{Attr, Item, ItemKind, Pat, PatKind},
    ty::{Prim, Ty},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("expression does not evaluate to an operation that takes qubit parameters")]
    #[diagnostic(code("Qdk.Qsc.Circuit.NoCircuitForOperation"))]
    #[diagnostic(help(
        "provide the name of a callable or a lambda expression that only takes qubits as parameters"
    ))]
    NoQubitParameters,
    #[error("cannot generate circuit for controlled invocation")]
    #[diagnostic(code("Qdk.Qsc.Circuit.ControlledUnsupported"))]
    #[diagnostic(help(
        "controlled invocations are not currently supported. consider wrapping the invocation in a lambda expression"
    ))]
    ControlledUnsupported,
    #[error("circuit input exceeds the maximum of {MAXIMUM_QUBITS_IN_CIRCUIT} qubits")]
    #[diagnostic(code("Qdk.Qsc.Circuit.TooManyQubits"))]
    TooManyQubits,
    #[error("program has features that are unsupported for circuit diagrams: {0}")]
    #[diagnostic(code("Qdk.Qsc.Circuit.UnsupportedFeature"))]
    UnsupportedFeature(String),
}

pub struct QubitParam {
    /// The number of array dimensions of the qubit input parameter.
    /// `Qubit` is 0, `Qubit[]` is 1, `Qubit[][]` is 2, etc.
    pub(crate) dimensions: u32,
    /// The selected lengths of the dimensions of an array parameter.
    pub(crate) sizes: Vec<u32>,
    /// The source offset of the parameter in the operation declaration.
    pub(crate) source_offset: u32,
}

impl QubitParam {
    /// The total number of qubit array elements for this input parameter.
    #[must_use]
    pub fn num_qubits(&self) -> u32 {
        self.sizes
            .iter()
            .copied()
            .fold(1, u32::saturating_mul)
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

        let mut param_info = get_qubit_param_info(&decl.input);

        if !param_info.is_empty() {
            let input_sizes = decl.attrs.iter().find_map(|attr| match attr {
                Attr::CircuitRenderingOptions(options) => options.input_sizes.as_deref(),
                _ => None,
            });
            apply_input_sizes(&mut param_info, input_sizes);
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
        let total_num_qubits = param_info
            .iter()
            .map(QubitParam::num_qubits)
            .fold(0, u32::saturating_add);
        if total_num_qubits > MAXIMUM_QUBITS_IN_CIRCUIT {
            return Err(Error::TooManyQubits);
        }
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
        if q.dimensions == 0 {
            call_args.push(format!("qs[{qs_start}]"));
        } else {
            call_args.push(build_nested_qubit_array_arg(qs_start, &q.sizes));
        }
        qs_start += q.num_qubits();
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

/// The default length of each dimension of a qubit-array input parameter.
const DEFAULT_NUM_QUBITS: u32 = 2;

/// The maximum number of qubits allocated when rendering a circuit.
const MAXIMUM_QUBITS_IN_CIRCUIT: u32 = 10_000;

/// Applies user-provided lengths to qubit-array input parameters.
///
/// Values apply to array dimensions in declaration order. Scalar parameters are skipped, extra
/// values are ignored, and dimensions without a value retain their default size.
fn apply_input_sizes(params: &mut [QubitParam], input_sizes: Option<&[u32]>) {
    if let Some(input_sizes) = input_sizes {
        params
            .iter_mut()
            .flat_map(|param| &mut param.sizes)
            .zip(input_sizes.iter().copied())
            .for_each(|(size, input_size)| *size = input_size);
    }
}

/// Constructs a nested qubit array argument for a circuit entry expression.
///
/// Generates explicit array constructors for multi-dimensional qubit array parameters.
/// For example, a 2D qubit array parameter receives nested array syntax: `[[qs[0..1], qs[2..3]], [qs[4..5], qs[6..7]]]`
/// Recursively partitions the qubit range using the length of each dimension.
fn build_nested_qubit_array_arg(start: u32, sizes: &[u32]) -> String {
    debug_assert!(!sizes.is_empty(), "array dimensions should be positive");

    if let [size] = sizes {
        let end = start + size - 1;
        return format!("qs[{start}..{end}]");
    }

    let chunk_width = sizes[1..].iter().product::<u32>();
    let chunks = (0..sizes[0])
        .map(|chunk_index| {
            build_nested_qubit_array_arg(start + chunk_index * chunk_width, &sizes[1..])
        })
        .collect::<Vec<_>>();
    format!("[{}]", chunks.join(", "))
}

fn get_qubit_param_info(input: &Pat) -> Vec<QubitParam> {
    match &input.ty {
        Ty::Prim(Prim::Qubit) => {
            return vec![QubitParam {
                dimensions: 0,
                sizes: vec![],
                source_offset: input.span.lo,
            }];
        }
        Ty::Array(ty) => {
            if let Some(element_dim) = get_array_dimension(ty) {
                let dim = element_dim + 1;
                return vec![QubitParam {
                    dimensions: dim,
                    sizes: vec![DEFAULT_NUM_QUBITS; dim as usize],
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
                            sizes: vec![DEFAULT_NUM_QUBITS; dimension as usize],
                            source_offset: p.span.lo,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                tys.iter()
                    .map(|ty| {
                        get_array_dimension(ty).map(|dimension| QubitParam {
                            dimensions: dimension,
                            sizes: vec![DEFAULT_NUM_QUBITS; dimension as usize],
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
