// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Error, Locals, Names, Res};
use crate::{
    compile,
    resolve::{Importable, Local, Resolver, imports::iter_valid_items},
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_ast::ast::{Idents, ImportKind, Item, ItemKind, PathKind};
use qsc_ast::{
    assigner::Assigner as AstAssigner,
    ast::{Ident, NodeId, Package, Path, TopLevelNode},
    mut_visit::MutVisitor,
    visit::{self, Visitor},
};

use qsc_data_structures::{
    language_features::LanguageFeatures,
    namespaces::{NamespaceId, NamespaceTreeRoot},
    span::Span,
    target::TargetCapabilityFlags,
};
use qsc_hir::{assigner::Assigner as HirAssigner, hir::PackageId};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::{fmt::Write, vec};

#[derive(Debug)]
enum Change {
    Res(Res),
    NamespaceId(NamespaceId),
    Import(Option<Res>, Res),
}

impl From<Res> for Change {
    fn from(res: Res) -> Self {
        Self::Res(res)
    }
}

impl From<NamespaceId> for Change {
    fn from(ns_id: NamespaceId) -> Self {
        Self::NamespaceId(ns_id)
    }
}

struct Renamer<'a> {
    names: &'a Names,
    changes: Vec<(Span, Change)>,
    namespaces: NamespaceTreeRoot,
    aliases: FxHashMap<Vec<Rc<str>>, NamespaceId>,
}

impl<'a> Renamer<'a> {
    fn new(names: &'a Names, namespaces: NamespaceTreeRoot) -> Self {
        Self {
            names,
            changes: Vec::new(),
            namespaces,
            aliases: FxHashMap::default(),
        }
    }

    fn rename(&self, input: &mut String) {
        for (span, change) in self.changes.iter().rev() {
            let name = match change {
                Change::Import(res_source, res) => {
                    format!(
                        "{} as {}",
                        res_source
                            .as_ref()
                            .map(Self::format_res)
                            .unwrap_or_default(),
                        Self::format_res(res)
                    )
                }
                Change::Res(res) => Self::format_res(res),
                Change::NamespaceId(ns_id) => format!("namespace{}", Into::<usize>::into(ns_id)),
            };
            input.replace_range((span.lo as usize)..(span.hi as usize), &name);
        }
    }

    fn format_res(res: &Res) -> String {
        match res {
            Res::Item(item, _) => format!("package{}_item{}", item.package, item.item),
            Res::Local(node) => format!("local{node}"),
            Res::PrimTy(prim) => format!("{prim:?}"),
            Res::UnitTy => "Unit".to_string(),
            Res::Param { id, .. } => format!("param{id}"),
            Res::Importable(item_kind) => match item_kind {
                Importable::Callable(item_id, _) | Importable::Ty(item_id, _) => {
                    format!("item{}", item_id.item)
                }
                Importable::Namespace(namespace_id, _) => {
                    format!("namespace{}", usize::from(namespace_id))
                }
            },
        }
    }
}

impl Visitor<'_> for Renamer<'_> {
    fn visit_path(&mut self, path: &Path) {
        if let Some(res) = self.names.get(path.id) {
            // The whole path node can be a resolved name
            self.changes.push((path.span, res.clone().into()));
            return;
        }

        let ns_id = self.find_namespace_id(path);
        if let Some(ns_id) = ns_id {
            // The whole path can be a namespace
            self.changes.push((path.span, ns_id.into()));
            return;
        }

        // Individual ident nodes can be resolved names
        visit::walk_path(self, path);
    }

    fn visit_idents(&mut self, idents: &[Ident]) {
        let ns_id = self.find_namespace_id(&idents);
        if let Some(ns_id) = ns_id {
            self.changes.push((idents.full_span(), ns_id.into()));
            return;
        }
        visit::walk_idents(self, idents);
    }

    fn visit_ident(&mut self, ident: &Ident) {
        if let Some(res) = self.names.get(ident.id) {
            self.changes.push((ident.span, res.clone().into()));
        }
    }

    fn visit_item(&mut self, item: &'_ Item) {
        match &*item.kind {
            ItemKind::Open(PathKind::Ok(namespace), Some(alias)) => {
                if let Some(ns_id) = self.namespaces.get_namespace_id(namespace.str_iter()) {
                    self.aliases.insert(vec![alias.name.clone()], ns_id);
                } else {
                    return;
                }
            }
            ItemKind::ImportOrExport(decl) => {
                for item in iter_valid_items(decl) {
                    let path_res = self.names.get(item.path.id);
                    let name_res = self.names.get(item.name().id);

                    match (path_res, name_res) {
                        (Some(source_res), Some(res)) => {
                            self.changes.push((
                                item.span,
                                Change::Import(Some(source_res.clone()), res.clone()),
                            ));
                        }
                        (None, Some(res)) => {
                            self.changes.push((
                                if let ImportKind::Direct { alias: Some(alias) } = &item.kind {
                                    alias.span
                                } else {
                                    Span {
                                        lo: item.span.hi,
                                        hi: item.span.hi,
                                    }
                                },
                                Change::Import(None, res.clone()),
                            ));
                        }
                        (Some(source_res), None) => {
                            self.changes
                                .push((item.path.span, Change::Res(source_res.clone())));
                        }
                        (None, None) => {}
                    }
                }
                return;
            }
            _ => (),
        }
        visit::walk_item(self, item);
    }
}

impl Renamer<'_> {
    fn find_namespace_id(&mut self, idents: &impl Idents) -> Option<NamespaceId> {
        self.namespaces
            .get_namespace_id(idents.str_iter())
            .or_else(|| {
                self.aliases
                    .get(&idents.rc_str_iter().cloned().collect::<Vec<_>>())
                    .copied()
            })
    }
}

fn check(input: &str, expect: &Expect) {
    expect.assert_eq(&resolve_names(input, TargetCapabilityFlags::all()));
}

fn check_with_capabilities(input: &str, capabilities: TargetCapabilityFlags, expect: &Expect) {
    expect.assert_eq(&resolve_names(input, capabilities));
}

fn resolve_names(input: &str, capabilities: TargetCapabilityFlags) -> String {
    let (package, names, _, errors, namespaces) =
        compile(input, LanguageFeatures::default(), capabilities);
    let mut renamer = Renamer::new(&names, namespaces);
    renamer.visit_package(&package);
    let mut output = input.to_string();
    renamer.rename(&mut output);
    if !errors.is_empty() {
        output += "\n";
    }
    for error in &errors {
        writeln!(output, "// {error:?}").expect("string should be writable");
    }
    output
}

fn compile(
    input: &str,
    language_features: LanguageFeatures,
    capabilities: TargetCapabilityFlags,
) -> (Package, Names, Locals, Vec<Error>, NamespaceTreeRoot) {
    let (namespaces, parse_errors) = qsc_parse::namespaces(input, None, language_features);
    assert!(parse_errors.is_empty(), "parse failed: {parse_errors:#?}");
    let mut package = Package {
        id: NodeId::default(),
        nodes: namespaces
            .into_iter()
            .map(TopLevelNode::Namespace)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        entry: None,
    };

    AstAssigner::new().visit_package(&mut package);

    let mut cond_compile = compile::preprocess::Conditional::new(capabilities);
    cond_compile.visit_package(&mut package);
    let dropped_names = cond_compile.into_names();

    let mut assigner = HirAssigner::new();
    // insert the core namespace

    // These tests do not happen in the context of a `PackageStore`,
    // so we just make up a fake PackageId.
    let package_id = PackageId::CORE.successor().successor();

    let mut globals = super::GlobalTable::new();
    let mut errors = globals.add_local_package(&mut assigner, &package, package_id);
    let mut resolver = Resolver::new(package_id, globals, dropped_names);
    resolver.resolve(&mut assigner, &package);

    let (names, globals, locals, mut resolve_errors) = resolver.into_result();
    errors.append(&mut resolve_errors);
    (package, names, locals, errors, globals.namespaces)
}

