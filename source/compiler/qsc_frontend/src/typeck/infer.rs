// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Error, ErrorKind};
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_hir::{
    hir::{ItemId, PrimField, Res},
    ty::{
        Arrow, ClassConstraint, FunctorSet, FunctorSetValue, GenericArg, InferFunctorId, InferTyId,
        Prim, Scheme, Ty, TypeParameter, Udt,
    },
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    cell::RefCell,
    collections::{BTreeSet, VecDeque, hash_map::Entry},
    fmt::Debug,
    rc::Rc,
};

const MAX_TY_RECURSION_DEPTH: i8 = 100;
const MAX_TY_SIZE: usize = 100;

#[derive(Debug, Default)]
struct Solution {
    tys: IndexMap<InferTyId, Ty>,
    functors: IndexMap<InferFunctorId, FunctorSet>,
}

#[derive(Clone, Debug)]
pub(super) enum Class {
    Add(Ty),
    Adj(Ty),
    Call {
        callee: Ty,
        input: ArgTy,
        output: Ty,
    },
    Ctl {
        op: Ty,
        with_ctls: Ty,
    },
    Eq(Ty),
    Exp {
        base: Ty,
        power: Ty,
    },
    HasField {
        record: Ty,
        name: String,
        item: Ty,
    },
    Struct(Ty),
    HasStructShape {
        record: Ty,
        is_copy: bool,
        fields: Vec<(String, Span)>,
    },
    HasIndex {
        container: Ty,
        index: Ty,
        item: Ty,
    },
    Integral(Ty),
    Iterable {
        container: Ty,
        item: Ty,
    },
    Mul(Ty),
    Sub(Ty),
    Div(Ty),
    Ord(Ty),
    Mod(Ty),
    Signed(Ty),
    Show(Ty),
    Unwrap {
        wrapper: Ty,
        base: Ty,
    },
    // A user-defined class
    // When we actually support this, and don't just use it to generate an error,
    // it should have an ID here instead of a name
    NonPrimitive(Rc<str>),
}

impl Class {
    fn dependencies(&self) -> Vec<&Ty> {
        match self {
            Self::Add(ty)
            | Self::Adj(ty)
            | Self::Eq(ty)
            | Self::Integral(ty)
            | Self::Mul(ty)
            | Self::Sub(ty)
            | Self::Div(ty)
            | Self::Mod(ty)
            | Self::Ord(ty)
            | Self::Signed(ty)
            | Self::Show(ty)
            | Self::Struct(ty) => {
                vec![ty]
            }
            Self::Call { callee, .. } => vec![callee],
            Self::Ctl { op, .. } => vec![op],
            Self::Exp { base, .. } => vec![base],
            Self::HasField { record, .. } | Self::HasStructShape { record, .. } => vec![record],
            Self::HasIndex {
                container, index, ..
            } => vec![container, index],
            Self::Iterable { container, .. } => vec![container],
            Self::Unwrap { wrapper, .. } => vec![wrapper],
            Self::NonPrimitive(_) => Vec::new(),
        }
    }

    fn map(self, mut f: impl FnMut(Ty) -> Ty) -> Self {
        match self {
            Self::Add(ty) => Self::Add(f(ty)),
            Self::Adj(ty) => Self::Adj(f(ty)),
            Self::Call {
                callee,
                input,
                output,
            } => Self::Call {
                callee: f(callee),
                input: input.map(&mut f),
                output: f(output),
            },
            Self::Ctl { op, with_ctls } => Self::Ctl {
                op: f(op),
                with_ctls: f(with_ctls),
            },
            Self::Eq(ty) => Self::Eq(f(ty)),
            Self::Exp { base, power } => Self::Exp {
                base: f(base),
                power: f(power),
            },
            Self::HasField { record, name, item } => Self::HasField {
                record: f(record),
                name,
                item: f(item),
            },
            Self::Struct(ty) => Self::Struct(f(ty)),
            Self::HasStructShape {
                record,
                is_copy,
                fields,
            } => Self::HasStructShape {
                record: f(record),
                is_copy,
                fields,
            },
            Self::HasIndex {
                container,
                index,
                item,
            } => Self::HasIndex {
                container: f(container),
                index: f(index),
                item: f(item),
            },
            Self::Integral(ty) => Self::Integral(f(ty)),
            Self::Iterable { container, item } => Self::Iterable {
                container: f(container),
                item: f(item),
            },
            Self::Sub(ty) => Self::Sub(f(ty)),
            Self::Mul(ty) => Self::Mul(f(ty)),
            Self::Div(ty) => Self::Div(f(ty)),
            Self::Ord(ty) => Self::Ord(f(ty)),
            Self::Mod(ty) => Self::Mod(f(ty)),
            Self::Signed(ty) => Self::Signed(f(ty)),

            Self::Show(ty) => Self::Show(f(ty)),
            Self::Unwrap { wrapper, base } => Self::Unwrap {
                wrapper: f(wrapper),
                base: f(base),
            },
            Self::NonPrimitive(name) => Self::NonPrimitive(name),
        }
    }

