// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This module contains tests for the compiler's error handling,
// specifically focusing on errors related to unsupported features
// and unimplemented statements in OpenQASM 3.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

const SOURCE: &str = include_str!("resources/openqasm_compiler_errors_test.qasm");

#[allow(clippy::too_many_lines)]
#[test]
fn check_compiler_error_spans_are_correct() {
    check_qasm_to_qsharp(
        SOURCE,
        &expect![[r#"
            Qdk.Qasm.Compiler.NotSupported

              x calibration grammar statements are not supported
                ,-[Test.qasm:11:1]
             10 | // NotSupported defcalgrammar
             11 | defcalgrammar "openpulse";
                : ^^^^^^^^^^^^^^^^^^^^^^^^^^
             12 | 
                `----

            Qdk.Qasm.Compiler.NotSupported

              x calibration statements are not supported
                ,-[Test.qasm:14:1]
             13 |     // NotSupported cal
             14 | ,-> cal {
             15 | |      // Defined within `cal`, so it may not leak back out to the enclosing blocks scope
             16 | |      float new_freq = 5.2e9;
             17 | |      // declare global port
             18 | |      extern port d0;
             19 | |      // reference `freq` variable from enclosing blocks scope
             20 | |      frame d0f = newframe(d0, freq, 0.0);
             21 | `-> }
             22 |     
                `----

            Qdk.Qasm.Compiler.NotSupported

              x def cal statements are not supported
                ,-[Test.qasm:24:1]
             23 |     // NotSupported defcal
             24 | ,-> defcal x $0 {
             25 | |      waveform xp = gaussian(1.0, 160t, 40dt);
             26 | |      // References frame and `new_freq` declared in top-level cal block
             27 | |      play(d0f, xp);
             28 | |      set_frequency(d0f, new_freq);
             29 | |      play(d0f, xp);
             30 | `-> }
             31 |     
                `----

            Qdk.Qasm.Compiler.NotSupported

              x delay statements are not supported
                ,-[Test.qasm:33:1]
             32 | // NotSupported
             33 | delay [2ns] q;
                : ^^^^^^^^^^^^^^
             34 | 
                `----

            Qdk.Qasm.Compiler.NotSupported

              x box with duration are not supported
                ,-[Test.qasm:35:6]
             34 | 
             35 | box [2ns] { // NotSupported box duration
                :      ^^^
             36 |     x [2ns] q; // NotSupported duration on gate call
                `----

            Qdk.Qasm.Compiler.NotSupported

              x gate call duration are not supported
                ,-[Test.qasm:36:8]
             35 | box [2ns] { // NotSupported box duration
             36 |     x [2ns] q; // NotSupported duration on gate call
                :        ^^^
             37 | }
                `----

            Qdk.Qasm.Compiler.NotSupported

              x mutable array references `mutable array[int[8], #dim = 1]` are not
              | supported
                ,-[Test.qasm:41:24]
             40 | // NotSupported mutable array reference
             41 | def mut_subroutine_dyn(mutable array[int[8], #dim = 1] arr_arg) {
                :                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             42 |    // body
                `----

            Qdk.Qasm.Compiler.NotSupported

              x mutable array references `mutable array[int[8], 2, 3]` are not supported
                ,-[Test.qasm:46:27]
             45 | // NotSupported mutable static sized array reference
             46 | def mut_subroutine_static(mutable array[int[8], 2, 3] arr_arg) {
                :                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^
             47 |    // body
                `----

            Qdk.Qasm.Compiler.Unimplemented

              x this statement is not yet handled during OpenQASM 3 import: extern
              | statements
                ,-[Test.qasm:70:1]
             69 | // Unimplemented
             70 | extern extern_func(int);
                : ^^^^^^^^^^^^^^^^^^^^^^^^
             71 | // End unimplemented statements.
                `----

            Qdk.Qasm.Compiler.NotSupported

              x hardware qubit operands are not supported
                ,-[Test.qasm:73:3]
             72 | // NotSupported hardware qubit
             73 | x $0;
                :   ^^
                `----
        "#]],
    );
}