#[test]
fn global_callable() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn global_callable_recursive() {
    check(
        indoc! {
            "namespace Foo {
                function A() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn global_callable_internal() {
    check(
        indoc! {"
            namespace Foo {
                internal function A() : Unit {}

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                internal function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn global_callable_duplicate_error() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
                operation A() : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }

            // Duplicate("A", "Foo", Span { lo: 57, hi: 58 })
        "#]],
    );
}

#[test]
fn global_path() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                function B() : Unit {
                    Foo.A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn open_namespace() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                open Foo;

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                open namespace3;

                function package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn open_alias() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                open Foo as F;

                function B() : Unit {
                    F.A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                open namespace3 as F;

                function package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn prelude_callable() {
    check(
        indoc! {"
            namespace Std.Core {
                function A() : Unit {}
            }

            namespace Foo {
                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace2 {
                function package2_item1() : Unit {}
            }

            namespace namespace3 {
                function package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn parent_namespace_shadows_prelude() {
    check(
        indoc! {"
            namespace Std.Core {
                function A() : Unit {}
            }

            namespace Foo {
                function A() : Unit {}

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace2 {
                function package2_item1() : Unit {}
            }

            namespace namespace3 {
                function package2_item3() : Unit {}

                function package2_item4() : Unit {
                    package2_item3();
                }
            }
        "#]],
    );
}

#[test]
fn open_shadows_prelude() {
    check(
        indoc! {"
            namespace Std.Core {
                function A() : Unit {}
            }

            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                open Foo;

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace2 {
                function package2_item1() : Unit {}
            }

            namespace namespace3 {
                function package2_item3() : Unit {}
            }

            namespace namespace4 {
                open namespace3;

                function package2_item5() : Unit {
                    package2_item3();
                }
            }
        "#]],
    );
}

#[test]
fn ambiguous_prelude() {
    check(
        indoc! {"
        namespace Std.Canon {
            function A() : Unit {}
        }

        namespace Std.Measurement {
            function A() : Unit {}
        }

        namespace Foo {
            function B() : Unit {
                A();
            }
        }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {}
            }

            namespace namespace5 {
                function package2_item5() : Unit {
                    A();
                }
            }

            // AmbiguousPrelude { name: "A", candidate_a: "Std.Canon", candidate_b: "Std.Measurement", span: Span { lo: 160, hi: 161 } }
        "#]],
    );
}

#[test]
fn local_var() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Int {
                    let local13 = 0;
                    local13
                }
            }
        "#]],
    );
}

#[test]
fn shadow_local() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    let y = {
                        let x = 1;
                        x
                    };
                    x + y
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Int {
                    let local13 = 0;
                    let local17 = {
                        let local22 = 1;
                        local22
                    };
                    local13 + local17
                }
            }
        "#]],
    );
}

#[test]
fn callable_param() {
    check(
        indoc! {"
            namespace Foo {
                function A(x : Int) : Int {
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1(local8 : Int) : Int {
                    local8
                }
            }
        "#]],
    );
}

#[test]
fn spec_param() {
    check(
        indoc! {"
            namespace Foo {
                operation A(q : Qubit) : (Qubit[], Qubit) {
                    controlled (cs, ...) {
                        (cs, q)
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8 : Qubit) : (Qubit[], Qubit) {
                    controlled (local23, ...) {
                        (local23, local8)
                    }
                }
            }
        "#]],
    );
}

#[test]
fn spec_param_shadow_disallowed() {
    check(
        indoc! {"
            namespace Foo {
                operation A(qs : Qubit[]) : Qubit[] {
                    controlled (qs, ...) {
                        qs
                    }
                    body ... {
                        qs
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8 : Qubit[]) : Qubit[] {
                    controlled (local20, ...) {
                        local20
                    }
                    body ... {
                        local8
                    }
                }
            }

            // DuplicateBinding("qs", Span { lo: 78, hi: 80 })
        "#]],
    );
}

#[test]
fn local_shadows_global() {
    check(
        indoc! {"
            namespace Foo {
                function x() : Unit {}

                function y() : Int {
                    x();
                    let x = 1;
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}

                function package2_item2() : Int {
                    package2_item1();
                    let local27 = 1;
                    local27
                }
            }
        "#]],
    );
}

#[test]
fn shadow_same_block() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    let x = x + 1;
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Int {
                    let local13 = 0;
                    let local17 = local13 + 1;
                    local17
                }
            }
        "#]],
    );
}

#[test]
fn parent_namespace_shadows_open() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                open Foo;

                function A() : Unit {}

                function B() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                open namespace3;

                function package2_item3() : Unit {}

                function package2_item4() : Unit {
                    package2_item3();
                }
            }
        "#]],
    );
}

#[test]
fn open_alias_shadows_global() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                function A() : Unit {}
            }

            namespace Baz {
                open Foo as Bar;

                function B() : Unit {
                    Bar.A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {}
            }

            namespace namespace5 {
                open namespace3 as Bar;

                function package2_item5() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn shadowing_disallowed_within_parameters() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x: Int, y: Double, x: Bool) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8: Int, local13: Double, local18: Bool) : Unit {}
            }

            // DuplicateBinding("x", Span { lo: 54, hi: 55 })
        "#]],
    );
}

#[test]
fn shadowing_disallowed_within_local_binding() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    let (first, second, first) = (1, 2, 3);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    let (local14, local16, local18) = (1, 2, 3);
                }
            }

            // DuplicateBinding("first", Span { lo: 74, hi: 79 })
        "#]],
    );
}

#[test]
fn shadowing_disallowed_within_for_loop() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    for (key, val, key) in [(1, 1, 1)] {}
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    for (local15, local17, local19) in [(1, 1, 1)] {}
                }
            }

            // DuplicateBinding("key", Span { lo: 69, hi: 72 })
        "#]],
    );
}

#[test]
fn shadowing_disallowed_within_lambda_param() {
    check(
        indoc! {"
            namespace Test {
                operation Foo() : Unit {
                    let f = (x, y, x) -> x + y + 1;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    let local13 = (local17, local19, local21) -> local21 + local19 + 1;
                }
            }

            // DuplicateBinding("x", Span { lo: 69, hi: 70 })
        "#]],
    );
}

#[test]
fn merged_aliases() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                function B() : Unit {}
            }

            namespace Baz {
                open Foo as Alias;
                open Bar as Alias;

                function C() : Unit {
                    Alias.A();
                    Alias.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {}
            }

            namespace namespace5 {
                open namespace3 as Alias;
                open namespace4 as Alias;

                function package2_item5() : Unit {
                    package2_item1();
                    package2_item3();
                }
            }
        "#]],
    );
}

#[test]
fn ty_decl() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;
                function B(a : A) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;
                function package2_item2(local14 : package2_item1) : Unit {}
            }
        "#]],
    );
}

#[test]
fn struct_decl() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}
                function B(a : A) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}
                function package2_item2(local11 : package2_item1) : Unit {}
            }
        "#]],
    );
}

#[test]
fn ty_decl_duplicate_error() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;
                newtype A = Bool;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;
                newtype package2_item2 = Bool;
            }

            // Duplicate("A", "Foo", Span { lo: 50, hi: 51 })
        "#]],
    );
}

#[test]
fn struct_decl_duplicate_error() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}
                struct A { first : Bool }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}
                struct package2_item2 { first : Bool }
            }

            // Duplicate("A", "Foo", Span { lo: 43, hi: 44 })
        "#]],
    );
}

#[test]
fn ty_decl_duplicate_error_on_built_in_ty() {
    check(
        indoc! {"
            namespace Std.Core {
                newtype Pauli = Unit;
            }
        "},
        &expect![[r#"
            namespace namespace2 {
                newtype package2_item1 = Unit;
            }

            // Duplicate("Pauli", "Std.Core", Span { lo: 33, hi: 38 })
        "#]],
    );
}

#[test]
fn struct_decl_duplicate_error_on_built_in_ty() {
    check(
        indoc! {"
            namespace Std.Core {
                struct Pauli {}
            }
        "},
        &expect![[r#"
            namespace namespace2 {
                struct package2_item1 {}
            }

            // Duplicate("Pauli", "Std.Core", Span { lo: 32, hi: 37 })
        "#]],
    );
}

#[test]
fn ty_decl_in_ty_decl() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;
                newtype B = A;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;
                newtype package2_item2 = package2_item1;
            }
        "#]],
    );
}

#[test]
fn struct_decl_in_struct_decl() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}
                struct B { a : A }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}
                struct package2_item2 { a : package2_item1 }
            }
        "#]],
    );
}

#[test]
fn ty_decl_recursive() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = A;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = package2_item1;
            }
        "#]],
    );
}

#[test]
fn struct_decl_recursive() {
    check(
        indoc! {"
            namespace Foo {
                struct A { a : A }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 { a : package2_item1 }
            }
        "#]],
    );
}

#[test]
fn ty_decl_cons() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;

                function B() : A {
                    A()
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;

                function package2_item2() : package2_item1 {
                    package2_item1()
                }
            }
        "#]],
    );
}

#[test]
fn struct_decl_call_cons() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}

                function B() : A {
                    A()
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}

                function package2_item2() : package2_item1 {
                    package2_item1()
                }
            }
        "#]],
    );
}

#[test]
fn struct_decl_cons() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}

                function B() : A {
                    new A {}
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}

                function package2_item2() : package2_item1 {
                    new package2_item1 {}
                }
            }
        "#]],
    );
}

#[test]
fn struct_decl_cons_with_fields() {
    check(
        indoc! {"
            namespace Foo {
                struct A {}
                struct B {}
                struct C { a : A, b : B }

                function D() : C {
                    new C { a = new A {}, b = new B {} }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 {}
                struct package2_item2 {}
                struct package2_item3 { a : package2_item1, b : package2_item2 }

                function package2_item4() : package2_item3 {
                    new package2_item3 { a = new package2_item1 {}, b = new package2_item2 {} }
                }
            }
        "#]],
    );
}

#[test]
fn struct_field_accessor() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C}
                struct C { i : Int }

                function D() : Unit {
                    let a = new A { b = new B { c = new C { i = 4 } } };
                    let i = a.b.c.i;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 { b : package2_item2 }
                struct package2_item2 { c : package2_item3}
                struct package2_item3 { i : Int }

                function package2_item4() : Unit {
                    let local37 = new package2_item1 { b = new package2_item2 { c = new package2_item3 { i = 4 } } };
                    let local56 = local37.b.c.i;
                }
            }
        "#]],
    );
}

#[test]
fn struct_field_accessor_with_expr() {
    check(
        indoc! {"
            namespace Foo {
                struct A { b : B }
                struct B { c : C}
                struct C { i : Int }

                function D() : Unit {
                    let a = new A { b = new B { c = new C { i = 4 } } };
                    let i = { a }.b.c.i;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                struct package2_item1 { b : package2_item2 }
                struct package2_item2 { c : package2_item3}
                struct package2_item3 { i : Int }

                function package2_item4() : Unit {
                    let local37 = new package2_item1 { b = new package2_item2 { c = new package2_item3 { i = 4 } } };
                    let local56 = { local37 }.b.c.i;
                }
            }
        "#]],
    );
}

#[test]
fn unknown_term() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    B();
                }
            }

            // NotFound("B", Span { lo: 50, hi: 51 })
        "#]],
    );
}

#[test]
fn unknown_ty() {
    check(
        indoc! {"
            namespace Foo {
                function A(b : B) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1(local8 : B) : Unit {}
            }

            // NotFound("B", Span { lo: 35, hi: 36 })
        "#]],
    );
}

#[test]
fn open_ambiguous_terms() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                function A() : Unit {}
            }

            namespace Baz {
                open Foo;
                open Bar;

                function C() : Unit {
                    A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {}
            }

            namespace namespace5 {
                open namespace3;
                open namespace4;

                function package2_item5() : Unit {
                    A();
                }
            }

            // Ambiguous { name: "A", first_open: "Foo", second_open: "Bar", name_span: Span { lo: 171, hi: 172 }, first_open_span: Span { lo: 117, hi: 120 }, second_open_span: Span { lo: 131, hi: 134 } }
        "#]],
    );
}

#[test]
fn open_ambiguous_tys() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;
            }

            namespace Bar {
                newtype A = Unit;
            }

            namespace Baz {
                open Foo;
                open Bar;

                function C(a : A) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;
            }

            namespace namespace4 {
                newtype package2_item3 = Unit;
            }

            namespace namespace5 {
                open namespace3;
                open namespace4;

                function package2_item5(local30 : A) : Unit {}
            }

            // Ambiguous { name: "A", first_open: "Foo", second_open: "Bar", name_span: Span { lo: 146, hi: 147 }, first_open_span: Span { lo: 107, hi: 110 }, second_open_span: Span { lo: 121, hi: 124 } }
        "#]],
    );
}

