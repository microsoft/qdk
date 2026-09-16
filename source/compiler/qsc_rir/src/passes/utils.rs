// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    builder,
    rir::{CallableId, Program},
};

pub(crate) fn find_callable(program: &Program, name: &str) -> Option<CallableId> {
    for (callable_id, callable) in program.callables.iter() {
        if callable.name == name {
            return Some(callable_id);
        }
    }
    None
}

pub(crate) fn add_m(program: &mut Program) -> CallableId {
    let m_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(m_id, builder::m_decl());
    m_id
}

pub(crate) fn add_cx(program: &mut Program) -> CallableId {
    let cx_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(cx_id, builder::cx_decl());
    cx_id
}

pub(crate) fn add_h(program: &mut Program) -> CallableId {
    let h_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(h_id, builder::h_decl());
    h_id
}

pub(crate) fn add_s(program: &mut Program) -> CallableId {
    let s_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(s_id, builder::s_decl());
    s_id
}

pub(crate) fn add_s_adj(program: &mut Program) -> CallableId {
    let s_adj_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(s_adj_id, builder::s_adj_decl());
    s_adj_id
}
