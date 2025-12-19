use std::sync::Arc;

use super::{PackageStore, SourceMap, compile};
use crate::compile::TargetCapabilityFlags;

use crate::compile::core;
use expect_test::Expect;
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_hir::hir::PackageId;

/// Runs a test where each successive package relies solely on the package before it
/// useful for testing chains of reexports
fn multiple_package_check(packages: Vec<(&str, &str)>) {
    multiple_package_check_inner(packages, None, true);
}

/// Runs a test where each successive package relies solely on the package before it
/// but without dependency aliases, same as how the Std library is consumed.
fn multiple_package_check_no_alias(packages: Vec<(&str, &str)>) {
    multiple_package_check_inner(packages, None, false);
}

fn multiple_package_check_expect_err(packages: Vec<(&str, &str)>, expect: &Expect) {
    multiple_package_check_inner(packages, Some(expect), true);
}

fn multiple_package_check_inner(
    packages: Vec<(&str, &str)>,
    expect: Option<&Expect>,
    use_alias: bool,
) {
    let mut store = PackageStore::new(core());
    let mut prev_id_and_name: Option<(PackageId, &str)> = None;
    let num_packages = packages.len();
    for (ix, (package_name, package_source)) in packages.into_iter().enumerate() {
        let is_last = ix == num_packages - 1;
        let deps = if let Some((prev_id, prev_name)) = prev_id_and_name {
            if use_alias {
                vec![(prev_id, Some(Arc::from(prev_name)))]
            } else {
                vec![(prev_id, None)]
            }
        } else {
            vec![]
        };

        let sources = SourceMap::new(
            [(
                Arc::from(format!("{package_name}.qs")),
                Arc::from(package_source),
            )],
            None,
        );

        let unit = compile(
            &store,
            &deps[..],
            sources,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        );

        match (is_last, &expect) {
            (true, Some(expect)) => {
                expect.assert_eq(&format!("{:#?}", unit.errors));
            }
            _ => {
                assert!(unit.errors.is_empty(), "{:#?}", unit.errors);
            }
        }

        prev_id_and_name = Some((store.insert(unit), package_name));
    }
}

/// This can be used to test multiple packages which internally have multiple source files, as opposed to the more simple `multiple_package_check`
/// which only allows one source file per package (for easy and quick test creation).
fn multiple_package_multiple_source_check(
    packages: Vec<(&str, Vec<(&str, &str)>)>,
    expect: Option<&Expect>,
) {
    let mut store = PackageStore::new(core());
    let mut prev_id_and_name: Option<(PackageId, &str)> = None;
    let num_packages = packages.len();
    for (ix, (package_name, sources)) in packages.into_iter().enumerate() {
        let is_last = ix == num_packages - 1;
        let deps = if let Some((prev_id, prev_name)) = prev_id_and_name {
            vec![(prev_id, Some(Arc::from(prev_name)))]
        } else {
            vec![]
        };

        let sources = SourceMap::new(
            sources.iter().map(|(name, source)| {
                (
                    Arc::from(format!("{package_name}/{name}.qs")),
                    Arc::from(*source),
                )
            }),
            None,
        );

        let unit = compile(
            &store,
            &deps[..],
            sources,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        );

        match (is_last, &expect) {
            (true, Some(expect)) => {
                expect.assert_eq(&format!("{:#?}", unit.errors));
            }
            _ => {
                assert!(unit.errors.is_empty(), "{:#?}", unit.errors);
            }
        }

        prev_id_and_name = Some((store.insert(unit), package_name));
    }
}

#[test]
fn export_callable() {
    multiple_package_check(vec![
        (
            "B",
            r#"
            namespace NamespaceB {
                function Foo() : Unit {}
                export Foo;
            }
            "#,
        ),
        (
            "A",
            r#"
            @EntryPoint()
            operation Main() : Unit {
                B.NamespaceB.Foo();
            }
            "#,
        ),
    ]);
}