#[test]
fn merged_aliases_ambiguous_terms() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }

            namespace Bar {
                function A() : Unit {}
            }

            namespace Baz {
                open Foo as Alias;
                open Bar as Alias;

                function C() : Unit {
                    Alias.A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                function package2_item3() : Unit {}
            }

            namespace namespace5 {
                open namespace3 as Alias;
                open namespace4 as Alias;

                function package2_item5() : Unit {
                    namespace4.A();
                }
            }

            // Ambiguous { name: "A", first_open: "Foo", second_open: "Bar", name_span: Span { lo: 195, hi: 196 }, first_open_span: Span { lo: 117, hi: 120 }, second_open_span: Span { lo: 140, hi: 143 } }
        "#]],
    );
}

#[test]
fn merged_aliases_ambiguous_tys() {
    check(
        indoc! {"
            namespace Foo {
                newtype A = Unit;
            }

            namespace Bar {
                newtype A = Unit;
            }

            namespace Baz {
                open Foo as Alias;
                open Bar as Alias;

                function C(a : Alias.A) : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = Unit;
            }

            namespace namespace4 {
                newtype package2_item3 = Unit;
            }

            namespace namespace5 {
                open namespace3 as Alias;
                open namespace4 as Alias;

                function package2_item5(local32 : namespace4.A) : Unit {}
            }

            // Ambiguous { name: "A", first_open: "Foo", second_open: "Bar", name_span: Span { lo: 170, hi: 171 }, first_open_span: Span { lo: 107, hi: 110 }, second_open_span: Span { lo: 130, hi: 133 } }
        "#]],
    );
}

#[test]
fn lambda_param() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    let f = x -> x + 1;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    let local13 = local16 -> local16 + 1;
                }
            }
        "#]],
    );
}

#[test]
fn lambda_shadows_local() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    let x = 1;
                    let f = x -> x + 1;
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Int {
                    let local13 = 1;
                    let local17 = local20 -> local20 + 1;
                    local13
                }
            }
        "#]],
    );
}

#[test]
fn for_loop_range() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    for i in 0..9 {
                        let _ = i;
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    for local14 in 0..9 {
                        let _ = local14;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn for_loop_var() {
    check(
        indoc! {"
            namespace Foo {
                function A(xs : Int[]) : Unit {
                    for x in xs {
                        let _ = x;
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1(local8 : Int[]) : Unit {
                    for local20 in local8 {
                        let _ = local20;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn for_loop_explicit_type() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    for i : Int in 0..9 {
                        let _ = i;
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    for local14 : Int in 0..9 {
                        let _ = local14;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn repeat_until() {
    check(
        indoc! {"
            namespace Foo {
                operation A() : Unit {
                    mutable cond = false;
                    repeat {
                        set cond = true;
                    } until cond;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    mutable local13 = false;
                    repeat {
                        set local13 = true;
                    } until local13;
                }
            }
        "#]],
    );
}

#[test]
fn repeat_until_fixup() {
    check(
        indoc! {"
            namespace Foo {
                operation A() : Unit {
                    mutable cond = false;
                    repeat {
                        set cond = false;
                    } until cond
                    fixup {
                        set cond = true;
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    mutable local13 = false;
                    repeat {
                        set local13 = false;
                    } until local13
                    fixup {
                        set local13 = true;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn repeat_until_fixup_scoping() {
    check(
        indoc! {"
        namespace Foo {
            operation A() : Unit {
                repeat {
                    mutable cond = false;
                }
                until cond
                fixup {
                    set cond = true;
                }
            }
        }"},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    repeat {
                        mutable local16 = false;
                    }
                    until cond
                    fixup {
                        set cond = true;
                    }
                }
            }
            // NotFound("cond", Span { lo: 118, hi: 122 })
            // NotFound("cond", Span { lo: 155, hi: 159 })
        "#]],
    );
}

#[test]
fn use_qubit() {
    check(
        indoc! {"
            namespace Foo {
                operation X(q : Qubit) : Unit {
                    body intrinsic;
                }
                operation A() : Unit {
                    use q = Qubit();
                    X(q);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8 : Qubit) : Unit {
                    body intrinsic;
                }
                operation package2_item2() : Unit {
                    use local26 = Qubit();
                    package2_item1(local26);
                }
            }
        "#]],
    );
}

#[test]
fn use_qubit_block() {
    check(
        indoc! {"
            namespace Foo {
                operation X(q : Qubit) : Unit {
                    body intrinsic;
                }
                operation A() : Unit {
                    use q = Qubit() {
                        X(q);
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8 : Qubit) : Unit {
                    body intrinsic;
                }
                operation package2_item2() : Unit {
                    use local26 = Qubit() {
                        package2_item1(local26);
                    }
                }
            }
        "#]],
    );
}

#[test]
fn use_qubit_block_qubit_restricted_to_block_scope() {
    check(
        indoc! {"
            namespace Foo {
                operation X(q : Qubit) : Unit {
                    body intrinsic;
                }
                operation A() : Unit {
                    use q = Qubit() {
                        X(q);
                    }
                    X(q);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8 : Qubit) : Unit {
                    body intrinsic;
                }
                operation package2_item2() : Unit {
                    use local26 = Qubit() {
                        package2_item1(local26);
                    }
                    package2_item1(q);
                }
            }

            // NotFound("q", Span { lo: 173, hi: 174 })
        "#]],
    );
}

#[test]
fn local_function() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Int {
                    function Bar() : Int { 2 }
                    Bar() + 1
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Int {
                    function package2_item2() : Int { 2 }
                    package2_item2() + 1
                }
            }
        "#]],
    );
}

#[test]
fn local_function_use_before_declare() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    Bar();
                    function Bar() : () {}
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    package2_item2();
                    function package2_item2() : () {}
                }
            }
        "#]],
    );
}

#[test]
fn local_function_is_really_local() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    function Bar() : () {}
                    Bar();
                }

                function Baz() : () { Bar(); }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    function package2_item3() : () {}
                    package2_item3();
                }

                function package2_item2() : () { Bar(); }
            }

            // NotFound("Bar", Span { lo: 119, hi: 122 })
        "#]],
    );
}

#[test]
fn local_function_is_not_closure() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let x = 2;
                    function Bar() : Int { x }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    let local11 = 2;
                    function package2_item2() : Int { x }
                }
            }

            // NotFound("x", Span { lo: 90, hi: 91 })
        "#]],
    );
}

#[test]
fn local_type() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    newtype Bar = Int;
                    let x = Bar(5);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    newtype package2_item2 = Int;
                    let local18 = package2_item2(5);
                }
            }
        "#]],
    );
}

#[test]
fn local_open() {
    check(
        indoc! {"
            namespace A { function Foo() : () { open B; Bar(); } }
            namespace B { function Bar() : () {} }
        "},
        &expect![[r#"
            namespace namespace3 { function package2_item1() : () { open namespace4; package2_item3(); } }
            namespace namespace4 { function package2_item3() : () {} }
        "#]],
    );
}

#[test]
fn local_open_shadows_parent_item() {
    check(
        indoc! {"
            namespace A {
                function Bar() : () {}
                function Foo() : () { open B; Bar(); }
            }

            namespace B { function Bar() : () {} }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {}
                function package2_item2() : () { open namespace4; package2_item4(); }
            }

            namespace namespace4 { function package2_item4() : () {} }
        "#]],
    );
}

#[test]
fn local_open_shadows_parent_open() {
    check(
        indoc! {"
            namespace A {
                open B;
                function Foo() : () { open C; Bar(); }
            }

            namespace B { function Bar() : () {} }
            namespace C { function Bar() : () {} }
        "},
        &expect![[r#"
            namespace namespace3 {
                open namespace4;
                function package2_item1() : () { open namespace5; package2_item5(); }
            }

            namespace namespace4 { function package2_item3() : () {} }
            namespace namespace5 { function package2_item5() : () {} }
        "#]],
    );
}

#[test]
fn update_array_index_var() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [2];
                    let i = 0;
                    let ys = xs w/ i <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    let local11 = [2];
                    let local16 = 0;
                    let local20 = local11 w/ local16 <- 3;
                }
            }
        "#]],
    );
}

#[test]
fn update_array_index_expr() {
    check(
        indoc! {"
            namespace A {
                function Foo() : () {
                    let xs = [2];
                    let i = 0;
                    let ys = xs w/ i + 1 <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : () {
                    let local11 = [2];
                    let local16 = 0;
                    let local20 = local11 w/ local16 + 1 <- 3;
                }
            }
        "#]],
    );
}

#[test]
fn update_udt_known_field_name() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ First <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First : Int, Second : Int);

                function package2_item2() : () {
                    let local24 = package2_item1(1, 2);
                    let local34 = local24 w/ First <- 3;
                }
            }
        "#]],
    );
}

#[test]
fn update_udt_known_field_name_expr() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ First + 1 <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First : Int, Second : Int);

                function package2_item2() : () {
                    let local24 = package2_item1(1, 2);
                    let local34 = local24 w/ First + 1 <- 3;
                }
            }

            // NotFound("First", Span { lo: 138, hi: 143 })
        "#]],
    );
}

#[test]
fn update_udt_unknown_field_name() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ Third <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First : Int, Second : Int);

                function package2_item2() : () {
                    let local24 = package2_item1(1, 2);
                    let local34 = local24 w/ Third <- 3;
                }
            }
        "#]],
    );
}

#[test]
fn update_udt_unknown_field_name_known_global() {
    check(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);

                function Third() : () {}

                function Foo() : () {
                    let p = Pair(1, 2);
                    let q = p w/ Third <- 3;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First : Int, Second : Int);

                function package2_item2() : () {}

                function package2_item3() : () {
                    let local30 = package2_item1(1, 2);
                    let local40 = local30 w/ Third <- 3;
                }
            }
        "#]],
    );
}