    fn check(self, udts: &FxHashMap<ItemId, Udt>, span: Span) -> (Vec<Constraint>, Vec<Error>) {
        match self {
            Class::Add(ty) if check_add(&ty) => (Vec::new(), Vec::new()),
            Class::Add(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassAdd(ty.display(), span))],
            ),
            Class::Adj(ty) => check_adj(ty, span),
            Class::Call {
                callee,
                input,
                output,
            } => check_call(callee, &input, output, span),
            Class::Ctl { op, with_ctls } => check_ctl(op, with_ctls, span),
            Class::Eq(ty) => check_eq(ty, span),
            Class::Exp { base, power } => check_exp(base, power, span),
            Class::HasField { record, name, item } => {
                check_has_field(udts, &record, name, item, span)
            }
            Class::Struct(ty) => check_struct(udts, &ty, span),
            Class::HasStructShape {
                record,
                is_copy,
                fields,
            } => check_has_struct_shape(udts, &record, is_copy, &fields, span),
            Class::HasIndex {
                container,
                index,
                item,
            } => check_has_index(container, index, item, span),
            Class::Integral(ty) if check_integral(&ty) => (Vec::new(), Vec::new()),
            Class::Integral(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassInteger(ty.display(), span))],
            ),
            Class::Iterable { container, item } => check_iterable(container, item, span),
            Class::Sub(ty) if check_sub(&ty) => (Vec::new(), Vec::new()),
            Class::Sub(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassSub(ty.display(), span))],
            ),
            Class::Mul(ty) if check_mul(&ty) => (Vec::new(), Vec::new()),
            Class::Mul(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassMul(ty.display(), span))],
            ),
            Class::Div(ty) if check_div(&ty) => (Vec::new(), Vec::new()),
            Class::Div(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassDiv(ty.display(), span))],
            ),
            Class::Ord(ty) if check_ord(&ty) => (Vec::new(), Vec::new()),
            Class::Ord(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassOrd(ty.display(), span))],
            ),
            Class::Signed(ty) if check_signed(&ty) => (Vec::new(), Vec::new()),
            Class::Signed(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassSigned(ty.display(), span))],
            ),
            Class::Mod(ty) if check_mod(&ty) => (Vec::new(), Vec::new()),
            Class::Mod(ty) => (
                Vec::new(),
                vec![Error(ErrorKind::MissingClassMod(ty.display(), span))],
            ),
            Class::Show(ty) => check_show(ty, span),
            Class::Unwrap { wrapper, base } => check_unwrap(udts, &wrapper, base, span),
            Class::NonPrimitive(_) => (vec![], vec![]),
        }
    }
}

fn check_mod(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Mod, ty)
}

fn check_signed(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Signed, ty)
}

fn check_ord(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Ord, ty)
}

fn check_div(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Div, ty)
}

fn check_mul(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Mul, ty)
}

fn check_sub(ty: &Ty) -> bool {
    check_num_constraint(&ClassConstraint::Sub, ty)
}

/// Meta-level descriptions about the source of a type.
/// The compiler uses the notion of "unresolved types" to
/// represent both divergent types (return expressions, similar to
/// the `never` type), and types with insufficient information to
/// be inferred.
/// We want to generate compiler errors in the latter case,
/// so we need to track where types came from. This `TySource`
/// struct allows us to know if a type originates from a divergent
/// source, and if it doesn't, we generate an ambiguous type error.
#[derive(Debug)]
pub(super) enum TySource {
    Divergent,
    NotDivergent { span: Span },
}

impl TySource {
    pub(super) fn not_divergent(span: Span) -> Self {
        TySource::NotDivergent { span }
    }

    pub(crate) fn divergent() -> TySource {
        TySource::Divergent
    }
}

/// An argument type and tags describing the call syntax. This represents the type of something
/// that appears in a _call_ to a _callable_, and an argument can be a hole, a given argument, or,
/// in the most standard case, a tuple. Foo(1, 2, 3) is [`ArgTy::Tuple`], not [`ArgTy::Given`].
#[derive(Clone, Debug)]
pub(super) enum ArgTy {
    /// A missing argument, indicating partial application.
    Hole(Ty),
    /// A given argument.
    Given(Ty),
    /// A list of arguments. This corresponds literally to tuple syntax, not to any expression of a tuple type.
    Tuple(Vec<ArgTy>),
}

impl ArgTy {
    /// Applies a function `f` to each type in the argument type.
    fn map(self, f: &mut impl FnMut(Ty) -> Ty) -> Self {
        match self {
            Self::Hole(ty) => Self::Hole(f(ty)),
            Self::Given(ty) => Self::Given(f(ty)),
            Self::Tuple(items) => Self::Tuple(items.into_iter().map(|i| i.map(f)).collect()),
        }
    }

    /// Applies the argument type to a parameter type, generating constraints and errors.
    fn apply(&self, param: &Ty, span: Span) -> App {
        match (self, param) {
            // If `arg` is a hole, then it doesn't matter what the param is,
            // because the hole can be anything.
            // However, we do know that the type of Arg must be Eq to the type of Param, so we
            // add that to the constraints.
            // Preserve the hole.
            (Self::Hole(arg), _) => App {
                holes: vec![param.clone()],
                constraints: vec![Constraint::Eq {
                    expected: param.clone(),
                    actual: arg.clone(),
                    span,
                }],
                errors: Vec::new(),
            },
            // If `arg` is a hole, then it doesn't matter what the param is,
            // because the hole can be anything.
            // However, we do know that the type of Arg must be Eq to the type of Param, so we
            // add that to the constraints.
            (Self::Given(arg), _) => App {
                holes: Vec::new(),
                constraints: vec![Constraint::Eq {
                    expected: param.clone(),
                    actual: arg.clone(),
                    span,
                }],
                errors: Vec::new(),
            },
            // if both the arg and the param are tuples, then we must check
            // the types of each element in the tuple and generate iterative applications.
            (Self::Tuple(args), Ty::Tuple(params)) => {
                let mut errors = Vec::new();
                if args.len() != params.len() {
                    errors.push(Error(ErrorKind::TyMismatch(
                        Ty::Tuple(params.clone()).display(),
                        self.to_ty().display(),
                        span,
                    )));
                }

                let mut holes = Vec::new();
                let mut constraints = Vec::new();
                for (arg, param) in args.iter().zip(params) {
                    let mut app = arg.apply(param, span);
                    constraints.append(&mut app.constraints);
                    errors.append(&mut app.errors);
                    if app.holes.len() > 1 {
                        holes.push(Ty::Tuple(app.holes));
                    } else {
                        holes.append(&mut app.holes);
                    }
                }

                App {
                    holes,
                    constraints,
                    errors,
                }
            }

            (Self::Tuple(_), Ty::Infer(_)) => App {
                holes: Vec::new(),
                constraints: vec![Constraint::Eq {
                    expected: param.clone(),
                    actual: self.to_ty(),
                    span,
                }],
                errors: Vec::new(),
            },
            (Self::Tuple(_), _) => App {
                holes: Vec::new(),
                constraints: Vec::new(),
                errors: vec![Error(ErrorKind::TyMismatch(
                    param.display(),
                    self.to_ty().display(),
                    span,
                ))],
            },
        }
    }

