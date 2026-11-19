// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Translation of FIR into closures. Everything that can be decided from the program text
//! alone is decided here, once, so that the closures only do work that depends on values.

use super::{
    Callee, Code, CodeCache, Flow, Rt, Site, SpecCell, SpecKind, call_callee, call_intrinsic,
    call_static, call_value, par_end, tag_udt,
};
use crate::{
    Error, PackageSpan, Variable,
    callable::TypeArgs,
    val::{
        self, Value, index_array, make_range, slice_array, update_functor_app, update_index_range,
        update_index_single,
    },
};
use qsc_data_structures::{functors::FunctorApp, span::Span};
use qsc_fir::{
    fir::{
        BinOp, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, Global, Lit,
        LocalVarId, PackageId, PackageStoreLookup, PatId, PatKind, PrimField, Res, SpecDecl,
        StmtId, StmtKind, StoreItemId, StringComponent, UnOp,
    },
    ty::{GenericArg, Prim, Ty},
};
use qsc_lowerer::map_fir_package_span_to_hir;
use rustc_hash::FxHashMap;
use std::{fmt::Write, rc::Rc};

/// Compiled code yielding an `O`.
type Thunk<O> = Box<dyn Fn(&mut Rt<'_>) -> Result<O, Flow>>;
type Cond = Thunk<bool>;
type StmtCode = Thunk<()>;

/// A compiled specialization of a callable.
pub(crate) struct CompiledSpec {
    pub(crate) nslots: usize,
    /// The shape of the declaration input; its leaves occupy the first slots, in order.
    pub(crate) input: ArgPat,
    /// The shape of the controlled specialization's control-qubit pattern, if any; its
    /// leaves follow the declaration input leaves.
    pub(crate) ctl_input: Option<ArgPat>,
    pub(crate) body: Code,
    /// Whether the body reads the lexical type context (to create closures or generic
    /// references). If not, callers do not need to materialize it.
    pub(crate) uses_type_args: bool,
}

/// Compiled top-level code.
pub(crate) struct Unit {
    pub(crate) nslots: usize,
    pub(crate) body: Code,
}

/// The shape of a parameter pattern: bound leaves are pushed to consecutive slots.
#[derive(Clone)]
pub(crate) enum ArgPat {
    Leaf,
    Skip,
    Tuple(Box<[ArgPat]>),
}

/// Pushes the leaves of `value`, destructured by `pat`, onto the slot stack.
pub(crate) fn push_pat(slots: &mut Vec<Value>, pat: &ArgPat, value: Value) {
    match pat {
        ArgPat::Leaf => slots.push(value),
        ArgPat::Skip => {}
        ArgPat::Tuple(pats) => {
            if pats.is_empty() {
                return;
            }
            let Value::Tuple(mut items, _) = value else {
                panic!("value should be a tuple");
            };
            if let Some(items) = Rc::get_mut(&mut items) {
                for (pat, item) in pats.iter().zip(items.iter_mut()) {
                    push_pat(slots, pat, std::mem::replace(item, Value::Int(0)));
                }
            } else {
                for (pat, item) in pats.iter().zip(items.iter()) {
                    push_pat(slots, pat, item.clone());
                }
            }
        }
    }
}

/// A binding pattern with resolved storage.
enum Bind {
    Slot(u32),
    Env(LocalVarId, Rc<str>, Span),
    Discard,
    Tuple(Box<[Bind]>),
}

fn bind(rt: &mut Rt<'_>, pat: &Bind, value: Value) {
    match pat {
        Bind::Slot(i) => *rt.slot_mut(*i) = value,
        Bind::Env(id, name, span) => rt.env.bind_variable_in_top_frame(
            *id,
            Variable {
                name: name.clone(),
                value,
                span: *span,
            },
        ),
        Bind::Discard => {}
        Bind::Tuple(pats) => {
            let items = value.unwrap_tuple();
            for (pat, item) in pats.iter().zip(items.iter()) {
                bind(rt, pat, item.clone());
            }
        }
    }
}

/// A variable that can be updated in place.
#[derive(Clone, Copy)]
enum Place {
    Slot(u32),
    Env(LocalVarId, PackageSpan),
}

fn place_mut<'r>(rt: &'r mut Rt<'_>, place: Place, span: Span) -> Result<&'r mut Value, Flow> {
    match place {
        Place::Slot(i) => Ok(rt.slot_mut(i)),
        Place::Env(id, var_span) => match rt.env.get_mut(id) {
            Some(var) => Ok(&mut var.value),
            None => Err(Flow::error(Error::UnboundName(var_span), span)),
        },
    }
}

/// The target of an assignment.
enum Target {
    Place(Place),
    Hole,
    Tuple(Box<[Target]>),
}

fn assign(rt: &mut Rt<'_>, target: &Target, value: Value, span: Span) -> Result<(), Flow> {
    match target {
        Target::Place(place) => *place_mut(rt, *place, span)? = value,
        Target::Hole => {}
        Target::Tuple(targets) => {
            let Value::Tuple(items, _) = value else {
                unreachable!("unassignable pattern should be disallowed by compiler");
            };
            for (target, item) in targets.iter().zip(items.iter()) {
                assign(rt, target, item.clone(), span)?;
            }
        }
    }
    Ok(())
}

fn is_unique_array(value: &Value) -> bool {
    matches!(value, Value::Array(arr) if Rc::strong_count(arr) + Rc::weak_count(arr) == 1)
}

/// A primitive operand that can be read without materializing a `Value`.
trait PrimVal: Copy + 'static {
    fn of(value: &Value) -> Self;
    fn lit(lit: &Lit) -> Option<Self>;
}

