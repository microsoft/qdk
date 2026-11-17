// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{get_rename, prepare_rename};
use crate::Encoding;
use crate::test_utils::openqasm::compile_with_markers;

/// Asserts that the rename locations given at the cursor position matches the expected rename locations.
/// The cursor position is indicated by a `↘` marker in the source text.
/// The expected rename location ranges are indicated by `◉` markers in the source text.
fn check(source_with_markers: &str) {
    let (compilation, cursor_position, target_spans) = compile_with_markers(source_with_markers);
    let actual = get_rename(&compilation, "<source>", cursor_position, Encoding::Utf8)
        .into_iter()
        .map(|l| l.range)
        .collect::<Vec<_>>();
    for target in &target_spans {
        assert!(actual.contains(target));
    }
    assert_eq!(target_spans.len(), actual.len());
}

/// Asserts that the prepare rename given at the cursor position returns None.
/// The cursor position is indicated by a `↘` marker in the source text.
fn assert_no_rename(source_with_markers: &str) {
    let (compilation, cursor_position, _) = compile_with_markers(source_with_markers);
    let actual = prepare_rename(&compilation, "<source>", cursor_position, Encoding::Utf8);
    assert!(actual.is_none());
}

#[test]
fn callable_def() {
    check(
        r#"
        def ◉Fo↘o◉(int x, int y, int z) {
            ◉Foo◉(x, y, z);
        }

        def Bar(int x, int y, int z) {
            ◉Foo◉(x, y, z);
        }
    "#,
    );
}

#[test]
fn callable_ref() {
    check(
        r#"
        def ◉Foo◉(int x, int y, int z) {
            ◉Foo◉(x, y, z);
        }

        def Bar(int x, int y, int z) {
            ◉Fo↘o◉(x, y, z);
        }
    "#,
    );
}

#[test]
fn gate_def() {
    check(
        r#"
        gate ◉Fo↘o◉(x, y, z) q { }

        gate Bar(x, y, z) q {
            ◉Foo◉(x, y, z) q;
        }
    "#,
    );
}

#[test]
fn gate_ref() {
    check(
        r#"
        gate ◉Foo◉(x, y, z) q { }

        gate Bar(x, y, z) q {
            ◉Fo↘o◉(x, y, z) q;
        }
    "#,
    );
}

#[test]
fn broadcast_register_rename_has_one_edit_per_source_token() {
    check(
        r#"
        include "stdgates.inc";
        qubit[8] ◉t↘argets◉;
        h ◉targets◉;
        "#,
    );
}

#[test]
fn equal_width_register_rename_does_not_duplicate_edits() {
    check(
        r#"
        include "stdgates.inc";
        qubit[4] ◉c↘ontrols◉;
        qubit[4] targets;
        cx ◉controls◉, targets;
        "#,
    );
}

#[test]
fn parameter_def() {
    check(
        r#"
        def Foo(int ◉↘x◉, int y, int z) {
            int temp = ◉x◉;
            Foo(◉x◉, y, z);
        }
    "#,
    );
}

#[test]
fn parameter_ref() {
    check(
        r#"
        def Foo(int ◉x◉, int y, int z) {
            int temp = ◉x◉;
            Foo(◉↘x◉, y, z);
        }
    "#,
    );
}

#[test]
fn local_def_in_def() {
    check(
        r#"
        int temp = x;
        def Foo(int x, int y, int z) {
            int ◉t↘emp◉ = x;
            Foo(◉temp◉, y, ◉temp◉);
        }
        Foo(temp, y, temp);
    "#,
    );
}

#[test]
fn local_ref_in_def() {
    check(
        r#"
        int temp = x;
        def Foo(int x, int y, int z) {
            int ◉temp◉ = x;
            Foo(◉t↘emp◉, y, ◉temp◉);
        }
        Foo(temp, y, temp);
    "#,
    );
}