    pub(super) fn to_ty(&self) -> Ty {
        match self {
            ArgTy::Hole(ty) | ArgTy::Given(ty) => ty.clone(),
            ArgTy::Tuple(items) => Ty::Tuple(items.iter().map(Self::to_ty).collect()),
        }
    }
}

/// The result of applying an argument to a callable.
struct App {
    /// The types of all missing arguments in order and preserving their recursive tuple structure.
    holes: Vec<Ty>,
    /// The constraints implied by the call.
    constraints: Vec<Constraint>,
    /// The errors from the call.
    errors: Vec<Error>,
}

#[derive(Debug)]
pub(super) enum Constraint {
    // Constraint that says a type must satisfy a class
    Class(Class, Span),
    // Constraint that says two types must be the same
    Eq {
        expected: Ty,
        actual: Ty,
        span: Span,
    },
    Superset {
        expected: FunctorSetValue,
        actual: FunctorSet,
        span: Span,
    },
}

#[derive(Debug)]
pub(super) struct Inferrer {
    solver: Solver,
    constraints: VecDeque<Constraint>,
    /// Metadata about the construction of types.
    ty_metadata: IndexMap<InferTyId, TySource>,
    next_ty: InferTyId,
    next_functor: InferFunctorId,
}

impl Inferrer {
    pub(super) fn new() -> Self {
        Self {
            solver: Solver::new(),
            constraints: VecDeque::new(),
            next_ty: InferTyId::default(),
            next_functor: InferFunctorId::default(),
            ty_metadata: IndexMap::default(),
        }
    }

    /// Introduces an equality constraint between the expected and actual types.
    pub(super) fn eq(&mut self, span: Span, expected: Ty, actual: Ty) {
        self.constraints.push_back(Constraint::Eq {
            expected,
            actual,
            span,
        });
    }

    /// Introduces a class constraint.
    pub(super) fn class(&mut self, span: Span, class: Class) {
        self.constraints.push_back(Constraint::Class(class, span));
    }

    /// Returns a unique type variable with specified constraints.
    fn constrained_ty(
        &mut self,
        meta: TySource,
        with_constraints: impl Fn(Ty) -> Box<[Constraint]>,
    ) -> Ty {
        let fresh = self.next_ty;
        self.next_ty = fresh.successor();
        self.ty_metadata.insert(fresh, meta);
        let constraints = with_constraints(Ty::Infer(fresh));
        self.constraints.extend(constraints);
        Ty::Infer(fresh)
    }

    /// Returns a unique unconstrained type variable.
    pub(super) fn fresh_ty(&mut self, meta: TySource) -> Ty {
        let fresh = self.next_ty;
        self.next_ty = fresh.successor();
        self.ty_metadata.insert(fresh, meta);
        Ty::Infer(fresh)
    }

    /// Returns a unique unconstrained functor variable.
    pub(super) fn fresh_functor(&mut self) -> FunctorSet {
        let fresh = self.next_functor;
        self.next_functor = fresh.successor();
        FunctorSet::Infer(fresh)
    }

    /// Instantiates the type scheme.
    pub(super) fn instantiate(&mut self, scheme: &Scheme, span: Span) -> (Arrow, Vec<GenericArg>) {
        let args: Vec<_> = scheme
            .params()
            .iter()
            .map(|param| match param {
                TypeParameter::Ty { bounds, .. } => {
                    GenericArg::Ty(self.constrained_ty(TySource::not_divergent(span), |ty| {
                        bounds
                            .0
                            .iter()
                            .map(|x| into_constraint(ty.clone(), x, span))
                            .collect()
                    }))
                }
                TypeParameter::Functor(expected) => {
                    let actual = self.fresh_functor();
                    self.constraints.push_back(Constraint::Superset {
                        expected: *expected,
                        actual,
                        span,
                    });
                    GenericArg::Functor(actual)
                }
            })
            .collect();

        let ty = scheme
            .instantiate(&args)
            .expect("scheme should instantiate with fresh arguments");

        (ty, args)
    }

    /// Solves for all variables given the accumulated constraints.
    pub(super) fn solve(&mut self, udts: &FxHashMap<ItemId, Udt>) -> Vec<Error> {
        while let Some(constraint) = self.constraints.pop_front() {
            for constraint in self.solver.constrain(udts, constraint).into_iter().rev() {
                self.constraints.push_front(constraint);
            }
        }
        let unresolved_ty_errs = self.find_unresolved_types();
        self.solver.default_functors(self.next_functor);
        self.solver
            .errors
            .drain(..)
            .chain(unresolved_ty_errs)
            .collect()
    }

