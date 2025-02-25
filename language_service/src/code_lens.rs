// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    compilation::{Compilation, CompilationKind},
    protocol::{CodeLens, CodeLensCommand, OperationInfo},
    qsc_utils::into_range,
};
use qsc::{
    circuit::qubit_param_info,
    hir::{ty::Ty, CallableKind, ItemKind},
    line_column::Encoding,
};

pub(crate) fn get_code_lenses(
    compilation: &Compilation,
    source_name: &str,
    position_encoding: Encoding,
) -> Vec<CodeLens> {
    if matches!(compilation.kind, CompilationKind::Notebook { .. }) {
        // entrypoint actions don't work in notebooks
        return vec![];
    }

    let user_unit = compilation.user_unit();
    let package = &user_unit.package;
    let source_span = compilation.package_span_of_source(source_name);

    package
        .items
        .iter()
        .fold(Vec::new(), |mut accum, (_, item)| {
            if source_span.contains(item.span.lo) {
                if let ItemKind::Callable(decl) = &item.kind {
                    if let Some(ItemKind::Namespace(ns, _)) = item
                        .parent
                        .and_then(|parent_id| package.items.get(parent_id))
                        .map(|parent| &parent.kind)
                    {
                        let namespace = ns.name();
                        let range = into_range(position_encoding, decl.span, &user_unit.sources);
                        let name = decl.name.name.clone();

                        if decl.input.ty == Ty::UNIT {
                            // For a callable that takes no input, always show all lenses except for circuit.
                            let expr = format!("{namespace}.{name}()");
                            accum = accum
                                .into_iter()
                                .chain([
                                    CodeLens {
                                        range,
                                        command: CodeLensCommand::Run(expr.clone()),
                                    },
                                    CodeLens {
                                        range,
                                        command: CodeLensCommand::Histogram(expr.clone()),
                                    },
                                    CodeLens {
                                        range,
                                        command: CodeLensCommand::Estimate(expr.clone()),
                                    },
                                    CodeLens {
                                        range,
                                        command: CodeLensCommand::Debug(expr),
                                    },
                                ])
                                .collect();
                        }
                        if decl.kind == CallableKind::Operation {
                            // If this is either an operation that takes no arguments or one that takes only qubit arguments,
                            // show the circuit lens.
                            if decl.input.ty == Ty::UNIT {
                                accum.push(CodeLens {
                                    range,
                                    command: CodeLensCommand::Circuit(OperationInfo {
                                        operation: format!("{namespace}.{name}"),
                                        total_num_qubits: 0,
                                    }),
                                });
                            } else if let Some((_, total_num_qubits)) = qubit_param_info(item) {
                                accum.push(CodeLens {
                                    range,
                                    command: CodeLensCommand::Circuit(OperationInfo {
                                        operation: format!("{namespace}.{name}"),
                                        total_num_qubits,
                                    }),
                                });
                            }
                        }

                        return accum;
                    }
                }
            }
            accum
        })
}
