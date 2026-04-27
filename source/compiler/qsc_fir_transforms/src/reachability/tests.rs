// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::fir::PackageLookup;

/// Compiles Q# source, runs reachability analysis, and returns a sorted
/// list of reachable callable names from the user package.
fn extract_reachable(source: &str) -> String {
    let (store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let reachable = collect_reachable_from_entry(&store, pkg_id);
    let package = store.get(pkg_id);
    let mut names: Vec<String> = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            names.push(decl.name.name.to_string());
        }
    }
    names.sort();
    names.join("\n")
}

fn check(source: &str, expect: &Expect) {
    expect.assert_eq(&extract_reachable(source));
}

#[test]
fn reachable_includes_direct_call_chain() {
    // Main calls A, A calls B. C is never called.
    check(
        indoc! {"
                namespace Test {
                    function B() : Unit {}
                    function A() : Unit { B(); }
                    function C() : Unit {}
                    @EntryPoint()
                    function Main() : Unit { A(); }
                }
            "},
        &expect![[r#"
                A
                B
                Main"#]],
    );
}

#[test]
fn unreachable_callable_excluded() {
    // Only Main is called; Orphan is unreachable.
    check(
        indoc! {"
                namespace Test {
                    function Orphan() : Unit {}
                    @EntryPoint()
                    function Main() : Unit {}
                }
            "},
        &expect![[r#"
                Main"#]],
    );
}

#[test]
fn transitive_chain_all_reachable() {
    // Main → A → B → C (full chain).
    check(
        indoc! {"
                namespace Test {
                    function C() : Unit {}
                    function B() : Unit { C(); }
                    function A() : Unit { B(); }
                    @EntryPoint()
                    function Main() : Unit { A(); }
                }
            "},
        &expect![[r#"
                A
                B
                C
                Main"#]],
    );
}

#[test]
fn diamond_call_graph() {
    // Main → A and Main → B, both call Leaf.
    check(
        indoc! {"
                namespace Test {
                    function Leaf() : Unit {}
                    function A() : Unit { Leaf(); }
                    function B() : Unit { Leaf(); }
                    @EntryPoint()
                    function Main() : Unit { A(); B(); }
                }
            "},
        &expect![[r#"
                A
                B
                Leaf
                Main"#]],
    );
}

#[test]
fn multiple_unreachable_functions() {
    check(
        indoc! {"
                namespace Test {
                    function Dead1() : Unit {}
                    function Dead2() : Unit {}
                    function Alive() : Unit {}
                    @EntryPoint()
                    function Main() : Unit { Alive(); }
                }
            "},
        &expect![[r#"
                Alive
                Main"#]],
    );
}

#[test]
fn entry_expression_followed() {
    // A single entry point with no calls — only Main is reachable.
    check(
        indoc! {"
                namespace Test {
                    function Helper() : Unit {}
                    @EntryPoint()
                    function Main() : Int { 42 }
                }
            "},
        &expect![[r#"
                Main"#]],
    );
}

#[test]
fn closure_inside_reachable_callable_followed() {
    // A closure defined inside a reachable callable — the callable
    // that the closure targets should also be reachable.
    check(
        indoc! {"
                namespace Test {
                    @EntryPoint()
                    function Main() : Int {
                        let f = (x) -> x + 1;
                        f(5)
                    }
                }
            "},
        &expect![[r#"
            <lambda>
            Main"#]],
    );
}

#[test]
fn recursive_callable_reachable() {
    // Recursive callable: Recurse calls itself.
    check(
        indoc! {"
                namespace Test {
                    function Recurse(n : Int) : Int {
                        if n <= 0 { 0 } else { Recurse(n - 1) }
                    }
                    @EntryPoint()
                    function Main() : Int { Recurse(5) }
                }
            "},
        &expect![[r#"
                Main
                Recurse"#]],
    );
}

#[test]
fn mutually_recursive_callables_reachable() {
    // Mutual recursion: Ping calls Pong, Pong calls Ping.
    check(
        indoc! {"
                namespace Test {
                    function Ping(n : Int) : Int {
                        if n <= 0 { 0 } else { Pong(n - 1) }
                    }
                    function Pong(n : Int) : Int { Ping(n) }
                    @EntryPoint()
                    function Main() : Int { Ping(3) }
                }
            "},
        &expect![[r#"
                Main
                Ping
                Pong"#]],
    );
}

#[test]
fn callable_only_in_unreachable_branch() {
    // A call inside a conditional branch that is syntactically present
    // but the function is still reachable because we do static analysis.
    check(
        indoc! {"
                namespace Test {
                    function DeadEnd() : Unit {}
                    @EntryPoint()
                    function Main() : Unit {
                        if false { DeadEnd(); }
                    }
                }
            "},
        &expect![[r#"
                DeadEnd
                Main"#]],
    );
}

#[test]
fn lambda_in_entry_expression() {
    // Lambda defined and invoked directly in the entry expression.
    check(
        indoc! {"
                namespace Test {
                    @EntryPoint()
                    function Main() : Int {
                        let add = (a, b) -> a + b;
                        add(3, 4)
                    }
                }
            "},
        &expect![[r#"
            <lambda>
            Main"#]],
    );
}

#[test]
fn cross_package_call_reachability_scoped_to_package() {
    // Calling a stdlib function from the user package. The reachable set
    // for the user package should include Main but should not include
    // any stdlib callable (reachability returns StoreItemIds across
    // packages, but our helper `extract_reachable` filters to user-package
    // callables only).
    check(
        indoc! {"
                namespace Test {
                    @EntryPoint()
                    function Main() : Int {
                        Microsoft.Quantum.Math.MaxI(1, 2)
                    }
                }
            "},
        &expect![[r#"
                Main"#]],
    );
}

#[test]
fn simulatable_intrinsic_callable_reachable() {
    // An operation with @SimulatableIntrinsic() should appear in the
    // reachable set when called from an entry point.
    check(
        indoc! {"
                namespace Test {
                    @SimulatableIntrinsic()
                    operation MyOp() : Unit {
                        body intrinsic;
                    }
                    @EntryPoint()
                    operation Main() : Unit {
                        MyOp();
                    }
                }
            "},
        &expect![[r#"
                Main
                MyOp"#]],
    );
}

#[test]
fn dangling_item_reference_is_ignored() {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {"
            namespace Test {
                function Helper() : Unit {}
                @EntryPoint()
                function Main() : Unit {
                    Helper();
                }
            }
        "});

    let package = store.get(pkg_id);
    let main_id = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => Some(item.id),
            _ => None,
        })
        .expect("Main should exist");
    let helper_id = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Helper" => Some(item.id),
            _ => None,
        })
        .expect("Helper should exist");

    store.get_mut(pkg_id).items.remove(helper_id);

    let reachable = collect_reachable_from_entry(&store, pkg_id);
    assert!(reachable.contains(&StoreItemId::from((pkg_id, main_id))));
    assert!(!reachable.contains(&StoreItemId::from((pkg_id, helper_id))));
}

#[test]
fn seeds_include_transitive_deps_unreachable_from_entry() {
    let (store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {"
            namespace Test {
                function Helper() : Unit {}
                function Unreachable() : Unit { Helper(); }
                @EntryPoint()
                function Main() : Unit {}
            }
        "});

    let package = store.get(pkg_id);

    let find_callable = |name: &str| -> StoreItemId {
        let local_id = package
            .items
            .values()
            .find_map(|item| match &item.kind {
                ItemKind::Callable(decl) if decl.name.name.as_ref() == name => Some(item.id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name} should exist"));
        StoreItemId::from((pkg_id, local_id))
    };

    let unreachable_id = find_callable("Unreachable");
    let helper_id = find_callable("Helper");

    // Baseline: neither Unreachable nor Helper is reachable from entry.
    let entry_only = collect_reachable_from_entry(&store, pkg_id);
    assert!(
        !entry_only.contains(&unreachable_id),
        "Unreachable should not be in the entry-only set"
    );
    assert!(
        !entry_only.contains(&helper_id),
        "Helper should not be in the entry-only set"
    );

    // With Unreachable as a seed, both it and its transitive dep Helper
    // should appear.
    let seeded = collect_reachable_with_seeds(&store, pkg_id, &[unreachable_id]);
    assert!(
        seeded.contains(&unreachable_id),
        "seed callable should be in the seeded set"
    );
    assert!(
        seeded.contains(&helper_id),
        "transitive dep of seed should be in the seeded set"
    );
}

#[test]
fn reachability_is_idempotent() {
    let source = indoc! {"
        namespace Test {
            function Helper() : Unit {}
            function Dead() : Unit {}
            @EntryPoint()
            function Main() : Unit { Helper(); }
        }
    "};
    let (store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let first = collect_reachable_from_entry(&store, pkg_id);
    let second = collect_reachable_from_entry(&store, pkg_id);
    assert_eq!(first, second, "reachability analysis should be idempotent");
}
