// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_ast::ast::{self, Idents, TypeParameter as AstTypeParameter};
use qsc_frontend::resolve;
use qsc_hir::{
    hir,
    ty::{self, TypeParameter as HirTypeParameter},
};
use regex_lite::Regex;
use std::{
    fmt::{Display, Formatter, Result},
    rc::Rc,
};

/// Trait describing a struct capable of resolving various ids found in the AST and HIR.
pub trait Lookup {
    /// Looks up the type of a node in user code
    fn get_ty(&self, expr_id: ast::NodeId) -> Option<&ty::Ty>;

    /// Looks up the resolution of a node in user code
    fn get_res(&self, id: ast::NodeId) -> Option<&resolve::Res>;

    /// Returns the hir `Item` node referred to by `item_id`,
    /// along with the `Package` and `PackageId` for the package
    /// that it was found in.
    fn resolve_item_relative_to_user_package(
        &self,
        item_id: &hir::ItemId,
    ) -> (&hir::Item, &hir::Package, hir::ItemId);

    /// Returns the hir `Item` node referred to by `res`.
    /// `Res`s can resolve to external packages, and the references
    /// are relative, so here we also need the
    /// local `PackageId` that the `res` itself came from.
    fn resolve_item_res(&self, res: &hir::Res) -> (&hir::Item, hir::ItemId);

    /// Returns the hir `Item` node referred to by `item_id`.
    /// `ItemId`s can refer to external packages, and the references
    /// are relative, so here we also need the local `PackageId`
    /// that the `ItemId` originates from.
    fn resolve_item(&self, item_id: &hir::ItemId) -> (&hir::Item, &hir::Package, hir::ItemId);
}

pub struct CodeDisplay<'a> {
    pub compilation: &'a dyn Lookup,
}

#[allow(clippy::unused_self)]
impl<'a> CodeDisplay<'a> {
    #[must_use]
    pub fn hir_callable_decl(&self, decl: &'a hir::CallableDecl) -> impl Display + '_ {
        HirCallableDecl { decl }
    }

    #[must_use]
    pub fn ast_callable_decl(&self, decl: &'a ast::CallableDecl) -> impl Display + '_ {
        AstCallableDecl {
            lookup: self.compilation,
            decl,
        }
    }

    #[must_use]
    pub fn name_ty_id(&self, name: &'a str, ty_id: ast::NodeId) -> impl Display + '_ {
        NameTyId {
            lookup: self.compilation,
            name,
            ty_id,
        }
    }

    #[must_use]
    pub fn ident_ty(&self, ident: &'a ast::Ident, ty: &'a ast::Ty) -> impl Display + '_ {
        IdentTy { ident, ty }
    }

    #[must_use]
    pub fn ident_ty_def(&self, ident: &'a ast::Ident, def: &'a ast::TyDef) -> impl Display + 'a {
        IdentTyDef { ident, def }
    }

    #[must_use]
    pub fn struct_decl(&self, decl: &'a ast::StructDecl) -> impl Display + 'a {
        StructDecl { decl }
    }

    #[must_use]
    pub fn hir_udt_field(&self, field: &'a ty::UdtField) -> impl Display + '_ {
        HirUdtField { field }
    }

    #[must_use]
    pub fn hir_udt(&self, udt: &'a ty::Udt) -> impl Display + '_ {
        HirUdt::new(udt)
    }

    #[must_use]
    pub fn hir_pat(&self, pat: &'a hir::Pat) -> impl Display + '_ {
        HirPat { pat }
    }

    #[must_use]
    pub fn get_param_offset(&self, decl: &hir::CallableDecl) -> u32 {
        HirCallableDecl { decl }.get_param_offset()
    }

    // The rest of the display implementations are not made public b/c they're not used,
    // but there's no reason they couldn't be
}

// Display impls for each syntax/hir element we may encounter

struct IdentTy<'a> {
    ident: &'a ast::Ident,
    ty: &'a ast::Ty,
}

impl Display for IdentTy<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{} : {}", self.ident.name, AstTy { ty: self.ty },)
    }
}

struct NameTyId<'a> {
    lookup: &'a dyn Lookup,
    name: &'a str,
    ty_id: ast::NodeId,
}

impl Display for NameTyId<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "{} : {}",
            self.name,
            TyId {
                lookup: self.lookup,
                ty_id: self.ty_id,
            },
        )
    }
}

struct HirCallableDecl<'a> {
    decl: &'a hir::CallableDecl,
}

