// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared contracts for contraction planning, preparation and repeated execution.

use std::fmt;

use tensornet::{ContractionPlan, ContractionQuery};

/// Requests used during path search, not execution-time allocation ceilings.
///
/// An optimizer echoes only honored requests in
/// [`PlanningReport::accepted_constraints`]. A field omitted from that echo
/// must not be interpreted as an honored constraint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlanningConstraints {
    /// Workspace budget in bytes for the optimizer to consider when choosing
    /// a plan. This is neither an allocation nor a measurement of memory
    /// consumed by search.
    pub workspace_bytes: Option<u64>,
}

/// One provider-labelled estimate, using that provider's counting convention.
///
/// Missing estimates are omitted from [`PlanningReport::estimates`], not
/// represented by a zero-valued entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostEstimate {
    pub provider: &'static str,
    pub quantity: EstimateKind,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EstimateKind {
    FlopCount,
    /// Elements, not bytes or a bound on device memory.
    LargestIntermediateElements,
}

/// Observations from planning, separate from a portable plan and from the
/// resources discovered during preparation.
///
/// A caller-supplied plan needs no optimizer and produces no planning report.
/// Optimizers with additional diagnostics can embed this report in their own
/// [`ContractionOptimizer::Report`] and expose it through [`AsRef`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanningReport {
    pub optimizer: &'static str,
    /// Elapsed search time in seconds, or `None` when not measured.
    pub search_seconds: Option<f64>,
    pub accepted_constraints: PlanningConstraints,
    pub estimates: Vec<CostEstimate>,
}

impl AsRef<PlanningReport> for PlanningReport {
    fn as_ref(&self) -> &PlanningReport {
        self
    }
}

/// Selects a portable plan without owning its later preparation or execution.
///
/// Settings and additional diagnostics remain optimizer-specific. A returned
/// plan must be valid for `query` and independent of the optimizer's lifetime.
pub trait ContractionOptimizer {
    type Settings;
    type Report: AsRef<PlanningReport>;
    type Error;

    fn optimize(
        &mut self,
        query: &ContractionQuery<'_>,
        constraints: PlanningConstraints,
        settings: Self::Settings,
    ) -> Result<(ContractionPlan, Self::Report), Self::Error>;
}

/// Ceilings on scratch allocations during preparation, not on coefficients,
/// output storage, total device memory, or sampled process memory.
///
/// `None` omits a caller-specified ceiling; it does not bypass allocation
/// failures. `Some(0)` is an actual zero-byte ceiling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutionLimits {
    pub device_scratch_bytes: Option<usize>,
    pub host_scratch_bytes: Option<usize>,
}

/// Resource facts discovered during preparation or execution.
///
/// `None` means unknown, not zero. `Some(0)` records a known zero. Required,
/// recommended and actually allocated scratch remain separate quantities;
/// optimizer estimates belong in [`PlanningReport`] instead.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceReport {
    /// Bytes in the most recent fully validated selection, counting shared
    /// inputs once. This does not imply that every native binding succeeded.
    pub selected_input_bytes: Option<usize>,
    pub selected_input_count: Option<usize>,
    /// All acquired resident input storage, including unselected candidates
    /// and allocations whose subsequent upload failed.
    pub resident_input_bytes: Option<usize>,
    pub resident_input_count: Option<usize>,
    /// Storage needed for the ordered output tensor.
    pub output_bytes: Option<usize>,
    pub device_scratch_minimum: Option<usize>,
    pub device_scratch_recommended: Option<usize>,
    pub device_scratch_allocated: Option<usize>,
    pub host_scratch_minimum: Option<usize>,
    pub host_scratch_recommended: Option<usize>,
    pub host_scratch_allocated: Option<usize>,
    /// Bytes actually allocated on the device by this owner, not total
    /// process/device usage or a prediction of future allocations.
    pub owned_device_bytes: Option<usize>,
}

impl AsRef<ResourceReport> for ResourceReport {
    fn as_ref(&self) -> &ResourceReport {
        self
    }
}

/// Preparation's primary error, any cleanup error, and the facts discovered
/// before failure. Cleanup must not erase that evidence or replace the cause.
///
/// In particular, allocation observations describe the failed attempt, not
/// the memory still live after cleanup.
///
/// The report and errors are stored by value so packaging this failure needs
/// no heap allocation. The backend-defined `E` may itself own allocations.
#[derive(Debug)]
pub struct PreparationFailure<E, R = ResourceReport> {
    pub partial: R,
    pub error: E,
    pub cleanup: Option<E>,
}

impl<E: fmt::Display, R> fmt::Display for PreparationFailure<E, R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "contraction preparation failed: {}", self.error)?;
        if let Some(cleanup) = &self.cleanup {
            write!(formatter, "; cleanup also failed: {cleanup}")?;
        }
        Ok(())
    }
}

impl<E: std::error::Error + 'static, R: fmt::Debug> std::error::Error for PreparationFailure<E, R> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Whether explicitly registered input storage can be replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMutability {
    Immutable,
    Mutable,
}

