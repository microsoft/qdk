// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::str::FromStr;
use qsc_ast::{
    ast::{
        Attr, ExprKind, Idents, ItemKind, Namespace, Package, PathKind, Stmt, StmtKind,
        TopLevelNode, UnOp,
    },
    mut_visit::{MutVisitor, walk_stmt},
    visit::Visitor,
};
use qsc_data_structures::{span::Span, target::Profile};
use qsc_hir::hir;
use std::rc::Rc;

use super::{SourceMap, TargetCapabilityFlags};

#[cfg(test)]
mod tests;

/// Transformation to detect `@EntryPoint` attribute in the AST.
#[derive(Default)]
pub struct DetectEntryPointProfile {
    pub profile: Option<(Profile, Span)>,
}

impl DetectEntryPointProfile {
    #[must_use]
    pub fn new() -> Self {
        Self { profile: None }
    }
}

impl Visitor<'_> for DetectEntryPointProfile {
    fn visit_attr(&mut self, attr: &Attr) {
        if hir::Attr::from_str(attr.name.name.as_ref()) == Ok(hir::Attr::EntryPoint) {
            // Try to parse the argument as a profile name
            if let ExprKind::Paren(inner) = attr.arg.kind.as_ref()
                && let ExprKind::Path(PathKind::Ok(path)) = inner.kind.as_ref()
                && let Ok(profile) = Profile::from_str(path.name.name.as_ref())
            {
                self.profile = Some((profile, path.span));
            }
        }
    }
}

#[derive(PartialEq, Hash, Clone, Debug)]
pub struct TrackedName {
    pub name: Rc<str>,
    pub namespace: Rc<str>,
}

pub(crate) struct Conditional {
    capabilities: TargetCapabilityFlags,
    dropped_names: Vec<TrackedName>,
    included_names: Vec<TrackedName>,
}

impl Conditional {
    pub(crate) fn new(capabilities: TargetCapabilityFlags) -> Self {
        Self {
            capabilities,
            dropped_names: Vec::new(),
            included_names: Vec::new(),
        }
    }

    pub(crate) fn into_names(self) -> Vec<TrackedName> {
        self.dropped_names
            .into_iter()
            .filter(|n| !self.included_names.contains(n))
            .collect()
    }
}

