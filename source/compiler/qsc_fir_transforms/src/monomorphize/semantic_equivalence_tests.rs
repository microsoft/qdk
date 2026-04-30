// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use proptest::prelude::*;

/// Generates syntactically valid Q# programs exercising monomorphization's
/// key code paths: single and multiple type parameters, nested generic calls,
/// and multiple instantiations of the same generic.
fn mono_pattern_strategy() -> impl Strategy<Value = String> {
    let val = || 0..50i64;

    prop_oneof![
        // 1. Single type parameter instantiated with Int.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Identity<'T>(x : 'T) : 'T {{ x }}
                function Main() : Int {{
                    Identity({a})
                }}
            }}
        "}),
        // 2. Single type parameter instantiated with Bool.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Identity<'T>(x : 'T) : 'T {{ x }}
                function IsPositive(n : Int) : Bool {{ n > 0 }}
                function Main() : Bool {{
                    Identity(IsPositive({a}))
                }}
            }}
        "}),
        // 3. Multiple instantiations of the same generic in one program.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Identity<'T>(x : 'T) : 'T {{ x }}
                function Main() : Int {{
                    let x = Identity({a});
                    let y = Identity(true);
                    let z = Identity({b});
                    x + z
                }}
            }}
        "}),
        // 4. Nested generic calls: generic calling generic.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Identity<'T>(x : 'T) : 'T {{ x }}
                function Wrap<'T>(x : 'T) : 'T {{ Identity(x) }}
                function Main() : Int {{
                    Wrap({a})
                }}
            }}
        "}),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn differential_monomorphize(source in mono_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