    fn find_unresolved_types(&mut self) -> Vec<Error> {
        self.ty_metadata
            .drain()
            .filter_map(|(id, meta)| {
                if self.solver.solution.tys.get(id).is_none() {
                    match meta {
                        TySource::Divergent => {
                            // here, we are resolving all divergent types to the unit type.
                            self.solver.solution.tys.insert(id, Ty::UNIT);
                            None
                        }
                        TySource::NotDivergent { span } => {
                            Some(Error(ErrorKind::AmbiguousTy(span)))
                        }
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    pub(super) fn substitute_ty(&mut self, ty: &mut Ty) {
        substitute_ty(&self.solver.solution, ty);
    }

    pub(super) fn substitute_functor(&mut self, functors: &mut FunctorSet) {
        substitute_functor(&self.solver.solution, functors);
    }

    pub(super) fn report_error(&mut self, error: impl Into<Error>) {
        self.solver.errors.push(error.into());
    }
}

#[derive(Debug)]
struct Solver {
    solution: Solution,
    pending_tys: FxHashMap<InferTyId, Vec<Class>>,
    pending_functors: FxHashMap<InferFunctorId, FunctorSetValue>,
    errors: Vec<Error>,
}

impl Solver {
    fn new() -> Self {
        Self {
            solution: Solution::default(),
            pending_tys: FxHashMap::default(),
            pending_functors: FxHashMap::default(),
            errors: Vec::new(),
        }
    }

    /// Given a constraint, attempts to narrow the constraint by either
    /// generating more specific constraints, or, if it cannot be narrowed further,
    /// returns an empty vector.
    fn constrain(
        &mut self,
        udts: &FxHashMap<ItemId, Udt>,
        constraint: Constraint,
    ) -> Vec<Constraint> {
        match constraint {
            Constraint::Class(class, span) => self.class(udts, class, span),
            Constraint::Eq {
                expected,
                actual,
                span,
            } => self.eq(expected, actual, span),
            Constraint::Superset {
                expected,
                actual,
                span,
            } => {
                self.superset(expected, actual, span);
                Vec::new()
            }
        }
    }

    /// Attempts to narrow a class constraint, returning more specific constraints if any.
    fn class(
        &mut self,
        udts: &FxHashMap<ItemId, Udt>,
        class: Class,
        span: Span,
    ) -> Vec<Constraint> {
        // true if a dependency of this class constraint is currently unknown, meaning we
        // have to come back to it later.
        // false if we know everything we need to know and this is solved
        let unknown_dependency = class.dependencies().into_iter().any(|ty| {
            if ty == &Ty::Err {
                true
                // if this needs to be inferred further, `unknown_ty` returns `Some(ty_id)`
            } else if let Some(infer) = unknown_ty(&self.solution.tys, ty) {
                self.pending_tys
                    .entry(infer)
                    .or_default()
                    .push(class.clone());
                true
            } else {
                false
            }
        });

        if unknown_dependency {
            Vec::new()
        } else {
            let (constraints, mut errors) = class
                .map(|ty| substituted_ty(&self.solution, ty))
                .check(udts, span);
            self.errors.append(&mut errors);
            constraints
        }
    }

    fn eq(&mut self, mut expected: Ty, mut actual: Ty, span: Span) -> Vec<Constraint> {
        // Only attempt to unify the types if they are fully substituted. If they are not,
        // this usually indicates an infinite recursion in the type inference, so further
        // unification would get stuck in a loop by creating recursive constraints.
        if substitute_ty(&self.solution, &mut expected)
            && substitute_ty(&self.solution, &mut actual)
        {
            self.unify(&expected, &actual, span)
        } else {
            Vec::new()
        }
    }

    fn superset(&mut self, expected: FunctorSetValue, mut actual: FunctorSet, span: Span) {
        substitute_functor(&self.solution, &mut actual);
        match (expected, actual) {
            (_, FunctorSet::Value(FunctorSetValue::CtlAdj))
            | (FunctorSetValue::Empty, _)
            | (FunctorSetValue::Adj, FunctorSet::Value(FunctorSetValue::Adj))
            | (FunctorSetValue::Ctl, FunctorSet::Value(FunctorSetValue::Ctl)) => {}
            (expected, FunctorSet::Infer(infer)) => match self.pending_functors.entry(infer) {
                Entry::Occupied(mut entry) => {
                    let functors = entry.get_mut();
                    *functors = functors.union(&expected);
                }
                Entry::Vacant(entry) => {
                    entry.insert(expected);
                }
            },
            (expected, actual) => self.errors.push(Error(ErrorKind::MissingFunctor(
                FunctorSet::Value(expected),
                actual,
                span,
            ))),
        }
    }

    fn unify(&mut self, ty1: &Ty, ty2: &Ty, span: Span) -> Vec<Constraint> {
        match (ty1, ty2) {
            (Ty::Err, _)
            | (_, Ty::Err)
            | (Ty::Udt(_, Res::Err), Ty::Udt(_, _))
            | (Ty::Udt(_, _), Ty::Udt(_, Res::Err)) => Vec::new(),
            (Ty::Array(item1), Ty::Array(item2)) => self.unify(item1, item2, span),
            (Ty::Arrow(arrow1), Ty::Arrow(arrow2)) => {
                if arrow1.kind != arrow2.kind {
                    self.errors.push(Error(ErrorKind::CallableMismatch(
                        arrow1.kind,
                        arrow2.kind,
                        span,
                    )));
                }

                let mut constraints =
                    self.unify(&arrow1.input.borrow(), &arrow2.input.borrow(), span);
                constraints.append(&mut self.unify(
                    &arrow1.output.borrow(),
                    &arrow2.output.borrow(),
                    span,
                ));

                match (*arrow1.functors.borrow(), *arrow2.functors.borrow()) {
                    (FunctorSet::Value(value1), FunctorSet::Value(value2))
                        if value2.satisfies(&value1) => {}
                    (FunctorSet::Infer(infer1), FunctorSet::Infer(infer2)) if infer1 == infer2 => {}
                    (FunctorSet::Infer(infer), functors) | (functors, FunctorSet::Infer(infer)) => {
                        constraints.append(&mut self.bind_functor(infer, functors, span));
                    }
                    _ => {
                        self.errors.push(Error(ErrorKind::FunctorMismatch(
                            *arrow1.functors.borrow(),
                            *arrow2.functors.borrow(),
                            span,
                        )));
                    }
                }

                constraints
            }
            (Ty::Infer(infer1), Ty::Infer(infer2)) if infer1 == infer2 => Vec::new(),
            (&Ty::Infer(infer), ty) | (ty, &Ty::Infer(infer)) => {
                self.bind_ty(infer, ty.clone(), span)
            }
            (
                Ty::Param {
                    name: name1,
                    id: id1,
                    bounds: bounds1,
                },
                Ty::Param {
                    name: _name2,
                    id: id2,
                    bounds: bounds2,
                },
            ) if id1 == id2 => {
                // concat the two sets of bounds
                #[allow(
                    clippy::mutable_key_type,
                    reason = "the BTreeSet is temporary and not used across mutations of the types"
                )]
                let bounds: BTreeSet<ClassConstraint> = bounds1
                    .0
                    .iter()
                    .chain(bounds2.0.iter())
                    .map(Clone::clone)
                    .collect();

                let merged_ty = Ty::Param {
                    name: name1.clone(),
                    id: *id1,
                    bounds: qsc_hir::ty::ClassConstraints(bounds.clone().into_iter().collect()),
                };
                bounds
                    .into_iter()
                    .map(|x| into_constraint(merged_ty.clone(), &x, span))
                    .collect()
            }
            (Ty::Prim(prim1), Ty::Prim(prim2)) if prim1 == prim2 => Vec::new(),
            (Ty::Tuple(items1), Ty::Tuple(items2)) => {
                if items1.len() != items2.len() {
                    self.errors.push(Error(ErrorKind::TyMismatch(
                        ty1.display(),
                        ty2.display(),
                        span,
                    )));
                }

                items1
                    .iter()
                    .zip(items2)
                    .flat_map(|(item1, item2)| self.unify(item1, item2, span))
                    .collect()
            }
            (Ty::Udt(_, res1), Ty::Udt(_, res2)) if res1 == res2 => Vec::new(),
            _ => {
                self.errors.push(Error(ErrorKind::TyMismatch(
                    ty1.display(),
                    ty2.display(),
                    span,
                )));
                Vec::new()
            }
        }
    }

    fn bind_ty(&mut self, infer: InferTyId, ty: Ty, span: Span) -> Vec<Constraint> {
        if ty.size() > MAX_TY_SIZE {
            self.errors
                .push(Error(ErrorKind::TySizeLimitExceeded(ty.display(), span)));
            return Vec::new();
        } else if links_to_infer_ty(&self.solution.tys, infer, &ty) {
            self.errors
                .push(Error(ErrorKind::RecursiveTypeConstraint(span)));
            return Vec::new();
        }
        self.solution.tys.insert(infer, ty.clone());
        let mut constraint = vec![Constraint::Eq {
            expected: ty,
            actual: Ty::Infer(infer),
            span,
        }];
        constraint.append(
            &mut self
                .pending_tys
                .remove(&infer)
                .map_or(Vec::new(), |pending| {
                    pending
                        .into_iter()
                        .map(|class| Constraint::Class(class, span))
                        .collect()
                }),
        );
        constraint
    }

    fn bind_functor(
        &mut self,
        infer: InferFunctorId,
        functors: FunctorSet,
        span: Span,
    ) -> Vec<Constraint> {
        self.solution.functors.insert(infer, functors);
        self.pending_functors
            .remove(&infer)
            .map_or(Vec::new(), |expected| {
                vec![Constraint::Superset {
                    expected,
                    actual: functors,
                    span,
                }]
            })
    }

    fn default_functors(&mut self, functor_end: InferFunctorId) {
        let mut functor = InferFunctorId::default();
        while functor < functor_end {
            if !self.solution.functors.contains_key(functor) {
                let value = self.pending_functors.remove(&functor).unwrap_or_default();
                self.solution
                    .functors
                    .insert(functor, FunctorSet::Value(value));
            }

            functor = functor.successor();
        }
    }
}

/// Replaces inferred tys with the underlying type that they refer to, if it has been solved
/// already.
fn substitute_ty(solution: &Solution, ty: &mut Ty) -> bool {
    fn substitute_ty_recursive(solution: &Solution, ty: &mut Ty, limit: i8) -> bool {
        if limit == 0 {
            // We've hit the recursion limit. Give up and leave the inferred types
            // as is. This should trigger an ambiguous type error later.
            // Return false only when recursion limit is hit so the caller can know
            // types have not been fully substituted.
            return false;
        }
        match ty {
            Ty::Err | Ty::Param { .. } | Ty::Prim(_) | Ty::Udt(_, _) => true,
            Ty::Array(item) => substitute_ty_recursive(solution, item, limit - 1),
            Ty::Arrow(arrow) => {
                // These updates require borrowing the values inside the RefCells mutably.
                // This should be safe because no other code should be borrowing these values at the same time,
                // but it will panic at runtime if any other borrows occur.
                let a = substitute_ty_recursive(
                    solution,
                    &mut arrow
                        .input
                        .try_borrow_mut()
                        .expect("should have unique access to arrow.input"),
                    limit - 1,
                );
                let b = substitute_ty_recursive(
                    solution,
                    &mut arrow
                        .output
                        .try_borrow_mut()
                        .expect("should have unique access to arrow.output"),
                    limit - 1,
                );
                substitute_functor(
                    solution,
                    &mut arrow
                        .functors
                        .try_borrow_mut()
                        .expect("should have unique access to arrow.functors"),
                );
                a && b
            }
            Ty::Tuple(items) => {
                let mut all_known = true;
                for item in items {
                    all_known = substitute_ty_recursive(solution, item, limit - 1) && all_known;
                }
                all_known
            }
            &mut Ty::Infer(infer) => {
                if let Some(new_ty) = solution.tys.get(infer) {
                    *ty = new_ty.clone();
                    substitute_ty_recursive(solution, ty, limit - 1)
                } else {
                    true
                }
            }
        }
    }

    substitute_ty_recursive(solution, ty, MAX_TY_RECURSION_DEPTH)
}

fn substituted_ty(solution: &Solution, mut ty: Ty) -> Ty {
    substitute_ty(solution, &mut ty);
    ty
}

fn substitute_functor(solution: &Solution, functors: &mut FunctorSet) {
    if let &mut FunctorSet::Infer(infer) = functors {
        if let Some(&new_functors) = solution.functors.get(infer) {
            *functors = new_functors;
            substitute_functor(solution, functors);
        }
    }
}

// `Some(ty)` if `given_type` has not been solved for yet, `None` if it is fully known/non-inferred
fn unknown_ty(solved_types: &IndexMap<InferTyId, Ty>, given_type: &Ty) -> Option<InferTyId> {
    match given_type {
        // if the given type is an inference type, check if we have solved for it
        &Ty::Infer(infer) => match solved_types.get(infer) {
            // if we have not solved for it, then indeed this is an unknown type
            None => Some(infer),
            // if we have solved for it, then we check if that solved type is itself
            // solved. It could have been solved to another inference type
            Some(solved_type) => unknown_ty(solved_types, solved_type),
        },
        // the given type is not an inference type so it is not unknown
        _ => None,
    }
}

/// Checks whether the given inference type is eventually pointed to by the given type,
/// indicating a recursive type constraint.
fn links_to_infer_ty(solution_tys: &IndexMap<InferTyId, Ty>, id: InferTyId, ty: &Ty) -> bool {
    match ty {
        Ty::Err | Ty::Param { .. } | Ty::Prim(_) | Ty::Udt(_, _) => false,
        Ty::Array(item) => links_to_infer_ty(solution_tys, id, item),
        Ty::Arrow(arrow) => {
            links_to_infer_ty(solution_tys, id, &arrow.input.borrow())
                || links_to_infer_ty(solution_tys, id, &arrow.output.borrow())
        }
        Ty::Infer(other_id) => {
            // if the other id is the same as the one we are checking, then this is a recursive type
            id == *other_id
                // OR if the other id is in the solutions tys, we need to continue the check
                // through the pointed to type.
                || solution_tys
                    .get(*other_id)
                    .is_some_and(|ty| links_to_infer_ty(solution_tys, id, ty))
        }
        Ty::Tuple(items) => items
            .iter()
            .any(|ty| links_to_infer_ty(solution_tys, id, ty)),
    }
}

fn check_add(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::BigInt | Prim::Double | Prim::Int | Prim::String) | Ty::Array(_) => true,
        Ty::Param { bounds, .. } => bounds
            .0
            .iter()
            .any(|bound| matches!(bound, ClassConstraint::Add)),
        Ty::Udt(_, Res::Item(id)) if *id == ItemId::complex() => true,
        _ => false,
    }
}

fn check_adj(ty: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    match ty {
        Ty::Arrow(arrow) => (
            vec![Constraint::Superset {
                expected: FunctorSetValue::Adj,
                actual: *arrow.functors.borrow(),
                span,
            }],
            Vec::new(),
        ),
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassAdj(ty.display(), span))],
        ),
    }
}