impl HirCallableDecl<'_> {
    fn get_param_offset(&self) -> u32 {
        let offset = match self.decl.kind {
            hir::CallableKind::Function => "function".len(),
            hir::CallableKind::Operation => "operation".len(),
        } + 1 // this is for the space between keyword and name
        + self.decl.name.name.len()
        + display_type_params(&self.decl.generics).len();

        u32::try_from(offset)
            .expect("failed to cast usize to u32 while calculating parameter offset")
    }
}

impl Display for HirCallableDecl<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let kind = match self.decl.kind {
            hir::CallableKind::Function => "function",
            hir::CallableKind::Operation => "operation",
        };

        write!(f, "{} {}", kind, self.decl.name.name)?;
        let type_params = display_type_params(&self.decl.generics);
        write!(f, "{type_params}")?;
        let input = HirPat {
            pat: &self.decl.input,
        };
        if matches!(self.decl.input.kind, hir::PatKind::Tuple(_)) {
            write!(f, "{input}")?;
        } else {
            write!(f, "({input})")?;
        }
        write!(
            f,
            " : {}{}",
            self.decl.output.display(),
            FunctorSetValue {
                functors: self.decl.functors,
            },
        )
    }
}

struct AstCallableDecl<'a> {
    lookup: &'a dyn Lookup,
    decl: &'a ast::CallableDecl,
}

impl Display for AstCallableDecl<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let kind = match self.decl.kind {
            ast::CallableKind::Function => "function",
            ast::CallableKind::Operation => "operation",
        };

        let functors = ast_callable_functors(self.decl);
        let functors = FunctorSetValue { functors };

        write!(f, "{} {}", kind, self.decl.name.name)?;
        if !self.decl.generics.is_empty() {
            let type_params = self
                .decl
                .generics
                .iter()
                .map(
                    |AstTypeParameter {
                         ty, constraints, ..
                     }| {
                        format!(
                            "{}{}",
                            ty.name,
                            if constraints.0.is_empty() {
                                Default::default()
                            } else {
                                format!(
                                    ": {}",
                                    constraints
                                        .0
                                        .iter()
                                        .map(|bound| {
                                            let constraint_parameters = bound
                                                .parameters
                                                .iter()
                                                .map(|x| format!("{}", AstTy { ty: &x.ty }))
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            format!(
                                                "{}{}",
                                                bound.name.name,
                                                if constraint_parameters.is_empty() {
                                                    Default::default()
                                                } else {
                                                    format!("[{constraint_parameters}]")
                                                }
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" + ")
                                )
                            }
                        )
                    },
                )
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "<{type_params}>")?;
        }
        let input = AstPat {
            pat: &self.decl.input,
            lookup: self.lookup,
        };
        if matches!(*self.decl.input.kind, ast::PatKind::Tuple(_)) {
            write!(f, "{input}")?;
        } else {
            write!(f, "({input})")?;
        }
        write!(
            f,
            " : {}{}",
            AstTy {
                ty: &self.decl.output
            },
            functors,
        )
    }
}

struct HirPat<'a> {
    pat: &'a hir::Pat,
}

impl Display for HirPat<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.pat.kind {
            hir::PatKind::Bind(name) => write!(f, "{} : {}", name.name, self.pat.ty.display()),
            hir::PatKind::Discard => write!(f, "_ : {}", self.pat.ty.display()),
            hir::PatKind::Tuple(items) => {
                let mut elements = items.iter();
                if let Some(elem) = elements.next() {
                    write!(f, "({}", HirPat { pat: elem })?;
                    for elem in elements {
                        write!(f, ", {}", HirPat { pat: elem })?;
                    }
                    write!(f, ")")
                } else {
                    write!(f, "()")
                }
            }
            hir::PatKind::Err => write!(f, "?"),
        }
    }
}

struct AstPat<'a> {
    lookup: &'a dyn Lookup,
    pat: &'a ast::Pat,
}