#[test]
fn local_def() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        int ◉t↘emp◉ = x;
        Foo(◉temp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn local_ref() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        int ◉temp◉ = x;
        Foo(◉t↘emp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn local_ref_cursor_touches_start() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        int ◉temp◉ = x;
        Foo(◉↘temp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn local_ref_cursor_touches_end() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        int ◉temp◉ = x;
        Foo(◉temp↘◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn input_def() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        input int ◉t↘emp◉;
        Foo(◉temp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn input_ref() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        input int ◉temp◉;
        Foo(◉t↘emp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn output_def() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        output int ◉t↘emp◉;
        Foo(◉temp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn output_ref() {
    check(
        r#"
        def Foo(int x, int y, int z) {
            int temp = x;
            Foo(temp, y, temp);
        }
        output int ◉temp◉;
        Foo(◉t↘emp◉, y, ◉temp◉);
    "#,
    );
}

#[test]
fn no_rename_openqasm_header() {
    assert_no_rename(
        r#"
    OP↘ENQASM 3.0;
    "#,
    );
}

#[test]
fn no_rename_keyword() {
    assert_no_rename(
        r#"
    inc↘lude "stdgates.inc";
    "#,
    );
}

#[test]
fn no_rename_type() {
    assert_no_rename(
        r#"
    in↘t x;
    "#,
    );
}

#[test]
fn no_rename_string_literal() {
    assert_no_rename(
        r#"
    include "He↘llo World!";
    "#,
    );
}

#[test]
fn rename_for_loop_iter_def() {
    check(
        r#"
    def Foo(int x, int y, int z) {}
    for int ◉i↘ndex◉ in [0:10] {
        int temp = ◉index◉;
        Foo(◉index◉, 0, 7 * ◉index◉ + 3);
    }
    "#,
    );
}

#[test]
fn rename_for_loop_iter_ref() {
    check(
        r#"
    def Foo(int x, int y, int z) {}
    for int ◉index◉ in [0:10] {
        int temp = ◉↘index◉;
        Foo(◉index◉, 0, 7 * ◉index◉ + 3);
    }
    "#,
    );
}

#[test]
fn no_rename_comment() {
    assert_no_rename(
        r#"
    OPENQASM 3.0;
    // He↘llo World!
    include "stdgates.inc";
    "#,
    );
}

#[test]
fn no_rename_std_item() {
    assert_no_rename(
        r#"
    OPENQASM 3.0;
    include "stdgates.inc";

    // Built-in operation identifier shouldn't be renameable
    qubit[1] q;
    ↘x q[0];
    "#,
    );
}

#[test]
fn no_rename_intrinsic_3_item() {
    assert_no_rename(
        r#"
    OPENQASM 3.0;
    // Built-in operation identifier shouldn't be renameable
    qubit q;
    ↘U(0.0, 0.0, 0.0) q;
    "#,
    );
}

#[test]
fn no_rename_intrinsic_2_item() {
    assert_no_rename(
        r#"
    OPENQASM 2.0;
    // Built-in operation identifier shouldn't be renameable
    qubit q;
    ↘U(0.0, 0.0, 0.0) q;
    "#,
    );
}

#[test]
fn no_rename_intrinsic_const() {
    assert_no_rename(
        r#"
    float i = ↘pi * 7. / 8.;
    "#,
    );
}

#[test]
fn no_rename_non_id_character() {
    assert_no_rename(
        r#"
    // Non-identifier character '='
    int x ↘= 0;
    "#,
    );
}

#[test]
fn ty_param_def() {
    check(
        r#"
    // Use a parameter identifier to model rename
    def Foo(int ◉↘t◉) -> int { return ◉t◉; }
    "#,
    );
}

#[test]
fn ty_param_ref() {
    check(
        r#"
    def Foo(int ◉t◉) -> int { return ◉↘t◉; }
    "#,
    );
}

#[test]
#[ignore = "index sets aren't yet supported"]
fn alias_index_set_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘kip◉ = 2;
    
    qubit[5] qreg0;
    qubit[5] qreg1;
    let my_reg = qreg0[{0 * ◉skip◉, ◉skip◉, ◉skip◉ * 2}] ++ qreg1[{◉skip◉ - 1, ◉skip◉ + ◉skip◉ / 2}];
    "#,
    );
}

#[test]
fn alias_range_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘kip◉ = 2;
    
    qubit[5] qreg0;
    qubit[5] qreg1;
    let my_reg = qreg0[◉skip◉-2:◉skip◉:2 * ◉skip◉ + 1] ++ qreg1[◉skip◉ - 1:◉skip◉:5];
    "#,
    );
}

#[test]
fn box_designator_expr_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    const duration ad = 2ns;
    box [◉size◉ * ad] {}
    "#,
    );
}

#[test]
fn delay_designator_expr_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    const duration ad = 2ns;
    delay [◉size◉ * ad] $0;
    "#,
    );
}

#[test]
fn box_and_delay_designator_expr_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    const duration ad = 2ns;
    box [◉size◉ * ad] {
        delay [◉size◉ * ad] $0;
    }
    "#,
    );
}

#[test]
fn gphase_and_gate_call_designator_expr_ref() {
    check(
        r#"
    include "stdgates.inc";

    // classical decl
    const int ◉s↘ize◉ = 5;

    U(0.0, 0.0, 0.0) [◉size◉ * 2ns] $0;
    gphase [◉size◉ * 2ns] $0;
    "#,
    );
}

#[test]
fn sized_for_loop_iter_ty_param_decl_def() {
    check(
        r#"
    // classical decl
    const int ◉size◉ = 5;

    // for stmt initializer var width
    // redefine size so the inner scope should be a different var
    for int[◉s↘ize◉] size in [◉size◉ - 0:◉size◉ * 2] {
        for int[size] i in [size - 0:size * 2] {
            // Do something with i
        }
    }
    "#,
    );
}

#[test]
fn sized_quantum_register_def_length_param_def() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // quantum decl bitarray length
    qubit[◉size◉] cdecl_qal;
    "#,
    );
}

#[test]
fn sized_bit_register_def_length_param_def() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical decl bitarray length
    bit[◉size◉] cdecl_bal = 3;
    "#,
    );
}