fn check_call(callee: Ty, input: &ArgTy, output: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    let Ty::Arrow(arrow) = callee else {
        return (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassCall(callee.display(), span))],
        );
    };

    // generate constraints for the arg ty that correspond to any class constraints specified in
    // the parameters

    let mut app = input.apply(&arrow.input.borrow(), span);
    let expected = if app.holes.len() > 1 {
        Ty::Arrow(Rc::new(Arrow {
            kind: arrow.kind,
            input: RefCell::new(Ty::Tuple(app.holes)),
            output: arrow.output.clone(),
            functors: arrow.functors.clone(),
        }))
    } else if let Some(hole) = app.holes.pop() {
        Ty::Arrow(Rc::new(Arrow {
            kind: arrow.kind,
            input: RefCell::new(hole),
            output: arrow.output.clone(),
            functors: arrow.functors.clone(),
        }))
    } else {
        arrow.output.borrow().clone()
    };

    app.constraints.push(Constraint::Eq {
        expected,
        actual: output,
        span,
    });
    (app.constraints, app.errors)
}

fn check_ctl(op: Ty, with_ctls: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    let Ty::Arrow(arrow) = op else {
        return (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassCtl(op.display(), span))],
        );
    };

    let qubit_array = Ty::Array(Box::new(Ty::Prim(Prim::Qubit)));
    let ctl_input = RefCell::new(Ty::Tuple(vec![qubit_array, arrow.input.borrow().clone()]));
    let actual = *arrow.functors.borrow();
    (
        vec![
            Constraint::Superset {
                expected: FunctorSetValue::Ctl,
                actual,
                span,
            },
            Constraint::Eq {
                expected: Ty::Arrow(Rc::new(Arrow {
                    kind: arrow.kind,
                    input: ctl_input,
                    output: arrow.output.clone(),
                    functors: arrow.functors.clone(),
                })),
                actual: with_ctls,
                span,
            },
        ],
        Vec::new(),
    )
}