impl Display for AstPat<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &*self.pat.kind {
            ast::PatKind::Bind(ident, anno) => match anno {
                Some(ty) => write!(f, "{}", IdentTy { ident, ty }),
                None => write!(
                    f,
                    "{}",
                    NameTyId {
                        lookup: self.lookup,
                        name: &ident.name,
                        ty_id: self.pat.id
                    }
                ),
            },
            ast::PatKind::Discard(anno) => match anno {
                Some(ty) => write!(f, "{}", AstTy { ty }),
                None => write!(
                    f,
                    "_ : {}",
                    TyId {
                        lookup: self.lookup,
                        ty_id: self.pat.id,
                    }
                ),
            },
            ast::PatKind::Elided => write!(f, "..."),
            ast::PatKind::Paren(item) => write!(
                f,
                "{}",
                AstPat {
                    lookup: self.lookup,
                    pat: item,
                }
            ),
            ast::PatKind::Tuple(items) => {
                let mut elements = items.iter();
                if let Some(elem) = elements.next() {
                    write!(
                        f,
                        "({}",
                        AstPat {
                            lookup: self.lookup,
                            pat: elem,
                        }
                    )?;
                    for elem in elements {
                        write!(
                            f,
                            ", {}",
                            AstPat {
                                lookup: self.lookup,
                                pat: elem,
                            }
                        )?;
                    }
                    write!(f, ")")
                } else {
                    write!(f, "()")
                }
            }
            ast::PatKind::Err => write!(f, "?"),
        }
    }
}

struct IdentTyDef<'a> {
    ident: &'a ast::Ident,
    def: &'a ast::TyDef,
}

impl Display for IdentTyDef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(fields) = as_struct(self.def) {
            write!(f, "struct {} ", self.ident.name)?;
            fmt_brace_seq(f, &fields, |item| IdentTy {
                ident: &item.name,
                ty: &item.ty,
            })
        } else {
            write!(
                f,
                "newtype {} = {}",
                self.ident.name,
                TyDef { def: self.def }
            )
        }
    }
}

struct StructDecl<'a> {
    decl: &'a ast::StructDecl,
}

impl Display for StructDecl<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "struct {} ", self.decl.name.name)?;
        fmt_brace_seq(f, &self.decl.fields, |item| IdentTy {
            ident: &item.name,
            ty: &item.ty,
        })
    }
}

struct HirUdt<'a> {
    udt: &'a ty::Udt,
    is_struct: bool,
}

impl<'a> HirUdt<'a> {
    fn new(udt: &'a ty::Udt) -> Self {
        HirUdt {
            udt,
            is_struct: udt.is_struct(),
        }
    }
}

impl Display for HirUdt<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.is_struct {
            match &self.udt.definition.kind {
                ty::UdtDefKind::Tuple(fields) => {
                    write!(f, "struct {} ", self.udt.name)?;
                    fmt_brace_seq(f, fields, UdtDef::new)?;
                }
                ty::UdtDefKind::Field(_) => {}
            }
            Ok(())
        } else {
            let udt_def = UdtDef::new(&self.udt.definition);
            write!(f, "newtype {} = {}", self.udt.name, udt_def)
        }
    }
}

struct UdtDef<'a> {
    name: Option<Rc<str>>,
    kind: UdtDefKind<'a>,
}

enum UdtDefKind<'a> {
    SingleTy(&'a ty::Ty),
    TupleTy(Vec<UdtDef<'a>>),
}

impl<'a> UdtDef<'a> {
    pub fn new(def: &'a ty::UdtDef) -> Self {
        match &def.kind {
            ty::UdtDefKind::Field(field) => UdtDef {
                name: field.name.clone(),
                kind: UdtDefKind::SingleTy(&field.ty),
            },
            ty::UdtDefKind::Tuple(defs) => UdtDef {
                name: None,
                kind: UdtDefKind::TupleTy(defs.iter().map(UdtDef::new).collect()),
            },
        }
    }
}

impl Display for UdtDef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(name) = &self.name {
            write!(f, "{name} : ")?;
        }

        match &self.kind {
            UdtDefKind::SingleTy(ty) => {
                write!(f, "{}", ty.display())
            }
            UdtDefKind::TupleTy(defs) => fmt_tuple(f, defs, |def| def),
        }
    }
}

struct HirUdtField<'a> {
    field: &'a ty::UdtField,
}

impl Display for HirUdtField<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(name) = &self.field.name {
            write!(f, "{name} : ")?;
        }
        write!(f, "{}", self.field.ty.display())
    }
}

struct FunctorSetValue {
    functors: ty::FunctorSetValue,
}

impl Display for FunctorSetValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let ty::FunctorSetValue::Empty = self.functors {
            Ok(())
        } else {
            write!(f, " is {}", self.functors)
        }
    }
}

struct TyId<'a> {
    lookup: &'a dyn Lookup,
    ty_id: ast::NodeId,
}

impl Display for TyId<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(ty) = self.lookup.get_ty(self.ty_id) {
            write!(f, "{}", ty.display())
        } else {
            write!(f, "?")
        }
    }
}

