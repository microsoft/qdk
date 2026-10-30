// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checked callable instantiations retained independently of runtime values.

#[cfg(test)]
mod tests;

use qsc_fir::{
    fir::StoreItemId,
    ty::{
        Arrow, FunctorSet, GenericArg, MAX_TYPE_DEPTH, MAX_TYPE_NODES, ParamId, Ty, TypeParameter,
    },
};
use std::{fmt, rc::Rc};
use thiserror::Error;

/// Type arguments are templates in the caller's lexical context. Keeping that
/// context shared avoids expanding recursive types during simulation.
#[derive(Clone, Default, PartialEq)]
pub enum TypeArgs {
    /// A declaration supplied without a checked source instantiation.
    #[default]
    Unbound,
    /// A checked callable with no generic parameters.
    Empty,
    Bound(Rc<Instantiation>),
}

/// An immutable binding environment owned by one callable declaration.
#[derive(Clone, PartialEq)]
pub struct Instantiation {
    pub owner: StoreItemId,
    pub args: Rc<[GenericArg]>,
    pub context: TypeArgs,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TypeArgsError {
    #[error("missing checked type argument {0}")]
    Missing(ParamId),
    #[error("checked type argument {0} has the wrong kind")]
    Kind(ParamId),
    #[error("callable type arguments are unresolved")]
    Unresolved,
    #[error("callable type expansion exceeds {MAX_TYPE_NODES} nodes or depth {MAX_TYPE_DEPTH}")]
    ExpansionLimit,
}

/// Shared budget for materializing one type or a forest of type arguments.
#[derive(Default)]
pub struct TypeBudget {
    nodes: usize,
}

impl TypeArgs {
    #[must_use]
    pub fn bind(owner: StoreItemId, args: Vec<GenericArg>, context: Self) -> Self {
        if args.is_empty() {
            Self::Empty
        } else {
            Self::Bound(Rc::new(Instantiation {
                owner,
                args: args.into(),
                context,
            }))
        }
    }

    /// Native callers cannot infer type parameters. Their implicit functor
    /// parameters can use the declaration's checked capability bounds.
    #[must_use]
    pub fn from_decl(owner: StoreItemId, parameters: &[TypeParameter]) -> Self {
        let Some(args) = parameters
            .iter()
            .map(|parameter| match parameter {
                TypeParameter::Functor(bound) => {
                    Some(GenericArg::Functor(FunctorSet::Value(*bound)))
                }
                TypeParameter::Ty { .. } => None,
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Self::Unbound;
        };
        Self::bind(owner, args, Self::Empty)
    }

    fn argument(&self, param: ParamId) -> Result<(&GenericArg, &Self), TypeArgsError> {
        let Self::Bound(binding) = self else {
            return Err(TypeArgsError::Missing(param));
        };
        binding
            .args
            .iter()
            .enumerate()
            .find_map(|(index, arg)| {
                (ParamId::from(index) == param).then_some((arg, &binding.context))
            })
            .ok_or(TypeArgsError::Missing(param))
    }

    pub fn resolve_argument(
        &self,
        param: ParamId,
        budget: &mut TypeBudget,
    ) -> Result<GenericArg, TypeArgsError> {
        let (arg, context) = self.argument(param)?;
        match arg {
            GenericArg::Ty(ty) => context.resolve_ty(ty, budget).map(GenericArg::Ty),
            GenericArg::Functor(functors) => {
                context.resolve_functors(*functors).map(GenericArg::Functor)
            }
        }
    }

    pub fn resolve_functors(&self, mut functors: FunctorSet) -> Result<FunctorSet, TypeArgsError> {
        let mut context = self;
        while let FunctorSet::Param(param) = functors {
            let (argument, caller) = context.argument(param)?;
            let GenericArg::Functor(value) = argument else {
                return Err(TypeArgsError::Kind(param));
            };
            functors = *value;
            context = caller;
        }
        match functors {
            FunctorSet::Value(_) => Ok(functors),
            _ => Err(TypeArgsError::Unresolved),
        }
    }

    pub fn resolve_ty(&self, ty: &Ty, budget: &mut TypeBudget) -> Result<Ty, TypeArgsError> {
        self.resolve_ty_at_depth(ty, budget, 1)
    }

    fn resolve_ty_at_depth<'a>(
        &'a self,
        mut ty: &'a Ty,
        budget: &mut TypeBudget,
        depth: usize,
    ) -> Result<Ty, TypeArgsError> {
        let mut context = self;
        while let Ty::Param(param) = ty {
            let (argument, caller) = context.argument(*param)?;
            let GenericArg::Ty(value) = argument else {
                return Err(TypeArgsError::Kind(*param));
            };
            ty = value;
            context = caller;
        }
        if depth > MAX_TYPE_DEPTH || budget.nodes == MAX_TYPE_NODES {
            return Err(TypeArgsError::ExpansionLimit);
        }
        budget.nodes += 1;
        match ty {
            Ty::Prim(_) | Ty::Udt(_) => Ok(ty.clone()),
            Ty::Array(item) => Ok(Ty::Array(Box::new(context.resolve_ty_at_depth(
                item,
                budget,
                depth + 1,
            )?))),
            Ty::Tuple(items) => {
                if items.len() > MAX_TYPE_NODES - budget.nodes {
                    return Err(TypeArgsError::ExpansionLimit);
                }
                items
                    .iter()
                    .map(|item| context.resolve_ty_at_depth(item, budget, depth + 1))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Ty::Tuple)
            }
            Ty::Arrow(arrow) => Ok(Ty::Arrow(Box::new(Arrow {
                kind: arrow.kind,
                input: Box::new(context.resolve_ty_at_depth(&arrow.input, budget, depth + 1)?),
                output: Box::new(context.resolve_ty_at_depth(&arrow.output, budget, depth + 1)?),
                functors: context.resolve_functors(arrow.functors)?,
            }))),
            Ty::Param(_) | Ty::Infer(_) | Ty::Err => Err(TypeArgsError::Unresolved),
        }
    }
}

impl fmt::Debug for TypeArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unbound => f.write_str("Unbound"),
            Self::Empty => f.write_str("Empty"),
            Self::Bound(binding) => f
                .debug_struct("TypeArgs")
                .field("owner", &binding.owner)
                .field("args", &binding.args)
                .finish_non_exhaustive(),
        }
    }
}
