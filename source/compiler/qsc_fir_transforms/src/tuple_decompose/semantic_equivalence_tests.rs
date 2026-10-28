// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(feature = "slow-proptest-tests")]
use indoc::formatdoc;
use indoc::indoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;

#[test]
fn struct_assignment_reads_original_fields_before_writing() {
    // The swap and aliased-source cases fail without RHS snapshots.
    // Repeated reads are controls: writing one field must not disturb another.
    for (replacement, expected) in [
        ("new P { A = s.B, B = s.A }", 21),
        ("new P { A = s.B, B = s.B }", 22),
        ("new P { A = s.A, B = s.A }", 11),
        ("new P { A = alias.B, B = s.A }", 21),
    ] {
        let source = format!(
            r#"
            struct P {{ A : Int, B : Int }}
            @EntryPoint() operation Main() : Int {{
                mutable s = new P {{ A = 1, B = 2 }};
                let alias = s;
                set s = {replacement};
                10 * s.A + s.B
            }}
            "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
    }
}

#[test]
fn nested_struct_assignment_preserves_original_rhs_values_and_effect_order() {
    // Plain field reads exercise scalarization directly. The logged variant
    // already passed before the fix and remains an effect-order control.
    for (first, second, last) in [
        ("s.Last", "s.First.A", "s.First.B"),
        (
            r#"Log("first", s.Last)"#,
            r#"Log("second", s.First.A)"#,
            r#"Log("last", s.First.B)"#,
        ),
    ] {
        let source = format!(
            r#"
            struct P {{ A : Int, B : Int }}
            struct Outer {{ First : P, Last : Int }}
            function Log(label : String, value : Int) : Int {{ Message(label); value }}
            @EntryPoint() operation Main() : Int {{
                mutable s = new Outer {{ First = new P {{ A = 1, B = 2 }}, Last = 3 }};
                set s = new Outer {{
                    First = new P {{ A = {first}, B = {second} }},
                    Last = {last}
                }};
                100 * s.First.A + 10 * s.First.B + s.Last
            }}
        "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(312),
        );
    }
}

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

#[cfg(feature = "slow-proptest-tests")]
fn tuple_decompose_tuple_local_pattern() -> impl Strategy<Value = String> {
    (2..=5usize, 1..=3usize).prop_map(|(width, depth)| {
        let type_defs = tuple_decompose_struct_defs(width, depth);
        let initial_value = tuple_decompose_struct_value(width, depth, 0);
        let first_access = tuple_decompose_field_path(0, depth);
        let last_access = tuple_decompose_field_path(width - 1, depth);

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

#[cfg(feature = "slow-proptest-tests")]
fn tuple_decompose_struct_defs(width: usize, depth: usize) -> String {
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

#[cfg(feature = "slow-proptest-tests")]
fn tuple_decompose_struct_value(width: usize, level: usize, offset: usize) -> String {
    let assignments = (0..width)
        .map(|field_index| {
            let value = if level == 1 {
                (offset + field_index).to_string()
            } else {
                let stride = width.pow(
                    u32::try_from(level - 1)
                        .expect("Depth should be small enough to avoid overflow"),
                );
                tuple_decompose_struct_value(width, level - 1, offset + field_index * stride)
            };
            format!("F{field_index} = {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("new TupleLevel{level} {{ {assignments} }}")
}

#[cfg(feature = "slow-proptest-tests")]
fn tuple_decompose_field_path(field_index: usize, depth: usize) -> String {
    (0..depth)
        .map(|_| format!("F{field_index}"))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn tuple_decompose_preserves_semantics(source in tuple_decompose_tuple_local_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
