use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use rustc_hash::FxHashMap;
use std::rc::Rc;

use super::replace_get_config_calls;
use qsc_eval::val::Value;

fn check(file: &str, config: &FxHashMap<Rc<str>, Value>, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, TargetCapabilityFlags::all()));
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[(std, None)],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = replace_get_config_calls(store.core(), &mut unit.package, config);
    assert!(errors.is_empty(), "{errors:?}");
    expect.assert_eq(&unit.package.to_string());
}

fn check_error(file: &str, config: &FxHashMap<Rc<str>, Value>, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, TargetCapabilityFlags::all()));
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[(std, None)],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = replace_get_config_calls(store.core(), &mut unit.package, config);
    expect.assert_debug_eq(&errors);
}

#[test]
fn folds_configured_string_get_config_call() {
    check(
        indoc! {r#"
        operation Main() : String {
            Std.Core.ConfigValue("key", "default")
        }
        "#},
        &[(Rc::from("key"), Value::String(Rc::from("configured")))]
            .into_iter()
            .collect(),
        &expect![[r#"
            Package:
                Item 0 [0-72] (Public):
                    Namespace (Ident 11 [0-72] "test"): Item 1
                Item 1 [0-72] (Internal):
                    Parent: 0
                    Callable 0 [0-72] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: String
                        functors: empty set
                        body: SpecDecl 3 [0-72]: Impl:
                            Block 4 [26-72] [Type String]:
                                Stmt 5 [32-70]: Expr: Expr 6 [32-70] [Type String]: String:
                                    Lit: "configured"
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn folds_configured_int_get_config_call() {
    check(
        indoc! {"
        operation Main() : Int {
            Std.Core.ConfigValue(\"key\", 0)
        }
        "},
        &[(Rc::from("key"), Value::Int(1))].into_iter().collect(),
        &expect![[r#"
            Package:
                Item 0 [0-61] (Public):
                    Namespace (Ident 11 [0-61] "test"): Item 1
                Item 1 [0-61] (Internal):
                    Parent: 0
                    Callable 0 [0-61] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [0-61]: Impl:
                            Block 4 [23-61] [Type Int]:
                                Stmt 5 [29-59]: Expr: Expr 6 [29-59] [Type Int]: Lit: Int(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn folds_configured_bool_get_config_call() {
    check(
        indoc! {"
        operation Main() : Bool {
            Std.Core.ConfigValue(\"key\", false)
        }
        "},
        &[(Rc::from("key"), Value::Bool(true))].into_iter().collect(),
        &expect![[r#"
            Package:
                Item 0 [0-66] (Public):
                    Namespace (Ident 11 [0-66] "test"): Item 1
                Item 1 [0-66] (Internal):
                    Parent: 0
                    Callable 0 [0-66] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Bool
                        functors: empty set
                        body: SpecDecl 3 [0-66]: Impl:
                            Block 4 [24-66] [Type Bool]:
                                Stmt 5 [30-64]: Expr: Expr 6 [30-64] [Type Bool]: Lit: Bool(true)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn folds_configured_double_get_config_call() {
    check(
        indoc! {"
        operation Main() : Double {
            Std.Core.ConfigValue(\"key\", 0.0)
        }
        "},
        &[(Rc::from("key"), Value::Double(1.0))]
            .into_iter()
            .collect(),
        &expect![[r#"
            Package:
                Item 0 [0-66] (Public):
                    Namespace (Ident 11 [0-66] "test"): Item 1
                Item 1 [0-66] (Internal):
                    Parent: 0
                    Callable 0 [0-66] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Double
                        functors: empty set
                        body: SpecDecl 3 [0-66]: Impl:
                            Block 4 [26-66] [Type Double]:
                                Stmt 5 [32-64]: Expr: Expr 6 [32-64] [Type Double]: Lit: Double(1)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn folds_default_get_config_call() {
    check(
        indoc! {"
        operation Main() : Int {
            Std.Core.ConfigValue(\"missing\", 123)
        }
        "},
        &FxHashMap::default(),
        &expect![[r#"
            Package:
                Item 0 [0-67] (Public):
                    Namespace (Ident 11 [0-67] "test"): Item 1
                Item 1 [0-67] (Internal):
                    Parent: 0
                    Callable 0 [0-67] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [0-67]: Impl:
                            Block 4 [23-67] [Type Int]:
                                Stmt 5 [29-65]: Expr: Expr 6 [29-65] [Type Int]: Lit: Int(123)
                        adj: <none>
                        ctl: <none>
                        ctl-adj: <none>"#]],
    );
}

#[test]
fn rejects_non_literal_get_config_argument() {
    check_error(
        indoc! {"
        operation Main() : Unit {
            let key = \"key\";
            let value = Std.Core.ConfigValue(key, 0);
        }
        "},
        &FxHashMap::default(),
        &expect![[r#"
            [
                NonLiteralArgument(
                    Span {
                        lo: 84,
                        hi: 87,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn rejects_get_config_value_with_wrong_type() {
    check_error(
        indoc! {"
        operation Main() : Unit {
            let value = Std.Core.ConfigValue(\"key\", 0);
        }
        "},
        &[(Rc::from("key"), Value::Bool(true))].into_iter().collect(),
        &expect![[r#"
            [
                TypeMismatch(
                    Span {
                        lo: 70,
                        hi: 71,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn rejects_config_value_with_unsupported_type() {
    check_error(
        indoc! {"
        operation Main() : Unit {
            let value = Std.Core.ConfigValue(\"key\", 0);
        }
        "},
        &[(Rc::from("key"), Value::unit())].into_iter().collect(),
        &expect![[r#"
            [
                UnsupportedType(
                    Span {
                        lo: 70,
                        hi: 71,
                    },
                ),
            ]
        "#]],
    );
}
