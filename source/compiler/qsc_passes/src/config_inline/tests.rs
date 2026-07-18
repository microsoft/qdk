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

fn check(file: &str, config: FxHashMap<Rc<str>, Value>, expect: &Expect) {
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

    let errors = replace_get_config_calls(store.core(), &mut unit.package, &config);
    assert!(errors.is_empty(), "{errors:?}");
    expect.assert_eq(&unit.package.to_string());
}

fn check_error(file: &str, config: FxHashMap<Rc<str>, Value>, expect: &Expect) {
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

    let errors = replace_get_config_calls(store.core(), &mut unit.package, &config);
    expect.assert_debug_eq(&errors);
}

#[test]
fn folds_configured_string_get_config_call() {
    check(
        indoc! {r#"
        operation Main() : String {
            Std.Core.GetConfig("key", "default")
        }
        "#},
        [(Rc::from("key"), Value::String(Rc::from("configured")))]
            .into_iter()
            .collect(),
        &expect![[r#"
            Package:
                Item 0 [0-70] (Public):
                    Namespace (Ident 11 [0-70] "test"): Item 1
                Item 1 [0-70] (Internal):
                    Parent: 0
                    Callable 0 [0-70] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: String
                        functors: empty set
                        body: SpecDecl 3 [0-70]: Impl:
                            Block 4 [26-70] [Type String]:
                                Stmt 5 [32-68]: Expr: Expr 6 [32-68] [Type String]: String:
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
            Std.Core.GetConfig(\"key\", 0)
        }
        "},
        [(Rc::from("key"), Value::Int(1))].into_iter().collect(),
        &expect![[r#"
            Package:
                Item 0 [0-59] (Public):
                    Namespace (Ident 11 [0-59] "test"): Item 1
                Item 1 [0-59] (Internal):
                    Parent: 0
                    Callable 0 [0-59] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [0-59]: Impl:
                            Block 4 [23-59] [Type Int]:
                                Stmt 5 [29-57]: Expr: Expr 6 [29-57] [Type Int]: Lit: Int(1)
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
            Std.Core.GetConfig(\"key\", false)
        }
        "},
        [(Rc::from("key"), Value::Bool(true))].into_iter().collect(),
        &expect![[r#"
            Package:
                Item 0 [0-64] (Public):
                    Namespace (Ident 11 [0-64] "test"): Item 1
                Item 1 [0-64] (Internal):
                    Parent: 0
                    Callable 0 [0-64] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Bool
                        functors: empty set
                        body: SpecDecl 3 [0-64]: Impl:
                            Block 4 [24-64] [Type Bool]:
                                Stmt 5 [30-62]: Expr: Expr 6 [30-62] [Type Bool]: Lit: Bool(true)
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
            Std.Core.GetConfig(\"key\", 0.0)
        }
        "},
        [(Rc::from("key"), Value::Double(1.0))]
            .into_iter()
            .collect(),
        &expect![[r#"
            Package:
                Item 0 [0-64] (Public):
                    Namespace (Ident 11 [0-64] "test"): Item 1
                Item 1 [0-64] (Internal):
                    Parent: 0
                    Callable 0 [0-64] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Double
                        functors: empty set
                        body: SpecDecl 3 [0-64]: Impl:
                            Block 4 [26-64] [Type Double]:
                                Stmt 5 [32-62]: Expr: Expr 6 [32-62] [Type Double]: Lit: Double(1)
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
            Std.Core.GetConfig(\"missing\", 123)
        }
        "},
        FxHashMap::default(),
        &expect![[r#"
            Package:
                Item 0 [0-65] (Public):
                    Namespace (Ident 11 [0-65] "test"): Item 1
                Item 1 [0-65] (Internal):
                    Parent: 0
                    Callable 0 [0-65] (operation):
                        name: Ident 1 [10-14] "Main"
                        input: Pat 2 [14-16] [Type Unit]: Unit
                        output: Int
                        functors: empty set
                        body: SpecDecl 3 [0-65]: Impl:
                            Block 4 [23-65] [Type Int]:
                                Stmt 5 [29-63]: Expr: Expr 6 [29-63] [Type Int]: Lit: Int(123)
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
            let value = Std.Core.GetConfig(key, 0);
        }
        "},
        FxHashMap::default(),
        &expect![[r#"
            [
                NonLiteralArgument(
                    Span {
                        lo: 82,
                        hi: 85,
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
            let value = Std.Core.GetConfig(\"key\", 0);
        }
        "},
        [(Rc::from("key"), Value::Bool(true))].into_iter().collect(),
        &expect![[r#"
            [
                TypeMismatch(
                    Span {
                        lo: 68,
                        hi: 69,
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
            let value = Std.Core.GetConfig(\"key\", 0);
        }
        "},
        [(Rc::from("key"), Value::unit())].into_iter().collect(),
        &expect![[r#"
            [
                UnsupportedType(
                    Span {
                        lo: 68,
                        hi: 69,
                    },
                ),
            ]
        "#]],
    );
}
