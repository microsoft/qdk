// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A closure-compiling evaluator for Q#.
//!
//! The classic evaluator in this crate walks an [`ExecGraph`](qsc_fir::fir::ExecGraph) and,
//! for every node, looks up expressions, variables, and callees by id at runtime. This module
//! follows the approach of "On Lisp" chapters 6 ("Functions as Representation") and 24
//! ("Prolog"): the FIR is analyzed *once* and translated into a network of Rust closures that
//! hold direct references to each other, so that only the work that truly depends on runtime
//! values remains to be done at runtime.
//!
//! - Local variables are resolved to frame slot indices at compile time.
//! - Static call sites link directly to the compiled specialization of their callee
//!   (compiled lazily on first call, then cached), and tuple arguments are pushed straight
//!   into the callee's parameter slots without building an intermediate tuple.
//! - Literals, array literals, intrinsic classification, functor application on static
//!   callees, and struct field layouts are all resolved at compile time.
//! - Error call stacks are collected while unwinding, so successful calls pay nothing for them.
//!
//! The compiled code does not borrow the FIR, so it can be cached across evaluations in a
//! [`CodeCache`] for as long as the FIR store is unchanged.

mod compiler;

use crate::{
    CallableCountKey, DelayedQubitReleaseStack, Env, Error, PackageSpan, QubitCounter,
    backend::{Backend, TracingBackend},
    callable::TypeArgs,
    debug::Frame,
    intrinsic, make_counting_key,
    output::Receiver,
    val::{Qubit, Value},
};
use compiler::{CompiledSpec, Unit};
use qsc_data_structures::{functors::FunctorApp, span::Span};
use qsc_fir::{
    fir::{ExprId, Global, PackageId, PackageStoreLookup, StmtId, StoreItemId},
    ty::Ty,
};
use qsc_lowerer::map_fir_package_span_to_hir;
use rand::{SeedableRng, rngs::StdRng};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{cell::RefCell, rc::Rc};

pub(crate) type R = Result<Value, Flow>;
pub(crate) type Code = Box<dyn Fn(&mut Rt<'_>) -> R>;

/// Non-local control flow: a `return` or a runtime error.
pub(crate) enum Flow {
    Return(Value),
    Error(Box<Failure>),
}

pub(crate) struct Failure {
    error: Error,
    /// Location inside the innermost frame not yet recorded in `frames`.
    span: Span,
    /// Frames recorded while unwinding, innermost first.
    frames: Vec<Frame>,
}

impl Flow {
    #[cold]
    pub(crate) fn error(error: Error, span: Span) -> Self {
        Flow::Error(Box::new(Failure {
            error,
            span,
            frames: Vec::new(),
        }))
    }

    /// Records the frame of a callable that the flow is leaving, as `capture_stack` would.
    #[cold]
    fn leave_frame(self, frame: &FrameInfo) -> Self {
        match self {
            Flow::Error(mut failure) => {
                failure.frames.push(Frame {
                    span: failure.span,
                    id: frame.id,
                    caller: frame.caller,
                    functor: frame.functor,
                    loop_iterations: Vec::new(),
                });
                failure.span = frame.site_span;
                Flow::Error(failure)
            }
            ret @ Flow::Return(_) => ret,
        }
    }
}

/// Everything the compiled code needs from the outside world. Making this a trait object keeps
/// compiled code independent of the backend and receiver types, so it can be cached.
pub(crate) trait Host {
    fn qubit_allocate(&mut self, stack: &[Frame]) -> Result<usize, String>;
    fn qubit_release(&mut self, q: usize, stack: &[Frame]) -> Result<bool, String>;
    #[allow(clippy::too_many_arguments)]
    fn intrinsic(
        &mut self,
        name: &str,
        name_span: PackageSpan,
        arg: Value,
        arg_span: PackageSpan,
        stack: &[Frame],
        rng: &mut StdRng,
    ) -> Result<Value, Error>;
}

struct HostImpl<'a, 'b, B: Backend, G: PackageStoreLookup> {
    sim: &'a mut TracingBackend<'b, B>,
    out: &'a mut dyn Receiver,
    globals: &'a G,
}

impl<B: Backend, G: PackageStoreLookup> Host for HostImpl<'_, '_, B, G> {
    fn qubit_allocate(&mut self, stack: &[Frame]) -> Result<usize, String> {
        self.sim.qubit_allocate(stack)
    }

    fn qubit_release(&mut self, q: usize, stack: &[Frame]) -> Result<bool, String> {
        self.sim.qubit_release(q, stack)
    }

    fn intrinsic(
        &mut self,
        name: &str,
        name_span: PackageSpan,
        arg: Value,
        arg_span: PackageSpan,
        stack: &[Frame],
        rng: &mut StdRng,
    ) -> Result<Value, Error> {
        intrinsic::call(
            name,
            name_span,
            arg,
            arg_span,
            stack,
            self.sim,
            rng,
            self.out,
            self.globals,
        )
    }
}

/// The runtime state threaded through compiled code.
pub(crate) struct Rt<'a> {
    host: &'a mut dyn Host,
    globals: &'a dyn PackageStoreLookup,
    cache: &'a CodeCache,
    env: &'a mut Env,
    /// The value stack holding the local slots of every active frame.
    slots: Vec<Value>,
    /// Index of slot 0 of the current frame.
    base: usize,
    type_args: TypeArgs,
    /// An explicit frame stack, maintained only when a tracer needs call stacks.
    track_frames: bool,
    frames: Vec<Frame>,
    call_counts: FxHashMap<CallableCountKey, i64>,
    qubit_counter: Option<QubitCounter>,
    dirty_qubits: FxHashSet<usize>,
    delayed: DelayedQubitReleaseStack,
    rng: StdRng,
    guard: StackGuard,
}

