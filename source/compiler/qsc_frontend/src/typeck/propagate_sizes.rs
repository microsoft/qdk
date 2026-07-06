// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_ast::ast::{NodeId, Package};
use qsc_data_structures::index_map::IndexMap;
use qsc_hir::ty::Ty;

use crate::{resolve::Res, typeck::Error};

pub fn propagate_array_sizes(
    _ast_package: &Package,
    _names: &IndexMap<NodeId, Res>,
    _tys: &mut IndexMap<NodeId, Ty>,
) -> Vec<Error> {
    Vec::new()
}
