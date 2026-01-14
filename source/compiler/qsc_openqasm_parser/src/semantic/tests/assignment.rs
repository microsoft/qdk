// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod dyn_array_ref;
mod static_array_ref;

use crate::semantic::tests::check_err;

use expect_test::expect;

#[test]
fn too_many_indices_in_indexed_assignment() {
    check_err(
        r#"
        array[float[32], 3, 2] multiDim = {{1.1, 1.2}, {2.1, 2.2}, {3.1, 3.2}};
        multiDim[1, 1, 3] = 2.3;
        "#,
        &expect![[r#"
            [Qasm.Lowerer.CannotIndexType

              x cannot index variables of type float[32]
               ,-[test:3:24]
             2 |         array[float[32], 3, 2] multiDim = {{1.1, 1.2}, {2.1, 2.2}, {3.1, 3.2}};
             3 |         multiDim[1, 1, 3] = 2.3;
               :                        ^
             4 |         
               `----
            ]"#]],
    );
}