impl PrimVal for i64 {
    #[inline]
    fn of(value: &Value) -> Self {
        match value {
            Value::Int(v) => *v,
            _ => panic!("value should be Int, got {}", value.type_name()),
        }
    }
    fn lit(lit: &Lit) -> Option<Self> {
        match lit {
            Lit::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl PrimVal for f64 {
    #[inline]
    fn of(value: &Value) -> Self {
        match value {
            Value::Double(v) => *v,
            _ => panic!("value should be Double, got {}", value.type_name()),
        }
    }
    fn lit(lit: &Lit) -> Option<Self> {
        match lit {
            Lit::Double(v) => Some(*v),
            _ => None,
        }
    }
}

enum Src<T> {
    Slot(u32),
    Const(T),
    Code(Code),
}

/// Builds a "superinstruction" for a binary operation on primitive operands, specialized
/// on where each operand comes from. Operands are read left to right. Callers never pass
/// two constants.
fn prim2<T: PrimVal, O: 'static>(
    lhs: Src<T>,
    rhs: Src<T>,
    f: impl Fn(T, T) -> Result<O, Flow> + 'static,
) -> Thunk<O> {
    match (lhs, rhs) {
        (Src::Slot(a), Src::Slot(b)) => Box::new(move |rt| f(T::of(rt.slot(a)), T::of(rt.slot(b)))),
        (Src::Slot(a), Src::Const(b)) => Box::new(move |rt| f(T::of(rt.slot(a)), b)),
        (Src::Const(a), Src::Slot(b)) => Box::new(move |rt| f(a, T::of(rt.slot(b)))),
        (Src::Slot(a), Src::Code(b)) => Box::new(move |rt| {
            let x = T::of(rt.slot(a));
            f(x, T::of(&b(rt)?))
        }),
        (Src::Code(a), Src::Slot(b)) => Box::new(move |rt| {
            let x = T::of(&a(rt)?);
            f(x, T::of(rt.slot(b)))
        }),
        (Src::Code(a), Src::Const(b)) => Box::new(move |rt| f(T::of(&a(rt)?), b)),
        (Src::Const(a), Src::Code(b)) => Box::new(move |rt| f(a, T::of(&b(rt)?))),
        (Src::Code(a), Src::Code(b)) => Box::new(move |rt| {
            let x = T::of(&a(rt)?);
            f(x, T::of(&b(rt)?))
        }),
        (Src::Const(_), Src::Const(_)) => unreachable!("constant operands are not specialized"),
    }
}

type BinFn = fn(Value, Value, PackageSpan, PackageSpan) -> Result<Value, Error>;

fn binop_fn(op: BinOp) -> BinFn {
    match op {
        BinOp::Add => |l, r, _, _| Ok(crate::eval_binop_add(l, r)),
        BinOp::AndB => |l, r, _, _| Ok(crate::eval_binop_andb(l, r)),
        BinOp::Div => crate::eval_binop_div,
        BinOp::Eq => crate::eval_binop_eq,
        BinOp::Exp => crate::eval_binop_exp,
        BinOp::Gt => |l, r, _, _| Ok(crate::eval_binop_gt(l, r)),
        BinOp::Gte => |l, r, _, _| Ok(crate::eval_binop_gte(l, r)),
        BinOp::Lt => |l, r, _, _| Ok(crate::eval_binop_lt(l, r)),
        BinOp::Lte => |l, r, _, _| Ok(crate::eval_binop_lte(l, r)),
        BinOp::Mod => crate::eval_binop_mod,
        BinOp::Mul => |l, r, _, _| Ok(crate::eval_binop_mul(l, r)),
        BinOp::Neq => crate::eval_binop_neq,
        BinOp::OrB => |l, r, _, _| Ok(crate::eval_binop_orb(l, r)),
        BinOp::Shl => crate::eval_binop_shl,
        BinOp::Shr => crate::eval_binop_shr,
        BinOp::Sub => |l, r, _, _| Ok(crate::eval_binop_sub(l, r)),
        BinOp::XorB => |l, r, _, _| Ok(crate::eval_binop_xorb(l, r)),
        BinOp::AndL | BinOp::OrL => unreachable!("logical operators are compiled as conditions"),
    }
}

fn spec_decl(decl: &CallableDecl, kind: SpecKind) -> &SpecDecl {
    match &decl.implementation {
        CallableImpl::Spec(spec) => match kind {
            SpecKind::Body => Some(&spec.body),
            SpecKind::Adj => spec.adj.as_ref(),
            SpecKind::Ctl => spec.ctl.as_ref(),
            SpecKind::CtlAdj => spec.ctl_adj.as_ref(),
        }
        .expect("missing specialization should be a compilation error"),
        CallableImpl::SimulatableIntrinsic(spec) => spec,
        CallableImpl::Intrinsic => panic!("intrinsics have no specializations"),
    }
}

fn callable_decl(globals: &dyn PackageStoreLookup, id: StoreItemId) -> &CallableDecl {
    match globals.get_global(id) {
        Some(Global::Callable(decl)) => decl,
        _ => panic!("callable should exist"),
    }
}

/// The shape of a pattern, without assigning any storage.
fn pat_shape(globals: &dyn PackageStoreLookup, package: PackageId, pat: PatId) -> ArgPat {
    match &globals.get_pat((package, pat).into()).kind {
        PatKind::Bind(_) => ArgPat::Leaf,
        PatKind::Discard => ArgPat::Skip,
        PatKind::Tuple(pats) => ArgPat::Tuple(
            pats.iter()
                .map(|p| pat_shape(globals, package, *p))
                .collect(),
        ),
    }
}

pub(crate) fn compile_spec(
    globals: &dyn PackageStoreLookup,
    cache: &CodeCache,
    cell: &SpecCell,
) -> CompiledSpec {
    let decl = callable_decl(globals, cell.id);
    let spec = spec_decl(decl, cell.kind);
    let mut c = Compiler::new(globals, cache, cell.id.package, false);
    let input = c.param_pat(decl.input);
    let ctl_input = spec.input.map(|pat| c.param_pat(pat));
    let body = c.block(spec.block);
    CompiledSpec {
        nslots: c.nslots as usize,
        input,
        ctl_input,
        body,
        uses_type_args: c.uses_type_args,
    }
}

pub(crate) fn compile_unit(
    globals: &dyn PackageStoreLookup,
    cache: &CodeCache,
    package: PackageId,
    stmts: &[StmtId],
    entry: Option<ExprId>,
    env_mode: bool,
) -> Unit {
    enum Step {
        Value(Code),
        Effect(StmtCode),
    }

    let mut c = Compiler::new(globals, cache, package, env_mode);
    let body: Code = if stmts.is_empty()
        && let Some(entry) = entry
    {
        c.expr(entry)
    } else {
        // Top-level statements leave the value of the last expression statement as the
        // result; a binding clears it and an item leaves it unchanged.
        let mut steps = Vec::new();
        for &stmt in stmts {
            match &globals.get_stmt((package, stmt).into()).kind {
                StmtKind::Expr(e) | StmtKind::Semi(e) => steps.push(Step::Value(c.expr(*e))),
                StmtKind::Local(..) => {
                    let code = c.stmt(stmt).expect("binding should produce code");
                    steps.push(Step::Effect(code));
                }
                StmtKind::Item(_) => {}
            }
        }
        if let Some(entry) = entry {
            steps.push(Step::Value(c.expr(entry)));
        }
        Box::new(move |rt: &mut Rt<'_>| {
            let mut result = None;
            for step in &steps {
                match step {
                    Step::Value(code) => result = Some(code(rt)?),
                    Step::Effect(code) => {
                        code(rt)?;
                        result = None;
                    }
                }
            }
            Ok(result.unwrap_or_else(Value::unit))
        })
    };
    Unit {
        nslots: c.nslots as usize,
        body,
    }
}

struct Compiler<'c> {
    globals: &'c dyn PackageStoreLookup,
    cache: &'c CodeCache,
    package: PackageId,
    /// Whether locals live in the persistent environment rather than in slots.
    env_mode: bool,
    slots: FxHashMap<LocalVarId, u32>,
    nslots: u32,
    uses_type_args: bool,
}

