// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    collections::{BTreeMap, BTreeSet},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use qsc_codegen::graph_pass::{
    PassAction, PassOutcome, apply_fragments, apply_patches, find_matches,
};
use qsc_qhdg::Graph;

use crate::{
    QDK_CALLBACK_ERROR, QDK_OK, QdkBytes, QdkDiagnostic, QdkGraph, QdkStatus, QdkString,
    boundary::{Error, boundary, input_slice, output, release, required},
    handle::Handle,
};

pub const QDK_PASS_REWRITE: u32 = 1;
pub const QDK_PASS_EXPAND: u32 = 2;
pub const QDK_DECISION_KEEP: u32 = 0;
pub const QDK_DECISION_REPLACE: u32 = 1;
pub const QDK_DECISION_REMOVE: u32 = 2;
pub const QDK_DECISION_EXPAND: u32 = 3;
pub const QDK_PASS_UNCHANGED: u32 = 1;
pub const QDK_PASS_COMMITTED: u32 = 2;
pub const QDK_PASS_BUDGET_EXHAUSTED: u32 = 3;
pub const QDK_PASS_OSCILLATION: u32 = 4;

/// Pointer-sized integer context cookie; generated C spelling is `uintptr_t`.
pub type QdkContext = usize;

/// Read-only callback-duration data. Copy strings before returning.
#[repr(C)]
pub struct QdkPassMatch {
    pub node: u64,
    pub operation: QdkString,
    pub operands: usize,
}

/// Return an owned decision (NULL means keep) and optional owned diagnostic.
/// Both outputs transfer to the library on return, even on failure. Never
/// unwind a foreign exception/panic across this ABI.
pub type QdkGraphCallback = Option<
    unsafe extern "C" fn(
        context: QdkContext,
        matched: *const QdkPassMatch,
        decision: *mut *mut QdkPassDecision,
        error: *mut *mut QdkDiagnostic,
    ) -> QdkStatus,
>;

/// Runs exactly once after successful registration and the last use of context.
/// Must not unwind. The integer cookie is not a retained foreign-memory pointer.
pub type QdkContextDestructor = Option<unsafe extern "C" fn(context: QdkContext)>;

/// Copied registration input. A zero `use_budget` selects the native default.
#[repr(C)]
pub struct QdkGraphPassOptions {
    pub name: QdkString,
    pub kind: u32,
    pub operations: *const QdkString,
    pub operation_count: usize,
    pub use_budget: u32,
    pub budget: usize,
}

/// Owning callback registration; invoke synchronously with `qdk_graph_pass_run`.
pub struct QdkGraphPass {
    pub(crate) inner: Handle<ForeignGraphPass>,
}

/// Independent owned decision. Returning it from a callback transfers ownership.
pub struct QdkPassDecision {
    inner: Handle<Option<PassAction>>,
}

/// Independent copied native outcome and foreign callback count.
pub struct QdkPassResult {
    inner: Handle<(PassOutcome, usize)>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkPassResultInfo {
    pub outcome: u32,
    pub applications: usize,
    pub pending: usize,
    pub callback_count: usize,
    pub rule_count: usize,
}

pub(crate) struct ForeignContext {
    pub cookie: QdkContext,
    destructor: QdkContextDestructor,
    accepted: AtomicBool,
}

impl ForeignContext {
    pub fn new(cookie: QdkContext, destructor: QdkContextDestructor) -> Arc<Self> {
        Arc::new(Self {
            cookie,
            destructor,
            accepted: AtomicBool::new(false),
        })
    }