#[test]
fn namespace_named_main_doesnt_create_main_namespace() {
    multiple_package_check_expect_err(
        vec![
            (
                "Main",
                "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;",
            ),
            (
                "C",
                r#"
            // this fails because `Main` is considered the "root"
            import Main.Main.Foo;
                    @EntryPoint()
                    operation Main() : Unit {
                        Foo(10, true);
                    }"#,
            ),
        ],
        &expect!([r#"
            [
                Error(
                    Resolve(
                        NotFound(
                            "Main.Main.Foo",
                            Span {
                                lo: 86,
                                hi: 99,
                            },
                        ),
                    ),
                ),
                Error(
                    Resolve(
                        NotFound(
                            "Foo",
                            Span {
                                lo: 205,
                                hi: 208,
                            },
                        ),
                    ),
                ),
                Error(
                    Type(
                        Error(
                            AmbiguousTy(
                                Span {
                                    lo: 205,
                                    hi: 218,
                                },
                            ),
                        ),
                    ),
                ),
            ]"#]),
    );
}

#[test]
fn namespaces_named_main_treated_as_root() {
    multiple_package_check(vec![
        (
            "Main",
            "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;",
        ),
        (
            "C",
            "
            // note that this is not Main.Main
            // and that  the namespace `Main` is omitted here
            import Main.Foo;
                    @EntryPoint()
                    operation Main() : Unit {
                        Foo(10, true);
                    }",
        ),
    ]);
}

#[test]
fn multiple_packages_reference_exports() {
    multiple_package_check(vec![
        (
            "PackageA",
            indoc! {"
                    operation Foo(x: Int, y: Bool) : Int {
                        x
                    }
                    export Foo;
                "},
        ),
        (
            "PackageB",
            indoc! {"
                    import PackageA.PackageA.Foo;
                    export Foo;
                "},
        ),
        (
            "PackageC",
            indoc! {"
                    import PackageB.PackageB.Foo;
                    @EntryPoint()
                    operation Main() : Unit {
                        Foo(10, true);
                    }
                "},
        ),
    ]);
}

#[test]
#[allow(clippy::too_many_lines)]
fn multiple_packages_disallow_unexported_imports() {
    multiple_package_check_expect_err(
        vec![
            (
                "PackageA",
                indoc! {"
                    function FunctionA() : Int {
                        1
                    }
                "},
            ),
            (
                "PackageB",
                indoc! {"
                    import PackageA.PackageA.FunctionA;
                    @EntryPoint()
                    function Main() : Unit {
                       FunctionA();
                    }
                "},
            ),
        ],
        &expect![[r#"
            [
                Error(
                    Resolve(
                        NotFound(
                            "PackageA.PackageA.FunctionA",
                            Span {
                                lo: 7,
                                hi: 34,
                            },
                        ),
                    ),
                ),
                Error(
                    Resolve(
                        NotFound(
                            "FunctionA",
                            Span {
                                lo: 78,
                                hi: 87,
                            },
                        ),
                    ),
                ),
                Error(
                    Type(
                        Error(
                            AmbiguousTy(
                                Span {
                                    lo: 78,
                                    hi: 89,
                                },
                            ),
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn reexport() {
    multiple_package_check(vec![
        (
            "PackageA",
            indoc! {"
                    export Std.Core.Length as Foo;
                "},
        ),
        (
            "PackageB",
            indoc! {"

                    import PackageA.PackageA.Foo;
                    @EntryPoint()
                    function Main() : Unit {
                        use qs = Qubit[2];
                        let len = Foo(qs);
                    }
                "},
        ),
    ]);
}

#[test]
fn reexport_export_has_alias() {
    multiple_package_check(vec![
        (
            "PackageA",
            indoc! {"
                operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo as Bar;
                "},
        ),
        (
            "PackageB",
            indoc! {"
                import PackageA.PackageA.Bar;
                "},
        ),
    ]);
}

#[test]
fn reexport_import_has_alias() {
    multiple_package_check(vec![
        (
            "PackageA",
            "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;
            ",
        ),
        (
            "PackageB",
            "
                import PackageA.PackageA.Foo as Bar;

                export Bar;
            ",
        ),
    ]);
}

#[test]
fn reexport_reexport_has_alias() {
    multiple_package_check(vec![
        (
            "PackageA",
            "
                operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;
            ",
        ),
        (
            "PackageB",
            "
                import PackageA.PackageA.Foo;
                export Foo as Bar;
            ",
        ),
        (
            "PackageC",
            "
                import PackageB.PackageB.Bar;
                @EntryPoint()
                operation Main() : Unit {
                    Bar(10, true);
                }
            ",
        ),
    ]);
}

#[test]
fn reexport_callable_combined_aliases() {
    multiple_package_check(vec![
        (
            "PackageA",
            "
                operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;
            ",
        ),
        (
            "PackageB",
            "
                import PackageA.PackageA.Foo;
                import PackageA.PackageA.Foo as Foo2;
                export Foo, Foo as Bar, Foo2, Foo2 as Bar2;
            ",
        ),
        (
            "PackageC",
            "
                import PackageB.PackageB.Foo, PackageB.PackageB.Bar, PackageB.PackageB.Foo2, PackageB.PackageB.Bar2;
                @EntryPoint()
                operation Main() : Unit {
                    Foo(10, true);
                    Foo2(10, true);
                    Bar(10, true);
                    Bar2(10, true);
                }
            ",
        ),
    ]);
}

#[test]
fn direct_reexport() {
    multiple_package_check(vec![
        (
            "A",
            "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo as Bar;",
        ),
        ("B", "export A.A.Bar as Baz;"),
        (
            "C",
            "import B.B.Baz as Quux;
                    @EntryPoint()
                    operation Main() : Unit {
                        Quux(10, true);
                    }",
        ),
    ]);
}

#[test]
fn reexports_still_type_check() {
    multiple_package_check_expect_err(
        vec![
            (
                "A",
                "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo as Bar;",
            ),
            (
                "B",
                "
                 export A.A.Bar as Baz;",
            ),
            (
                "C",
                "import B.B.Baz as Quux;
                    @EntryPoint()
                    operation Main() : Unit {
                        Quux(10, 10);
                    }",
            ),
        ],
        &expect![[r#"
            [
                Error(
                    Type(
                        Error(
                            TyMismatch(
                                "Bool",
                                "Int",
                                Span {
                                    lo: 128,
                                    hi: 140,
                                },
                            ),
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn namespaces_named_lowercase_main_not_treated_as_root() {
    multiple_package_check(vec![
        (
            "main",
            "operation Foo(x: Int, y: Bool) : Int {
                    x
                }
                export Foo;",
        ),
        (
            "C",
            "
            import main.main.Foo;
                    @EntryPoint()
                    operation Main() : Unit {
                        Foo(10, true);
                    }",
        ),
    ]);
}

#[test]
fn aliased_export_via_aliased_import() {
    multiple_package_check(vec![
        (
            "MyGithubLibrary",
            r#"
        namespace TestPackage {

            import Subpackage.Subpackage.Hello as SubHello;

            export HelloFromGithub;
            export SubHello;

            /// This is a Doc String!
            function HelloFromGithub() : Unit {
                SubHello();
            }
        }

        namespace Subpackage.Subpackage {
            function Hello() : Unit {}
            export Hello;
        }

        "#,
        ),
        (
            "UserCode",
            r#"
           import MyGithubLibrary.TestPackage.SubHello;
           import MyGithubLibrary.TestPackage.HelloFromGithub;
           import MyGithubLibrary.Subpackage.Subpackage as P;

            function Main() : Unit {
                HelloFromGithub();
                SubHello();
                P.Hello();
            }
         "#,
        ),
    ]);
}

#[test]
fn udt_reexport_with_alias() {
    multiple_package_multiple_source_check(
        vec![
            (
                "A",
                vec![
                    ("FileOne", "struct Foo { content: Bool }"),
                    ("FileTwo", "export FileOne.Foo as Bar;"),
                ],
            ),
            ("B", vec![("FileThree", "export A.FileTwo.Bar as Baz;")]),
            (
                "C",
                vec![(
                    "FileFour",
                    "@EntryPoint()
            operation Main() : Unit {
                let x = new B.FileThree.Baz { content = true };
            }",
                )],
            ),
        ],
        Some(&expect!["[]"]),
    );
}

#[test]
fn callable_reexport() {
    multiple_package_check(vec![
        (
            "A",
            "function Foo() : Unit {  }
                export Foo as Bar;",
        ),
        (
            "B",
            "           @EntryPoint()
            operation Main() : Unit {
                let x = A.A.Bar();
            }",
        ),
    ]);
}

#[test]
fn old_syntax_udt_reexported() {
    multiple_package_multiple_source_check(
        vec![
            (
                "A",
                vec![
                    (
                        "FileOne",
                        "
                     struct Foo { content: Bool, otherContent: Bool }
                     newtype Bar = ( content: Bool, otherContent: Bool );
                     ",
                    ),
                    ("FileTwo", "export FileOne.Foo as Bar, FileOne.Bar as Baz;"),
                ],
            ),
            (
                "B",
                vec![(
                    "FileThree",
                    "export A.FileTwo.Bar as Baz, A.FileTwo.Baz as Quux;",
                )],
            ),
            (
                "C",
                vec![(
                    "FileFour",
                    "@EntryPoint()
            operation Main() : Unit {
            // use old UDT syntax with both a struct and a newtype
                let x = B.FileThree.Baz(true, true);
                let x = B.FileThree.Quux(true, true);
            }",
                )],
            ),
        ],
        Some(&expect!["[]"]),
    );
}

#[test]
fn namespace_export_alias() {
    multiple_package_check(vec![
        (
            "B",
            r#"
            namespace NamespaceB {
                function Foo() : Unit {}
                export Foo;
            }
            namespace Bar {
                export NamespaceB as NamespaceBAlias;
            }
            "#,
        ),
        (
            "A",
            r#"
            @EntryPoint()
            operation Main() : Unit {
                B.Bar.NamespaceBAlias.Foo();
            }
            "#,
        ),
    ]);
}

#[test]
fn namespace_reexport_alias_in_main() {
    multiple_package_check(vec![
        (
            "B",
            r#"
            namespace NamespaceB {
                function Foo() : Unit {}
                export Foo;
            }
            namespace Main {
                export NamespaceB as NamespaceBAlias;
            }
            "#,
        ),
        (
            "A",
            r#"
            @EntryPoint()
            operation Main() : Unit {
                B.NamespaceBAlias.Foo();
            }
            "#,
        ),
    ]);
}

#[test]
fn namespace_cross_package_reexport_disallowed() {
    // Cross-package namespace reexports are not currently allowed,
    // since the individual items in the namespace are not
    // visible to any consumers, making the namespace reexport
    // effectively useless.
    multiple_package_check_expect_err(
        vec![
            (
                "B",
                "
                function Foo() : Unit {}
                export Foo;
                ",
            ),
            (
                "A",
                "
                export B.B as NamespaceBAlias;
                ",
            ),
        ],
        &expect![[r#"
            [
                Error(
                    Lower(
                        CrossPackageNamespaceReexport(
                            Span {
                                lo: 24,
                                hi: 27,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn namespace_export_from_dependency() {
    multiple_package_check_expect_err(
        vec![
            (
                "B",
                "
                namespace B1 {
                    function Foo() : Unit {}
                    export Foo;
                }
                namespace B2 {
                    export B1 as B3;
                }
                ",
            ),
            (
                "A",
                "
                operation A1() : Unit {
                    B.B2.B3.Foo();
                }
                operation A2() : Unit {
                    import B.B2.B3;
                    B3.Foo();
                }
                operation A3() : Unit {
                    import B.B2.B3.*;
                    Foo();
                }
                ",
            ),
        ],
        &expect!["[]"],
    );
}

#[test]
fn namespace_export_in_expr() {
    multiple_package_check_expect_err(
        vec![
            (
                "B",
                r#"
            namespace Foo {
                function Bar() : Unit {}
                export Bar;
            }
            namespace Main {
                export Foo;
            }
            "#,
            ),
            (
                "A",
                r#"
            @EntryPoint()
            operation Main() : Unit {
                B.Foo;
            }
            "#,
            ),
        ],
        &expect![[r#"
            [
                Error(
                    Resolve(
                        NotFound(
                            "B.Foo",
                            Span {
                                lo: 81,
                                hi: 86,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn merged_namespace_export() {
    multiple_package_check(vec![
        (
            "B",
            r#"
            namespace Foo {
                function Bar() : Unit {}
                export Bar;
            }
            namespace Foo {
                function Baz() : Unit {}
                export Baz;
            }
            namespace Main {
                export Foo as FooAlias;
            }
            "#,
        ),
        (
            "A",
            r#"
            @EntryPoint()
            operation Main() : Unit {
                B.FooAlias.Bar();
                B.FooAlias.Baz();
            }
            "#,
        ),
    ]);
}

#[test]
fn use_udt() {
    multiple_package_check(vec![
        (
            "B",
            r#"
            namespace Foo {
                struct Bar { x: Int }
                export Bar;
            }
            "#,
        ),
        (
            "A",
            r#"
            import B.Foo.Bar;
            operation Main() : Unit {
                let b = new Bar { x = 42 };
                let b: Bar = Bar(42,);
            }
            "#,
        ),
    ]);
}

#[test]
fn wildcard_import_then_export() {
    multiple_package_check(vec![
        (
            "PackageA",
            "
                namespace Foo {
                    operation Bar() : Unit {}
                }

                namespace Main {
                    import Foo.*;
                    export Bar;
                    export Bar as Baz;
                }
            ",
        ),
        (
            "PackageB",
            "
                @EntryPoint()
                operation Main() : Unit {
                    PackageA.Bar();
                    PackageA.Baz();
                }
            ",
        ),
    ]);
}

#[test]
fn reexport_operation_no_dep_alias() {
    multiple_package_check_no_alias(vec![
        (
            "PackageA",
            "
                namespace Foo {
                    operation Bar() : Unit {}
                }

                namespace Qux {
                    export Foo.Bar;
                    export Foo.Bar as Baz;
                }
            ",
        ),
        (
            "PackageB",
            "
                function Main() : Unit {
                    Qux.Bar();
                    Qux.Baz();
                }
            ",
        ),
    ]);
}

#[test]
fn import_then_reexport_operation_no_dep_alias() {
    multiple_package_check_no_alias(vec![
        (
            "PackageA",
            "
                namespace Foo {
                    operation Bar() : Unit {}
                }

                namespace Qux {
                    import Foo.*;
                    export Bar;
                    export Bar as Baz;
                }
            ",
        ),
        (
            "PackageB",
            "
                function Main() : Unit {
                    Qux.Bar();
                    Qux.Baz();
                }
            ",
        ),
    ]);
}

#[test]
fn merge_with_namespace_from_dependency() {
    // Since we allow namespace declarations to be merged with
    // existing known namespaces, even from other packages, the
    // below will cause all namespace declarations to be merged
    // into a single one, causing all functions to be available
    // through both of the declared namespace paths.
    multiple_package_check(vec![
        (
            "PackageA",
            "
                namespace Foo {
                    operation Bar() : Unit {}
                    export Bar;
                }

                namespace Bar {
                    export Foo as FooAlias;
                }
            ",
        ),
        (
            "PackageB",
            "
                namespace PackageA.Foo {
                    operation Baz(): Unit {}
                }

                namespace PackageA.Bar.FooAlias {
                    operation Qux() : Unit {}
                }

                namespace Main {
                    function Main() : Unit {
                        PackageA.Foo.Bar();
                        PackageA.Foo.Baz();
                        PackageA.Foo.Qux();
                        PackageA.Bar.FooAlias.Bar();
                        PackageA.Bar.FooAlias.Baz();
                        PackageA.Bar.FooAlias.Qux();
                    }
                }
            ",
        ),
    ]);
}