/// Checks that the class `Eq` is implemented for the given type.
fn check_eq(ty: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    match ty {
        Ty::Prim(
            Prim::BigInt
            | Prim::Bool
            | Prim::Double
            | Prim::Int
            | Prim::Qubit
            | Prim::Range
            | Prim::Result
            | Prim::String
            | Prim::Pauli,
        ) => (Vec::new(), Vec::new()),
        Ty::Array(item) => (vec![Constraint::Class(Class::Eq(*item), span)], Vec::new()),
        Ty::Tuple(items) => (
            items
                .into_iter()
                .map(|item| Constraint::Class(Class::Eq(item), span))
                .collect(),
            Vec::new(),
        ),
        Ty::Param { ref bounds, .. } => {
            // check if the bounds contain Eq

            match bounds
                .0
                .iter()
                .find(|bound| matches!(bound, ClassConstraint::Eq))
            {
                Some(_) => (Vec::new(), Vec::new()),
                None => (
                    Vec::new(),
                    vec![Error(ErrorKind::MissingClassEq(ty.display(), span))],
                ),
            }
        }
        Ty::Udt(_, Res::Item(id)) if id == ItemId::complex() => (Vec::new(), Vec::new()),
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassEq(ty.display(), span))],
        ),
    }
}

fn check_exp(base: Ty, given_power: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    match base {
        Ty::Prim(Prim::BigInt) => (
            vec![Constraint::Eq {
                expected: Ty::Prim(Prim::Int),
                actual: given_power,
                span,
            }],
            Vec::new(),
        ),
        Ty::Prim(Prim::Double | Prim::Int) => (
            vec![Constraint::Eq {
                expected: base,
                actual: given_power,
                span,
            }],
            Vec::new(),
        ),
        Ty::Udt(name, Res::Item(id)) if id == ItemId::complex() => (
            vec![Constraint::Eq {
                expected: Ty::Udt(name, Res::Item(id)),
                actual: given_power,
                span,
            }],
            Vec::new(),
        ),
        Ty::Param { ref bounds, .. } => {
            // check if the bounds contain Exp

            match bounds
                .0
                .iter()
                .find(|bound| matches!(bound, ClassConstraint::Exp { .. }))
            {
                Some(ClassConstraint::Exp {
                    power: power_from_param,
                }) => (
                    vec![Constraint::Eq {
                        actual: given_power,
                        expected: power_from_param.clone(),
                        span,
                    }],
                    Vec::new(),
                ),
                _ => (
                    Vec::new(),
                    vec![Error(ErrorKind::MissingClassExp(base.display(), span))],
                ),
            }
        }
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassExp(base.display(), span))],
        ),
    }
}