impl MutVisitor for Conditional {
    fn visit_namespace(&mut self, namespace: &mut Namespace) {
        namespace.items = namespace
            .items
            .iter()
            .filter_map(|item| {
                if matches_config(&item.attrs, self.capabilities) {
                    match item.kind.as_ref() {
                        ItemKind::Callable(callable) => {
                            self.included_names.push(TrackedName {
                                name: callable.name.name.clone(),
                                namespace: namespace.name.full_name(),
                            });
                        }
                        ItemKind::Ty(ident, _) => self.included_names.push(TrackedName {
                            name: ident.name.clone(),
                            namespace: namespace.name.full_name(),
                        }),
                        _ => {}
                    }
                    Some(item.clone())
                } else {
                    match item.kind.as_ref() {
                        ItemKind::Callable(callable) => {
                            self.dropped_names.push(TrackedName {
                                name: callable.name.name.clone(),
                                namespace: namespace.name.full_name(),
                            });
                        }
                        ItemKind::Ty(ident, _) => self.dropped_names.push(TrackedName {
                            name: ident.name.clone(),
                            namespace: namespace.name.full_name(),
                        }),
                        _ => {}
                    }
                    None
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        if let StmtKind::Item(item) = stmt.kind.as_mut() {
            if matches_config(&item.attrs, self.capabilities) {
                match item.kind.as_ref() {
                    ItemKind::Callable(callable) => {
                        self.included_names.push(TrackedName {
                            name: callable.name.name.clone(),
                            namespace: Rc::from(""),
                        });
                    }
                    ItemKind::Ty(ident, _) => self.included_names.push(TrackedName {
                        name: ident.name.clone(),
                        namespace: Rc::from(""),
                    }),
                    _ => {}
                }
            } else {
                match item.kind.as_ref() {
                    ItemKind::Callable(callable) => {
                        self.dropped_names.push(TrackedName {
                            name: callable.name.name.clone(),
                            namespace: Rc::from(""),
                        });
                    }
                    ItemKind::Ty(ident, _) => self.dropped_names.push(TrackedName {
                        name: ident.name.clone(),
                        namespace: Rc::from(""),
                    }),
                    _ => {}
                }
                stmt.kind = Box::new(StmtKind::Empty);
            }
        }
    }
}

fn matches_config(attrs: &[Box<Attr>], capabilities: TargetCapabilityFlags) -> bool {
    let attrs: Vec<_> = attrs
        .iter()
        .filter(|attr| hir::Attr::from_str(attr.name.name.as_ref()) == Ok(hir::Attr::Config))
        .collect();

    if attrs.is_empty() {
        return true;
    }
    let mut found_capabilities = TargetCapabilityFlags::empty();
    let mut disallowed_capabilities = TargetCapabilityFlags::empty();
    let mut base = false;
    let mut not_base = false;

    // When checking attributes, anything we don't recognize (invalid form or invalid capability) gets
    // left in the compilation by returning true. This ensures that later compilation steps, specifically lowering
    // from AST to HIR, can check the attributes and return errors as appropriate.
    for attr in attrs {
        if let ExprKind::Paren(inner) = attr.arg.kind.as_ref() {
            match inner.kind.as_ref() {
                ExprKind::Path(PathKind::Ok(path)) => {
                    if let Ok(capability) = TargetCapabilityFlags::from_str(path.name.name.as_ref())
                    {
                        if capability.is_empty() {
                            base = true;
                        }
                        found_capabilities |= capability;
                    } else {
                        return true; // Unknown capability, so we assume it matches
                    }
                }
                ExprKind::UnOp(UnOp::NotL, inner) => {
                    if let ExprKind::Path(PathKind::Ok(path)) = inner.kind.as_ref() {
                        if let Ok(capability) =
                            TargetCapabilityFlags::from_str(path.name.name.as_ref())
                        {
                            if capability.is_empty() {
                                not_base = true;
                            }
                            disallowed_capabilities |= capability;
                        } else {
                            return true; // Unknown capability, so we assume it matches
                        }
                    } else {
                        return true; // Unknown config attribute, so we assume it matches
                    }
                }
                _ => return true, // Unknown config attribute, so we assume it matches
            }
        } else {
            // Something other than a parenthesized expression, so we assume it matches
            return true;
        }
    }
    if found_capabilities.is_empty() && disallowed_capabilities.is_empty() {
        if not_base && !base {
            // There was at least one config attribute, but it was "not Base" so
            // ensure that the capabilities are not empty.
            return capabilities != TargetCapabilityFlags::empty();
        } else if base && !not_base {
            // There was at least one config attribute, but it was Base
            // Therefore, we only match if there are no capabilities
            return capabilities == TargetCapabilityFlags::empty();
        }

        // The config specified both "Base" and "not Base" which is a contradiction, but we
        // drop the item in this case.
        return false;
    }
    capabilities.contains(found_capabilities)
        && (disallowed_capabilities.is_empty() || !capabilities.contains(disallowed_capabilities))
}

// Visitor to remove spans from circuit callables defined in QSC files.
// This will remove the spans for the contents of these circuit callables,
// but it is important for the language server that the span for the callable itself
// is preserved, so that the user can navigate to the definition of the callable.
pub(crate) struct RemoveCircuitSpans {
    qsc_spans: Vec<Span>,
}

impl RemoveCircuitSpans {
    pub(crate) fn new(sources: &SourceMap) -> Self {
        let qsc_spans = sources
            .iter()
            .filter(|source| {
                std::path::Path::new(source.name.as_ref())
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("qsc"))
            })
            .map(|source| {
                let start = source.offset;
                let end = start
                    + u32::try_from(source.contents.len()).expect("source length exceeds u32::MAX");
                Span { lo: start, hi: end }
            })
            .collect();

        Self { qsc_spans }
    }
}

impl MutVisitor for RemoveCircuitSpans {
    fn visit_package(&mut self, package: &mut Package) {
        // We only want to visit namespaces
        package.nodes.iter_mut().for_each(|n| match n {
            TopLevelNode::Namespace(ns) => self.visit_namespace(ns),
            TopLevelNode::Stmt(_) => {}
        });
    }

    fn visit_namespace(&mut self, namespace: &mut Namespace) {
        // We only want to visit circuit callables
        namespace.items.iter_mut().for_each(|item| {
            if let ItemKind::Callable(callable) = item.kind.as_mut() {
                // Check if the callable's span is inside a QSC file's span
                self.qsc_spans
                    .iter()
                    .any(|s| s.lo <= callable.span.lo && s.hi >= callable.span.hi)
                    .then(|| {
                        self.visit_callable_decl(callable);
                    });
            }
        });
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        stmt.span = Span::default(); // Clear the span for the statement
        walk_stmt(self, stmt);
    }
}