    pub fn accept(&self) {
        self.accepted.store(true, Ordering::Relaxed);
    }
}

impl Drop for ForeignContext {
    fn drop(&mut self) {
        if self.accepted.load(Ordering::Relaxed)
            && let Some(destroy) = self.destructor
        {
            // SAFETY: successful registration transfers cookie lifetime.
            // The caller guarantees this foreign destructor does not unwind.
            unsafe { destroy(self.cookie) };
        }
    }
}

#[derive(Clone)]
pub(crate) struct ForeignGraphPass {
    pub name: String,
    operations: BTreeSet<String>,
    expands: bool,
    budget: Option<usize>,
    callback: unsafe extern "C" fn(
        QdkContext,
        *const QdkPassMatch,
        *mut *mut QdkPassDecision,
        *mut *mut QdkDiagnostic,
    ) -> QdkStatus,
    pub context: Arc<ForeignContext>,
}

pub(crate) fn callback_status(
    status: QdkStatus,
    diagnostic: Option<Box<QdkDiagnostic>>,
) -> Result<(), Error> {
    if status == QDK_OK {
        if let Some(diagnostic) = diagnostic {
            return Err(Error::new(
                QDK_CALLBACK_ERROR,
                format!(
                    "callback returned success with a diagnostic: {}",
                    diagnostic.message
                ),
            ));
        }
        return Ok(());
    }
    Err(Error::new(
        QDK_CALLBACK_ERROR,
        diagnostic.map_or_else(
            || format!("foreign callback failed with status {status}"),
            |diagnostic| diagnostic.message,
        ),
    ))
}

impl ForeignGraphPass {
    pub(crate) unsafe fn read(
        options: *const QdkGraphPassOptions,
        callback: QdkGraphCallback,
        cookie: QdkContext,
        destructor: QdkContextDestructor,
    ) -> Result<Self, Error> {
        // SAFETY: caller provides the options and declared input extents.
        unsafe {
            let options = required(options, "graph pass options")?;
            let name = options.name.read()?.to_owned();
            let expands = match options.kind {
                QDK_PASS_REWRITE => false,
                QDK_PASS_EXPAND => true,
                _ => return Err(Error::invalid("unknown graph pass kind")),
            };
            let budget = match options.use_budget {
                0 => None,
                1 if !expands => Some(options.budget),
                1 => {
                    return Err(Error::invalid(
                        "fragment passes do not have a rewrite budget",
                    ));
                }
                _ => return Err(Error::invalid("use_budget must be zero or one")),
            };
            let callback =
                callback.ok_or_else(|| Error::invalid("graph callback must be non-NULL"))?;
            let operations = input_slice(options.operations, options.operation_count)?
                .iter()
                .map(|name| Ok(name.read()?.to_owned()))
                .collect::<Result<_, Error>>()?;
            Ok(Self {
                name,
                operations,
                expands,
                budget,
                callback,
                context: ForeignContext::new(cookie, destructor),
            })
        }
    }

    pub fn run(&self, graph: &Graph) -> Result<(Graph, PassOutcome, usize), Error> {
        let matches = find_matches(graph, &self.operations);
        let crossings = matches.len();
        let budget = self.budget.unwrap_or_else(|| crossings.max(1));
        let mut decisions = BTreeMap::new();
        for matched in matches {
            let view = QdkPassMatch {
                node: matched.node.0,
                operation: QdkString::borrowed(&matched.operation),
                operands: matched.operands,
            };
            let mut decision = ptr::null_mut();
            let mut diagnostic = ptr::null_mut();
            // SAFETY: all borrowed data and output slots live through this synchronous
            // call. Registration requires foreign code to obey the no-unwind contract.
            let status = unsafe {
                (self.callback)(
                    self.context.cookie,
                    &raw const view,
                    &raw mut decision,
                    &raw mut diagnostic,
                )
            };
            // SAFETY: callback outputs transfer fresh library-owned allocations,
            // or NULL. Adopting before inspecting status releases them on all paths.
            let (decision, diagnostic) = unsafe {
                (
                    (!decision.is_null()).then(|| Box::from_raw(decision)),
                    (!diagnostic.is_null()).then(|| Box::from_raw(diagnostic)),
                )
            };
            callback_status(status, diagnostic)?;
            if let Some(decision) = decision {
                let action = decision.inner.access()?.clone();
                if let Some(action) = action {
                    if self.expands != matches!(action, PassAction::Expand(_)) {
                        return Err(Error::new(
                            QDK_CALLBACK_ERROR,
                            "decision does not match the registered pass family",
                        ));
                    }
                    decisions.insert(matched.node, action);
                }
            }
        }
        let (graph, outcome) = if self.expands {
            apply_fragments(graph, &decisions)
        } else {
            apply_patches(graph, &decisions, budget)
        }
        .map_err(|error| Error::invalid(error.to_string()))?;
        Ok((graph, outcome, crossings))
    }
}

/// Registers an owning graph callback. Success transfers responsibility for
/// context destruction; failure does not. Callbacks run on the calling thread.
/// Contexts and callbacks must support sequential use on any calling thread;
/// no concurrent access to this handle is permitted.
/// # Safety
/// Input extents/function pointers are valid; outputs are disjoint. Retained
/// state uses an integer token (for example runtime/cgo.Handle), not a Go pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_pass_new(
    options: *const QdkGraphPassOptions,
    callback: QdkGraphCallback,
    context: QdkContext,
    destructor: QdkContextDestructor,
    out: *mut *mut QdkGraphPass,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies input extents, valid callbacks and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let pass = ForeignGraphPass::read(options, callback, context, destructor)?;
            let context = Arc::clone(&pass.context);
            let owned = Box::new(QdkGraphPass {
                inner: Handle::new(pass),
            });
            context.accept();
            *out = Box::into_raw(owned);
            Ok(())
        })
    }
}