#[test]
fn sized_classical_def_ty_param_def() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical decl int width
    int[◉size◉] cdecl_iw = 3;

    // classical decl uint width
    uint[◉size◉] cdecl_uiw = 3;

    // classical decl float width
    float[◉size◉] cdecl_fw = 3.14;

    // classical decl angle width
    angle[◉size◉] cdecl_aw = pi;

    // complex type width
    complex[float[◉size◉]] w = 1.0 + 2.0im;

    // const decl width
    const float[◉size◉] ccdecl_cfw = 1.0 * ◉size◉;

    // const complex type width
    const complex[float[◉size◉]] ccw = 1.0 + 2.0im;
    "#,
    );
}

#[test]
fn sized_io_scalar_ty_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // input decl width
    input float[◉size◉] ifw;
    "#,
    );
}

#[test]
fn sized_old_style_length_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // old style classical length
    creg old_creg[◉size◉];

    // old style quantum length
    qreg old_qreg[◉size◉];
    "#,
    );
}

#[test]
fn sized_cast_ty_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // cast width
    float[◉size◉] cast = float[◉size◉](4);
    "#,
    );
}

#[test]
fn array_decls_ty_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical array decl type width
    array[int[◉size◉], 5] cadecl_itw;
    "#,
    );
}

#[test]
fn array_decls_dims_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical array dims
    array[int, ◉size◉, 2 * ◉size◉] cadecl_itw_dims_sizes;
    "#,
    );
}

#[test]
fn array_decls_ty_size_and_dims_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical array dims
    array[int[◉size◉], ◉size◉, 2 * ◉size◉] cadecl_itw_dims_sizes;
    "#,
    );
}

#[test]
fn complex_array_decls_ty_param_and_dims_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // complex array size and dims
    array[complex[float[◉size◉ - 3]], ◉size◉, 2 * ◉size◉] cadecl_ctw_dims_sizes;
    "#,
    );
}

#[test]
fn io_array_decls_ty_param_ref() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // classical array size and dims
    input array[int[◉size◉], ◉size◉, 2 * ◉size◉] cadecl_iitw_dims_sizes;

    // classical array size and dims
    output array[int[◉size◉], ◉size◉, 2 * ◉size◉] cadecl_oitw_dims_sizes;

    // complex array size and dims
    input array[complex[float[◉size◉ - 3]], ◉size◉, 2 * ◉size◉] cadecl_ictw_dims_sizes;

    // complex array size and dims
    output array[complex[float[◉size◉ - 3]], ◉size◉, 2 * ◉size◉] cadecl_octw_dims_sizes;
    "#,
    );
}

#[test]
fn def_ty_params_and_returns() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // return ty width
    def sample_def_return(int t) -> int[◉size◉] { return t; }

    // def param ty width
    def sample_def_param(int[◉size◉] t) -> int { return t; }

    // return ty width
    def sample_def_complex_return(int c) -> complex[float[◉size◉]] { return c; }

    // def param ty width
    def sample_def_complex_param(complex[float[◉size◉]] c) -> complex { return c; }

    // def param array ty width
    def sample_def_array_param(readonly array[int[◉size◉], ◉size◉, 2 * ◉size◉] c) -> int { return 0; }

    // def param array ty width
    def sample_def_mut_array_param(mutable array[int[◉size◉], ◉size◉, 2 * ◉size◉] c) -> int { return 0; }
    "#,
    );
}

#[test]
fn def_dyn_array_ty_params() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // def param dyn array ty width and dims
    def sample_def_dyn_array_param(readonly array[int[◉size◉], dim = 1 * ◉size◉] c) -> int { return 0; }
    "#,
    );
}

#[test]
fn extern_ty_params_and_returns() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // return ty width
    extern sample_def_return(int) -> int[◉size◉];

    // def param ty width
    extern sample_def_param(int[◉size◉]) -> int;

    // return complex ty width
    extern sample_def_complex_return(int) -> complex[float[◉size◉]];

    // def param complex ty width
    extern sample_def_complex_param(complex[float[◉size◉]]) -> complex;

    // extern def param array ty width
    extern sample_extern_def_array_param(readonly array[int[◉size◉], ◉size◉, 2 * ◉size◉]) -> int;

    // extern def param mut array ty width
    extern sample_extern_def_mut_array_param(mutable array[int[◉size◉], ◉size◉, 2 * ◉size◉]) -> int;

    // extern def param creg ty width
    extern sample_extern_def_creg_param(creg[2 * ◉size◉]) -> int;
    "#,
    );
}

#[test]
fn extern_dyn_array_ty_params_and_returns() {
    check(
        r#"
    // classical decl
    const int ◉s↘ize◉ = 5;

    // extern def param mut array ty width
    extern sample_extern_def_mut_dyn_array_param(readonly array[int[◉size◉], dim = 1 * ◉size◉]) -> int;
    "#,
    );
}

#[test]
fn def_captures_ref_original_symbol_def() {
    check(
        r#"
    // classical decl
    const int ◉n↘Qubits◉ = 5;

    def PrepareUniform(qubit[◉nQubits◉] q) -> bit[◉nQubits◉] {
        bit[◉nQubits◉] results;
        int ivar = ◉nQubits◉;
        for int i in [0:◉nQubits◉-1] {
        }
    }
    "#,
    );
}