impl Rt<'_> {
    #[inline]
    pub(crate) fn slot(&self, i: u32) -> &Value {
        &self.slots[self.base + i as usize]
    }

    #[inline]
    pub(crate) fn slot_mut(&mut self, i: u32) -> &mut Value {
        &mut self.slots[self.base + i as usize]
    }

    /// Mirrors `State::capture_stack`: each frame reports the location executing within it.
    fn capture_stack(&self, current: Span) -> Vec<Frame> {
        let mut frames = self.frames.clone();
        let mut span = current;
        for frame in frames.iter_mut().rev() {
            std::mem::swap(&mut frame.span, &mut span);
        }
        frames
    }

    fn stack_if_tracing(&self, current: Span) -> Vec<Frame> {
        if self.track_frames {
            self.capture_stack(current)
        } else {
            Vec::new()
        }
    }

    fn count_call(&mut self, id: StoreItemId, functor: FunctorApp) {
        if !self.call_counts.is_empty()
            && let Some(count) = self.call_counts.get_mut(&make_counting_key(id, functor))
        {
            *count += 1;
        }
    }
}

/// Native recursion depth guard. Compiled code recurses on the Rust stack, so very deep Q#
/// recursion continues on a fresh thread with a large stack (the calling thread blocks, so
/// execution remains strictly sequential).
#[derive(Clone, Copy)]
struct StackGuard {
    limit: usize,
}

const ENTRY_STACK_BUDGET: usize = 256 * 1024;
const FRESH_STACK_SIZE: usize = 256 * 1024 * 1024;
const FRESH_STACK_BUDGET: usize = FRESH_STACK_SIZE - 4 * 1024 * 1024;

impl StackGuard {
    #[inline(never)]
    fn here(budget: usize) -> Self {
        let marker = 0u8;
        let addr = std::ptr::addr_of!(marker) as usize;
        Self {
            limit: addr.saturating_sub(budget),
        }
    }

    #[inline]
    fn exceeded(self) -> bool {
        if cfg!(target_family = "wasm") {
            return false;
        }
        let marker = 0u8;
        (std::ptr::addr_of!(marker) as usize) < self.limit
    }
}

struct AssertSend<T>(T);
// SAFETY: the value is only touched by one thread at a time; the sending thread blocks
// until the receiving thread has finished with it.
unsafe impl<T> Send for AssertSend<T> {}
impl<T> AssertSend<T> {
    fn into_inner(self) -> T {
        self.0
    }
}

