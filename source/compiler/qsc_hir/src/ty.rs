// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indenter::{Indented, indented};
use qsc_data_structures::span::Span;
use rustc_hash::FxHashMap;

use crate::hir::{CallableKind, FieldPath, Functor, ItemId, PackageId, Res};
use std::{
    cell::RefCell,
    fmt::{self, Debug, Display, Formatter, Write},
    rc::Rc,
};

fn set_indentation<'a, 'b>(
    indent: Indented<'a, Formatter<'b>>,
    level: usize,
) -> Indented<'a, Formatter<'b>> {
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        _ => unimplemented!("indentation level not supported"),
    }
}

/// A type.
#[derive(Clone, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub enum Ty {
    /// An array type.
    Array(Box<Ty>),
    /// An arrow type: `->` for a function or `=>` for an operation.
    Arrow(Rc<Arrow>),
    /// A placeholder type variable used during type inference.
    Infer(InferTyId),
    /// A type parameter.
    Param {
        name: Rc<str>,
        id: ParamId,
        bounds: ClassConstraints,
    },
    /// A primitive type.
    Prim(Prim),
    /// A tuple type.
    Tuple(Vec<Ty>),
    /// A user-defined type.
    Udt(Rc<str>, Res),
    /// An invalid type.
    #[default]
    Err,
}

/// Container type for a collection of class constraints, so we can define methods on it.
#[derive(Clone, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub struct ClassConstraints(pub Box<[ClassConstraint]>);

impl ClassConstraints {
    #[must_use]
    pub fn contains_iterable_bound(&self) -> bool {
        self.0
            .iter()
            .any(|bound| matches!(bound, ClassConstraint::Iterable { .. }))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for ClassConstraints {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            Ok(())
        } else {
            let bounds = self
                .0
                .iter()
                .map(|bound| format!("{bound}"))
                .collect::<Vec<_>>()
                .join(" + ");
            write!(f, "{bounds}")
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ClassConstraint {
    /// Whether or not 'T can be compared via Eq to values of the same domain.
    Eq,
    /// Whether or not 'T can be added to values of the same domain via the + operator.
    Add,
    Exp {
        // `base` is inferred to be the self type
        power: Ty,
    },
    /// If 'T is iterable, then it can be iterated over and the items inside are yielded (of type `item`).
    Iterable { item: Ty },
    /// Whether or not 'T can be divided by values of the same domain via the / operator.
    Div,
    /// Whether or not 'T can be subtracted from values of the same domain via the - operator.
    Sub,
    /// Whether or not 'T can be multiplied by values of the same domain via the * operator.
    Mul,
    /// Whether or not 'T can be taken modulo values of the same domain via the % operator.
    Mod,
    /// Whether or not 'T can be compared via Ord to values of the same domain.
    Ord,
    /// Whether or not 'T can be signed.
    Signed,
    /// Whether or not 'T is an integral type (can be used in bit shifting operators).
    Integral,
    /// Whether or not 'T can be displayed as a string (converted to a string).
    Show,
    /// A class that is not built-in to the compiler.
    NonNativeClass(Rc<str>),
}

impl std::fmt::Display for ClassConstraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ClassConstraint::Eq => write!(f, "Eq"),
            ClassConstraint::NonNativeClass(name) => write!(f, "{name}"),
            ClassConstraint::Add => write!(f, "Add"),
            ClassConstraint::Exp { power } => write!(f, "Exp[{power}]"),
            ClassConstraint::Iterable { item } => write!(f, "Iterable<{item}>"),
            ClassConstraint::Integral => write!(f, "Integral"),
            ClassConstraint::Show => write!(f, "Show"),
            ClassConstraint::Div => write!(f, "Div"),
            ClassConstraint::Sub => write!(f, "Sub"),
            ClassConstraint::Mul => write!(f, "Mul"),
            ClassConstraint::Mod => write!(f, "Mod"),
            ClassConstraint::Ord => write!(f, "Ord"),
            ClassConstraint::Signed => write!(f, "Signed"),
        }
    }
}

impl Ty {
    /// The unit type.
    pub const UNIT: Self = Self::Tuple(Vec::new());