#[test]
fn unknown_namespace() {
    check(
        indoc! {"
            namespace A {
                import Std.Fake.*;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                import Std.Fake.*;
            }

            // NotFound("Std.Fake", Span { lo: 25, hi: 33 })
        "#]],
    );
}

#[test]
fn empty_namespace_works() {
    check(
        indoc! {"
            namespace A {
                open B;
                function foo(): Unit{}
            }
            namespace B {}
        "},
        &expect![[r#"
            namespace namespace3 {
                open namespace4;
                function package2_item1(): Unit{}
            }
            namespace namespace4 {}
        "#]],
    );
}

#[test]
fn cyclic_namespace_dependency_supported() {
    check(
        indoc! {"
            namespace A {
                open B;
                operation Foo() : Unit {
                    Bar();
                }
            }
            namespace B {
                open A;
                operation Bar() : Unit {
                    Foo();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                open namespace4;
                operation package2_item1() : Unit {
                    package2_item3();
                }
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn bind_items_in_repeat() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {
                    repeat {
                        function C() : Unit {}
                    } until false
                    fixup {
                        function D() : Unit {}
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    repeat {
                        function package2_item2() : Unit {}
                    } until false
                    fixup {
                        function package2_item3() : Unit {}
                    }
                }
            }
        "#]],
    );
}

#[test]
fn bind_items_in_qubit_use_block() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {
                    use q = Qubit() {
                        function C() : Unit {}
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    use local13 = Qubit() {
                        function package2_item2() : Unit {}
                    }
                }
            }
        "#]],
    );
}

#[test]
fn use_bound_item_in_another_bound_item() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {
                    function C() : Unit {
                        D();
                    }
                    function D() : Unit {}
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    function package2_item2() : Unit {
                        package2_item3();
                    }
                    function package2_item3() : Unit {}
                }
            }
        "#]],
    );
}

#[test]
fn use_unbound_generic() {
    check(
        indoc! {"
            namespace A {
                function B<'T>(x: 'U) : 'U {
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1<param0>(local9: 'U) : 'U {
                    local9
                }
            }

            // NotFound("'U", Span { lo: 36, hi: 38 })
            // NotFound("'U", Span { lo: 42, hi: 44 })
        "#]],
    );
}

#[test]
fn resolve_local_generic() {
    check(
        indoc! {"
            namespace A {
                function B<'T>(x: 'T) : 'T {
                    x
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1<param0>(local9: param0) : param0 {
                    local9
                }
            }
        "#]],
    );
}

#[test]
fn incorrect_single_qubit_allocation_syntax_gets_extra_help() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {
                    let q = Qubit();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    let local13 = Qubit();
                }
            }

            // NotFoundQubit(Span { lo: 57, hi: 62 })
        "#]],
    );
}

#[test]
fn incorrect_qubit_array_allocation_syntax_gets_extra_help() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {
                    let q = Qubit[5];
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    let local13 = Qubit[5];
                }
            }

            // NotFoundQubit(Span { lo: 57, hi: 62 })
        "#]],
    );
}

#[test]
fn dropped_base_callable_from_unrestricted() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Base)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::all(),
        &expect![[r#"
            namespace namespace3 {
                @Config(Base)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 100, hi: 107 })
        "#]],
    );
}

#[test]
fn dropped_base_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Base)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(Base)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 100, hi: 107 })
        "#]],
    );
}

#[test]
fn dropped_not_base_callable_from_base() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not Base)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::empty(),
        &expect![[r#"
            namespace namespace3 {
                @Config(not Base)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 104, hi: 111 })
        "#]],
    );
}

#[test]
fn resolved_not_base_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not Base)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(not Base)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn dropped_base_and_not_base_callable_from_base() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Base)
                @Config(not Base)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::empty(),
        &expect![[r#"
            namespace namespace3 {
                @Config(Base)
                @Config(not Base)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 122, hi: 129 })
        "#]],
    );
}

#[test]
fn resolved_not_unrestricted_callable_from_base() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not Unrestricted)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::empty(),
        &expect![[r#"
            namespace namespace3 {
                @Config(not Unrestricted)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn resolved_not_unrestricted_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not Unrestricted)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(not Unrestricted)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn dropped_not_unrestricted_callable_from_unrestricted() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not Unrestricted)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::all(),
        &expect![[r#"
            namespace namespace3 {
                @Config(not Unrestricted)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 112, hi: 119 })
        "#]],
    );
}

#[test]
fn resolved_adaptive_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn resolved_adaptive_callable_from_unrestricted() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::all(),
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn dropped_not_higher_level_callable_from_unrestricted() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::all(),
        &expect![[r#"
            namespace namespace3 {
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 121, hi: 128 })
        "#]],
    );
}

#[test]
fn resolved_not_higher_level_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(not HigherLevelConstructs)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn resolved_not_higher_level_callable_from_base() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::empty(),
        &expect![[r#"
            namespace namespace3 {
                @Config(not HigherLevelConstructs)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn dropped_not_higher_level_and_adaptive_callable_from_base() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::empty(),
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 143, hi: 150 })
        "#]],
    );
}

#[test]
fn dropped_not_higher_level_and_adaptive_callable_from_unrestricted() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::all(),
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 143, hi: 150 })
        "#]],
    );
}