#[cold]
#[inline(never)]
fn on_fresh_stack(rt: &mut Rt<'_>, body: &Code) -> R {
    let saved = rt.guard;
    let payload = AssertSend((
        std::ptr::from_mut(rt).cast::<Rt<'static>>(),
        std::ptr::from_ref(body),
    ));
    let result = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(FRESH_STACK_SIZE)
            .spawn_scoped(scope, move || {
                let (rt, body) = payload.into_inner();
                // SAFETY: the parent thread holds `rt` and `body` exclusively and is blocked on
                // this thread, so the pointers remain valid and unaliased for its duration.
                let (rt, body) = unsafe { (&mut *rt, &*body) };
                rt.guard = StackGuard::here(FRESH_STACK_BUDGET);
                AssertSend(body(rt))
            })
            .expect("should be able to spawn evaluation thread")
            .join()
    });
    rt.guard = saved;
    match result {
        Ok(r) => r.into_inner(),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// The specialization selected by a functor application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SpecKind {
    Body,
    Adj,
    Ctl,
    CtlAdj,
}

impl SpecKind {
    pub(crate) fn of(functor: FunctorApp) -> Self {
        match (functor.adjoint, functor.controlled) {
            (false, 0) => SpecKind::Body,
            (true, 0) => SpecKind::Adj,
            (false, _) => SpecKind::Ctl,
            (true, _) => SpecKind::CtlAdj,
        }
    }
}

