// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::fmt::Display;

use crate::{builder::LexicalScope, circuit::PackageOffset};
use qsc_fir::fir;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeStack<SourceLocation, Scope> {
    caller: Vec<SourceLocation>,
    scope: Scope,
}

impl<SourceLocation, Scope> ScopeStack<SourceLocation, Scope>
where
    Scope: std::fmt::Debug + std::fmt::Display + Default + PartialEq,
    SourceLocation: PartialEq + Sized,
{
    pub(crate) fn caller(&self) -> &[SourceLocation] {
        &self.caller
    }

    pub(crate) fn current_lexical_scope(&self) -> &Scope {
        assert!(!self.is_top(), "top scope has no lexical scope");
        &self.scope
    }

    pub(crate) fn is_top(&self) -> bool {
        self.caller.is_empty() && self.scope == Scope::default()
    }

    pub(crate) fn top() -> Self {
        ScopeStack {
            caller: Vec::new(),
            scope: Scope::default(),
        }
    }

    pub(crate) fn resolve_scope(
        &self,
        scope_resolver: &impl ScopeResolver<ScopeId = Scope>,
    ) -> LexicalScope {
        if self.is_top() {
            LexicalScope::top()
        } else {
            scope_resolver.resolve_scope(&self.scope)
        }
    }

    #[allow(dead_code)]
    pub fn fmt(
        &self,
        dbg_stuff: &impl DbgStuffExt<Scope = Scope, SourceLocation = SourceLocation>,
    ) -> String {
        if self.is_top() {
            return "<top>".to_string();
        }

        let call_stack = self.caller();

        let mut names: Vec<String> = call_stack
            .iter()
            .map(|location| fmt_location(dbg_stuff, location))
            .collect();
        names.push(self.current_lexical_scope().to_string());
        names.join("->")
    }
}

fn fmt_location<Scope, SourceLocation>(
    dbg_stuff: &impl DbgStuffExt<Scope = Scope, SourceLocation = SourceLocation>,
    location: &SourceLocation,
) -> String
where
    Scope: Display,
{
    let scope_id = &dbg_stuff.lexical_scope(location);
    format!("{scope_id}@{}", dbg_stuff.source_location(location).offset)
}

pub(crate) trait ScopeResolver {
    type ScopeId;
    fn resolve_scope(&self, scope: &Self::ScopeId) -> LexicalScope;
}

pub(crate) trait DbgStuffExt {
    type SourceLocation: PartialEq + Sized + Clone + PartialEq;
    type Scope: std::fmt::Debug + std::fmt::Display + Default + PartialEq;

    fn package_id(&self, location: &Self::SourceLocation) -> fir::PackageId;
    fn lexical_scope(&self, location: &Self::SourceLocation) -> Self::Scope;
    fn source_location(&self, location: &Self::SourceLocation) -> PackageOffset;

    /// full is a call stack
    /// prefix is a scope stack
    /// if prefix isn't a prefix of full, return None
    /// if it is, return the rest of full after removing prefix,
    /// starting from the first location in full that is in the scope of prefix.scope
    fn strip_scope_stack_prefix(
        &self,
        full_call_stack: &[Self::SourceLocation],
        prefix_scope_stack: &ScopeStack<Self::SourceLocation, Self::Scope>,
    ) -> Option<Vec<Self::SourceLocation>> {
        if prefix_scope_stack.is_top() {
            return Some(full_call_stack.to_vec());
        }

        if full_call_stack.len() > prefix_scope_stack.caller().len()
            && let Some(rest) = full_call_stack.strip_prefix(prefix_scope_stack.caller())
            && self.lexical_scope(&rest[0]) == *prefix_scope_stack.current_lexical_scope()
        {
            assert!(!rest.is_empty());
            return Some(rest.to_vec());
        }
        None
    }

    fn scope_stack(
        &self,
        instruction_stack: &[Self::SourceLocation],
    ) -> ScopeStack<Self::SourceLocation, Self::Scope>
    where
        Self::SourceLocation: Clone,
    {
        instruction_stack
            .split_last()
            .map_or(ScopeStack::top(), |(youngest, prefix)| ScopeStack::<
                Self::SourceLocation,
                Self::Scope,
            > {
                caller: prefix.to_vec(),
                scope: self.lexical_scope(youngest),
            })
    }
}