struct AstTy<'a> {
    ty: &'a ast::Ty,
}

impl Display for AstTy<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.ty.kind.as_ref() {
            ast::TyKind::Array(ty) => write!(f, "{}[]", AstTy { ty }),
            ast::TyKind::Arrow(kind, input, output, functors) => {
                let arrow = match kind {
                    ast::CallableKind::Function => "->",
                    ast::CallableKind::Operation => "=>",
                };
                write!(
                    f,
                    "({} {} {}{})",
                    AstTy { ty: input },
                    arrow,
                    AstTy { ty: output },
                    FunctorExpr { functors }
                )
            }
            ast::TyKind::Hole => write!(f, "_"),
            ast::TyKind::Paren(ty) => write!(f, "{}", AstTy { ty }),
            ast::TyKind::Path(path) => write!(f, "{}", AstPathKind { path }),
            ast::TyKind::Param(AstTypeParameter { ty, .. }) => write!(f, "{}", ty.name),
            ast::TyKind::Tuple(tys) => fmt_tuple(f, tys, |ty| AstTy { ty }),
            ast::TyKind::Err => write!(f, "?"),
        }
    }
}

struct FunctorExpr<'a> {
    functors: &'a Option<Box<ast::FunctorExpr>>,
}

impl Display for FunctorExpr<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.functors {
            Some(functors) => {
                let functors = eval_functor_expr(functors);
                write!(f, "{}", FunctorSetValue { functors })
            }
            None => Ok(()),
        }
    }
}

struct AstPathKind<'a> {
    path: &'a ast::PathKind,
}

impl Display for AstPathKind<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let ast::PathKind::Ok(path) = self.path {
            write!(f, "{}", path.full_name())
        } else {
            write!(f, "?")
        }
    }
}

struct TyDef<'a> {
    def: &'a ast::TyDef,
}

impl Display for TyDef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.def.kind.as_ref() {
            ast::TyDefKind::Field(name, ty) => match name {
                Some(name) => write!(f, "{} : {}", name.name, AstTy { ty }),
                None => write!(f, "{}", AstTy { ty }),
            },
            ast::TyDefKind::Paren(def) => write!(f, "{}", TyDef { def }),
            ast::TyDefKind::Tuple(tys) => fmt_tuple(f, tys, |def| TyDef { def }),
            ast::TyDefKind::Err => write!(f, "?"),
        }
    }
}

fn fmt_tuple<'a, I, O>(
    formatter: &mut Formatter,
    items: &'a [I],
    map: impl Fn(&'a I) -> O,
) -> Result
where
    O: Display,
{
    let mut elements = items.iter();
    if let Some(elem) = elements.next() {
        write!(formatter, "({}", map(elem))?;
        if elements.len() == 0 {
            write!(formatter, ",)")?;
        } else {
            for elem in elements {
                write!(formatter, ", {}", map(elem))?;
            }
            write!(formatter, ")")?;
        }
    } else {
        write!(formatter, "Unit")?;
    }
    Ok(())
}

fn fmt_brace_seq<'a, I, O>(
    formatter: &mut Formatter<'_>,
    items: &'a [I],
    map: impl Fn(&'a I) -> O,
) -> Result
where
    O: Display,
{
    write!(formatter, "{{ ")?;
    if let Some((last, most)) = items.split_last() {
        for item in most {
            write!(formatter, "{}, ", map(item))?;
        }
        write!(formatter, "{} ", map(last))?;
    }
    write!(formatter, "}}")
}

fn display_type_params(generics: &[HirTypeParameter]) -> String {
    let type_params = generics
        .iter()
        .filter_map(|generic| match generic {
            HirTypeParameter::Ty { name, bounds } => Some(format!(
                "{}{}",
                name,
                if bounds.is_empty() {
                    Default::default()
                } else {
                    format!(": {bounds}")
                }
            )),
            HirTypeParameter::Functor(_) => None,
        })
        .collect::<Vec<_>>()
        .join(", ");
    if type_params.is_empty() {
        type_params
    } else {
        format!("<{type_params}>")
    }
}

//
// helpers that don't manipulate any strings
//