/// A resolved callee.
#[derive(Clone)]
pub(crate) enum Callee {
    Udt(StoreItemId),
    Intrinsic(Rc<IntrinsicInfo>),
    Spec(Rc<SpecCell>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicKind {
    Allocate,
    Borrow,
    Release,
    Counting,
    Length,
    Other,
}

pub(crate) struct IntrinsicInfo {
    id: StoreItemId,
    name: Rc<str>,
    kind: IntrinsicKind,
    decl_span: PackageSpan,
    output_is_unit: bool,
}

/// A link to one specialization of a callable, filled with compiled code on first use.
pub(crate) struct SpecCell {
    pub(crate) id: StoreItemId,
    pub(crate) kind: SpecKind,
    /// Simulatable intrinsics always run their body and are not call-counted.
    pub(crate) simulatable: bool,
    code: RefCell<Option<Rc<CompiledSpec>>>,
}

impl SpecCell {
    #[inline]
    fn get(&self, rt: &Rt<'_>) -> Rc<CompiledSpec> {
        if let Some(code) = &*self.code.borrow() {
            return code.clone();
        }
        self.compile(rt)
    }

    #[cold]
    fn compile(&self, rt: &Rt<'_>) -> Rc<CompiledSpec> {
        let code = Rc::new(compiler::compile_spec(rt.globals, rt.cache, self));
        *self.code.borrow_mut() = Some(code.clone());
        code
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct UnitKey {
    package: PackageId,
    stmts: Vec<StmtId>,
    entry: Option<ExprId>,
    env_mode: bool,
}

/// Compiled code for a FIR store. Must be cleared whenever the FIR store changes.
#[derive(Default)]
pub struct CodeCache {
    callees: RefCell<FxHashMap<(StoreItemId, SpecKind), Callee>>,
    units: RefCell<FxHashMap<UnitKey, Rc<Unit>>>,
}

impl CodeCache {
    /// Drops all compiled code.
    pub fn clear(&self) {
        // Compiled specializations link to each other (including recursively), so break
        // those cycles explicitly before dropping the map.
        for callee in self.callees.borrow().values() {
            if let Callee::Spec(cell) = callee {
                cell.code.borrow_mut().take();
            }
        }
        self.callees.borrow_mut().clear();
        self.units.borrow_mut().clear();
    }

    pub(crate) fn callee(
        &self,
        globals: &dyn PackageStoreLookup,
        id: StoreItemId,
        kind: SpecKind,
    ) -> Option<Callee> {
        if let Some(callee) = self.callees.borrow().get(&(id, kind)) {
            return Some(callee.clone());
        }
        let callee = match globals.get_global(id)? {
            Global::Udt => Callee::Udt(id),
            Global::Callable(decl) => match &decl.implementation {
                qsc_fir::fir::CallableImpl::Intrinsic => {
                    let name = decl.name.name.clone();
                    let kind = match name.as_ref() {
                        "__quantum__rt__qubit_allocate" => IntrinsicKind::Allocate,
                        "__quantum__rt__qubit_borrow" => IntrinsicKind::Borrow,
                        "__quantum__rt__qubit_release" => IntrinsicKind::Release,
                        "Length" => IntrinsicKind::Length,
                        n if crate::is_counting_call(n) => IntrinsicKind::Counting,
                        _ => IntrinsicKind::Other,
                    };
                    Callee::Intrinsic(Rc::new(IntrinsicInfo {
                        id,
                        name,
                        kind,
                        decl_span: map_fir_package_span_to_hir(decl.span),
                        output_is_unit: decl.output == Ty::UNIT,
                    }))
                }
                qsc_fir::fir::CallableImpl::Spec(_) => Callee::Spec(Rc::new(SpecCell {
                    id,
                    kind,
                    simulatable: false,
                    code: RefCell::new(None),
                })),
                qsc_fir::fir::CallableImpl::SimulatableIntrinsic(_) => {
                    Callee::Spec(Rc::new(SpecCell {
                        id,
                        kind: SpecKind::Body,
                        simulatable: true,
                        code: RefCell::new(None),
                    }))
                }
            },
        };
        self.callees.borrow_mut().insert((id, kind), callee.clone());
        Some(callee)
    }
}

impl Drop for CodeCache {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Identifies the code to evaluate at the top level.
pub enum UnitSource<'s> {
    /// An entry expression evaluated in a fresh environment; locals live in frame slots.
    Entry { package: PackageId, expr: ExprId },
    /// Top-level statements (and an optional entry expression) whose locals live in the
    /// persistent environment, as notebooks and REPLs require.
    Fragments {
        package: PackageId,
        stmts: &'s [StmtId],
        entry: Option<ExprId>,
    },
}

/// Information about a call site used for error reporting and frames.
pub(crate) struct Site {
    /// The span of the whole call expression in the caller.
    pub(crate) span: Span,
    pub(crate) caller: PackageId,
    pub(crate) callee_span: PackageSpan,
    pub(crate) arg_span: PackageSpan,
}

struct FrameInfo {
    id: StoreItemId,
    functor: FunctorApp,
    caller: PackageId,
    site_span: Span,
}

#[allow(clippy::too_many_arguments)]
fn with_runtime<B: Backend, G: PackageStoreLookup, T>(
    cache: &CodeCache,
    seed: Option<u64>,
    globals: &G,
    env: &mut Env,
    sim: &mut TracingBackend<'_, B>,
    receiver: &mut impl Receiver,
    f: impl FnOnce(&mut Rt<'_>) -> T,
) -> T {
    let rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let track_frames = sim.is_stacks_enabled();
    let mut host = HostImpl {
        sim,
        out: receiver,
        globals,
    };
    let mut rt = Rt {
        host: &mut host,
        globals,
        cache,
        env,
        slots: Vec::with_capacity(256),
        base: 0,
        type_args: TypeArgs::Empty,
        track_frames,
        frames: Vec::new(),
        call_counts: FxHashMap::default(),
        qubit_counter: None,
        dirty_qubits: FxHashSet::default(),
        delayed: DelayedQubitReleaseStack::default(),
        rng,
        guard: StackGuard::here(ENTRY_STACK_BUDGET),
    };
    f(&mut rt)
}

fn finish(result: R) -> Result<Value, (Error, Vec<Frame>)> {
    match result {
        Ok(value) | Err(Flow::Return(value)) => Ok(value),
        Err(Flow::Error(failure)) => {
            let mut frames = failure.frames;
            frames.reverse();
            Err((failure.error, frames))
        }
    }
}

/// Evaluates top-level code with the closure-compiling evaluator.
/// # Errors
/// Returns the first error encountered during execution, with its call stack.
#[allow(clippy::too_many_arguments)]
pub fn eval<B: Backend>(
    cache: &CodeCache,
    source: &UnitSource<'_>,
    seed: Option<u64>,
    globals: &impl PackageStoreLookup,
    env: &mut Env,
    sim: &mut TracingBackend<'_, B>,
    receiver: &mut impl Receiver,
) -> Result<Value, (Error, Vec<Frame>)> {
    let key = match source {
        UnitSource::Entry { package, expr } => UnitKey {
            package: *package,
            stmts: Vec::new(),
            entry: Some(*expr),
            env_mode: false,
        },
        UnitSource::Fragments {
            package,
            stmts,
            entry,
        } => UnitKey {
            package: *package,
            stmts: stmts.to_vec(),
            entry: *entry,
            env_mode: true,
        },
    };
    let cached = cache.units.borrow().get(&key).cloned();
    let unit = if let Some(unit) = cached {
        unit
    } else {
        let unit = Rc::new(compiler::compile_unit(
            globals,
            cache,
            key.package,
            &key.stmts,
            key.entry,
            key.env_mode,
        ));
        cache.units.borrow_mut().insert(key, unit.clone());
        unit
    };
    with_runtime(cache, seed, globals, env, sim, receiver, |rt| {
        rt.slots.resize(unit.nslots, Value::Int(0));
        let result = (unit.body)(rt);
        rt.slots.clear();
        finish(result)
    })
}

/// Invokes a callable value with the closure-compiling evaluator.
/// # Errors
/// Returns the first error encountered during execution, with its call stack.
#[allow(clippy::too_many_arguments)]
pub fn invoke<B: Backend>(
    cache: &CodeCache,
    package: PackageId,
    seed: Option<u64>,
    globals: &impl PackageStoreLookup,
    env: &mut Env,
    sim: &mut TracingBackend<'_, B>,
    receiver: &mut impl Receiver,
    callable: Value,
    args: Value,
) -> Result<Value, (Error, Vec<Frame>)> {
    let site = Site {
        span: Span::default(),
        caller: package,
        callee_span: PackageSpan {
            package: qsc_lowerer::map_fir_package_to_hir(package),
            span: Span::default(),
        },
        arg_span: PackageSpan {
            package: qsc_lowerer::map_fir_package_to_hir(package),
            span: Span::default(),
        },
    };
    with_runtime(cache, seed, globals, env, sim, receiver, |rt| {
        finish(call_value(rt, callable, args, &site))
    })
}

/// Runs a compiled specialization whose input slots were pushed starting at `base`.
#[inline]
fn enter(
    rt: &mut Rt<'_>,
    spec: &CompiledSpec,
    base: usize,
    type_args: impl FnOnce(&Rt<'_>) -> TypeArgs,
    frame: &FrameInfo,
) -> R {
    rt.slots.resize(base + spec.nslots, Value::Int(0));
    let saved_base = std::mem::replace(&mut rt.base, base);
    let saved_type_args = if spec.uses_type_args {
        let type_args = type_args(rt);
        Some(std::mem::replace(&mut rt.type_args, type_args))
    } else {
        None
    };
    if rt.track_frames {
        rt.frames.push(Frame {
            span: frame.site_span,
            id: frame.id,
            caller: frame.caller,
            functor: frame.functor,
            loop_iterations: Vec::new(),
        });
    }
    let result = if rt.guard.exceeded() {
        on_fresh_stack(rt, &spec.body)
    } else {
        (spec.body)(rt)
    };
    if rt.track_frames {
        rt.frames.pop();
    }
    if let Some(type_args) = saved_type_args {
        rt.type_args = type_args;
    }
    rt.base = saved_base;
    rt.slots.truncate(base);
    match result {
        Ok(value) | Err(Flow::Return(value)) => Ok(value),
        Err(flow) => Err(flow.leave_frame(frame)),
    }
}

/// Calls a statically linked specialization whose decl input leaves were pushed at `base`.
#[inline]
pub(crate) fn call_static(
    rt: &mut Rt<'_>,
    cell: &SpecCell,
    base: usize,
    functor: FunctorApp,
    type_args: impl FnOnce(&Rt<'_>) -> TypeArgs,
    site: &Site,
) -> R {
    let spec = cell.get(rt);
    if !cell.simulatable {
        rt.count_call(cell.id, functor);
    }
    let frame = FrameInfo {
        id: cell.id,
        functor,
        caller: site.caller,
        site_span: site.span,
    };
    enter(rt, &spec, base, type_args, &frame)
}

fn tag_udt(id: StoreItemId, arg: Value) -> Value {
    match arg {
        Value::Tuple(items, _) => Value::Tuple(items, Some(id.into())),
        arg => arg,
    }
}

/// Calls a resolved callee with a fully evaluated argument value.
#[allow(clippy::too_many_arguments)]
pub(crate) fn call_callee(
    rt: &mut Rt<'_>,
    callee: &Callee,
    functor: FunctorApp,
    fixed_args: Option<Rc<[Value]>>,
    type_args: TypeArgs,
    arg: Value,
    site: &Site,
) -> R {
    match callee {
        Callee::Udt(id) => Ok(tag_udt(*id, arg)),
        Callee::Intrinsic(info) => call_intrinsic(rt, info, functor, arg, site),
        Callee::Spec(cell) => {
            let spec = cell.get(rt);
            let frame = FrameInfo {
                id: cell.id,
                functor,
                caller: site.caller,
                site_span: site.span,
            };
            let base = rt.slots.len();
            if let Some(ctl_pat) = &spec.ctl_input {
                assert!(
                    functor.controlled > 0,
                    "spec pattern tuple used without controlled functor"
                );
                let mut tup = arg;
                let mut ctls = vec![];
                for _ in 0..functor.controlled {
                    let [c, rest] = &*tup.unwrap_tuple() else {
                        panic!("tuple should be arity 2");
                    };
                    ctls.extend_from_slice(&c.clone().unwrap_array());
                    tup = rest.clone();
                }
                if !crate::are_ctls_unique(&ctls, &tup) {
                    return Err(
                        Flow::error(Error::QubitUniqueness(site.arg_span), site.span)
                            .leave_frame(&frame),
                    );
                }
                compiler::push_pat(
                    &mut rt.slots,
                    &spec.input,
                    crate::merge_fixed_args(fixed_args, tup),
                );
                compiler::push_pat(&mut rt.slots, ctl_pat, Value::Array(ctls.into()));
            } else {
                compiler::push_pat(
                    &mut rt.slots,
                    &spec.input,
                    crate::merge_fixed_args(fixed_args, arg),
                );
            }
            if !cell.simulatable {
                rt.count_call(cell.id, functor);
            }
            let type_args = if matches!(type_args, TypeArgs::Unbound) {
                match rt.globals.get_global(cell.id) {
                    Some(Global::Callable(decl)) => TypeArgs::from_decl(cell.id, &decl.generics),
                    _ => type_args,
                }
            } else {
                type_args
            };
            enter(rt, &spec, base, move |_| type_args, &frame)
        }
    }
}

/// Calls a callable value, as `State::eval_call` does.
pub(crate) fn call_value(rt: &mut Rt<'_>, callee: Value, arg: Value, site: &Site) -> R {
    let (id, functor, fixed_args, type_args) = match callee {
        Value::Closure(inner) => (
            inner.id,
            inner.functor,
            Some(inner.fixed_args),
            inner.type_args,
        ),
        Value::Global(id, functor) => (id, functor, None, TypeArgs::Empty),
        Value::GlobalWithArgs(global) => {
            (global.id, global.functor, None, global.type_args.clone())
        }
        _ => panic!("value is not callable"),
    };
    let Some(callee) = rt.cache.callee(rt.globals, id, SpecKind::of(functor)) else {
        return Err(Flow::error(Error::UnboundName(site.callee_span), site.span));
    };
    call_callee(rt, &callee, functor, fixed_args, type_args, arg, site)
}

/// Calls an intrinsic, as `State::eval_intrinsic` does.
pub(crate) fn call_intrinsic(
    rt: &mut Rt<'_>,
    info: &IntrinsicInfo,
    functor: FunctorApp,
    arg: Value,
    site: &Site,
) -> R {
    let frame = FrameInfo {
        id: info.id,
        functor,
        caller: site.caller,
        site_span: site.span,
    };
    if info.kind == IntrinsicKind::Counting {
        return counting_call(rt, &info.name, arg, site.arg_span)
            .map_err(|e| Flow::error(e, site.span).leave_frame(&frame));
    }
    rt.count_call(info.id, functor);
    intrinsic_body(rt, info, arg, site)
        .map_err(|e| Flow::error(e, info.decl_span.span).leave_frame(&frame))
}

fn intrinsic_body(
    rt: &mut Rt<'_>,
    info: &IntrinsicInfo,
    arg: Value,
    site: &Site,
) -> Result<Value, Error> {
    let callee_span = info.decl_span;
    let arg_span = site.arg_span;
    match info.kind {
        IntrinsicKind::Length => match arg.unwrap_array().len().try_into() {
            Ok(len) => Ok(Value::Int(len)),
            Err(_) => Err(Error::ArrayTooLarge(arg_span)),
        },
        IntrinsicKind::Allocate | IntrinsicKind::Borrow => {
            let borrow = info.kind == IntrinsicKind::Borrow;
            if let Some(q) = rt.delayed.allocate_delayed_qubit() {
                if borrow {
                    rt.dirty_qubits.insert(q.0);
                } else {
                    rt.dirty_qubits.remove(&q.0);
                }
                Ok(Value::Qubit(
                    rt.env
                        .qubits
                        .get(&q)
                        .expect("qubit should be tracked")
                        .clone()
                        .into(),
                ))
            } else {
                let stack = rt.stack_if_tracing(site.span);
                let q = rt
                    .host
                    .qubit_allocate(&stack)
                    .map_err(|e| Error::SimulationError(e, callee_span))?;
                let q = Rc::new(Qubit(q));
                rt.env.track_qubit(Rc::clone(&q));
                if let Some(counter) = &mut rt.qubit_counter {
                    counter.allocated(q.0);
                }
                if borrow {
                    rt.dirty_qubits.insert(q.0);
                }
                Ok(Value::Qubit(q.into()))
            }
        }
        IntrinsicKind::Release => {
            let qubit = arg
                .unwrap_qubit()
                .try_deref()
                .ok_or(Error::QubitDoubleRelease(arg_span))?;
            if !rt.delayed.delay_release_qubit(*qubit) {
                rt.env.qubits.remove(&qubit);
                let stack = rt.stack_if_tracing(site.span);
                let is_zero = rt
                    .host
                    .qubit_release(qubit.0, &stack)
                    .map_err(|e| Error::SimulationError(e, callee_span))?;
                let is_borrowed = rt.dirty_qubits.remove(&qubit.0);
                if !is_zero && !is_borrowed {
                    return Err(Error::ReleasedQubitNotZero(qubit.0, arg_span));
                }
            }
            Ok(Value::unit())
        }
        IntrinsicKind::Counting => unreachable!("counting calls are handled separately"),
        IntrinsicKind::Other => {
            let stack = rt.stack_if_tracing(site.span);
            let val =
                rt.host
                    .intrinsic(&info.name, callee_span, arg, arg_span, &stack, &mut rt.rng)?;
            if !info.output_is_unit && matches!(&val, Value::Tuple(t, None) if t.is_empty()) {
                return Err(Error::UnsupportedIntrinsicType(
                    info.name.to_string(),
                    callee_span,
                ));
            }
            Ok(val)
        }
    }
}

fn counting_call(
    rt: &mut Rt<'_>,
    name: &str,
    arg: Value,
    span: PackageSpan,
) -> Result<Value, Error> {
    let counting_key = |arg: Value| match arg {
        Value::Closure(closure) => make_counting_key(closure.id, closure.functor),
        Value::Global(id, functor) => make_counting_key(id, functor),
        Value::GlobalWithArgs(global) => make_counting_key(global.id, global.functor),
        _ => panic!("value should be callable"),
    };
    match name {
        "StartCountingOperation" | "StartCountingFunction" => {
            if rt.call_counts.insert(counting_key(arg), 0).is_some() {
                Err(Error::CallableAlreadyCounted(span))
            } else {
                Ok(Value::unit())
            }
        }
        "StopCountingOperation" | "StopCountingFunction" => {
            if let Some(count) = rt.call_counts.remove(&counting_key(arg)) {
                Ok(Value::Int(count))
            } else {
                Err(Error::CallableNotCounted(span))
            }
        }
        "StartCountingQubits" => {
            if rt.qubit_counter.replace(QubitCounter::default()).is_some() {
                Err(Error::QubitsAlreadyCounted(span))
            } else {
                Ok(Value::unit())
            }
        }
        "StopCountingQubits" => {
            if let Some(qubit_counter) = rt.qubit_counter.take() {
                Ok(Value::Int(qubit_counter.into_count()))
            } else {
                Err(Error::QubitsNotCounted(span))
            }
        }
        _ => panic!("unknown counting call"),
    }
}

/// Releases the qubits whose release was delayed by a `parallel` expression.
pub(crate) fn par_end(rt: &mut Rt<'_>, span: PackageSpan) -> Result<(), Flow> {
    let stack = rt.stack_if_tracing(span.span);
    for qubit in rt.delayed.remove_layer() {
        rt.env.qubits.remove(&qubit);
        let is_borrowed = rt.dirty_qubits.remove(&qubit.0);
        match rt.host.qubit_release(qubit.0, &stack) {
            Ok(true) => {}
            Ok(false) if is_borrowed => {}
            Ok(false) => {
                return Err(Flow::error(
                    Error::ReleasedQubitNotZero(qubit.0, span),
                    span.span,
                ));
            }
            Err(e) => return Err(Flow::error(Error::SimulationError(e, span), span.span)),
        }
    }
    Ok(())
}