    #[must_use]
    pub fn with_package(&self, package: PackageId) -> Self {
        match self {
            Ty::Infer(_) | Ty::Param { .. } | Ty::Prim(_) | Ty::Err => self.clone(),
            Ty::Array(item) => Ty::Array(Box::new(item.with_package(package))),
            Ty::Arrow(arrow) => Ty::Arrow(Rc::new(arrow.with_package(package))),
            Ty::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|item| item.with_package(package))
                    .collect(),
            ),
            Ty::Udt(name, res) => Ty::Udt(name.clone(), *res),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::Array(item) => {
                format!("{}[]", item.display())
            }
            Ty::Arrow(arrow) => {
                let arrow_symbol = match arrow.kind {
                    CallableKind::Function => "->",
                    CallableKind::Operation => "=>",
                };

                let functors = match *arrow.functors.borrow() {
                    FunctorSet::Value(FunctorSetValue::Empty)
                    | FunctorSet::Param(_, FunctorSetValue::Empty) => String::new(),
                    FunctorSet::Value(_) | FunctorSet::Infer(_) => {
                        format!(" is {}", arrow.functors.borrow())
                    }
                    FunctorSet::Param(_, functors) => {
                        format!(" is {functors}")
                    }
                };
                format!(
                    "({} {arrow_symbol} {}{functors})",
                    arrow.input.borrow().display(),
                    arrow.output.borrow().display()
                )
            }
            Ty::Infer(_) | Ty::Err => "?".to_string(),
            Ty::Param { name, .. } | Ty::Udt(name, _) => name.to_string(),
            Ty::Prim(prim) => format!("{prim:?}"),
            Ty::Tuple(items) => {
                if items.is_empty() {
                    "Unit".to_string()
                } else if items.len() == 1 {
                    let item = items.first().expect("expected single item");
                    format!("({},)", item.display())
                } else {
                    let items = items.iter().map(Ty::display).collect::<Vec<_>>().join(", ");
                    format!("({items})")
                }
            }
        }
    }

    /// Calculates the size of the type, which represents its structural complexity.
    /// This is used to avoid "large" types that can result in hangs during type inference.
    pub fn size(&self) -> usize {
        match self {
            Ty::Array(item) => item.size() + 1,
            Ty::Arrow(arrow) => arrow.input.borrow().size() + arrow.output.borrow().size() + 1,
            Ty::Infer(_) | Ty::Err | Ty::Prim(_) | Ty::Param { .. } | Ty::Udt(_, _) => 1,
            Ty::Tuple(items) => items.iter().map(Ty::size).sum::<usize>() + 1,
        }
    }
}

impl Display for Ty {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Ty::Array(item) => write!(f, "{item}[]"),
            Ty::Arrow(arrow) => Display::fmt(arrow, f),
            Ty::Infer(infer) => Display::fmt(infer, f),
            Ty::Param { name, id, .. } => {
                write!(f, "Param<\"{name}\": {id}>")
            }
            Ty::Prim(prim) => Debug::fmt(prim, f),
            Ty::Tuple(items) => {
                if items.is_empty() {
                    f.write_str("Unit")
                } else {
                    f.write_char('(')?;
                    if let Some((first, rest)) = items.split_first() {
                        Display::fmt(first, f)?;
                        if rest.is_empty() {
                            f.write_char(',')?;
                        } else {
                            for item in rest {
                                f.write_str(", ")?;
                                Display::fmt(item, f)?;
                            }
                        }
                    }
                    f.write_str(")")
                }
            }
            Ty::Udt(name, res) => {
                write!(f, "UDT<\"{name}\": {res}>")
            }
            Ty::Err => f.write_char('?'),
        }
    }
}

#[derive(Debug)]
/// A type scheme.
pub struct Scheme {
    params: Vec<TypeParameter>,
    ty: Box<Arrow>,
}

impl Scheme {
    /// Creates a new type scheme.
    #[must_use]
    pub fn new(params: Vec<TypeParameter>, ty: Box<Arrow>) -> Self {
        Self { params, ty }
    }