// i'm using the wildcard below to enforce that Ty::Param is always matched in the err branch, as
// it shouldn't be constrained by HasField as long as we don't support structural typing
#[allow(clippy::wildcard_in_or_patterns)]
fn check_has_field(
    udts: &FxHashMap<ItemId, Udt>,
    record: &Ty,
    name: String,
    item: Ty,
    span: Span,
) -> (Vec<Constraint>, Vec<Error>) {
    match (name.parse(), &record) {
        (Ok(PrimField::Start), Ty::Prim(Prim::Range | Prim::RangeFrom))
        | (
            Ok(PrimField::Step),
            Ty::Prim(Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull),
        )
        | (Ok(PrimField::End), Ty::Prim(Prim::Range | Prim::RangeTo)) => (
            vec![Constraint::Eq {
                expected: item,
                actual: Ty::Prim(Prim::Int),
                span,
            }],
            Vec::new(),
        ),
        (_, Ty::Udt(_, Res::Item(id))) => {
            match udts.get(id).and_then(|udt| udt.field_ty_by_name(&name)) {
                Some(ty) => (
                    vec![Constraint::Eq {
                        expected: id
                            .package
                            .map_or_else(|| ty.clone(), |package_id| ty.with_package(package_id)),
                        actual: item,
                        span,
                    }],
                    Vec::new(),
                ),
                None => (
                    Vec::new(),
                    vec![Error(ErrorKind::MissingClassHasField(
                        record.display(),
                        name,
                        span,
                    ))],
                ),
            }
        }
        // `HasField` cannot be used to constrain an arbitrary type parameter, it is used
        // internally only, so it will never resolve to a ty param.
        (_, Ty::Param { .. }) | _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassHasField(
                record.display(),
                name,
                span,
            ))],
        ),
    }
}

fn ty_to_udt<'a>(udts: &'a FxHashMap<ItemId, Udt>, record: &Ty) -> Option<&'a Udt> {
    match record {
        Ty::Udt(_, Res::Item(id)) => udts.get(id),
        _ => None,
    }
}

fn check_struct(
    udts: &FxHashMap<ItemId, Udt>,
    record: &Ty,
    span: Span,
) -> (Vec<Constraint>, Vec<Error>) {
    match ty_to_udt(udts, record) {
        Some(udt) if udt.is_struct() => (Vec::new(), Vec::new()),
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassStruct(record.display(), span))],
        ),
    }
}

fn check_has_struct_shape(
    udts: &FxHashMap<ItemId, Udt>,
    record: &Ty,
    is_copy: bool,
    fields: &[(String, Span)],
    span: Span,
) -> (Vec<Constraint>, Vec<Error>) {
    let mut errors = Vec::new();

    // Check for duplicate fields.
    let mut seen = FxHashSet::default();
    for (field_name, field_span) in fields {
        if !seen.insert(field_name) {
            errors.push(Error(ErrorKind::DuplicateField(
                record.display(),
                field_name.clone(),
                *field_span,
            )));
        }
    }

    match ty_to_udt(udts, record) {
        Some(udt) => {
            // We could compare the actual field names, but the HasField constraint already
            // ensures all the listed fields are valid, and we just checked against duplicates,
            // so we can just check the count.

            let definition_field_count = match &udt.definition.kind {
                qsc_hir::ty::UdtDefKind::Field(_) => 0,
                qsc_hir::ty::UdtDefKind::Tuple(fields) => fields.len(),
            };

            if (is_copy && fields.len() > definition_field_count)
                || (!is_copy && fields.len() != definition_field_count)
            {
                errors.push(Error(ErrorKind::MissingClassCorrectFieldCount(
                    record.display(),
                    span,
                )));
            }

            (Vec::new(), errors)
        }
        None => (Vec::new(), errors),
    }
}