fn ast_callable_functors(callable: &ast::CallableDecl) -> ty::FunctorSetValue {
    let mut functors = callable
        .functors
        .as_ref()
        .map_or(ty::FunctorSetValue::Empty, |f| {
            eval_functor_expr(f.as_ref())
        });

    if let ast::CallableBody::Specs(specs) = callable.body.as_ref() {
        for spec in specs {
            let spec_functors = match spec.spec {
                ast::Spec::Body => ty::FunctorSetValue::Empty,
                ast::Spec::Adj => ty::FunctorSetValue::Adj,
                ast::Spec::Ctl => ty::FunctorSetValue::Ctl,
                ast::Spec::CtlAdj => ty::FunctorSetValue::CtlAdj,
            };
            functors = functors.union(&spec_functors);
        }
    }

    functors
}

fn eval_functor_expr(expr: &ast::FunctorExpr) -> ty::FunctorSetValue {
    match expr.kind.as_ref() {
        ast::FunctorExprKind::BinOp(op, lhs, rhs) => {
            let lhs_functors = eval_functor_expr(lhs);
            let rhs_functors = eval_functor_expr(rhs);
            match op {
                ast::SetOp::Union => lhs_functors.union(&rhs_functors),
                ast::SetOp::Intersect => lhs_functors.intersect(&rhs_functors),
            }
        }
        ast::FunctorExprKind::Lit(ast::Functor::Adj) => ty::FunctorSetValue::Adj,
        ast::FunctorExprKind::Lit(ast::Functor::Ctl) => ty::FunctorSetValue::Ctl,
        ast::FunctorExprKind::Paren(inner) => eval_functor_expr(inner),
    }
}

fn as_struct(ty_def: &ast::TyDef) -> Option<Vec<ast::FieldDef>> {
    match ty_def.kind.as_ref() {
        ast::TyDefKind::Paren(inner) => as_struct(inner),
        ast::TyDefKind::Tuple(fields) => {
            let mut converted_fields = Vec::new();
            for field in fields {
                let field = remove_parens(field);
                match field.kind.as_ref() {
                    ast::TyDefKind::Field(Some(name), field_ty) => {
                        converted_fields.push(ast::FieldDef {
                            id: field.id,
                            span: field.span,
                            name: name.clone(),
                            ty: field_ty.clone(),
                        });
                    }
                    _ => return None,
                }
            }
            Some(converted_fields)
        }
        ast::TyDefKind::Err | ast::TyDefKind::Field(..) => None,
    }
}

fn remove_parens(ty_def: &ast::TyDef) -> &ast::TyDef {
    match ty_def.kind.as_ref() {
        ast::TyDefKind::Paren(inner) => remove_parens(inner.as_ref()),
        _ => ty_def,
    }
}

//
// parsing functions for working with doc comments
//

/// Takes a doc string from Q# and increases all of the markdown header levels by one level.
/// i.e. `# Summary` becomes `## Summary`
#[must_use]
pub fn increase_header_level(doc: &str) -> String {
    let re = Regex::new(r"(?mi)^(#+)( [\s\S]+?)$").expect("Invalid regex");
    re.replace_all(doc, "$1#$2").to_string()
}

/// Takes a doc string from Q# and returns the contents of the `# Summary` section. If no
/// such section can be found, returns the original doc string.
#[must_use]
pub fn parse_doc_for_summary(doc: &str) -> String {
    let re = Regex::new(r"(?mi)(?:^# Summary$)([\s\S]*?)(?:(^# .*)|\z)").expect("Invalid regex");
    match re.captures(doc) {
        Some(captures) => {
            let capture = captures
                .get(1)
                .expect("Didn't find the capture for the given regex");
            capture.as_str()
        }
        None => doc,
    }
    .trim()
    .to_string()
}

/// Takes a doc string from a Q# callable and the name of a parameter of
/// that callable. Returns the description of that parameter found in the
/// doc string. If no description is found, returns the empty string.
#[must_use]
pub fn parse_doc_for_param(doc: &str, param: &str) -> String {
    let re = Regex::new(r"(?mi)(?:^# Input$)([\s\S]*?)(?:(^# .*)|\z)").expect("Invalid regex");
    let input = match re.captures(doc) {
        Some(captures) => {
            let capture = captures
                .get(1)
                .expect("Didn't find the capture for the given regex");
            capture.as_str()
        }
        None => return String::new(),
    }
    .trim();

    let re = Regex::new(format!(r"(?mi)(?:^## {param}$)([\s\S]*?)(?:(^(#|##) .*)|\z)").as_str())
        .expect("Invalid regex");
    match re.captures(input) {
        Some(captures) => {
            let capture = captures
                .get(1)
                .expect("Didn't find the capture for the given regex");
            capture.as_str()
        }
        None => return String::new(),
    }
    .trim()
    .to_string()
}