    #[must_use]
    pub fn with_package(&self, package: PackageId) -> Self {
        Self {
            params: self.params.clone(),
            ty: Box::new(Arrow {
                kind: self.ty.kind,
                input: RefCell::new(self.ty.input.borrow().with_package(package)),
                output: RefCell::new(self.ty.output.borrow().with_package(package)),
                functors: self.ty.functors.clone(),
            }),
        }
    }

    /// The generic parameters to the type.
    #[must_use]
    pub fn params(&self) -> &[TypeParameter] {
        &self.params
    }

    /// Instantiates this type scheme with the given arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if the given arguments do not match the scheme parameters.
    pub fn instantiate(&self, args: &[GenericArg]) -> Result<Arrow, InstantiationError> {
        if args.len() == self.params.len() {
            let args: FxHashMap<_, _> = self
                .params
                .iter()
                .enumerate()
                .map(|(ix, _)| ParamId::from(ix))
                .zip(args)
                .collect();
            instantiate_arrow_ty(|name| args.get(name).copied(), &self.ty)
        } else {
            Err(InstantiationError::Arity)
        }
    }
}

/// A type scheme instantiation error.
#[derive(Debug)]
pub enum InstantiationError {
    /// The number of generic arguments does not match the number of generic parameters.
    Arity,
    /// A generic argument does not match the kind of its corresponding generic parameter.
    Kind(ParamId),
    /// An in invalid type bound was provided.
    Bound(ParamId),
}

fn instantiate_ty<'a>(
    arg: impl Fn(&ParamId) -> Option<&'a GenericArg> + Copy,
    ty: &Ty,
) -> Result<Ty, InstantiationError> {
    match ty {
        Ty::Err | Ty::Infer(_) | Ty::Prim(_) | Ty::Udt(_, _) => Ok(ty.clone()),
        Ty::Array(item) => Ok(Ty::Array(Box::new(instantiate_ty(arg, item)?))),
        Ty::Arrow(arrow) => Ok(Ty::Arrow(Rc::new(instantiate_arrow_ty(arg, arrow)?))),
        Ty::Param { id, .. } => match arg(id) {
            Some(GenericArg::Ty(ty_arg)) => Ok(ty_arg.clone()),
            Some(_) => Err(InstantiationError::Kind(*id)),
            None => Ok(ty.clone()),
        },
        Ty::Tuple(items) => Ok(Ty::Tuple(
            items
                .iter()
                .map(|item| instantiate_ty(arg, item))
                .collect::<Result<_, _>>()?,
        )),
    }
}

fn instantiate_arrow_ty<'a>(
    arg: impl Fn(&ParamId) -> Option<&'a GenericArg> + Copy,
    arrow: &Arrow,
) -> Result<Arrow, InstantiationError> {
    let input = instantiate_ty(arg, &arrow.input.borrow())?;
    let output = instantiate_ty(arg, &arrow.output.borrow())?;
    let functors = if let FunctorSet::Param(param, _) = *arrow.functors.borrow() {
        match arg(&param) {
            Some(GenericArg::Functor(functor_arg)) => *functor_arg,
            Some(_) => return Err(InstantiationError::Kind(param)),
            None => *arrow.functors.borrow(),
        }
    } else {
        *arrow.functors.borrow()
    };

    Ok(Arrow {
        kind: arrow.kind,
        input: RefCell::new(input),
        output: RefCell::new(output),
        functors: RefCell::new(functors),
    })
}

impl Display for TypeParameter {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TypeParameter::Ty { name, bounds, .. } => write!(
                f,
                "type {name}{}",
                if bounds.0.is_empty() {
                    String::new()
                } else {
                    format!(" bounds: {bounds}",)
                }
            ),
            TypeParameter::Functor(min) => write!(f, "functor ({min})"),
        }
    }
}

/// The kind of a generic parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeParameter {
    /// A type parameter.
    Ty {
        name: Rc<str>,
        bounds: ClassConstraints,
    },
    /// A functor parameter with a minimal set (lower bound) of functors.
    /// if `'T is Adj` then `functor ('T)` is the minimal set of functors.
    /// This can currently only occur on lambda expressions.
    Functor(FunctorSetValue),
}

/// The name of a generic type parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamName {
    /// The span.
    pub span: Span,
    /// The name.
    pub name: Rc<str>,
}

impl Display for TypeParamName {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{} \"{}\"", self.span, self.name)
    }
}

