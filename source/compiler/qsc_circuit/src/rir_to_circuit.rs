// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(crate) mod tracer;

use std::vec;

use crate::{
    Ket, Measurement, Operation, Register, Unitary,
    circuit::{ResolvedSourceLocation, SourceLocation},
    rir_to_circuit::tracer::{ResultRegister, WireId},
};
use qsc_data_structures::{
    debug::{DbgInfo, DbgMetadataScope},
    span::PackageSpan,
};
use qsc_frontend::{compile::PackageStore, location::Location};
use qsc_hir::hir::PackageId;

#[derive(Clone, Debug)]
pub(crate) enum DbgLocationKind {
    Resolved(PackageSpan),
    Unresolved(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct Op {
    kind: OperationKind,
    label: String,
    target_qubits: Vec<WireId>,
    control_qubits: Vec<WireId>,
    target_results: Vec<ResultRegister>,
    control_results: Vec<ResultRegister>,
    is_adjoint: bool,
    args: Vec<String>,
}

type DbgLocationId = usize;
type DbgScopeId = usize;

#[derive(Clone, Debug, PartialEq)]
struct InstructionStack(Vec<DbgLocationId>); // Can be empty

#[derive(Clone, Debug, PartialEq)]
struct ScopeStack {
    caller: InstructionStack,
    scope: DbgScopeId,
}

#[derive(Clone, Debug)]
enum OperationKind {
    Unitary { location: Option<DbgLocationKind> },
    Measurement { location: Option<DbgLocationKind> },
    Ket { location: Option<DbgLocationKind> },
}

impl From<Op> for Operation {
    fn from(value: Op) -> Self {
        let args = value.args.into_iter().collect();

        let targets = value
            .target_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q.0,
                result: None,
            })
            .chain(
                value
                    .target_results
                    .into_iter()
                    .map(|ResultRegister(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
            .collect();
        let controls = value
            .control_qubits
            .into_iter()
            .map(|q| Register {
                qubit: q.0,
                result: None,
            })
            .chain(
                value
                    .control_results
                    .into_iter()
                    .map(|ResultRegister(q, r)| Register {
                        qubit: q,
                        result: Some(r),
                    }),
            )
            .collect();

        let dbg_location = match &value.kind {
            OperationKind::Unitary { location }
            | OperationKind::Measurement { location }
            | OperationKind::Ket { location } => location,
        };

        let source = dbg_location.as_ref().and_then(to_source_location);

        match value.kind {
            OperationKind::Unitary { .. } => Operation::Unitary(Unitary {
                gate: value.label,
                args,
                children: vec![],
                targets,
                controls,
                is_adjoint: value.is_adjoint,
                source,
            }),
            OperationKind::Measurement { .. } => Operation::Measurement(Measurement {
                gate: value.label,
                args,
                children: vec![],
                qubits: controls,
                results: targets,
                source,
            }),
            OperationKind::Ket { .. } => Operation::Ket(Ket {
                gate: value.label,
                args,
                children: vec![],
                targets,
                source,
            }),
        }
    }
}

pub(crate) fn to_source_location(d: &DbgLocationKind) -> Option<SourceLocation> {
    if let DbgLocationKind::Resolved(location) = d {
        Some(SourceLocation::Unresolved(*location))
    } else {
        None
    }
}

pub(crate) fn resolve_location_metadata(operations: &mut [Op], dbg_info: &DbgInfo) {
    for op in operations {
        match &mut op.kind {
            OperationKind::Unitary { location }
            | OperationKind::Measurement { location }
            | OperationKind::Ket { location } => {
                if let Some(l) = location {
                    resolve_location_if_unresolved(dbg_info, l);
                }
            }
        }
    }
}

pub(crate) fn resolve_location_if_unresolved(dbg_info: &DbgInfo, location: &mut DbgLocationKind) {
    let resolved = match location {
        DbgLocationKind::Resolved(_) => None,
        DbgLocationKind::Unresolved(dbg_location) => resolve_location(dbg_info, *dbg_location),
    };
    if let Some(resolved) = resolved {
        *location = DbgLocationKind::Resolved(resolved);
    }
}

fn resolve_location(dbg_info: &DbgInfo, dbg_location: usize) -> Option<PackageSpan> {
    instruction_logical_stack(dbg_info, dbg_location)
        .and_then(|s| s.0.last().copied())
        .map(|l| dbg_info.dbg_locations[l].location)
}

pub(crate) fn fill_in_dbg_metadata(operations: &mut [Operation], package_store: &PackageStore) {
    for op in operations {
        let children_columns = op.children_mut();
        for column in children_columns {
            fill_in_dbg_metadata(&mut column.components, package_store);
        }

        let source = op.source_mut();
        if let Some(source) = source {
            resolve_source_location_if_unresolved(source, package_store);
        }
    }
}

pub(crate) fn resolve_source_location_if_unresolved(
    source: &mut SourceLocation,
    package_store: &PackageStore,
) {
    let location = match source {
        SourceLocation::Resolved(_) => None,
        SourceLocation::Unresolved(metadata_package_span) => Some(*metadata_package_span),
    };

    if let Some(location) = &location {
        let location = Location::from(
            location.span,
            location.package,
            package_store,
            qsc_data_structures::line_column::Encoding::Utf8,
        );
        *source = SourceLocation::Resolved(ResolvedSourceLocation {
            file: location.source.to_string(),
            line: location.range.start.line,
            column: location.range.start.column,
        });
    }
}

fn instruction_logical_stack(
    dbg_info: &DbgInfo,
    dbg_location_idx: usize,
) -> Option<InstructionStack> {
    let mut location_stack = vec![];
    let mut current_location_idx = Some(dbg_location_idx);

    while let Some(location_idx) = current_location_idx {
        location_stack.push(location_idx);
        let location = dbg_info
            .dbg_locations
            .get(location_idx)
            .expect("dbg location should exist");
        current_location_idx = location.inlined_at;
    }

    // filter out scopes in std and core
    location_stack.retain(|location| {
        let scope = &dbg_info.dbg_metadata_scopes[dbg_info.dbg_locations[*location].scope];
        match scope {
            DbgMetadataScope::SubProgram { name: _, location } => {
                let package_id = location.package;
                package_id != PackageId::CORE && package_id != PackageId::CORE.successor()
            }
        }
    });

    location_stack.reverse();

    Some(InstructionStack(location_stack))
}

fn loc_name(dbg_info: &DbgInfo, location: DbgLocationId) -> (String, u32) {
    let dbg_location = &dbg_info.dbg_locations[location];
    let scope_id: DbgScopeId = dbg_location.scope;
    let scope_name = scope_name(&dbg_info.dbg_metadata_scopes, scope_id);
    let offset = dbg_location.location.span.lo;

    (scope_name, offset)
}

fn scope_name(dbg_metadata_scopes: &[DbgMetadataScope], scope_id: usize) -> String {
    let scope = &dbg_metadata_scopes[scope_id];

    match scope {
        DbgMetadataScope::SubProgram { name, location: _ } => name.to_string(),
    }
}

#[allow(dead_code)]
fn fmt_scope_stack(dbg_info: &DbgInfo, stack: &ScopeStack) -> String {
    let mut names: Vec<String> = stack
        .caller
        .0
        .iter()
        .map(|loc| fmt_loc(dbg_info, *loc))
        .collect();
    names.push(scope_name(&dbg_info.dbg_metadata_scopes, stack.scope));
    names.join("->")
}

fn fmt_loc(dbg_info: &DbgInfo, location: usize) -> String {
    let (name, offset) = loc_name(dbg_info, location);
    format!("{name}@{offset}")
}