/// Copies the registration name.
/// # Safety
/// Pass is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_pass_name(
    pass: *const QdkGraphPass,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live pass and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = required(pass, "graph pass")?
                .inner
                .access()?
                .name
                .as_bytes()
                .to_vec()
                .into();
            Ok(())
        })
    }
}

/// Creates an owned decision. KEEP/REMOVE take no operation names, REPLACE
/// takes one, and EXPAND takes a nonempty sequence. Inputs are copied.
/// # Safety
/// Input extents are valid; outputs are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pass_decision_new(
    kind: u32,
    operations: *const QdkString,
    count: usize,
    out: *mut *mut QdkPassDecision,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies input extents and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let operations = input_slice(operations, count)?
                .iter()
                .map(|name| Ok(name.read()?.to_owned()))
                .collect::<Result<Vec<_>, Error>>()?;
            let action = match (kind, operations.as_slice()) {
                (QDK_DECISION_KEEP, []) => None,
                (QDK_DECISION_REMOVE, []) => Some(PassAction::Remove),
                (QDK_DECISION_REPLACE, [operation]) => Some(PassAction::Replace(operation.clone())),
                (QDK_DECISION_EXPAND, [_, ..]) => Some(PassAction::Expand(operations)),
                _ => {
                    return Err(Error::invalid(
                        "decision kind and operation count do not match",
                    ));
                }
            };
            *out = Box::into_raw(Box::new(QdkPassDecision {
                inner: Handle::new(action),
            }));
            Ok(())
        })
    }
}

/// Runs native matching and patch/fragment application without changing input.
/// Failure leaves graph and result outputs NULL; callback side effects cannot
/// be rolled back. Active input graph/pass release and reentry are refused.
/// # Safety
/// Handles are live; writable outputs are disjoint. Callback code must not
/// retain borrowed match data or unwind across the ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_pass_run(
    pass: *const QdkGraphPass,
    graph: *const QdkGraph,
    out: *mut *mut QdkGraph,
    result: *mut *mut QdkPassResult,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live handles and disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = output(result)?;
            let pass = required(pass, "graph pass")?.inner.access()?;
            let graph = required(graph, "graph")?.inner.access()?;
            let (graph, outcome, crossings) = pass.run(&graph)?;
            let owned_graph = Box::new(QdkGraph {
                inner: Handle::new(graph),
            });
            let report = Box::new(QdkPassResult {
                inner: Handle::new((outcome, crossings)),
            });
            *out = Box::into_raw(owned_graph);
            *result = Box::into_raw(report);
            Ok(())
        })
    }
}

/// Copies the native outcome, applied/pending counts and actual callback count.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pass_result_info(
    result: *const QdkPassResult,
    out: *mut QdkPassResultInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live input and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(result, "pass result")?.inner.access()?;
            let (outcome, pending, rule_count) = match &value.0 {
                PassOutcome::Unchanged => (QDK_PASS_UNCHANGED, 0, 0),
                PassOutcome::Committed { .. } => (QDK_PASS_COMMITTED, 0, 0),
                PassOutcome::BudgetExhausted { pending, .. } => {
                    (QDK_PASS_BUDGET_EXHAUSTED, *pending, 0)
                }
                PassOutcome::Oscillation { rules, .. } => (QDK_PASS_OSCILLATION, 0, rules.len()),
            };
            *out = QdkPassResultInfo {
                outcome,
                applications: value.0.applications(),
                pending,
                callback_count: value.1,
                rule_count,
            };
            Ok(())
        })
    }
}

/// Copies one native oscillation rule name.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pass_result_rule(
    result: *const QdkPassResult,
    index: usize,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live input and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(result, "pass result")?.inner.access()?;
            let PassOutcome::Oscillation { rules, .. } = &value.0 else {
                return Err(Error::new(
                    crate::QDK_WRONG_VARIANT,
                    "pass did not report oscillation",
                ));
            };
            let rule = rules
                .get(index)
                .ok_or_else(|| Error::invalid("rule index is out of range"))?;
            *out = rule.as_bytes().to_vec().into();
            Ok(())
        })
    }
}

/// Releases a callback registration and its context after its last use.
/// # Safety
/// Slot contains NULL or a live exclusively owned registration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_pass_free(
    value: *mut *mut QdkGraphPass,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Releases a decision not transferred to a callback result.
/// # Safety
/// Slot contains NULL or a live exclusively owned decision.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pass_decision_free(
    value: *mut *mut QdkPassDecision,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Releases an independent pass report.
/// # Safety
/// Slot contains NULL or a live exclusively owned report.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pass_result_free(
    value: *mut *mut QdkPassResult,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
