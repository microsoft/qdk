// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;
use proptest::prelude::*;

#[test]
fn udt_construction_and_field_access_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let p = new Pair { X = 5, Y = 3 };
                p.X - p.Y
            }
        }
    "#});
}

#[test]
fn udt_returned_from_function_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Wrapper { Value : Int }

            function MakeWrapper(v : Int) : Wrapper {
                new Wrapper { Value = v }
            }

            @EntryPoint()
            function Main() : Int {
                let w = MakeWrapper(42);
                w.Value
            }
        }
    "#});
}

#[test]
fn nested_udt_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Inner { A : Int, B : Int }
            struct Outer { First : Inner, Second : Int }

            @EntryPoint()
            function Main() : Int {
                let inner = new Inner { A = 10, B = 20 };
                let outer = new Outer { First = inner, Second = 30 };
                outer.First.A + outer.First.B + outer.Second
            }
        }
    "#});
}

#[test]
fn pretty_print_after_udt_erase_is_non_empty() {
    let source = indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }
        }
    "#};
    let (store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::UdtErase);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    // After UDT erasure the rendered Q# replaces struct construction with
    // tuple literals and uses `::Item<N>` field access. Verify non-empty.
    assert!(
        !rendered.is_empty(),
        "pretty-printed Q# after UDT erasure should not be empty"
    );
}

fn udt_erasure_pattern() -> impl Strategy<Value = String> {
    (1..=4usize, prop::bool::ANY).prop_map(|(field_count, use_copy_update)| {
        let fields = (0..field_count)
            .map(|field_index| format!("F{field_index} : Int"))
            .collect::<Vec<_>>()
            .join(", ");
        let assignments = (0..field_count)
            .map(|field_index| format!("F{field_index} = {field_index}"))
            .collect::<Vec<_>>()
            .join(", ");

        if use_copy_update {
            let updated_field = field_count - 1;
            let result = (0..field_count)
                .map(|field_index| format!("updated.F{field_index}"))
                .collect::<Vec<_>>()
                .join(" + ");

            formatdoc! {r#"
                namespace Test {{
                    struct Generated {{ {fields} }}

                    @EntryPoint()
                    function Main() : Int {{
                        let record = new Generated {{ {assignments} }};
                        let updated = new Generated {{ ...record, F{updated_field} = 99 }};
                        {result}
                    }}
                }}
            "#}
        } else {
            let result = (0..field_count)
                .map(|field_index| format!("record.F{field_index}"))
                .collect::<Vec<_>>()
                .join(" + ");

            formatdoc! {r#"
                namespace Test {{
                    struct Generated {{ {fields} }}

                    @EntryPoint()
                    function Main() : Int {{
                        let record = new Generated {{ {assignments} }};
                        {result}
                    }}
                }}
            "#}
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn udt_erasure_preserves_semantics(source in udt_erasure_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