/// A generic parameter ID.
#[derive(Clone, Copy, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ParamId(u32);

impl ParamId {
    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl From<usize> for ParamId {
    fn from(value: usize) -> Self {
        ParamId(
            value
                .try_into()
                .expect("Type Parameter ID does not fit into u32"),
        )
    }
}

impl From<ParamId> for usize {
    fn from(value: ParamId) -> Self {
        value.0 as usize
    }
}

impl Display for ParamId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// An argument to a generic parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericArg {
    /// A type argument.
    Ty(Ty),
    /// A functor argument.
    Functor(FunctorSet),
}

impl Display for GenericArg {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            GenericArg::Ty(ty) => Display::fmt(ty, f),
            GenericArg::Functor(functors) => Display::fmt(functors, f),
        }
    }
}

/// An arrow type: `->` for a function or `=>` for an operation.
#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Arrow {
    /// Whether the callable is a function or an operation.
    pub kind: CallableKind,
    /// The input type to the callable.
    pub input: RefCell<Ty>,
    /// The output type from the callable.
    pub output: RefCell<Ty>,
    /// The functors supported by the callable.
    pub functors: RefCell<FunctorSet>,
}

impl Arrow {
    #[must_use]
    pub fn with_package(&self, package: PackageId) -> Self {
        Self {
            kind: self.kind,
            input: RefCell::new(self.input.borrow().with_package(package)),
            output: RefCell::new(self.output.borrow().with_package(package)),
            functors: self.functors.clone(),
        }
    }
}

impl Display for Arrow {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let arrow = match self.kind {
            CallableKind::Function => "->",
            CallableKind::Operation => "=>",
        };
        write!(
            f,
            "({} {arrow} {}",
            self.input.borrow(),
            self.output.borrow()
        )?;
        if self.functors != RefCell::new(FunctorSet::Value(FunctorSetValue::Empty)) {
            f.write_str(" is ")?;
            Display::fmt(&self.functors.borrow(), f)?;
        }
        f.write_char(')')
    }
}

/// A primitive type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum Prim {
    /// The big integer type.
    BigInt,
    /// The boolean type.
    Bool,
    /// The floating-point type.
    Double,
    /// The integer type.
    Int,
    /// The Pauli operator type.
    Pauli,
    /// The qubit type.
    Qubit,
    /// The range type.
    Range,
    /// The range type without a lower bound.
    RangeTo,
    /// The range type without an upper bound.
    RangeFrom,
    /// The range type without lower and upper bounds.
    RangeFull,
    /// The measurement result type.
    Result,
    /// The string type.
    String,
}

/// A set of functors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FunctorSet {
    /// An evaluated set.
    Value(FunctorSetValue),
    /// A functor parameter.
    Param(ParamId, FunctorSetValue),
    /// A placeholder functor variable used during type inference.
    Infer(InferFunctorId),
}

impl FunctorSet {
    /// Returns the contained value.
    ///
    /// # Panics
    ///
    /// Panics if this set does not have a value.
    #[must_use]
    pub fn expect_value(self, msg: &str) -> FunctorSetValue {
        match self {
            Self::Value(value) | Self::Param(_, value) => value,
            Self::Infer(_) => panic!("{msg}"),
        }
    }
}

impl Display for FunctorSet {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Value(value) => Display::fmt(value, f),
            Self::Param(param, _) => write!(f, "Param<{param}>"),
            Self::Infer(infer) => Display::fmt(infer, f),
        }
    }
}

/// The value of a functor set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub enum FunctorSetValue {
    /// The empty set.
    #[default]
    Empty,
    /// The singleton adjoint set.
    Adj,
    /// The singleton controlled set.
    Ctl,
    /// The set of controlled and adjoint.
    CtlAdj,
}

impl FunctorSetValue {
    /// True if this set contains the functor.
    #[must_use]
    pub fn contains(&self, functor: &Functor) -> bool {
        match self {
            Self::Empty => false,
            Self::Adj => matches!(functor, Functor::Adj),
            Self::Ctl => matches!(functor, Functor::Ctl),
            Self::CtlAdj => matches!(functor, Functor::Adj | Functor::Ctl),
        }
    }

