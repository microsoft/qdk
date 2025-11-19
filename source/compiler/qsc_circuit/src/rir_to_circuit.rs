// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::builder::{LexicalScope, ScopeId, SourceLocationMetadata, SourceLookup};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeStack {
    caller: Vec<SourceLocationMetadata>,
    scope: ScopeId,
}

impl ScopeStack {
    pub(crate) fn caller(&self) -> &[SourceLocationMetadata] {
        &self.caller
    }

    pub(crate) fn current_lexical_scope(&self) -> ScopeId {
        assert!(!self.is_top(), "top scope has no lexical scope");
        self.scope
    }

    pub(crate) fn is_top(&self) -> bool {
        self.caller.is_empty() && self.scope == ScopeId::default()
    }

    pub(crate) fn top() -> Self {
        ScopeStack {
            caller: Vec::new(),
            scope: ScopeId::default(),
        }
    }

    pub(crate) fn resolve_scope(&self, scope_resolver: &impl SourceLookup) -> LexicalScope {
        if self.is_top() {
            LexicalScope::top()
        } else {
            scope_resolver.resolve_scope(self.scope)
        }
    }

    #[allow(dead_code)]
    pub fn fmt(&self, scope_resolver: &impl SourceLookup) -> String {
        if self.is_top() {
            return "<top>".to_string();
        }

        let call_stack = self.caller();

        let mut names: Vec<String> = call_stack
            .iter()
            .map(|location| fmt_location(location, scope_resolver))
            .collect();
        names.push(
            scope_resolver
                .resolve_scope(self.current_lexical_scope())
                .name(),
        );
        names.join("->")
    }
}

fn fmt_location(location: &SourceLocationMetadata, scope_resolver: &impl SourceLookup) -> String {
    let scope_id = &location.lexical_scope();
    format!(
        "{}@{}",
        scope_resolver.resolve_scope(*scope_id).name(),
        location.source_location().offset
    )
}

/// full is a call stack
/// prefix is a scope stack
/// if prefix isn't a prefix of full, return None
/// if it is, return the rest of full after removing prefix,
/// starting from the first location in full that is in the scope of prefix.scope
pub(crate) fn strip_scope_stack_prefix(
    full_call_stack: &[SourceLocationMetadata],
    prefix_scope_stack: &ScopeStack,
) -> Option<Vec<SourceLocationMetadata>> {
    if prefix_scope_stack.is_top() {
        return Some(full_call_stack.to_vec());
    }

    if full_call_stack.len() > prefix_scope_stack.caller().len()
        && let Some(rest) = full_call_stack.strip_prefix(prefix_scope_stack.caller())
        && rest[0].lexical_scope() == prefix_scope_stack.current_lexical_scope()
    {
        assert!(!rest.is_empty());
        return Some(rest.to_vec());
    }
    None
}

pub(crate) fn scope_stack(instruction_stack: &[SourceLocationMetadata]) -> ScopeStack {
    instruction_stack
        .split_last()
        .map_or(ScopeStack::top(), |(youngest, prefix)| ScopeStack {
            caller: prefix.to_vec(),
            scope: youngest.lexical_scope(),
        })
}