/// Prepares an already-selected plan independently of numerical inputs.
///
/// Preparation must revalidate the plan against `query`,
/// check backend capabilities and respect `limits`. It must not search for a
/// path, complete or binarize a plan, or silently change its semantics.
/// Input topology and ordered output axes come from `query`, not the plan.
///
/// Backends may choose private intermediate layouts without changing the
/// selected contractions or the logical tensor axes. This is layout lowering,
/// not path search; input-buffer interpretation and output ordering must remain
/// as specified. It does not authorize approximation.
///
/// Model validity and executor capabilities are distinct: the initial native
/// executable subset is unsliced, pairwise contraction, but a model-valid
/// arbitrary-arity or single-input plan may be rejected by a backend.
/// Plan validation errors, unsupported features and resource-limit rejections
/// must remain distinguishable in the backend-defined error type.
///
/// The prepared owner may exclusively borrow this Context, but must not borrow
/// the method-local query or plan. Closing it does not close the Context.
///
/// A Context borrow cannot overlap another preparation:
/// ```compile_fail
/// use qdk_simulators::execution::{ContractionContext, ExecutableContraction, ExecutionLimits};
/// use tensornet::{ContractionQuery, ContractionPlan};
/// fn overlapping<C: ContractionContext>(c: &mut C, q: &ContractionQuery<'_>, p: &ContractionPlan) {
///     let first = c.prepare(q, p, ExecutionLimits::default()).ok().unwrap();
///     let second = c.prepare(q, p, ExecutionLimits::default()).ok().unwrap();
///     first.close();
///     second.close();
/// }
/// ```
///
/// Nor can the Context be consumed while its executable is live:
/// ```compile_fail
/// use qdk_simulators::execution::{ContractionContext, ExecutableContraction, ExecutionLimits};
/// use tensornet::{ContractionQuery, ContractionPlan};
/// fn early_close<C: ContractionContext>(mut c: C, q: &ContractionQuery<'_>, p: &ContractionPlan) {
///     let executable = c.prepare(q, p, ExecutionLimits::default()).ok().unwrap();
///     drop(c);
///     executable.close();
/// }
/// ```
pub trait ContractionContext {
    type Executable<'context>: ExecutableContraction<Error = Self::Error, Report = Self::Report>
    where
        Self: 'context;
    type Report: AsRef<ResourceReport>;
    type Error;

    #[allow(
        clippy::result_large_err,
        reason = "packaging preparation failure evidence must not require a heap allocation"
    )]
    fn prepare(
        &mut self,
        query: &ContractionQuery<'_>,
        plan: &ContractionPlan,
        limits: ExecutionLimits,
    ) -> Result<Self::Executable<'_>, PreparationFailure<Self::Error, Self::Report>>;
}

/// Owns prepared resources and explicitly registered inputs for one plan.
///
/// Unlike a portable [`ContractionPlan`], this is a live executable owner.
/// Its implementation retains or borrows the backend context needed by its
/// resources; it is not a transferable description for another host/backend.
pub trait ExecutableContraction {
    /// Synchronous input view; implementations must not retain it after the call.
    type Input<'input>;
    /// Opaque identity local to this executable, not a slot or a wire label.
    type InputId: Clone;
    type Report: AsRef<ResourceReport>;
    /// Independently owned output that remains usable after subsequent
    /// executions and after closing the prepared owner.
    type Output;
    type Error;

    /// Validates shape, ordered layout and finite values before storing input.
    /// Storage is retained until close, even when no execution selects it.
    /// Any failure poisons the executable, including preflight validation.
    fn register_input(
        &mut self,
        input: Self::Input<'_>,
        mutability: InputMutability,
    ) -> Result<Self::InputId, Self::Error>;

    /// Replaces same-shape mutable storage without growing its allocation.
    /// Every slot selecting this identity sees the new values. Immutable or
    /// foreign identities fail explicitly. Any failure poisons the executable.
    fn replace_input(
        &mut self,
        id: Self::InputId,
        input: Self::Input<'_>,
    ) -> Result<(), Self::Error>;

    /// Executes and synchronizes required work before returning. Repeated
    /// success replaces internal output, never previously returned values.
    /// Implementations must establish their resources' required execution
    /// context and report backend failures rather than selecting a different
    /// backend or silently falling back.
    ///
    /// The complete selection contains one identity per input slot and is
    /// validated before binding or contracting. Selecting retained candidates
    /// does not allocate or upload input data. No noise sampling occurs here.
    ///
    /// Failure makes this owner unusable for further input operations: subsequent
    /// calls must return a distinct unusable-state error without retrying.
    /// Resource evidence remains inspectable and explicit close remains
    /// available.
    fn execute(&mut self, inputs: &[Self::InputId]) -> Result<Self::Output, Self::Error>;

    /// Discovered facts, available before the first execution and retained
    /// after an execution failure.
    fn resources(&self) -> &Self::Report;

    /// Consumes the owner and reports cleanup failure. This result does not
    /// replace or combine with any earlier execution error; callers retain
    /// both outcomes themselves.
    ///
    /// A second explicit close is impossible, even when this call fails.
    /// Backend cleanup and best-effort `Drop` behavior do not replace explicit
    /// close for observing errors.
    fn close(self) -> Result<(), Self::Error>;
}