impl<'c> Compiler<'c> {
    fn new(
        globals: &'c dyn PackageStoreLookup,
        cache: &'c CodeCache,
        package: PackageId,
        env_mode: bool,
    ) -> Self {
        Self {
            globals,
            cache,
            package,
            env_mode,
            slots: FxHashMap::default(),
            nslots: 0,
            uses_type_args: false,
        }
    }

    fn get_expr(&self, id: ExprId) -> &'c Expr {
        self.globals.get_expr((self.package, id).into())
    }

    fn slot(&mut self, id: LocalVarId) -> u32 {
        *self.slots.entry(id).or_insert_with(|| {
            let slot = self.nslots;
            self.nslots += 1;
            slot
        })
    }

    fn place(&mut self, id: LocalVarId, span: PackageSpan) -> Place {
        if self.env_mode {
            Place::Env(id, span)
        } else {
            Place::Slot(self.slot(id))
        }
    }

    /// Assigns consecutive slots to the leaves of a parameter pattern.
    fn param_pat(&mut self, pat: PatId) -> ArgPat {
        let pat = self.globals.get_pat((self.package, pat).into());
        match &pat.kind {
            PatKind::Bind(ident) => {
                let slot = self.nslots;
                self.nslots += 1;
                self.slots.insert(ident.id, slot);
                ArgPat::Leaf
            }
            PatKind::Discard => ArgPat::Skip,
            PatKind::Tuple(pats) => {
                ArgPat::Tuple(pats.iter().map(|p| self.param_pat(*p)).collect())
            }
        }
    }

    fn bind_pat(&mut self, pat: PatId) -> Bind {
        let pat = self.globals.get_pat((self.package, pat).into());
        match &pat.kind {
            PatKind::Bind(ident) => {
                if self.env_mode {
                    Bind::Env(ident.id, ident.name.clone(), ident.span.span)
                } else {
                    Bind::Slot(self.slot(ident.id))
                }
            }
            PatKind::Discard => Bind::Discard,
            PatKind::Tuple(pats) => Bind::Tuple(pats.iter().map(|p| self.bind_pat(*p)).collect()),
        }
    }

    fn block(&mut self, id: BlockId) -> Code {
        let block = self.globals.get_block((self.package, id).into());
        let (init, last) = match block.stmts.split_last() {
            Some((last, init)) => match &self.globals.get_stmt((self.package, *last).into()).kind {
                StmtKind::Expr(e) => (init, Some(*e)),
                _ => (block.stmts.as_slice(), None),
            },
            None => (block.stmts.as_slice(), None),
        };
        let mut init: Vec<StmtCode> = init.iter().filter_map(|s| self.stmt(*s)).collect();
        let last = last.map(|e| self.expr(e));
        match (init.len(), last) {
            (0, Some(last)) => last,
            (0, None) => Box::new(|_| Ok(Value::unit())),
            (1, Some(last)) => {
                let first = init.pop().expect("one statement");
                Box::new(move |rt| {
                    first(rt)?;
                    last(rt)
                })
            }
            (_, Some(last)) => {
                let init = init.into_boxed_slice();
                Box::new(move |rt| {
                    for stmt in &init {
                        stmt(rt)?;
                    }
                    last(rt)
                })
            }
            (_, None) => {
                let init = init.into_boxed_slice();
                Box::new(move |rt| {
                    for stmt in &init {
                        stmt(rt)?;
                    }
                    Ok(Value::unit())
                })
            }
        }
    }

    fn stmt(&mut self, id: StmtId) -> Option<StmtCode> {
        let stmt = self.globals.get_stmt((self.package, id).into());
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                let code = self.expr(*e);
                Some(Box::new(move |rt| code(rt).map(drop)))
            }
            StmtKind::Item(_) => None,
            StmtKind::Local(_, pat, e) => {
                if !self.env_mode
                    && let ExprKind::Var(Res::Local(src), _) = &self.get_expr(*e).kind
                    && let Bind::Slot(i) = self.bind_pat(*pat)
                {
                    let j = self.slot(*src);
                    return Some(Box::new(move |rt| {
                        let value = rt.slot(j).clone();
                        *rt.slot_mut(i) = value;
                        Ok(())
                    }));
                }
                let code = self.expr(*e);
                match self.bind_pat(*pat) {
                    Bind::Slot(i) => Some(Box::new(move |rt| {
                        let value = code(rt)?;
                        *rt.slot_mut(i) = value;
                        Ok(())
                    })),
                    pat => Some(Box::new(move |rt| {
                        let value = code(rt)?;
                        bind(rt, &pat, value);
                        Ok(())
                    })),
                }
            }
        }
    }

    fn src<T: PrimVal>(&mut self, id: ExprId) -> Src<T> {
        let e = self.get_expr(id);
        match &e.kind {
            ExprKind::Var(Res::Local(var), _) if !self.env_mode => Src::Slot(self.slot(*var)),
            ExprKind::Lit(lit) => match T::lit(lit) {
                Some(v) => Src::Const(v),
                None => Src::Code(self.expr(id)),
            },
            _ => Src::Code(self.expr(id)),
        }
    }

    /// Picks a specialized primitive implementation for `lhs op rhs`, if one applies.
    fn prim_binop(&mut self, span: Span, op: BinOp, lhs: ExprId, rhs: ExprId) -> Option<Code> {
        let lhs_ty = &self.get_expr(lhs).ty;
        let rhs_ty = &self.get_expr(rhs).ty;
        let both_lits = matches!(self.get_expr(lhs).kind, ExprKind::Lit(_))
            && matches!(self.get_expr(rhs).kind, ExprKind::Lit(_));
        if both_lits {
            return None;
        }
        let rhs_span = map_fir_package_span_to_hir(self.get_expr(rhs).span);
        let div_zero = move || Flow::error(Error::DivZero(rhs_span), span);
        if *lhs_ty == Ty::Prim(Prim::Int) && *rhs_ty == Ty::Prim(Prim::Int) {
            if !matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::AndB
                    | BinOp::OrB
                    | BinOp::XorB
            ) {
                return None;
            }
            let (l, r) = (self.src::<i64>(lhs), self.src::<i64>(rhs));
            Some(match op {
                BinOp::Add => prim2(l, r, |a: i64, b| Ok(Value::Int(a.wrapping_add(b)))),
                BinOp::Sub => prim2(l, r, |a: i64, b| Ok(Value::Int(a.wrapping_sub(b)))),
                BinOp::Mul => prim2(l, r, |a: i64, b| Ok(Value::Int(a.wrapping_mul(b)))),
                BinOp::Div => prim2(l, r, move |a: i64, b| {
                    if b == 0 {
                        Err(div_zero())
                    } else {
                        Ok(Value::Int(a.wrapping_div(b)))
                    }
                }),
                BinOp::Mod => prim2(l, r, move |a: i64, b| {
                    if b == 0 {
                        Err(div_zero())
                    } else {
                        Ok(Value::Int(a.wrapping_rem(b)))
                    }
                }),
                BinOp::AndB => prim2(l, r, |a: i64, b| Ok(Value::Int(a & b))),
                BinOp::OrB => prim2(l, r, |a: i64, b| Ok(Value::Int(a | b))),
                BinOp::XorB => prim2(l, r, |a: i64, b| Ok(Value::Int(a ^ b))),
                _ => unreachable!(),
            })
        } else if *lhs_ty == Ty::Prim(Prim::Double) && *rhs_ty == Ty::Prim(Prim::Double) {
            if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div) {
                return None;
            }
            let (l, r) = (self.src::<f64>(lhs), self.src::<f64>(rhs));
            Some(match op {
                BinOp::Add => prim2(l, r, |a: f64, b| Ok(Value::Double(a + b))),
                BinOp::Sub => prim2(l, r, |a: f64, b| Ok(Value::Double(a - b))),
                BinOp::Mul => prim2(l, r, |a: f64, b| Ok(Value::Double(a * b))),
                BinOp::Div => prim2(l, r, |a: f64, b| Ok(Value::Double(a / b))),
                _ => unreachable!(),
            })
        } else {
            None
        }
    }

    /// Picks a specialized primitive comparison for `lhs op rhs`, if one applies.
    fn prim_cmp(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId) -> Option<Cond> {
        macro_rules! cmp {
            ($t:ty) => {{
                let (l, r) = (self.src::<$t>(lhs), self.src::<$t>(rhs));
                Some(match op {
                    BinOp::Lt => prim2(l, r, |a: $t, b| Ok(a < b)),
                    BinOp::Lte => prim2(l, r, |a: $t, b| Ok(a <= b)),
                    BinOp::Gt => prim2(l, r, |a: $t, b| Ok(a > b)),
                    BinOp::Gte => prim2(l, r, |a: $t, b| Ok(a >= b)),
                    #[allow(clippy::float_cmp)]
                    BinOp::Eq => prim2(l, r, |a: $t, b| Ok(a == b)),
                    #[allow(clippy::float_cmp)]
                    BinOp::Neq => prim2(l, r, |a: $t, b| Ok(a != b)),
                    _ => unreachable!(),
                })
            }};
        }

        if !matches!(
            op,
            BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte | BinOp::Eq | BinOp::Neq
        ) {
            return None;
        }
        let both_lits = matches!(self.get_expr(lhs).kind, ExprKind::Lit(_))
            && matches!(self.get_expr(rhs).kind, ExprKind::Lit(_));
        let ty = &self.get_expr(lhs).ty;
        if both_lits || *ty != self.get_expr(rhs).ty {
            return None;
        }
        match ty {
            Ty::Prim(Prim::Int) => cmp!(i64),
            Ty::Prim(Prim::Double) => cmp!(f64),
            _ => None,
        }
    }

    /// Compiles a Boolean expression into a closure that yields a `bool` directly.
    fn cond(&mut self, id: ExprId) -> Cond {
        let e = self.get_expr(id);
        match &e.kind {
            ExprKind::BinOp(BinOp::AndL, lhs, rhs) => {
                let (l, r) = (self.cond(*lhs), self.cond(*rhs));
                Box::new(move |rt| Ok(l(rt)? && r(rt)?))
            }
            ExprKind::BinOp(BinOp::OrL, lhs, rhs) => {
                let (l, r) = (self.cond(*lhs), self.cond(*rhs));
                Box::new(move |rt| Ok(l(rt)? || r(rt)?))
            }
            ExprKind::UnOp(UnOp::NotL, operand) => {
                let c = self.cond(*operand);
                Box::new(move |rt| Ok(!c(rt)?))
            }
            ExprKind::Lit(Lit::Bool(b)) => {
                let b = *b;
                Box::new(move |_| Ok(b))
            }
            ExprKind::Var(Res::Local(var), _) if !self.env_mode => {
                let i = self.slot(*var);
                Box::new(move |rt| Ok(matches!(rt.slot(i), Value::Bool(true))))
            }
            ExprKind::BinOp(op, lhs, rhs) => match self.prim_cmp(*op, *lhs, *rhs) {
                Some(cond) => cond,
                None => self.value_cond(id),
            },
            _ => self.value_cond(id),
        }
    }

    fn value_cond(&mut self, id: ExprId) -> Cond {
        let code = self.expr_general(id);
        Box::new(move |rt| Ok(matches!(code(rt)?, Value::Bool(true))))
    }

    fn expr(&mut self, id: ExprId) -> Code {
        let e = self.get_expr(id);
        match &e.kind {
            ExprKind::BinOp(BinOp::AndL | BinOp::OrL, ..) | ExprKind::UnOp(UnOp::NotL, _) => {
                let cond = self.cond(id);
                Box::new(move |rt| Ok(Value::Bool(cond(rt)?)))
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                if let Some(cond) = self.prim_cmp(*op, *lhs, *rhs) {
                    return Box::new(move |rt| Ok(Value::Bool(cond(rt)?)));
                }
                if let Some(code) = self.prim_binop(e.span.span, *op, *lhs, *rhs) {
                    return code;
                }
                self.expr_general(id)
            }
            _ => self.expr_general(id),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn expr_general(&mut self, id: ExprId) -> Code {
        let e = self.get_expr(id);
        let span = e.span.span;
        let hir_span = map_fir_package_span_to_hir(e.span);
        match &e.kind {
            ExprKind::Lit(lit) => {
                let value = crate::lit_to_val(lit);
                Box::new(move |_| Ok(value.clone()))
            }
            ExprKind::ArrayLit(items) => {
                let value = Value::Array(Rc::new(
                    items
                        .iter()
                        .map(|i| match &self.get_expr(*i).kind {
                            ExprKind::Lit(lit) => crate::lit_to_val(lit),
                            _ => panic!("expr kind should be lit"),
                        })
                        .collect(),
                ));
                Box::new(move |_| Ok(value.clone()))
            }
            ExprKind::Array(items) => {
                let items = self.exprs(items);
                Box::new(move |rt| {
                    let mut values = Vec::with_capacity(items.len());
                    for item in &items {
                        values.push(item(rt)?);
                    }
                    Ok(Value::Array(Rc::new(values)))
                })
            }
            ExprKind::ArrayRepeat(item, size) => {
                let (item, size) = (self.expr(*item), self.expr(*size));
                Box::new(move |rt| {
                    let item = item(rt)?;
                    let size = size(rt)?.unwrap_int();
                    match usize::try_from(size) {
                        Ok(size) => Ok(Value::Array(Rc::new(vec![item; size]))),
                        Err(_) => Err(Flow::error(Error::InvalidArrayLength(size, hir_span), span)),
                    }
                })
            }
            ExprKind::Assign(lhs, rhs) => {
                let rhs = self.expr(*rhs);
                match self.target(*lhs) {
                    Target::Place(Place::Slot(i)) => Box::new(move |rt| {
                        let value = rhs(rt)?;
                        *rt.slot_mut(i) = value;
                        Ok(Value::unit())
                    }),
                    target => Box::new(move |rt| {
                        let value = rhs(rt)?;
                        assign(rt, &target, value, span)?;
                        Ok(Value::unit())
                    }),
                }
            }
            ExprKind::AssignOp(op, lhs, rhs) => self.assign_op(e, *op, *lhs, *rhs),
            ExprKind::AssignField(record, field, replace) => {
                let replace = self.expr(*replace);
                let read = self.expr(*record);
                let target = self.target(*record);
                let field = field.clone();
                Box::new(move |rt| {
                    let value = replace(rt)?;
                    let record = read(rt)?;
                    assign(rt, &target, update_field(record, &field, &value), span)?;
                    Ok(Value::unit())
                })
            }
            ExprKind::AssignIndex(container, index, replace) => {
                self.assign_index(e, *container, *index, *replace)
            }
            ExprKind::BinOp(BinOp::AndL | BinOp::OrL, ..) | ExprKind::UnOp(UnOp::NotL, _) => {
                let cond = self.cond(id);
                Box::new(move |rt| Ok(Value::Bool(cond(rt)?)))
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                let f = binop_fn(*op);
                let ls = map_fir_package_span_to_hir(self.get_expr(*lhs).span);
                let rs = map_fir_package_span_to_hir(self.get_expr(*rhs).span);
                let (l, r) = (self.expr(*lhs), self.expr(*rhs));
                Box::new(move |rt| {
                    let lv = l(rt)?;
                    let rv = r(rt)?;
                    f(lv, rv, ls, rs).map_err(|e| Flow::error(e, span))
                })
            }
            ExprKind::Block(block) => self.block(*block),
            ExprKind::Call(callee, arg) => self.call(e, *callee, *arg),
            ExprKind::Closure(captures, item) => {
                self.uses_type_args = true;
                let reads: Box<[Code]> = captures
                    .iter()
                    .map(|var| self.local(*var, hir_span))
                    .collect();
                let id = StoreItemId {
                    package: self.package,
                    item: *item,
                };
                Box::new(move |rt| {
                    let mut fixed_args = Vec::with_capacity(reads.len());
                    for read in &reads {
                        fixed_args.push(read(rt)?);
                    }
                    Ok(Value::Closure(Box::new(val::Closure {
                        fixed_args: fixed_args.into(),
                        id,
                        functor: FunctorApp::default(),
                        type_args: rt.type_args.clone(),
                    })))
                })
            }
            ExprKind::Fail(msg) => {
                let msg = self.expr(*msg);
                Box::new(move |rt| {
                    let msg = msg(rt)?.unwrap_string().to_string();
                    Err(Flow::error(Error::UserFail(msg, hir_span), span))
                })
            }
            ExprKind::Field(record, field) => {
                let record = self.expr(*record);
                let field = field.clone();
                Box::new(move |rt| Ok(field_access(record(rt)?, &field)))
            }
            ExprKind::Hole => Box::new(|_| Ok(Value::unit())),
            ExprKind::If(cond, if_true, if_false) => {
                let cond = self.cond(*cond);
                let if_true = self.expr(*if_true);
                match if_false {
                    Some(if_false) => {
                        let if_false = self.expr(*if_false);
                        Box::new(move |rt| if cond(rt)? { if_true(rt) } else { if_false(rt) })
                    }
                    None => Box::new(move |rt| {
                        if cond(rt)? {
                            if_true(rt)?;
                        }
                        Ok(Value::unit())
                    }),
                }
            }
            ExprKind::Index(container, index) => self.index(span, *container, *index),
            ExprKind::Parallel(limit, body) => {
                let limit = limit.map(|l| {
                    let l_span = map_fir_package_span_to_hir(self.get_expr(l).span);
                    (self.expr(l), l_span)
                });
                let body = self.expr(*body);
                Box::new(move |rt| {
                    let limit = match &limit {
                        Some((code, l_span)) => {
                            let v = code(rt)?.unwrap_int();
                            if v < 0 {
                                return Err(Flow::error(
                                    Error::InvalidNegativeInt(v, *l_span),
                                    l_span.span,
                                ));
                            }
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            Some(v as usize)
                        }
                        None => None,
                    };
                    rt.delayed.add_layer(limit);
                    let result = body(rt);
                    if matches!(result, Err(Flow::Error(_))) {
                        return result;
                    }
                    par_end(rt, hir_span)?;
                    result
                })
            }
            ExprKind::Range(start, step, end) => {
                let start = start.map(|s| self.expr(s));
                let step = step.map(|s| self.expr(s));
                let end = end.map(|s| self.expr(s));
                Box::new(move |rt| {
                    let start = match &start {
                        Some(c) => Some(c(rt)?.unwrap_int()),
                        None => None,
                    };
                    let step = match &step {
                        Some(c) => c(rt)?.unwrap_int(),
                        None => val::DEFAULT_RANGE_STEP,
                    };
                    let end = match &end {
                        Some(c) => Some(c(rt)?.unwrap_int()),
                        None => None,
                    };
                    Ok(Value::Range(Box::new(val::Range { start, step, end })))
                })
            }
            ExprKind::Return(value) => {
                let value = self.expr(*value);
                Box::new(move |rt| Err(Flow::Return(value(rt)?)))
            }
            ExprKind::Struct(res, copy, fields) => {
                let Res::Item(item) = res else {
                    panic!("UDT should be an item");
                };
                let udt = Rc::new(StoreItemId {
                    package: item.package,
                    item: item.item,
                });
                let copy = copy.map(|c| self.expr(c));
                let len = fields.len();
                let fields: Box<[(usize, Code)]> = fields
                    .iter()
                    .map(|f| {
                        let index = match &f.field {
                            Field::Path(path) => match path.indices.as_slice() {
                                &[i] => i,
                                _ => panic!("field path for struct should have a single index"),
                            },
                            _ => panic!("invalid field for struct"),
                        };
                        (index, self.expr(f.value))
                    })
                    .collect();
                Box::new(move |rt| {
                    let mut items = match &copy {
                        Some(copy) => copy(rt)?.unwrap_tuple().to_vec(),
                        None => vec![Value::Int(0); len],
                    };
                    for (index, value) in &fields {
                        items[*index] = value(rt)?;
                    }
                    Ok(Value::Tuple(items.into(), Some(udt.clone())))
                })
            }
            ExprKind::String(components) => {
                enum Part {
                    Lit(Rc<str>),
                    Expr(Code),
                }

                if let [StringComponent::Lit(s)] = components.as_slice() {
                    let value = Value::String(s.clone());
                    return Box::new(move |_| Ok(value.clone()));
                }
                let parts: Box<[Part]> = components
                    .iter()
                    .map(|c| match c {
                        StringComponent::Lit(s) => Part::Lit(s.clone()),
                        StringComponent::Expr(e) => Part::Expr(self.expr(*e)),
                    })
                    .collect();
                Box::new(move |rt| {
                    let mut s = String::new();
                    for part in &parts {
                        match part {
                            Part::Lit(lit) => s.push_str(lit),
                            Part::Expr(code) => {
                                write!(s, "{}", code(rt)?)
                                    .expect("writing to string should succeed");
                            }
                        }
                    }
                    Ok(Value::String(s.into()))
                })
            }
            ExprKind::UpdateIndex(container, index, replace) => {
                let index_span = map_fir_package_span_to_hir(self.get_expr(*index).span);
                let index = self.expr(*index);
                let replace = self.expr(*replace);
                let container = self.expr(*container);
                Box::new(move |rt| {
                    let index = index(rt)?;
                    let update = replace(rt)?;
                    let values = container(rt)?.unwrap_array();
                    match index {
                        Value::Int(index) => {
                            update_index_single(&values, index, update, index_span)
                        }
                        Value::Range(r) => {
                            update_index_range(&values, r.start, r.step, r.end, update, index_span)
                        }
                        _ => unreachable!("array should only be indexed by Int or Range"),
                    }
                    .map_err(|e| Flow::error(e, span))
                })
            }
            ExprKind::Tuple(items) => {
                if items.is_empty() {
                    return Box::new(|_| Ok(Value::unit()));
                }
                let items = self.exprs(items);
                Box::new(move |rt| {
                    let mut values = Vec::with_capacity(items.len());
                    for item in &items {
                        values.push(item(rt)?);
                    }
                    Ok(Value::Tuple(values.into(), None))
                })
            }
            ExprKind::UnOp(op, operand) => {
                let operand = self.expr(*operand);
                let op = *op;
                Box::new(move |rt| Ok(unop(op, operand(rt)?)))
            }
            ExprKind::UpdateField(record, field, replace) => {
                let replace = self.expr(*replace);
                let record = self.expr(*record);
                let field = field.clone();
                Box::new(move |rt| {
                    let value = replace(rt)?;
                    Ok(update_field(record(rt)?, &field, &value))
                })
            }
            ExprKind::Var(Res::Local(var), _) => self.local(*var, hir_span),
            ExprKind::Var(Res::Item(item), args) => {
                let id = StoreItemId {
                    package: item.package,
                    item: item.item,
                };
                if args.is_empty() {
                    let value = Value::Global(id, FunctorApp::default());
                    Box::new(move |_| Ok(value.clone()))
                } else {
                    self.uses_type_args = true;
                    let args: Rc<[GenericArg]> = args.clone().into();
                    Box::new(move |rt| {
                        Ok(Value::global_with_args(
                            id,
                            FunctorApp::default(),
                            TypeArgs::bind(id, args.to_vec(), rt.type_args.clone()),
                        ))
                    })
                }
            }
            ExprKind::Var(Res::Err, _) => panic!("resolution error"),
            ExprKind::While(cond, block) => {
                let cond = self.cond(*cond);
                let body = self.block(*block);
                Box::new(move |rt| {
                    while cond(rt)? {
                        body(rt)?;
                    }
                    Ok(Value::unit())
                })
            }
        }
    }

    fn exprs(&mut self, ids: &[ExprId]) -> Box<[Code]> {
        ids.iter().map(|i| self.expr(*i)).collect()
    }

    fn local(&mut self, var: LocalVarId, span: PackageSpan) -> Code {
        if self.env_mode {
            Box::new(move |rt| match rt.env.get(var) {
                Some(v) => Ok(v.value.clone()),
                None => Err(Flow::error(Error::UnboundName(span), span.span)),
            })
        } else {
            let i = self.slot(var);
            Box::new(move |rt| Ok(rt.slot(i).clone()))
        }
    }

    fn target(&mut self, id: ExprId) -> Target {
        let e = self.get_expr(id);
        match &e.kind {
            ExprKind::Hole => Target::Hole,
            ExprKind::Var(Res::Local(var), _) => {
                Target::Place(self.place(*var, map_fir_package_span_to_hir(e.span)))
            }
            ExprKind::Tuple(items) => {
                Target::Tuple(items.iter().map(|i| self.target(*i)).collect())
            }
            _ => unreachable!("unassignable pattern should be disallowed by compiler"),
        }
    }

    fn var_place(&mut self, id: ExprId) -> Place {
        match self.target(id) {
            Target::Place(place) => place,
            _ => unreachable!("in-place update target should be a variable"),
        }
    }

    #[allow(clippy::many_single_char_names)]
    fn assign_op(&mut self, e: &Expr, op: BinOp, lhs: ExprId, rhs: ExprId) -> Code {
        let span = e.span.span;
        let lhs_e = self.get_expr(lhs);
        let place = self.var_place(lhs);
        if matches!(lhs_e.ty, Ty::Array(_)) {
            // `set arr += [...]` appends in place when the array is uniquely referenced.
            let f = binop_fn(op);
            let ls = map_fir_package_span_to_hir(lhs_e.span);
            let rs = map_fir_package_span_to_hir(self.get_expr(rhs).span);
            let rhs = self.expr(rhs);
            return Box::new(move |rt| {
                let r = rhs(rt)?;
                let v = place_mut(rt, place, span)?;
                if is_unique_array(v) {
                    v.append_array(r);
                } else {
                    let l = v.clone();
                    *v = f(l, r, ls, rs).map_err(|e| Flow::error(e, span))?;
                }
                Ok(Value::unit())
            });
        }
        match op {
            BinOp::AndL | BinOp::OrL => {
                let read = self.expr(lhs);
                let rhs = self.expr(rhs);
                let short_on = op == BinOp::OrL;
                Box::new(move |rt| {
                    let l = read(rt)?;
                    let v = if matches!(l, Value::Bool(true)) == short_on {
                        l
                    } else {
                        rhs(rt)?
                    };
                    *place_mut(rt, place, span)? = v;
                    Ok(Value::unit())
                })
            }
            _ => {
                if let Place::Slot(i) = place
                    && lhs_e.ty == Ty::Prim(Prim::Int)
                    && self.get_expr(rhs).ty == Ty::Prim(Prim::Int)
                    && matches!(
                        op,
                        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
                    )
                {
                    let rhs_span = map_fir_package_span_to_hir(self.get_expr(rhs).span);
                    let f = move |l: i64, r: i64| -> Result<i64, Flow> {
                        match op {
                            BinOp::Add => Ok(l.wrapping_add(r)),
                            BinOp::Sub => Ok(l.wrapping_sub(r)),
                            BinOp::Mul => Ok(l.wrapping_mul(r)),
                            _ if r == 0 => Err(Flow::error(Error::DivZero(rhs_span), span)),
                            BinOp::Div => Ok(l.wrapping_div(r)),
                            _ => Ok(l.wrapping_rem(r)),
                        }
                    };
                    return match self.src::<i64>(rhs) {
                        Src::Slot(j) => Box::new(move |rt| {
                            let r = i64::of(rt.slot(j));
                            let l = i64::of(rt.slot(i));
                            *rt.slot_mut(i) = Value::Int(f(l, r)?);
                            Ok(Value::unit())
                        }),
                        Src::Const(r) => Box::new(move |rt| {
                            let l = i64::of(rt.slot(i));
                            *rt.slot_mut(i) = Value::Int(f(l, r)?);
                            Ok(Value::unit())
                        }),
                        Src::Code(rhs) => Box::new(move |rt| {
                            // The variable is read before the right-hand side is evaluated.
                            let l = i64::of(rt.slot(i));
                            let r = i64::of(&rhs(rt)?);
                            *rt.slot_mut(i) = Value::Int(f(l, r)?);
                            Ok(Value::unit())
                        }),
                    };
                }
                let f = binop_fn(op);
                let ls = map_fir_package_span_to_hir(lhs_e.span);
                let rs = map_fir_package_span_to_hir(self.get_expr(rhs).span);
                let read = self.expr(lhs);
                let rhs = self.expr(rhs);
                Box::new(move |rt| {
                    let l = read(rt)?;
                    let r = rhs(rt)?;
                    let v = f(l, r, ls, rs).map_err(|e| Flow::error(e, span))?;
                    *place_mut(rt, place, span)? = v;
                    Ok(Value::unit())
                })
            }
        }
    }

    fn assign_index(
        &mut self,
        e: &Expr,
        container: ExprId,
        index: ExprId,
        replace: ExprId,
    ) -> Code {
        let span = e.span.span;
        let index_span = map_fir_package_span_to_hir(self.get_expr(index).span);
        let index = self.expr(index);
        let replace = self.expr(replace);
        let place = self.var_place(container);
        Box::new(move |rt| {
            let index = index(rt)?;
            let update = replace(rt)?;
            let v = place_mut(rt, place, span)?;
            let result = if is_unique_array(v) {
                update_in_place(v, index, update, index_span)
            } else {
                let values = v.clone().unwrap_array();
                match index {
                    Value::Int(index) => update_index_single(&values, index, update, index_span),
                    Value::Range(r) => {
                        update_index_range(&values, r.start, r.step, r.end, update, index_span)
                    }
                    _ => unreachable!("array should only be indexed by Int or Range"),
                }
                .map(|new| *v = new)
            };
            result.map_err(|e| Flow::error(e, span))?;
            Ok(Value::unit())
        })
    }

    fn index(&mut self, span: Span, container: ExprId, index: ExprId) -> Code {
        let index_e = self.get_expr(index);
        let index_span = map_fir_package_span_to_hir(index_e.span);
        // Indexing a local array by a local or literal Int reads the element in place,
        // without first cloning the array reference.
        if let ExprKind::Var(Res::Local(var), _) = &self.get_expr(container).kind
            && !self.env_mode
            && index_e.ty == Ty::Prim(Prim::Int)
            && matches!(
                index_e.kind,
                ExprKind::Var(Res::Local(_), _) | ExprKind::Lit(Lit::Int(_))
            )
        {
            let c = self.slot(*var);
            return match self.src::<i64>(index) {
                Src::Slot(j) => Box::new(move |rt| {
                    let i = i64::of(rt.slot(j));
                    let Value::Array(arr) = rt.slot(c) else {
                        panic!("value should be Array");
                    };
                    index_array(arr, i, index_span).map_err(|e| Flow::error(e, span))
                }),
                Src::Const(i) => Box::new(move |rt| {
                    let Value::Array(arr) = rt.slot(c) else {
                        panic!("value should be Array");
                    };
                    index_array(arr, i, index_span).map_err(|e| Flow::error(e, span))
                }),
                Src::Code(_) => unreachable!("index is a local or a literal"),
            };
        }
        let container = self.expr(container);
        let index = self.expr(index);
        Box::new(move |rt| {
            let arr = container(rt)?.unwrap_array();
            match index(rt)? {
                Value::Int(i) => index_array(&arr, i, index_span),
                Value::Range(r) => slice_array(&arr, r.start, r.step, r.end, index_span),
                _ => panic!("array should only be indexed by Int or Range"),
            }
            .map_err(|e| Flow::error(e, span))
        })
    }

    /// Resolves a callee expression to a global and functor application at compile time.
    fn static_callee(&self, id: ExprId) -> Option<(StoreItemId, FunctorApp, &'c [GenericArg])> {
        match &self.get_expr(id).kind {
            ExprKind::Var(Res::Item(item), args) => Some((
                StoreItemId {
                    package: item.package,
                    item: item.item,
                },
                FunctorApp::default(),
                args.as_slice(),
            )),
            ExprKind::UnOp(UnOp::Functor(functor), inner) => {
                let (id, app, args) = self.static_callee(*inner)?;
                Some((id, update_functor_app(*functor, app), args))
            }
            _ => None,
        }
    }

    fn call(&mut self, e: &Expr, callee: ExprId, arg: ExprId) -> Code {
        let site = Site {
            span: e.span.span,
            caller: self.package,
            callee_span: map_fir_package_span_to_hir(self.get_expr(callee).span),
            arg_span: map_fir_package_span_to_hir(self.get_expr(arg).span),
        };
        if let Some((id, functor, generic_args)) = self.static_callee(callee)
            && let Some(resolved) = self.cache.callee(self.globals, id, SpecKind::of(functor))
        {
            match resolved {
                Callee::Udt(id) => {
                    let arg = self.expr(arg);
                    return Box::new(move |rt| Ok(tag_udt(id, arg(rt)?)));
                }
                Callee::Intrinsic(info) => {
                    let arg = self.expr(arg);
                    return Box::new(move |rt| {
                        let value = arg(rt)?;
                        call_intrinsic(rt, &info, functor, value, &site)
                    });
                }
                Callee::Spec(cell) => {
                    let generic_args: Option<Rc<[GenericArg]>> = if generic_args.is_empty() {
                        None
                    } else {
                        self.uses_type_args = true;
                        Some(generic_args.to_vec().into())
                    };
                    let decl = callable_decl(self.globals, id);
                    if spec_decl(decl, cell.kind).input.is_none() {
                        let shape = pat_shape(self.globals, id.package, decl.input);
                        let mut steps = Vec::new();
                        self.plan(arg, &shape, &mut steps);
                        let steps = steps.into_boxed_slice();
                        return Box::new(move |rt| {
                            let base = rt.slots.len();
                            for step in &steps {
                                if let Err(flow) = step.run(rt) {
                                    rt.slots.truncate(base);
                                    return Err(flow);
                                }
                            }
                            call_static(
                                rt,
                                &cell,
                                base,
                                functor,
                                |rt| type_args_for(id, generic_args.as_deref(), rt),
                                &site,
                            )
                        });
                    }
                    let arg = self.expr(arg);
                    let callee = Callee::Spec(cell);
                    return Box::new(move |rt| {
                        let value = arg(rt)?;
                        let type_args = type_args_for(id, generic_args.as_deref(), rt);
                        call_callee(rt, &callee, functor, None, type_args, value, &site)
                    });
                }
            }
        }
        let callee = self.expr(callee);
        let arg = self.expr(arg);
        Box::new(move |rt| {
            let f = callee(rt)?;
            let value = arg(rt)?;
            call_value(rt, f, value, &site)
        })
    }

    /// Plans how to push the argument expression straight into the callee's parameter
    /// slots, avoiding intermediate tuples where the argument is a tuple literal.
    fn plan(&mut self, arg: ExprId, shape: &ArgPat, steps: &mut Vec<Step>) {
        match shape {
            ArgPat::Leaf => steps.push(Step::Push(self.expr(arg))),
            ArgPat::Skip => steps.push(Step::Drop(self.expr(arg))),
            ArgPat::Tuple(shapes) => match &self.get_expr(arg).kind {
                ExprKind::Tuple(items) if items.len() == shapes.len() => {
                    for (item, shape) in items.iter().zip(shapes.iter()) {
                        self.plan(*item, shape, steps);
                    }
                }
                _ => steps.push(Step::Spread(self.expr(arg), shape.clone())),
            },
        }
    }
}

fn type_args_for(id: StoreItemId, args: Option<&[GenericArg]>, rt: &Rt<'_>) -> TypeArgs {
    match args {
        None => TypeArgs::Empty,
        Some(args) => TypeArgs::bind(id, args.to_vec(), rt.type_args.clone()),
    }
}

enum Step {
    Push(Code),
    Drop(Code),
    Spread(Code, ArgPat),
}

impl Step {
    #[inline]
    fn run(&self, rt: &mut Rt<'_>) -> Result<(), Flow> {
        match self {
            Step::Push(code) => {
                let value = code(rt)?;
                rt.slots.push(value);
            }
            Step::Drop(code) => {
                code(rt)?;
            }
            Step::Spread(code, shape) => {
                let value = code(rt)?;
                push_pat(&mut rt.slots, shape, value);
            }
        }
        Ok(())
    }
}

fn update_in_place(
    v: &mut Value,
    index: Value,
    update: Value,
    span: PackageSpan,
) -> Result<(), Error> {
    match index {
        Value::Int(index) => {
            if index < 0 {
                return Err(Error::InvalidNegativeInt(index, span));
            }
            v.update_array(index, update, span)
        }
        Value::Range(r) => {
            let rhs = update.unwrap_array();
            let range = {
                let Value::Array(arr) = &*v else {
                    panic!("variable should be an array");
                };
                make_range(arr, r.start, r.step, r.end, span)?
            };
            for (idx, rhs) in range.zip(rhs.iter()) {
                if idx < 0 {
                    return Err(Error::InvalidNegativeInt(idx, span));
                }
                v.update_array(idx, rhs.clone(), span)?;
            }
            Ok(())
        }
        _ => unreachable!("array should only be indexed by Int or Range"),
    }
}

fn field_access(record: Value, field: &Field) -> Value {
    match (record, field) {
        (Value::Range(inner), Field::Prim(PrimField::Start)) => Value::Int(
            inner
                .start
                .expect("range access should be validated by compiler"),
        ),
        (Value::Range(inner), Field::Prim(PrimField::Step)) => Value::Int(inner.step),
        (Value::Range(inner), Field::Prim(PrimField::End)) => Value::Int(
            inner
                .end
                .expect("range access should be validated by compiler"),
        ),
        (record, Field::Path(path)) => {
            crate::follow_field_path(record, &path.indices).expect("field path should be valid")
        }
        (value, field) => panic!("invalid field access. value: {value:?}, field: {field:?}"),
    }
}

fn update_field(record: Value, field: &Field, value: &Value) -> Value {
    match (record, field) {
        (Value::Range(mut inner), Field::Prim(PrimField::Start)) => {
            inner.start = Some(value.clone().unwrap_int());
            Value::Range(inner)
        }
        (Value::Range(mut inner), Field::Prim(PrimField::Step)) => {
            inner.step = value.clone().unwrap_int();
            Value::Range(inner)
        }
        (Value::Range(mut inner), Field::Prim(PrimField::End)) => {
            inner.end = Some(value.clone().unwrap_int());
            Value::Range(inner)
        }
        (record, Field::Path(path)) => crate::update_field_path(&record, &path.indices, value)
            .expect("field path should be valid"),
        _ => panic!("invalid field access"),
    }
}

fn unop(op: UnOp, val: Value) -> Value {
    match op {
        UnOp::Functor(functor) => match val {
            Value::Closure(inner) => Value::Closure(
                val::Closure {
                    functor: update_functor_app(functor, inner.functor),
                    ..*inner
                }
                .into(),
            ),
            Value::Global(id, app) => Value::Global(id, update_functor_app(functor, app)),
            Value::GlobalWithArgs(global) => Value::global_with_args(
                global.id,
                update_functor_app(functor, global.functor),
                global.type_args.clone(),
            ),
            // Partial evaluation's placeholder for an unresolved callable propagates upward.
            Value::Tuple(inner, None) if inner.is_empty() => Value::Tuple(inner, None),
            _ => panic!("value should be callable"),
        },
        UnOp::Neg => match val {
            Value::BigInt(v) => Value::BigInt(-v),
            Value::Double(v) => Value::Double(-v),
            Value::Int(v) => Value::Int(v.wrapping_neg()),
            Value::Tuple(v, Some(id)) if *id.as_ref() == StoreItemId::complex() => {
                let real = v[0].get_double();
                let imag = v[1].get_double();
                Value::Tuple(
                    vec![Value::Double(-real), Value::Double(-imag)].into(),
                    Some(Rc::new(StoreItemId::complex())),
                )
            }
            _ => panic!("value should be number"),
        },
        UnOp::NotB => match val {
            Value::Int(v) => Value::Int(!v),
            Value::BigInt(v) => Value::BigInt(!v),
            _ => panic!("value should be Int or BigInt"),
        },
        UnOp::NotL => match val {
            Value::Bool(b) => Value::Bool(!b),
            _ => panic!("value should be bool"),
        },
        UnOp::Pos => match val {
            Value::BigInt(_) | Value::Int(_) | Value::Double(_) => val,
            Value::Tuple(_, Some(ref id)) if *id.as_ref() == StoreItemId::complex() => val,
            _ => panic!("value should be number"),
        },
        UnOp::Unwrap => match val {
            Value::Tuple(v, _) => Value::Tuple(v, None),
            _ => val,
        },
    }
}