#[test]
fn resolved_not_higher_level_and_adaptive_callable_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function Dropped() : Unit {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                @Config(not HigherLevelConstructs)
                function package2_item1() : Unit {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn dropped_floating_point_from_adaptive() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(FloatingPointComputations)
                function Dropped() : Double {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            namespace namespace3 {
                @Config(FloatingPointComputations)
                function Dropped() : Double {}

                function package2_item1() : Unit {
                    Dropped();
                }
            }

            // NotAvailable("Dropped", "A.Dropped", Span { lo: 123, hi: 130 })
        "#]],
    );
}

#[test]
fn resolved_adaptive_and_integer_from_adaptive_and_integer() {
    check_with_capabilities(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                @Config(IntegerComputations)
                function Dropped() : Double {}

                function B() : Unit {
                    Dropped();
                }
            }
        "},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                @Config(IntegerComputations)
                function package2_item1() : Double {}

                function package2_item2() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn multiple_definition_dropped_is_not_found() {
    check(
        indoc! {"
            namespace A {
                @Config(Adaptive)
                operation B() : Unit {}
                @Config(Base)
                operation B() : Unit {}
                @Config(Base)
                operation C() : Unit {}
                @Config(Adaptive)
                operation C() : Unit {}
            }
            namespace D {
                operation E() : Unit {
                    B();
                    C();
                }
                operation F() : Unit {
                    open A;
                    B();
                    C();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                @Config(Adaptive)
                operation package2_item1() : Unit {}
                @Config(Base)
                operation B() : Unit {}
                @Config(Base)
                operation C() : Unit {}
                @Config(Adaptive)
                operation package2_item2() : Unit {}
            }
            namespace namespace4 {
                operation package2_item4() : Unit {
                    B();
                    C();
                }
                operation package2_item5() : Unit {
                    open namespace3;
                    package2_item1();
                    package2_item2();
                }
            }

            // NotFound("B", Span { lo: 257, hi: 258 })
            // NotFound("C", Span { lo: 270, hi: 271 })
        "#]],
    );
}

#[test]
fn disallow_duplicate_intrinsic() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {
                    body intrinsic;
                }
            }
            namespace B {
                operation B() : Unit {
                    body intrinsic;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    body intrinsic;
                }
            }
            namespace namespace4 {
                operation package2_item3() : Unit {
                    body intrinsic;
                }
            }

            // DuplicateIntrinsic("B", Span { lo: 101, hi: 102 })
        "#]],
    );
}

#[test]
fn disallow_duplicate_intrinsic_and_non_intrinsic_collision() {
    check(
        indoc! {"
            namespace A {
                internal operation C() : Unit {
                    body intrinsic;
                }
            }
            namespace B {
                operation C() : Unit {}
            }
            namespace B {
                operation C() : Unit {
                    body intrinsic;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                internal operation package2_item1() : Unit {
                    body intrinsic;
                }
            }
            namespace namespace4 {
                operation package2_item3() : Unit {}
            }
            namespace namespace4 {
                operation package2_item5() : Unit {
                    body intrinsic;
                }
            }

            // Duplicate("C", "B", Span { lo: 154, hi: 155 })
            // DuplicateIntrinsic("C", Span { lo: 154, hi: 155 })
        "#]],
    );
}

#[test]
fn disallow_duplicate_intrinsic_and_simulatableintrinsic() {
    check(
        indoc! {"
            namespace A {
                operation C() : Unit {
                    body intrinsic;
                }
            }
            namespace B {
                @SimulatableIntrinsic()
                operation C() : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    body intrinsic;
                }
            }
            namespace namespace4 {
                @SimulatableIntrinsic()
                operation package2_item3() : Unit {}
            }

            // DuplicateIntrinsic("C", Span { lo: 129, hi: 130 })
        "#]],
    );
}

#[allow(clippy::cast_possible_truncation)]
fn check_locals(input: &str, expect: &Expect) {
    let parts = input.split('↘').collect::<Vec<_>>();
    assert_eq!(
        parts.len(),
        2,
        "input must contain exactly one cursor marker"
    );
    let cursor_offset = parts[0].len() as u32;
    let source = parts.join("");

    let (_, _, locals, _, _) = compile(
        &source,
        LanguageFeatures::default(),
        TargetCapabilityFlags::all(),
    );

    let locals = locals.get_all_at_offset(cursor_offset);
    let actual = locals.iter().fold(String::new(), |mut output, l| {
        let _ = match l {
            Local::Item(item_id, name) => writeln!(output, "{name} ({item_id})"),
            Local::TyParam(param_id, name) => writeln!(output, "{name} (ty_param {param_id})"),
            Local::Var(node_id, name) => writeln!(output, "{name} (var {node_id})"),
            Local::NamespaceImport(namespace_id, Some(alias)) => {
                writeln!(output, "{alias} (namespace {})", usize::from(namespace_id))
            }
            Local::NamespaceImport(namespace_id, None) => {
                writeln!(output, "namespace {}", usize::from(namespace_id))
            }
        };
        output
    });

    expect.assert_eq(&actual);
}

#[test]
fn get_locals_vars() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    ↘
                    let y = 0;
                }
            }
        "},
        &expect![[r#"
            x (var 13)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_vars_shadowing_same_scope() {
    check_locals(
        indoc! {r#"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    let x = "foo";
                    ↘
                }
            }
        "#},
        &expect![[r#"
            x (var 17)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_vars_parent_scope() {
    check_locals(
        indoc! {r#"
            namespace Foo {
                function A() : Int {
                    let x = 0;
                    {
                        let y = 0;
                        ↘
                    }
                }
            }
        "#},
        &expect![[r#"
            y (var 20)
            x (var 13)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_params() {
    check_locals(
        indoc! {r#"
            namespace Foo {
                function A(x : Int) : Int {
                    ↘
                }
            }
        "#},
        &expect![[r#"
            x (var 8)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_spec_params() {
    check_locals(
        indoc! {"
            namespace Foo {
                operation A(q : Qubit) : (Qubit[], Qubit) {
                    controlled (cs, ...) {
                        ↘
                    }
                }
            }
        "},
        &expect![[r#"
            cs (var 23)
            q (var 8)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_before_binding() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    let y = 0;
                    let x = { ↘ };
                }
            }
        "},
        &expect![[r#"
            y (var 13)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_lambda_params() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    let y = 0;
                    let f = x -> { ↘ };
                }
            }
        "},
        &expect![[r#"
            x (var 20)
            y (var 13)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_for_loop() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    for x in 0..1 {
                        ↘
                    }
                }
            }
        "},
        &expect![[r#"
            x (var 14)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_for_loop_before_binding() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    for x in 0..{ ↘ } {
                    }
                }
            }
        "},
        &expect![[r#"
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_items() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    ↘
                    function B() : Unit {}
                    newtype Bar = String;
                }
            }
        "},
        &expect![[r#"
            Bar (Item 3 (Package 2))
            B (Item 2 (Package 2))
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_local_item_hide_parent_scope_variables() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    let x = 3;
                    function B() : Unit {
                        let y = 3;
                        ↘
                    }
                }
            }
        "},
        &expect![[r#"
            y (var 26)
            B (Item 2 (Package 2))
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_shadow_parent_scope() {
    check_locals(
        indoc! {r#"
            namespace Foo {
                function A() : Unit {
                    let x = "foo";
                    {
                        let x = 0;
                        ↘
                    }
                }
            }
        "#},
        &expect![[r#"
            x (var 20)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_type_params() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A<'T>(t: 'T) : Unit {
                    {
                        ↘
                    }
                }
            }
        "},
        &expect![[r#"
            t (var 9)
            'T (ty_param 0)
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_block_scope_boundary() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    {
                        let x = 0;
                    }↘
                }
            }
        "},
        &expect![[r#"
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_block_scope_boundary_begin() {
    check_locals(
        indoc! {"
            namespace Foo {
                function A() : Int {
                    ↘{
                        function Bar(): Unit {}
                    }
                }
            }
        "},
        &expect![[r#"
            namespace 3
        "#]],
    );
}

#[test]
fn get_locals_item_imports() {
    check_locals(
        indoc! {"
            namespace Bar {
                function A() : Unit {}
            }
            namespace Foo {
                import Bar.A;
                import Bar.A as B;
                function C() : Int {
                    ↘
                }
            }
        "},
        &expect![[r#"
            A (Item 1 (Package 2))
            B (Item 1 (Package 2))
            namespace 4
        "#]],
    );
}

#[test]
fn get_locals_namespace_imports() {
    check_locals(
        indoc! {"
            namespace Bar {}
            namespace Foo {
                import Bar;
                import Bar as Baz;
                import Bar.*;
                function A() : Int {
                    ↘
                }
            }
        "},
        &expect![[r#"
            namespace 4
            namespace 3
            Bar (namespace 3)
            Baz (namespace 3)
        "#]],
    );
}

#[test]
fn use_after_scope() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    {
                        let x = 42;
                    }
                    x; // x should not be accessible here
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    {
                        let local16 = 42;
                    }
                    x; // x should not be accessible here
                }
            }

            // NotFound("x", Span { lo: 94, hi: 95 })
        "#]],
    );
}

#[test]
fn nested_function_definition() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    function B() : Unit {
                        function C() : Unit {}
                        C();
                    }
                    B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    function package2_item2() : Unit {
                        function package2_item3() : Unit {}
                        package2_item3();
                    }
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn variable_in_nested_blocks() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    {
                        let x = 10;
                        {
                            let y = x + 5;
                            y; // Should be accessible
                        }
                        y; // Should not be accessible
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    {
                        let local16 = 10;
                        {
                            let local23 = local16 + 5;
                            local23; // Should be accessible
                        }
                        y; // Should not be accessible
                    }
                }
            }

            // NotFound("y", Span { lo: 190, hi: 191 })
        "#]],
    );
}

#[test]
fn function_call_with_namespace_alias() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }
            namespace Bar {
                open Foo as F;
                function B() : Unit {
                    F.A();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                open namespace3 as F;
                function package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn type_alias_in_function_scope() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    newtype MyInt = Int;
                    let x : MyInt = MyInt(5);
                }
                function B() : Unit {
                    let z: MyInt = MyInt(5); // this should be a different type (and unresolved)
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    newtype package2_item3 = Int;
                    let local20 : package2_item3 = package2_item3(5);
                }
                function package2_item2() : Unit {
                    let local40: MyInt = MyInt(5); // this should be a different type (and unresolved)
                }
            }

            // NotFound("MyInt", Span { lo: 152, hi: 157 })
            // NotFound("MyInt", Span { lo: 160, hi: 165 })
        "#]],
    );
}

#[test]
fn lambda_inside_lambda() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {
                    let f = () -> {
                        let g = (x) -> x + 1;
                        g(10);
                    };
                    f();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {
                    let local13 = () -> {
                        let local20 = (local24) -> local24 + 1;
                        local20(10);
                    };
                    local13();
                }
            }
        "#]],
    );
}

#[test]
fn nested_namespaces_with_same_function_name() {
    check(
        indoc! {"
            namespace Foo {
                function A() : Unit {}
            }
            namespace Bar {
                function A() : Unit {}
                function B() : Unit {
                    Foo.A();
                    A(); // Should call Bar.A without needing to qualify
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                function package2_item3() : Unit {}
                function package2_item4() : Unit {
                    package2_item1();
                    package2_item3(); // Should call Bar.A without needing to qualify
                }
            }
        "#]],
    );
}

#[test]
fn newtype_with_invalid_field_type() {
    check(
        indoc! {"
            namespace Foo {
                newtype Complex = (Re: Real, Im: Imaginary); // Imaginary is not a valid type
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (Re: Real, Im: Imaginary); // Imaginary is not a valid type
            }

            // NotFound("Real", Span { lo: 43, hi: 47 })
            // NotFound("Imaginary", Span { lo: 53, hi: 62 })
        "#]],
    );
}

#[test]
fn newtype_with_tuple_destructuring() {
    check(
        indoc! {"
            namespace Foo {
                newtype Pair = (First: Int, Second: Int);
                function Destructure(pair: Pair) : Int {
                    let (first, second) = pair;
                    first + second
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First: Int, Second: Int);
                function package2_item2(local21: package2_item1) : Int {
                    let (local32, local34) = local21;
                    local32 + local34
                }
            }
        "#]],
    );
}

#[test]
fn items_resolve_according_to_implicit_hierarchy() {
    check(
        indoc! {"
namespace Foo {
  @EntryPoint()
  function Main(): Int {
    Foo()
  }

  function Foo() : Int {
    Bar.Baz.Quux()
  }
}

namespace Foo.Bar.Baz {
  function Quux() : Int { 6 }
}
"},
        &expect![[r#"
            namespace namespace3 {
              @EntryPoint()
              function package2_item1(): Int {
                package2_item2()
              }

              function package2_item2() : Int {
                package2_item4()
              }
            }

            namespace namespace5 {
              function package2_item4() : Int { 6 }
            }
        "#]],
    );
}

#[test]
fn basic_hierarchical_namespace() {
    check(
        indoc! {"
    namespace Foo.Bar.Baz {
        operation Quux() : Unit {}
    }
    namespace A {
        open Foo;
        operation Main() : Unit {
            Bar.Baz.Quux();
        }
    }
    namespace B {
        open Foo.Bar;
        operation Main() : Unit {
            Baz.Quux();
        }
    }"},
        &expect![[r#"
            namespace namespace5 {
                operation package2_item1() : Unit {}
            }
            namespace namespace6 {
                open namespace3;
                operation package2_item3() : Unit {
                    package2_item1();
                }
            }
            namespace namespace7 {
                open namespace4;
                operation package2_item5() : Unit {
                    package2_item1();
                }
            }"#]],
    );
}

#[test]
fn test_katas_shadowing_use_case() {
    check(
        indoc! {"namespace Kata {
    operation ApplyX() : Unit {
        // Do nothing.
    }
}

namespace Kata.Verification {
    operation CheckSolution() : Bool {
        let _ = Kata.ApplyX();
        let _ = ApplyX();
    }

    operation ApplyX() : Unit {}
}
" },
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    // Do nothing.
                }
            }

            namespace namespace4 {
                operation package2_item3() : Bool {
                    let _ = package2_item1();
                    let _ = package2_item4();
                }

                operation package2_item4() : Unit {}
            }
        "#]],
    );
}

#[test]
fn open_can_access_parent_scope() {
    check(
        indoc! {r#"
namespace Foo.Bar {
    operation Hello() : Unit {

    }
}

namespace Foo {
    open Bar;
    @EntryPoint()
    operation Main() : Unit {
        Hello();
    }
}"#},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {

                }
            }

            namespace namespace3 {
                open Bar;
                @EntryPoint()
                operation package2_item3() : Unit {
                    package2_item1();
                }
            }"#]],
    );
}

#[test]
fn test_export_statement() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {
                }
                export ApplyX;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                }
                export item1 as package2_item2;
            }
        "#]],
    );
}

#[test]
fn test_complicated_nested_export_statement() {
    check(
        indoc! {"
            namespace Foo {
                export Foo.Bar.Baz.Quux.HelloWorld;
            }

            namespace Foo.Bar.Baz.Quux {
                function HelloWorld() : Unit {}
            }

            namespace Foo.Bar {
                export Baz.Quux.HelloWorld;
            }

            namespace Foo.Bar.Baz {
                export Quux.HelloWorld;
            }

            namespace Foo.Bar.Graule {
                // HelloWorld should be available from all namespaces
                operation Main() : Unit {
                    Foo.Bar.Baz.Quux.HelloWorld();
                    Foo.Bar.Baz.HelloWorld();
                    Foo.Bar.HelloWorld();
                    Foo.HelloWorld();
                    open Foo;
                    HelloWorld();
                }
                // and we should be able to re-export it
                export Foo.HelloWorld;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export item3 as package2_item1;
            }

            namespace namespace6 {
                function package2_item3() : Unit {}
            }

            namespace namespace4 {
                export item3 as package2_item5;
            }

            namespace namespace5 {
                export item3 as package2_item7;
            }

            namespace namespace7 {
                // HelloWorld should be available from all namespaces
                operation package2_item9() : Unit {
                    package2_item3();
                    package2_item3();
                    package2_item3();
                    package2_item3();
                    open namespace3;
                    package2_item3();
                }
                // and we should be able to re-export it
                export item3 as package2_item10;
            }
        "#]],
    );
}

#[test]
fn exports_aware_of_opens() {
    check(
        indoc! {r#"
            namespace Foo {
                operation F() : Unit {}
            }

            namespace Main {
                open Foo;
                export F;
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }

            namespace namespace4 {
                open namespace3;
                export item1 as package2_item3;
            }
        "#]],
    );
}

#[test]
fn export_symbol_and_call_it() {
    check(
        indoc! {"
            namespace Foo {
                export Foo.Bar.Baz.Quux.Function;
            }

            namespace Foo.Bar.Baz.Quux {
                function Function() : Unit {}
            }

            namespace Main {
                open Foo;
                operation Main() : Unit {
                    Foo.Function();
                    Function();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export item3 as package2_item1;
            }

            namespace namespace6 {
                function package2_item3() : Unit {}
            }

            namespace namespace7 {
                open namespace3;
                operation package2_item5() : Unit {
                    package2_item3();
                    package2_item3();
                }
            }
        "#]],
    );
}

#[test]
fn multiple_exports() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Main {
                import Foo.ApplyX as X, Foo.ApplyY as Y;
                operation Main() : Unit {
                    X();
                    Y();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace4 {
                import item1 as package2_item4, item2 as package2_item5;
                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn no_exports() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    ApplyX();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_non_existent_symbol() {
    check(
        indoc! {"
            namespace Foo {
                export NonExistent;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export NonExistent as package2_item1;
            }

            // NotFound("NonExistent", Span { lo: 27, hi: 38 })
        "#]],
    );
}

#[test]
fn export_symbol_from_nested_namespace() {
    check(
        indoc! {"
            namespace Foo.Bar {
                operation ApplyX() : Unit {}
            }

            namespace Foo {
                export Bar.ApplyX;
            }

            namespace Main {
                open Foo;
                operation Main() : Unit {
                    Bar.ApplyX();
                    ApplyX();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
            }

            namespace namespace3 {
                export item1 as package2_item3;
            }

            namespace namespace5 {
                open namespace3;
                operation package2_item5() : Unit {
                    package2_item1();
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn disallow_exporting_local_vars() {
    check(
        indoc! {"
            namespace Foo {
                operation Main() : Unit {
                    let x = 5;
                }
                export x;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    let local13 = 5;
                }
                export x as package2_item2;
            }

            // NotFound("x", Span { lo: 82, hi: 83 })
        "#]],
    );
}

#[test]
fn disallow_export_non_item() {
    check(
        indoc! {"
            namespace Bar {}

            namespace Foo {
                operation Main() : Unit {
                }
                export Unit;
            }
        "},
        &expect![[r#"
            namespace namespace3 {}

            namespace namespace4 {
                operation package2_item2() : Unit {
                }
                export Unit as package2_item3;
            }

            // NotFound("Unit", Span { lo: 81, hi: 85 })
        "#]],
    );
}

#[test]
fn export_udt() {
    check(
        indoc! {"
            namespace Foo {
                newtype Pair = (First: Int, Second: Int);
                export Pair;
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    Pair(1, 2);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First: Int, Second: Int);
                export item1 as package2_item2;
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item4() : Unit {
                    package2_item1(1, 2);
                }
            }
        "#]],
    );
}

#[test]
fn export_with_alias() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                export ApplyX as SomeAlias;
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    SomeAlias();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                export item1 as package2_item2;
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn multiple_exports_with_aliases() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
                export ApplyX as SomeAlias, ApplyY as AnotherAlias;
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    SomeAlias();
                    AnotherAlias();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
                export item1 as package2_item3, item2 as package2_item4;
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn aliased_exports_call_with_qualified_paths() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
                export ApplyX as SomeAlias, ApplyY as AnotherAlias;
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    Foo.SomeAlias();
                    Foo.AnotherAlias();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
                export item1 as package2_item3, item2 as package2_item4;
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn reexport_from_full_path_with_alias() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                export ApplyX as SomeAlias;
            }
            namespace Main {
                open Foo;
                export Foo.SomeAlias as AnotherAlias;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                export item1 as package2_item2;
            }
            namespace namespace4 {
                open namespace3;
                export item1 as package2_item4;
            }
        "#]],
    );
}

#[test]
fn disallow_repeated_exports() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                export ApplyX;
                export ApplyX;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                export item1 as package2_item2;
                export item1 as package2_item3;
            }

            // DuplicateExport { name: "ApplyX", span: Span { lo: 79, hi: 85 }, existing_span: Span { lo: 60, hi: 66 } }
        "#]],
    );
}

#[test]
fn disallow_repeated_exports_inline() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                export ApplyX, ApplyX;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                export item1 as package2_item2, item1 as package2_item3;
            }

            // DuplicateExport { name: "ApplyX", span: Span { lo: 68, hi: 74 }, existing_span: Span { lo: 60, hi: 66 } }
        "#]],
    );
}

#[test]
fn order_of_exports_does_not_matter() {
    check(
        indoc! {"
            namespace Bar {
                export Foo.ApplyX;
                export ApplyY;
                operation ApplyY() : Unit {}
            }
            namespace Foo {
                operation ApplyX() : Unit {}
            }

        "},
        &expect![[r#"
            namespace namespace3 {
                export item5 as package2_item1;
                export item3 as package2_item2;
                operation package2_item3() : Unit {}
            }
            namespace namespace4 {
                operation package2_item5() : Unit {}
            }

        "#]],
    );
}

#[test]
fn export_udt_and_construct_it() {
    check(
        indoc! {"
            namespace Foo {
                newtype Pair = (First: Int, Second: Int);
                export Pair;
            }
            namespace Main {
                open Foo;
                operation Main() : Unit {
                    let z: Pair = Pair(1, 2);
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                newtype package2_item1 = (First: Int, Second: Int);
                export item1 as package2_item2;
            }
            namespace namespace4 {
                open namespace3;
                operation package2_item4() : Unit {
                    let local34: package2_item1 = package2_item1(1, 2);
                }
            }
        "#]],
    );
}
#[test]
fn import_single_item() {
    check(
        indoc! {"
            namespace Foo {
                function Bar() : Unit {}
            }
            namespace Main {
                import Foo.Bar;
                operation Main() : Unit {
                    Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                import item1 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_namespace() {
    check(
        indoc! {"
            namespace Foo.Bar {
                function Baz() : Unit {}
            }
            namespace Main {
                import Foo.Bar;
                operation Main() : Unit {
                    Bar.Baz();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                function package2_item1() : Unit {}
            }
            namespace namespace5 {
                import namespace4 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_non_existent_item() {
    check(
        indoc! {"
            namespace Foo {
            }
            namespace Main {
                import Foo.Bar;
                operation Main() : Unit {
                    Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
            }
            namespace namespace4 {
                import Foo.Bar as package2_item2;
                operation package2_item3() : Unit {
                    Bar();
                }
            }

            // NotFound("Foo.Bar", Span { lo: 46, hi: 53 })
            // NotFound("Bar", Span { lo: 93, hi: 96 })
        "#]],
    );
}

#[test]
fn import_name_collision_with_callable() {
    check(
        indoc! {"
            namespace Foo {
                function Bar() : Unit {}
            }
            namespace Main {
                function Bar() : Unit {}
                import Foo.Bar;
                operation Main() : Unit {
                    Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                function package2_item3() : Unit {}
                import item1 as package2_item4;
                operation package2_item5() : Unit {
                    package2_item3();
                }
            }

            // Duplicate("Bar", "Main", Span { lo: 108, hi: 111 })
        "#]],
    );
}

#[test]
fn import_with_alias() {
    check(
        indoc! {"
            namespace Foo {
                function Bar() : Unit {}
            }
            namespace Main {
                import Foo.Bar as Baz;
                operation Main() : Unit {
                    Baz();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                import item1 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_non_item() {
    check(
        indoc! {"
            namespace Main {
                import Unit;
                operation Main() : Unit {
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                import Unit as package2_item1;
                operation package2_item2() : Unit {
                }
            }

            // NotFound("Unit", Span { lo: 28, hi: 32 })
        "#]],
    );
}

#[test]
fn import_namespace_nested() {
    check(
        indoc! {"
            namespace Foo.Bar.Baz {
                operation Quux() : Unit {}
            }
            namespace Main {
                import Foo.Bar;
                operation Main() : Unit {
                    Bar.Baz.Quux();
                }
            }
        "},
        &expect![[r#"
            namespace namespace5 {
                operation package2_item1() : Unit {}
            }
            namespace namespace6 {
                import namespace4 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_single_namespace() {
    check(
        indoc! {"
            namespace Foo {
                operation Bar() : Unit {}
            }
            namespace Main {
                import Foo;

                operation Main() : Unit {
                    Foo.Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                import namespace3 as package2_item3;

                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_shadowing_function() {
    check(
        indoc! {"
            namespace Foo {
                operation Bar() : Unit {}
            }
            namespace Main {
                operation Bar() : Unit {}
                operation Main() : Unit {
                    import Foo.Bar;
                    Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                operation package2_item3() : Unit {}
                operation package2_item4() : Unit {
                    import item1 as package2_item5;
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_non_existent_namespace() {
    check(
        indoc! {"
            namespace Main {
                operation Main() : Unit {
                    import NonExistent;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    import NonExistent as package2_item2;
                }
            }

            // NotFound("NonExistent", Span { lo: 62, hi: 73 })
        "#]],
    );
}

#[test]
fn import_self() {
    check(
        indoc! {"
            namespace Main {
                operation Foo() : Unit {
                    import Foo;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    import item1 as package2_item2;
                }
            }
        "#]],
    );
}

// imports with the same name create collisions
// in the global scope
#[test]
fn import_duplicate_symbol_in_global_scope() {
    check(
        indoc! { r#"
            namespace Main {
                import Foo.Bar.Baz, Foo.Bar.Baz;
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                import item4 as package2_item1, item4 as package2_item2;
            }

            namespace namespace5 {
                operation package2_item4() : Unit {}
            }

            // Duplicate("Baz", "Main", Span { lo: 49, hi: 52 })
        "#]],
    );
}

// just like callable and UDT declarations, shadowing is
// allowed in local scopes for jupyter cell re-runnability
#[test]
fn import_duplicate_symbol_in_local_scope() {
    check(
        indoc! { r#"
            namespace Main {
                operation Main() : Unit {
                    import Foo.Bar.Baz, Foo.Bar.Baz;
                }
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    import item3 as package2_item4, item3 as package2_item5;
                }
            }

            namespace namespace5 {
                operation package2_item3() : Unit {}
            }
        "#]],
    );
}

// naming collisions in global scope
#[test]
fn import_duplicate_symbol_different_source_global_scope() {
    check(
        indoc! { r#"
            namespace Main {
                import Foo.Bar.Baz, Foo.Bar;
                import Bar.Baz;
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                import item5 as package2_item1, namespace5 as package2_item2;
                import item5 as package2_item3;
            }

            namespace namespace5 {
                operation package2_item5() : Unit {}
            }

            // Duplicate("Baz", "Main", Span { lo: 65, hi: 68 })
        "#]],
    );
}

// this should be allowed for jupyter cell re-runnability
#[test]
fn import_duplicate_symbol_different_source_local_scope() {
    check(
        indoc! { r#"
            namespace Main {
                operation Main() : Unit {
                    import Foo.Bar.Baz, Foo.Bar;
                    import Bar.Baz;
                    Baz();
                }
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    import item3 as package2_item4, namespace5 as package2_item5;
                    import item3 as package2_item6;
                    package2_item3();
                }
            }

            namespace namespace5 {
                operation package2_item3() : Unit {}
            }
        "#]],
    );
}

// this should be allowed for jupyter cell re-runnability
#[test]
fn disallow_importing_different_items_with_same_name() {
    check(
        indoc! { r#"
            namespace Main {
                import Foo.Bar.Baz, Foo.Bar.Baz2 as Baz;
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
                operation Baz2() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                import item4 as package2_item1, item5 as package2_item2;
            }

            namespace namespace5 {
                operation package2_item4() : Unit {}
                operation package2_item5() : Unit {}
            }

            // Duplicate("Baz", "Main", Span { lo: 57, hi: 60 })
        "#]],
    );
}

#[test]
fn import_takes_precedence_over_local_decl() {
    check(
        indoc! { r#"
            namespace Main {
                operation Baz() : Unit {
                    import Foo.Bar.Baz;
                    Baz();
                }
            }

            namespace Foo.Bar {
                operation Baz() : Unit {}
            }
        "# },
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {
                    import item3 as package2_item4;
                    package2_item3();
                }
            }

            namespace namespace5 {
                operation package2_item3() : Unit {}
            }
        "#]],
    );
}

#[test]
fn import_then_export() {
    check(
        indoc! {"
            namespace Foo {
                operation Bar() : Unit {}
            }
            namespace Main {
                import Foo.Bar;
                export Bar;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                import item1 as package2_item3;
                export item1 as package2_item4;
            }
        "#]],
    );
}

#[test]
fn import_namespace_advanced() {
    check(
        indoc! {"
            namespace A.B.C.D.E {
                operation DumpMachine() : Unit {}
            }
            namespace TestOne {
                import A;
                operation Main() : Unit {
                    A.B.C.D.E.DumpMachine();
                }
            }
            namespace TestTwo {
                import A.B;
                operation Main() : Unit {
                    B.C.D.E.DumpMachine();
                }
            }
            namespace TestThree {
                import A.B.C;
                operation Main() : Unit {
                    C.D.E.DumpMachine();
                }
            }
            namespace TestFour {
                import A.B.C.D;
                operation Main() : Unit {
                    D.E.DumpMachine();
                }
            }
            namespace TestFive {
                import A.B.C.D.E;
                operation Main() : Unit {
                    E.DumpMachine();
                }
            }
            namespace TestSix {
                import A.B.C.D.E.DumpMachine;
                operation Main() : Unit {
                    DumpMachine();
                }
            }
        "},
        &expect![[r#"
            namespace namespace7 {
                operation package2_item1() : Unit {}
            }
            namespace namespace8 {
                import namespace3 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
            namespace namespace9 {
                import namespace4 as package2_item6;
                operation package2_item7() : Unit {
                    package2_item1();
                }
            }
            namespace namespace10 {
                import namespace5 as package2_item9;
                operation package2_item10() : Unit {
                    package2_item1();
                }
            }
            namespace namespace11 {
                import namespace6 as package2_item12;
                operation package2_item13() : Unit {
                    package2_item1();
                }
            }
            namespace namespace12 {
                import namespace7 as package2_item15;
                operation package2_item16() : Unit {
                    package2_item1();
                }
            }
            namespace namespace13 {
                import item1 as package2_item18;
                operation package2_item19() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_namespace_does_not_open_it() {
    check(
        indoc! {"
            namespace Microsoft.Quantum.Diagnostics {
                operation DumpMachine() : Unit {}
            }
            namespace Main {
                import Microsoft.Quantum.Diagnostics;
                operation Main() : Unit {
                    Diagnostics.DumpMachine();
                    DumpMachine();
                }
            }
        "},
        &expect![[r#"
            namespace namespace5 {
                operation package2_item1() : Unit {}
            }
            namespace namespace6 {
                import namespace5 as package2_item3;
                operation package2_item4() : Unit {
                    package2_item1();
                    DumpMachine();
                }
            }

            // NotFound("DumpMachine", Span { lo: 214, hi: 225 })
        "#]],
    );
}

#[test]
fn invalid_import() {
    check(
        indoc! {"
            namespace Main {
                import A.B.C;
                operation Main() : Unit {
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                import A.B.C as package2_item1;
                operation package2_item2() : Unit {
                }
            }

            // NotFound("A.B.C", Span { lo: 28, hi: 33 })
        "#]],
    );
}

#[test]
fn export_namespace() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Main {
                export Foo;
            }
            namespace Test {
                open Main.Foo;
                operation Main() : Unit {
                    ApplyX();
                    ApplyY();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace4 {
                export namespace3 as package2_item4;
            }
            namespace namespace5 {
                open namespace3;
                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn export_namespace_contains_children() {
    check(
        indoc! {"
            namespace Foo.Bar {
                operation ApplyX() : Unit {}
            }
            namespace Main {
                export Foo;
            }
            namespace Test {
                open Main.Foo.Bar;
                operation Main() : Unit {
                    ApplyX();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
            }
            namespace namespace5 {
                export namespace3 as package2_item3;
            }
            namespace namespace6 {
                open namespace4;
                operation package2_item5() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_namespace_cyclic() {
    check(
        indoc! {"
            namespace Foo {
                export Bar;
            }
            namespace Bar {
                export Foo;
                operation Hello() : Unit {}
            }
            namespace Main {
                open Foo.Bar.Foo.Bar.Foo.Bar;
                import Foo.Bar.Foo.Bar.Foo.Bar;
                operation Main() : Unit { Hello(); Bar.Hello(); }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export namespace4 as package2_item1;
            }
            namespace namespace4 {
                export namespace3 as package2_item3;
                operation package2_item4() : Unit {}
            }
            namespace namespace5 {
                open namespace4;
                import namespace4 as package2_item6;
                operation package2_item7() : Unit { package2_item4(); package2_item4(); }
            }
        "#]],
    );
}

#[test]
fn export_direct_cycle() {
    check(
        indoc! {"
            namespace Foo {
                export Foo;
            }

            namespace Main {
                open Foo.Foo.Foo.Foo.Foo;
                operation Main() : Unit { }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export namespace3 as package2_item1;
            }

            namespace namespace4 {
                open namespace3;
                operation package2_item3() : Unit { }
            }
        "#]],
    );
}

#[test]
fn export_item_cycle() {
    check(
        indoc! {"
            namespace Foo {
                export AA as A;
                export A as AA;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export AA as  as package2_item1;
                export A as  as package2_item2;
            }

            // NotFound("AA", Span { lo: 27, hi: 29 })
            // NotFound("A", Span { lo: 47, hi: 48 })
        "#]],
    );
}

#[test]
fn export_namespace_with_alias() {
    check(
        indoc! {"
            namespace Foo.Bar {
                operation ApplyX() : Unit {}
            }
            namespace Main {
                export Foo.Bar as Baz;
            }
            namespace Test {
                open Main.Baz;
                operation Main() : Unit {
                    ApplyX();
                    Main.Baz.ApplyX();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
            }
            namespace namespace5 {
                export namespace4 as package2_item3;
            }
            namespace namespace6 {
                open namespace4;
                operation package2_item5() : Unit {
                    package2_item1();
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn import_glob() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Main {
                import Foo.*;
                operation Main() : Unit {
                    ApplyX();
                    ApplyY();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace4 {
                import Foo.*;
                operation package2_item4() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn import_aliased_glob() {
    check(
        indoc! {"
            namespace Foo {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Main {
                import Foo as Bar;
                operation Main() : Unit {
                    Bar.ApplyX();
                    Bar.ApplyY();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace4 {
                import namespace3 as package2_item4;
                operation package2_item5() : Unit {
                    package2_item1();
                    package2_item2();
                }
            }
        "#]],
    );
}

#[test]
fn import_glob_in_list() {
    check(
        indoc! {"
            namespace Foo.Bar {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Foo.Bar.Baz {
                operation ApplyZ() : Unit {}
            }
            namespace Main {
                import Foo.Bar.*, Foo.Bar.Baz.ApplyZ;
                operation Main() : Unit {
                    ApplyX();
                    ApplyY();
                    Baz.ApplyZ();
                    ApplyZ();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace5 {
                operation package2_item4() : Unit {}
            }
            namespace namespace6 {
                import Foo.Bar.*, item4 as package2_item6;
                operation package2_item7() : Unit {
                    package2_item1();
                    package2_item2();
                    package2_item4();
                    package2_item4();
                }
            }
        "#]],
    );
}

#[test]
fn import_glob_in_list_with_alias() {
    check(
        indoc! {"
            namespace Foo.Bar {
                operation ApplyX() : Unit {}
                operation ApplyY() : Unit {}
            }
            namespace Foo.Bar.Baz {
                operation ApplyZ() : Unit {}
            }
            namespace Main {
                import Foo.Bar as Alias, Foo.Bar.Baz.ApplyZ as Foo;
                operation Main() : Unit {
                    Alias.ApplyX();
                    Alias.ApplyY();
                    Alias.Baz.ApplyZ();
                    Foo();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {}
            }
            namespace namespace5 {
                operation package2_item4() : Unit {}
            }
            namespace namespace6 {
                import namespace4 as package2_item6, item4 as package2_item7;
                operation package2_item8() : Unit {
                    package2_item1();
                    package2_item2();
                    package2_item4();
                    package2_item4();
                }
            }
        "#]],
    );
}

#[test]
fn import_newtype() {
    check(
        indoc! {r#"
            namespace Foo {
                import Bar.NewType; // no error

                operation FooOperation() : Unit {
                    let x: NewType = NewType("a");
                }
            }

            namespace Bar {
                newtype NewType = String;
                export NewType;
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                import item4 as package2_item1; // no error

                operation package2_item2() : Unit {
                    let local17: package2_item4 = package2_item4("a");
                }
            }

            namespace namespace4 {
                newtype package2_item4 = String;
                export item4 as package2_item5;
            }
        "#]],
    );
}

#[test]
fn glob_import_ns_not_found() {
    check(
        indoc! {r#"
            namespace Main {
                import Bar.*;
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                import Bar.*;
            }

            // NotFound("Bar", Span { lo: 28, hi: 31 })
        "#]],
    );
}

#[test]
fn allow_export_of_namespace_within_itself() {
    check(
        indoc! {r#"
            namespace Foo {
                export Foo;
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                export namespace3 as package2_item1;
            }
        "#]],
    );
}

#[test]
fn export_of_item_with_same_name_as_namespace_resolves_to_item() {
    check(
        indoc! {r#"
            namespace Foo {
                operation Foo() : Unit {}
                export Foo;
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                export item1 as package2_item2;
            }
        "#]],
    );
}

#[test]
fn export_of_item_with_same_name_as_namespace_resolves_to_item_even_when_before_item() {
    check(
        indoc! {r#"
            namespace Foo {
                export Foo;
                operation Foo() : Unit {}
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                export item2 as package2_item1;
                operation package2_item2() : Unit {}
            }
        "#]],
    );
}

#[test]
fn ty_param_name_is_in_scope() {
    check(
        indoc! {r#"
            namespace Foo {
                operation Foo<'T: Eq>(a: 'T) : Unit {
                    let x: 'T = a;
                }
            }
        "#},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1<param0: Eq>(local10: param0) : Unit {
                    let local19: param0 = local10;
                }
            }
        "#]],
    );
}

#[test]
fn export_namespace_by_name_only() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {}
            }

            namespace C {
                export A;
            }

            namespace D {
                import C.A as CA;

                operation E() : Unit {
                    C.A.B();
                    CA.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }

            namespace namespace4 {
                export namespace3 as package2_item3;
            }

            namespace namespace5 {
                import namespace3 as package2_item5;

                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_comes_before_open() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {}
            }

            namespace C {
                export B;
                open A;
            }

            namespace D {
                function E() : Unit {
                    C.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                export item1 as package2_item3;
                open namespace3;
            }

            namespace namespace5 {
                function package2_item5() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_comes_before_glob_import() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {}
            }

            namespace C {
                export B;
                import A.*;
            }

            namespace D {
                function E() : Unit {
                    C.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                export item1 as package2_item3;
                import A.*;
            }

            namespace namespace5 {
                function package2_item5() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_comes_after_reexport() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {}
            }

            namespace D {
                export C.B;
            }

            namespace C {
                export A.B;
            }

            namespace E {
                operation F() : Unit {
                    D.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                export item1 as package2_item3;
            }

            namespace namespace5 {
                export item1 as package2_item5;
            }

            namespace namespace6 {
                operation package2_item7() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn using_callable_export_as_ty() {
    check(
        indoc! {"
            namespace A {
                function B() : Unit {}
            }

            namespace C {
                export A.B;
            }

            namespace E {
                operation F() : Unit {
                    let x: C.B = 1;
                    C.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }

            namespace namespace4 {
                export item1 as package2_item3;
            }

            namespace namespace5 {
                operation package2_item5() : Unit {
                    let local29: namespace4.B = 1;
                    package2_item1();
                }
            }

            // NotFound("C.B", Span { lo: 133, hi: 136 })
        "#]],
    );
}

#[test]
fn unresolved_local_import() {
    check(
        indoc! {"
            namespace Main {
                operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;
            }

            namespace Bar {
                operation Main() : Unit {
                    import Main.Main.Foo;

                    operation ZZZ() : Unit {
                        Foo(10, true);
                    }
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1(local8: Int, local13: Bool) : Int {
                    local8
                }
                export item1 as package2_item2;
            }

            namespace namespace4 {
                operation package2_item4() : Unit {
                    import Main.Main.Foo as package2_item5;

                    operation package2_item6() : Unit {
                        Foo(10, true);
                    }
                }
            }

            // NotFound("Main.Main.Foo", Span { lo: 156, hi: 169 })
            // NotFound("Foo", Span { lo: 217, hi: 220 })
        "#]],
    );
}

#[test]
fn wildcard_import_child_namespace() {
    check(
        indoc! {r#"
            namespace Foo.Bar {
                operation Hello() : Unit {
                }
            }

            namespace Foo {
                import Bar.*;

                @EntryPoint()
                operation Main() : Unit {
                    Hello();
                }
            }
        "#},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {
                }
            }

            namespace namespace3 {
                import Bar.*;

                @EntryPoint()
                operation package2_item3() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn direct_import_child_namespace() {
    check(
        indoc! {r#"
            namespace Foo.Bar {
                operation Hello() : Unit {
                }
            }

            namespace Foo {
                import Bar as Baz;

                @EntryPoint()
                operation Main() : Unit {
                    Baz.Hello();
                }
            }
        "#},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {
                }
            }

            namespace namespace3 {
                import namespace4 as package2_item3;

                @EntryPoint()
                operation package2_item4() : Unit {
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_above_declaration() {
    check(
        indoc! {"
            namespace Foo {
                export ApplyX;
                operation ApplyX() : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                export item2 as package2_item1;
                operation package2_item2() : Unit {}
            }
        "#]],
    );
}

#[test]
fn export_name_collision_separate_namespace_declarations() {
    check(
        indoc! {"
            namespace A {
                function Foo() : Unit {}
            }
            namespace B {
                function Foo() : Unit {}
            }
            namespace C {
                export A.Foo;
            }
            namespace C {
                export B.Foo;
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                function package2_item3() : Unit {}
            }
            namespace namespace5 {
                export item1 as package2_item5;
            }
            namespace namespace5 {
                export item3 as package2_item7;
            }

            // Duplicate("Foo", "C", Span { lo: 151, hi: 154 })
        "#]],
    );
}

#[test]
fn import_shadowing_local_scope() {
    check(
        indoc! {"
            namespace Foo {
                function Bar() : Unit {}
            }
            namespace Main {
                function Bar() : Unit {}
                operation Main() : Unit {
                    import Foo.Bar;
                    Bar();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                function package2_item1() : Unit {}
            }
            namespace namespace4 {
                function package2_item3() : Unit {}
                operation package2_item4() : Unit {
                    import item1 as package2_item5;
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_from_local_scope_disallowed() {
    check(
        indoc! {"
            namespace Main {
                operation Foo() : Unit {}
                operation Main() : Unit {
                    export Foo;
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
                operation package2_item2() : Unit {
                    export Foo as package2_item3;
                }
            }

            // ExportFromLocalScope(Span { lo: 85, hi: 96 })
        "#]],
    );
}

#[test]
fn export_item_clobbers_namespace() {
    check(
        indoc! {"
            namespace A.B.C {
                operation X() : Unit {}
            }
            namespace A {
                operation D() : Unit {}
                export D as B;
            }
        "},
        &expect![[r#"
            namespace namespace5 {
                operation package2_item1() : Unit {}
            }
            namespace namespace3 {
                operation package2_item3() : Unit {}
                export item3 as package2_item4;
            }

            // Duplicate("B", "A", Span { lo: 106, hi: 107 })
        "#]],
    );
}

#[test]
fn export_namespace_clobbers_namespace() {
    check(
        indoc! {"
            namespace A.B.C {
                operation X() : Unit {}
            }
            namespace A.D {
                operation E() : Unit {}
            }
            namespace A {
                export D as B;
            }
        "},
        &expect![[r#"
            namespace namespace5 {
                operation package2_item1() : Unit {}
            }
            namespace namespace6 {
                operation package2_item3() : Unit {}
            }
            namespace namespace3 {
                export namespace6 as package2_item5;
            }

            // Duplicate("B", "A", Span { lo: 124, hi: 125 })
        "#]],
    );
}

#[test]
fn import_namespace_existing_namespace_collision() {
    check(
        indoc! {"
            namespace A.B {
                operation C() : Unit {}
            }
            namespace A.D {
                operation E() : Unit {}
            }
            namespace A {
                import D as B;
                operation F (): Unit {
                    B.E();
                    B.C();
                }
            }
        "},
        &expect![[r#"
            namespace namespace4 {
                operation package2_item1() : Unit {}
            }
            namespace namespace5 {
                operation package2_item3() : Unit {}
            }
            namespace namespace3 {
                import namespace5 as package2_item5;
                operation package2_item6 (): Unit {
                    B.E();
                    package2_item1();
                }
            }

            // Duplicate("B", "A", Span { lo: 122, hi: 123 })
            // NotFound("B.E", Span { lo: 160, hi: 163 })
        "#]],
    );
}

#[test]
fn item_name_collision_namespace_part() {
    check(
        indoc! {"
            namespace A.B.C {
            }
            namespace A {
                operation B() : Unit {}
            }
        "},
        &expect![[r#"
            namespace namespace5 {
            }
            namespace namespace3 {
                operation package2_item2() : Unit {}
            }

            // Duplicate("B", "A", Span { lo: 48, hi: 49 })
        "#]],
    );
}

#[test]
fn chained_import_namespace() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {}
            }
            namespace C {
                operation D() : Unit {
                    import A as E;
                    import E as F;
                    A.B();
                    F.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                operation package2_item3() : Unit {
                    import namespace3 as package2_item4;
                    import namespace3 as package2_item5;
                    package2_item1();
                    package2_item1();
                }
            }
        "#]],
    );
}

#[test]
fn export_imported_namespace() {
    check(
        indoc! {"
            namespace A {
                operation B() : Unit {}
            }
            namespace C {
                import A as E;
                export E as F;
            }
            namespace G {
                operation D() : Unit {
                    A.B();
                    C.F.B();
                }
            }
        "},
        &expect![[r#"
            namespace namespace3 {
                operation package2_item1() : Unit {}
            }
            namespace namespace4 {
                import namespace3 as package2_item3;
                export namespace3 as package2_item4;
            }
            namespace namespace5 {
                operation package2_item6() : Unit {
                    package2_item1();
                    package2_item1();
                }
            }
        "#]],
    );
}