fn check_has_index(
    container: Ty,
    index: Ty,
    item: Ty,
    span: Span,
) -> (Vec<Constraint>, Vec<Error>) {
    match (container, index) {
        (Ty::Array(container_item), Ty::Prim(Prim::Int)) => (
            vec![Constraint::Eq {
                expected: *container_item,
                actual: item,
                span,
            }],
            Vec::new(),
        ),
        (
            container @ Ty::Array(_),
            Ty::Prim(Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull),
        ) => (
            vec![Constraint::Eq {
                expected: container,
                actual: item,
                span,
            }],
            Vec::new(),
        ),
        (container, index) => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassHasIndex(
                container.display(),
                index.display(),
                span,
            ))],
        ),
    }
}

fn check_integral(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::BigInt | Prim::Int) => true,
        Ty::Param { bounds, .. } => bounds
            .0
            .iter()
            .any(|bound| matches!(bound, ClassConstraint::Integral)),
        _ => false,
    }
}

fn check_iterable(container: Ty, item: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    match container {
        Ty::Prim(Prim::Range) => (
            vec![Constraint::Eq {
                expected: Ty::Prim(Prim::Int),
                actual: item,
                span,
            }],
            Vec::new(),
        ),
        Ty::Array(container_item) => (
            vec![Constraint::Eq {
                expected: *container_item,
                actual: item,
                span,
            }],
            Vec::new(),
        ),
        Ty::Param { .. } => (
            Vec::default(),
            vec![Error(ErrorKind::UnrecognizedClass {
                span,
                name: "Iterable".into(),
            })],
        ),
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassIterable(
                container.display(),
                span,
            ))],
        ),
    }
}

/// Some constraints are just true if the type is numeric, this used to be the class Num, but now
/// we support different operators as separate classes.
fn check_num_constraint(constraint: &ClassConstraint, ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::BigInt | Prim::Double | Prim::Int) => true,
        Ty::Udt(_, Res::Item(id))
            if *id == ItemId::complex()
                && !matches!(constraint, ClassConstraint::Ord | ClassConstraint::Mod) =>
        {
            true
        }
        Ty::Param { bounds, .. } => {
            // check if the bounds contain Num
            bounds.0.contains(constraint)
        }
        _ => false,
    }
}

fn check_show(ty: Ty, span: Span) -> (Vec<Constraint>, Vec<Error>) {
    match ty {
        Ty::Array(item) => (
            vec![Constraint::Class(Class::Show(*item), span)],
            Vec::new(),
        ),
        Ty::Prim(_) => (Vec::new(), Vec::new()),
        Ty::Tuple(items) => (
            items
                .into_iter()
                .map(|item| Constraint::Class(Class::Show(item), span))
                .collect(),
            Vec::new(),
        ),
        Ty::Param { ref bounds, .. } => {
            // check if the bounds contain Show
            match bounds
                .0
                .iter()
                .find(|bound| matches!(bound, ClassConstraint::Show))
            {
                Some(_) => (Vec::new(), Vec::new()),
                None => (
                    Vec::new(),
                    vec![Error(ErrorKind::MissingClassShow(ty.display(), span))],
                ),
            }
        }
        _ => (
            Vec::new(),
            vec![Error(ErrorKind::MissingClassShow(ty.display(), span))],
        ),
    }
}

fn check_unwrap(
    udts: &FxHashMap<ItemId, Udt>,
    wrapper: &Ty,
    base: Ty,
    span: Span,
) -> (Vec<Constraint>, Vec<Error>) {
    if let Ty::Udt(_, Res::Item(id)) = wrapper {
        if let Some(udt) = udts.get(id) {
            return (
                vec![Constraint::Eq {
                    expected: base,
                    actual: id.package.map_or_else(
                        || udt.get_pure_ty(),
                        |package_id| udt.get_pure_ty().with_package(package_id),
                    ),
                    span,
                }],
                Vec::new(),
            );
        }
    }

    (
        Vec::new(),
        vec![Error(ErrorKind::MissingClassUnwrap(
            wrapper.display(),
            span,
        ))],
    )
}

/// Given an HIR class constraint, produce an actual type system constraint.
fn into_constraint(ty: Ty, bound: &ClassConstraint, span: Span) -> Constraint {
    match bound {
        ClassConstraint::Eq => Constraint::Class(Class::Eq(ty), span),
        ClassConstraint::Exp { power } => Constraint::Class(
            Class::Exp {
                // `ty` here is basically `Self` -- so Exp[Double] is a type that can be raised to
                // the power of a double.
                // Exponentiation is a _closed_ operation, meaning the domain and codomain are the
                // same.
                base: ty.clone(),
                power: power.clone(),
            },
            span,
        ),
        ClassConstraint::Add => Constraint::Class(Class::Add(ty), span),
        ClassConstraint::Iterable { item } => Constraint::Class(
            Class::Iterable {
                item: item.clone(),
                container: ty.clone(),
            },
            span,
        ),
        ClassConstraint::NonNativeClass(name) => {
            Constraint::Class(Class::NonPrimitive(name.clone()), span)
        }
        ClassConstraint::Show => Constraint::Class(Class::Show(ty), span),
        ClassConstraint::Integral => Constraint::Class(Class::Integral(ty), span),
        ClassConstraint::Ord => Constraint::Class(Class::Ord(ty), span),
        ClassConstraint::Mul => Constraint::Class(Class::Mul(ty), span),
        ClassConstraint::Div => Constraint::Class(Class::Div(ty), span),
        ClassConstraint::Sub => Constraint::Class(Class::Sub(ty), span),
        ClassConstraint::Signed => Constraint::Class(Class::Signed(ty), span),
        ClassConstraint::Mod => Constraint::Class(Class::Mod(ty), span),
    }
}