    /// The intersection of this set and another set.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, _)
            | (_, Self::Empty)
            | (Self::Adj, Self::Ctl)
            | (Self::Ctl, Self::Adj) => Self::Empty,
            (Self::Adj, Self::Adj) => Self::Adj,
            (Self::Ctl, Self::Ctl) => Self::Ctl,
            (Self::CtlAdj, &set) | (&set, Self::CtlAdj) => set,
        }
    }

    /// The union of this set and another set.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, &set) | (&set, Self::Empty) => set,
            (Self::Adj, Self::Adj) => Self::Adj,
            (Self::Ctl, Self::Ctl) => Self::Ctl,
            (Self::CtlAdj, _)
            | (_, Self::CtlAdj)
            | (Self::Adj, Self::Ctl)
            | (Self::Ctl, Self::Adj) => Self::CtlAdj,
        }
    }

    #[must_use]
    pub fn satisfies(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (_, Self::Empty)
                | (Self::Adj | Self::CtlAdj, Self::Adj)
                | (Self::Ctl | Self::CtlAdj, Self::Ctl)
                | (Self::CtlAdj, Self::CtlAdj)
        )
    }
}

impl Display for FunctorSetValue {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty set"),
            Self::Adj => f.write_str("Adj"),
            Self::Ctl => f.write_str("Ctl"),
            Self::CtlAdj => f.write_str("Adj + Ctl"),
        }
    }
}

/// The item for a user-defined type.
#[derive(Clone, Debug, PartialEq)]
pub struct Udt {
    /// The span.
    pub span: Span,
    /// The name.
    pub name: Rc<str>,
    // The definition.
    pub definition: UdtDef,
}

impl Udt {
    #[must_use]
    pub fn get_pure_ty(&self) -> Ty {
        fn get_pure_ty(def: &UdtDef) -> Ty {
            match &def.kind {
                UdtDefKind::Field(field) => field.ty.clone(),
                UdtDefKind::Tuple(tup) => Ty::Tuple(tup.iter().map(get_pure_ty).collect()),
            }
        }
        get_pure_ty(&self.definition)
    }

    /// The type scheme of the constructor for this type definition.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the constructed type.
    #[must_use]
    pub fn cons_scheme(&self, id: ItemId) -> Scheme {
        Scheme {
            params: Vec::new(),
            ty: Box::new(Arrow {
                kind: CallableKind::Function,
                input: RefCell::new(self.get_pure_ty()),
                output: RefCell::new(Ty::Udt(self.name.clone(), Res::Item(id))),
                functors: RefCell::new(FunctorSet::Value(FunctorSetValue::Empty)),
            }),
        }
    }

    /// The path to the field with the given name. Returns [None] if this user-defined type does not
    /// have a field with the given name.
    #[must_use]
    pub fn field_path(&self, name: &str) -> Option<FieldPath> {
        Self::find_field_path(&self.definition, name)
    }

    fn find_field_path(def: &UdtDef, name: &str) -> Option<FieldPath> {
        match &def.kind {
            UdtDefKind::Field(field) => field.name.as_ref().and_then(|field_name| {
                if field_name.as_ref() == name {
                    Some(FieldPath::default())
                } else {
                    None
                }
            }),
            UdtDefKind::Tuple(defs) => defs.iter().enumerate().find_map(|(i, def)| {
                Self::find_field_path(def, name).map(|mut path| {
                    path.indices.insert(0, i);
                    path
                })
            }),
        }
    }

    /// The field at the given path. Returns [None] if the path is not valid for this
    /// user-defined type.
    #[must_use]
    pub fn find_field(&self, path: &FieldPath) -> Option<&UdtField> {
        let mut udt_def = &self.definition;
        for &index in &path.indices {
            let UdtDefKind::Tuple(items) = &udt_def.kind else {
                return None;
            };
            udt_def = &items[index];
        }
        let UdtDefKind::Field(field) = &udt_def.kind else {
            return None;
        };
        Some(field)
    }

    /// The field with the given name. Returns [None] if this user-defined type does not
    /// have a field with the given name.
    #[must_use]
    pub fn find_field_by_name(&self, name: &str) -> Option<&UdtField> {
        Self::find_field_by_name_rec(&self.definition, name)
    }

