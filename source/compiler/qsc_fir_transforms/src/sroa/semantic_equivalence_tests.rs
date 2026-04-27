// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;
use proptest::prelude::*;

#[test]
fn tuple_local_split_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                let pair = (10, 20);
                let (a, b) = pair;
                a + b
            }
        }
    "#});
}

#[test]
fn struct_field_access_split_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Point { X : Int, Y : Int }

            @EntryPoint()
            function Main() : Int {
                let p = new Point { X = 3, Y = 7 };
                p.X * p.Y
            }
        }
    "#});
}

#[test]
fn mutable_tuple_update_split_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                mutable pair = (1, 2);
                let (a, b) = pair;
                set pair = (a + 10, b + 20);
                let (c, d) = pair;
                c + d
            }
        }
    "#});
}

fn sroa_tuple_local_pattern() -> impl Strategy<Value = String> {
    (2..=5usize, 1..=3usize).prop_map(|(width, depth)| {
        let type_defs = sroa_struct_defs(width, depth);
        let initial_value = sroa_struct_value(width, depth, 0);
        let first_access = sroa_field_path(0, depth);
        let last_access = sroa_field_path(width - 1, depth);

        formatdoc! {r#"
            namespace Test {{
            {type_defs}

                @EntryPoint()
                function Main() : Int {{
                    let tupleValue = {initial_value};
                    tupleValue.{first_access} + tupleValue.{last_access}
                }}
            }}
        "#}
    })
}

fn sroa_struct_defs(width: usize, depth: usize) -> String {
    (1..=depth)
        .map(|level| {
            let field_ty = if level == 1 {
                "Int".to_string()
            } else {
                format!("TupleLevel{}", level - 1)
            };
            let fields = (0..width)
                .map(|field_index| format!("F{field_index} : {field_ty}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("    struct TupleLevel{level} {{ {fields} }}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sroa_struct_value(width: usize, level: usize, offset: usize) -> String {
    let assignments = (0..width)
        .map(|field_index| {
            let value = if level == 1 {
                (offset + field_index).to_string()
            } else {
                let stride = width.pow((level - 1) as u32);
                sroa_struct_value(width, level - 1, offset + field_index * stride)
            };
            format!("F{field_index} = {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("new TupleLevel{level} {{ {assignments} }}")
}

fn sroa_field_path(field_index: usize, depth: usize) -> String {
    (0..depth)
        .map(|_| format!("F{field_index}"))
        .collect::<Vec<_>>()
        .join(".")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn sroa_preserves_semantics(source in sroa_tuple_local_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