    fn find_field_by_name_rec<'a>(def: &'a UdtDef, name: &str) -> Option<&'a UdtField> {
        match &def.kind {
            UdtDefKind::Field(field) => field.name.as_ref().and_then(|field_name| {
                if field_name.as_ref() == name {
                    Some(field)
                } else {
                    None
                }
            }),
            UdtDefKind::Tuple(defs) => defs
                .iter()
                .find_map(|def| Self::find_field_by_name_rec(def, name)),
        }
    }

    /// The type of the field at the given path. Returns [None] if the path is not valid for this
    /// user-defined type.
    #[must_use]
    pub fn field_ty(&self, path: &FieldPath) -> Option<&Ty> {
        self.find_field(path).map(|field| &field.ty)
    }

    /// The type of the field with the given name. Returns [None] if this user-defined type does not
    /// have a field with the given name.
    #[must_use]
    pub fn field_ty_by_name(&self, name: &str) -> Option<&Ty> {
        self.find_field_by_name(name).map(|field| &field.ty)
    }

    /// Returns true if the udt satisfies the conditions for a struct.
    /// Conditions for a struct are that the udt is a tuple with all its top-level fields named.
    /// Otherwise, returns false.
    #[must_use]
    pub fn is_struct(&self) -> bool {
        match &self.definition.kind {
            UdtDefKind::Field(_) => false,
            UdtDefKind::Tuple(fields) => fields.iter().all(|field| match &field.kind {
                UdtDefKind::Field(field) => {
                    if let (Some(name), Some(_)) = (&field.name, &field.name_span) {
                        !name.is_empty()
                    } else {
                        false
                    }
                }
                UdtDefKind::Tuple(_) => false,
            }),
        }
    }
}

impl Display for Udt {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "UDT {}:", self.span)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\n{}", self.definition)?;
        Ok(())
    }
}

/// A UDT type definition.
#[derive(Clone, Debug, PartialEq)]
pub struct UdtDef {
    /// The span.
    pub span: Span,
    /// The type definition kind.
    pub kind: UdtDefKind,
}

impl Display for UdtDef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "TyDef {}: {}", self.span, self.kind)
    }
}

/// A UDT type definition kind.
#[derive(Clone, Debug, PartialEq)]
pub enum UdtDefKind {
    /// A field definition with an optional name but required type.
    Field(UdtField),
    /// A tuple.
    Tuple(Vec<UdtDef>),
}

impl Display for UdtDefKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        match &self {
            UdtDefKind::Field(field) => {
                write!(indent, "Field:")?;
                indent = set_indentation(indent, 1);
                write!(indent, "{field}")?;
            }
            UdtDefKind::Tuple(ts) => {
                if ts.is_empty() {
                    write!(indent, "Unit")?;
                } else {
                    write!(indent, "Tuple:")?;
                    indent = set_indentation(indent, 1);
                    for t in ts {
                        write!(indent, "\n{t}")?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// A user-defined type.
#[derive(Clone, Debug, PartialEq)]
pub struct UdtField {
    /// The span of the field name.
    pub name_span: Option<Span>,
    /// The field name.
    pub name: Option<Rc<str>>,
    // The field type.
    pub ty: Ty,
}

impl Display for UdtField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(n) = &self.name
            && let Some(s) = &self.name_span
        {
            write!(f, "\nname: {n} {s}")?;
        }
        write!(f, "\ntype: {}", self.ty)?;
        Ok(())
    }
}

/// A placeholder type variable used during type inference.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct InferTyId(usize);

impl InferTyId {
    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for InferTyId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "?{}", self.0)
    }
}

impl From<usize> for InferTyId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<InferTyId> for usize {
    fn from(value: InferTyId) -> Self {
        value.0
    }
}

/// A placeholder functor variable used during type inference.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InferFunctorId(usize);

impl InferFunctorId {
    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for InferFunctorId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "f?{}", self.0)
    }
}

impl From<usize> for InferFunctorId {
    fn from(value: usize) -> Self {
        InferFunctorId(value)
    }
}

impl From<InferFunctorId> for usize {
    fn from(value: InferFunctorId) -> Self {
        value.0
    }
}
