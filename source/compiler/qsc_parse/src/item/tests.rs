// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    parse, parse_attr, parse_implicit_namespace, parse_import_or_export, parse_open,
    parse_spec_decl, parse_top_level_nodes,
};
use crate::{
    scan::ParserContext,
    tests::{check, check_vec, check_vec_v2_preview},
};
use expect_test::expect;

fn parse_namespaces(s: &mut ParserContext) -> Result<Vec<qsc_ast::ast::Namespace>, crate::Error> {
    super::parse_namespaces(s)
}

#[test]
fn body_intrinsic() {
    check(
        parse_spec_decl,
        "body intrinsic;",
        &expect!["SpecDecl _id_ [0-15] (Body): Gen: Intrinsic"],
    );
}

#[test]
fn adjoint_self() {
    check(
        parse_spec_decl,
        "adjoint self;",
        &expect!["SpecDecl _id_ [0-13] (Adj): Gen: Slf"],
    );
}

#[test]
fn adjoint_invert() {
    check(
        parse_spec_decl,
        "adjoint invert;",
        &expect!["SpecDecl _id_ [0-15] (Adj): Gen: Invert"],
    );
}

// unit tests for file_name_to_namespace_name
#[test]
fn file_name_to_namespace_name() {
    let raw = "foo/bar.qs";
    check(
        |s| parse_implicit_namespace(raw, s),
        "",
        &expect![[r#"Namespace _id_ [0-0] ([Ident _id_ [0-0] "foo", Ident _id_ [0-0] "bar"]):"#]],
    );
}

#[test]
fn controlled_distribute() {
    check(
        parse_spec_decl,
        "controlled distribute;",
        &expect!["SpecDecl _id_ [0-22] (Ctl): Gen: Distribute"],
    );
}

#[test]
fn controlled_adjoint_auto() {
    check(
        parse_spec_decl,
        "controlled adjoint auto;",
        &expect!["SpecDecl _id_ [0-24] (CtlAdj): Gen: Auto"],
    );
}

#[test]
fn spec_gen_missing_semi() {
    check(
        parse_spec_decl,
        "body intrinsic",
        &expect![[r#"
            Error(
                Token(
                    Semi,
                    Eof,
                    Span {
                        lo: 14,
                        hi: 14,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn spec_invalid_gen() {
    check(
        parse_spec_decl,
        "adjoint foo;",
        &expect![[r#"
            Error(
                Token(
                    Open(
                        Brace,
                    ),
                    Semi,
                    Span {
                        lo: 11,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn open_no_alias() {
    check(
        parse,
        "open Foo.Bar.Baz;",
        &expect![[r#"
            Item _id_ [0-17]:
                Open (Path _id_ [5-16]:
                    Ident _id_ [5-8] "Foo"
                    Ident _id_ [9-12] "Bar"
                    Ident _id_ [13-16] "Baz")"#]],
    );
}

#[test]
fn open_alias() {
    check(
        parse,
        "open Foo.Bar.Baz as Baz;",
        &expect![[r#"
            Item _id_ [0-24]:
                Open (Path _id_ [5-16]:
                    Ident _id_ [5-8] "Foo"
                    Ident _id_ [9-12] "Bar"
                    Ident _id_ [13-16] "Baz") (Ident _id_ [20-23] "Baz")"#]],
    );
}

#[test]
fn struct_decl_empty() {
    check(
        parse,
        "struct Foo { }",
        &expect![[r#"
            Item _id_ [0-14]:
                Struct _id_ [0-14] (Ident _id_ [7-10] "Foo"): <empty>"#]],
    );
}

#[test]
fn struct_decl() {
    check(
        parse,
        "struct Foo { field : Int }",
        &expect![[r#"
            Item _id_ [0-26]:
                Struct _id_ [0-26] (Ident _id_ [7-10] "Foo"):
                    FieldDef _id_ [13-24] (Ident _id_ [13-18] "field"): Type _id_ [21-24]: Path: Path _id_ [21-24] (Ident _id_ [21-24] "Int")"#]],
    );
}

#[test]
fn struct_decl_no_fields() {
    check(
        parse,
        "struct Foo { }",
        &expect![[r#"
            Item _id_ [0-14]:
                Struct _id_ [0-14] (Ident _id_ [7-10] "Foo"): <empty>"#]],
    );
}

#[test]
fn struct_decl_multiple_fields() {
    check(
        parse,
        "struct Foo { x : Int, y : Double, z : String }",
        &expect![[r#"
            Item _id_ [0-46]:
                Struct _id_ [0-46] (Ident _id_ [7-10] "Foo"):
                    FieldDef _id_ [13-20] (Ident _id_ [13-14] "x"): Type _id_ [17-20]: Path: Path _id_ [17-20] (Ident _id_ [17-20] "Int")
                    FieldDef _id_ [22-32] (Ident _id_ [22-23] "y"): Type _id_ [26-32]: Path: Path _id_ [26-32] (Ident _id_ [26-32] "Double")
                    FieldDef _id_ [34-44] (Ident _id_ [34-35] "z"): Type _id_ [38-44]: Path: Path _id_ [38-44] (Ident _id_ [38-44] "String")"#]],
    );
}

#[test]
fn ty_decl() {
    check(
        parse,
        "newtype Foo = Unit;",
        &expect![[r#"
            Item _id_ [0-19]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-18]: Field:
                    Type _id_ [14-18]: Path: Path _id_ [14-18] (Ident _id_ [14-18] "Unit")"#]],
    );
}

#[test]
fn ty_decl_field_name() {
    check(
        parse,
        "newtype Foo = Bar : Int;",
        &expect![[r#"
            Item _id_ [0-24]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-23]: Field:
                    Ident _id_ [14-17] "Bar"
                    Type _id_ [20-23]: Path: Path _id_ [20-23] (Ident _id_ [20-23] "Int")"#]],
    );
}

#[test]
fn ty_decl_doc() {
    check(
        parse,
        "/// This is a
        /// doc comment.
        newtype Foo = Int;",
        &expect![[r#"
            Item _id_ [0-65]:
                doc:
                    This is a
                    doc comment.
                New Type (Ident _id_ [55-58] "Foo"): TyDef _id_ [61-64]: Field:
                    Type _id_ [61-64]: Path: Path _id_ [61-64] (Ident _id_ [61-64] "Int")"#]],
    );
}

#[test]
fn udt_item_doc() {
    check(
        parse,
        "newtype Foo = (
        /// doc-string for arg1
        arg1 : Int,
        /// doc-string for arg2
        arg2 : Int
    );",
        &expect![[r#"
            Item _id_ [0-125]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-124]: Tuple:
                    TyDef _id_ [56-66]: Field:
                        Ident _id_ [56-60] "arg1"
                        Type _id_ [63-66]: Path: Path _id_ [63-66] (Ident _id_ [63-66] "Int")
                    TyDef _id_ [108-118]: Field:
                        Ident _id_ [108-112] "arg2"
                        Type _id_ [115-118]: Path: Path _id_ [115-118] (Ident _id_ [115-118] "Int")"#]],
    );
}

#[test]
fn callable_param_doc() {
    check(
        parse,
        "
        operation Foo(
          /// the input
        input: Int): Unit {}
        ",
        &expect![[r#"
            Item _id_ [9-76]:
                Callable _id_ [9-76] (Operation):
                    name: Ident _id_ [19-22] "Foo"
                    input: Pat _id_ [22-67]: Paren:
                        Pat _id_ [56-66]: Bind:
                            Ident _id_ [56-61] "input"
                            Type _id_ [63-66]: Path: Path _id_ [63-66] (Ident _id_ [63-66] "Int")
                    output: Type _id_ [69-73]: Path: Path _id_ [69-73] (Ident _id_ [69-73] "Unit")
                    body: Block: Block _id_ [74-76]: <empty>"#]],
    );
}

#[test]
fn callable_return_doc() {
    check(
        parse,
        "
        operation Foo(input: Int):
        /// the return type
        Unit {}
        ",
        &expect![[r#"
            Item _id_ [9-79]:
                Callable _id_ [9-79] (Operation):
                    name: Ident _id_ [19-22] "Foo"
                    input: Pat _id_ [22-34]: Paren:
                        Pat _id_ [23-33]: Bind:
                            Ident _id_ [23-28] "input"
                            Type _id_ [30-33]: Path: Path _id_ [30-33] (Ident _id_ [30-33] "Int")
                    output: Type _id_ [72-76]: Path: Path _id_ [72-76] (Ident _id_ [72-76] "Unit")
                    body: Block: Block _id_ [77-79]: <empty>"#]],
    );
}

#[test]
fn nested_udt_item_doc() {
    check(
        parse,
        "newtype Nested = (Double,
            (
                /// Doc comment 1
                ItemName : Int,
                /// Doc comment 2
                String,
                (
                    /// Doc comment 3
                    ItemName: String
                )
            )
        );",
        &expect![[r#"
            Item _id_ [0-299]:
                New Type (Ident _id_ [8-14] "Nested"): TyDef _id_ [17-298]: Tuple:
                    TyDef _id_ [18-24]: Field:
                        Type _id_ [18-24]: Path: Path _id_ [18-24] (Ident _id_ [18-24] "Double")
                    TyDef _id_ [38-288]: Tuple:
                        TyDef _id_ [90-104]: Field:
                            Ident _id_ [90-98] "ItemName"
                            Type _id_ [101-104]: Path: Path _id_ [101-104] (Ident _id_ [101-104] "Int")
                        TyDef _id_ [156-162]: Field:
                            Type _id_ [156-162]: Path: Path _id_ [156-162] (Ident _id_ [156-162] "String")
                        TyDef _id_ [180-274]: Paren:
                            TyDef _id_ [240-256]: Field:
                                Ident _id_ [240-248] "ItemName"
                                Type _id_ [250-256]: Path: Path _id_ [250-256] (Ident _id_ [250-256] "String")"#]],
    );
}

#[test]
fn allow_docstring_basic_type() {
    check(
        parse,
        "newtype Nested = (Double,
            (
            ItemName:
                /// Doc comment
                String
            )
        );",
        &expect![[r#"
            Item _id_ [0-141]:
                New Type (Ident _id_ [8-14] "Nested"): TyDef _id_ [17-140]: Tuple:
                    TyDef _id_ [18-24]: Field:
                        Type _id_ [18-24]: Path: Path _id_ [18-24] (Ident _id_ [18-24] "Double")
                    TyDef _id_ [38-130]: Paren:
                        TyDef _id_ [52-116]: Field:
                            Ident _id_ [52-60] "ItemName"
                            Type _id_ [110-116]: Path: Path _id_ [110-116] (Ident _id_ [110-116] "String")"#]],
    );
}

#[test]
fn ty_def_invalid_field_name() {
    check(
        parse,
        "newtype Foo = Bar.Baz : Int[];",
        &expect![[r#"
            Error(
                Convert(
                    "identifier",
                    "type",
                    Span {
                        lo: 14,
                        hi: 21,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn ty_def_tuple() {
    check(
        parse,
        "newtype Foo = (Int, Int);",
        &expect![[r#"
            Item _id_ [0-25]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-24]: Field:
                    Type _id_ [14-24]: Tuple:
                        Type _id_ [15-18]: Path: Path _id_ [15-18] (Ident _id_ [15-18] "Int")
                        Type _id_ [20-23]: Path: Path _id_ [20-23] (Ident _id_ [20-23] "Int")"#]],
    );
}

#[test]
fn ty_def_tuple_one_named() {
    check(
        parse,
        "newtype Foo = (X : Int, Int);",
        &expect![[r#"
            Item _id_ [0-29]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-28]: Tuple:
                    TyDef _id_ [15-22]: Field:
                        Ident _id_ [15-16] "X"
                        Type _id_ [19-22]: Path: Path _id_ [19-22] (Ident _id_ [19-22] "Int")
                    TyDef _id_ [24-27]: Field:
                        Type _id_ [24-27]: Path: Path _id_ [24-27] (Ident _id_ [24-27] "Int")"#]],
    );
}

#[test]
fn ty_def_tuple_both_named() {
    check(
        parse,
        "newtype Foo = (X : Int, Y : Int);",
        &expect![[r#"
            Item _id_ [0-33]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-32]: Tuple:
                    TyDef _id_ [15-22]: Field:
                        Ident _id_ [15-16] "X"
                        Type _id_ [19-22]: Path: Path _id_ [19-22] (Ident _id_ [19-22] "Int")
                    TyDef _id_ [24-31]: Field:
                        Ident _id_ [24-25] "Y"
                        Type _id_ [28-31]: Path: Path _id_ [28-31] (Ident _id_ [28-31] "Int")"#]],
    );
}

#[test]
fn ty_def_nested_tuple() {
    check(
        parse,
        "newtype Foo = ((X : Int, Y : Int), Z : Int);",
        &expect![[r#"
            Item _id_ [0-44]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-43]: Tuple:
                    TyDef _id_ [15-33]: Tuple:
                        TyDef _id_ [16-23]: Field:
                            Ident _id_ [16-17] "X"
                            Type _id_ [20-23]: Path: Path _id_ [20-23] (Ident _id_ [20-23] "Int")
                        TyDef _id_ [25-32]: Field:
                            Ident _id_ [25-26] "Y"
                            Type _id_ [29-32]: Path: Path _id_ [29-32] (Ident _id_ [29-32] "Int")
                    TyDef _id_ [35-42]: Field:
                        Ident _id_ [35-36] "Z"
                        Type _id_ [39-42]: Path: Path _id_ [39-42] (Ident _id_ [39-42] "Int")"#]],
    );
}

#[test]
fn ty_def_tuple_with_name() {
    check(
        parse,
        "newtype Foo = Pair : (Int, Int);",
        &expect![[r#"
            Item _id_ [0-32]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-31]: Field:
                    Ident _id_ [14-18] "Pair"
                    Type _id_ [21-31]: Tuple:
                        Type _id_ [22-25]: Path: Path _id_ [22-25] (Ident _id_ [22-25] "Int")
                        Type _id_ [27-30]: Path: Path _id_ [27-30] (Ident _id_ [27-30] "Int")"#]],
    );
}

#[test]
fn ty_def_tuple_array() {
    check(
        parse,
        "newtype Foo = (Int, Int)[];",
        &expect![[r#"
        Item _id_ [0-27]:
            New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-26]: Field:
                Type _id_ [14-26]: Array: Type _id_ [14-24]: Tuple:
                    Type _id_ [15-18]: Path: Path _id_ [15-18] (Ident _id_ [15-18] "Int")
                    Type _id_ [20-23]: Path: Path _id_ [20-23] (Ident _id_ [20-23] "Int")"#]],
    );
}

#[test]
fn ty_def_tuple_lambda_args() {
    check(
        parse,
        "newtype Foo = (Int, Int) -> Int;",
        &expect![[r#"
            Item _id_ [0-32]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-31]: Field:
                    Type _id_ [14-31]: Arrow (Function):
                        param: Type _id_ [14-24]: Tuple:
                            Type _id_ [15-18]: Path: Path _id_ [15-18] (Ident _id_ [15-18] "Int")
                            Type _id_ [20-23]: Path: Path _id_ [20-23] (Ident _id_ [20-23] "Int")
                        return: Type _id_ [28-31]: Path: Path _id_ [28-31] (Ident _id_ [28-31] "Int")"#]],
    );
}

#[test]
fn ty_def_duplicate_comma() {
    check(
        parse,
        "newtype Foo = (Int,, Int);",
        &expect![[r#"
            Item _id_ [0-26]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-25]: Tuple:
                    TyDef _id_ [15-18]: Field:
                        Type _id_ [15-18]: Path: Path _id_ [15-18] (Ident _id_ [15-18] "Int")
                    TyDef _id_ [19-19]: Err
                    TyDef _id_ [21-24]: Field:
                        Type _id_ [21-24]: Path: Path _id_ [21-24] (Ident _id_ [21-24] "Int")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 19,
                            hi: 19,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn ty_def_initial_comma() {
    check(
        parse,
        "newtype Foo = (, Int);",
        &expect![[r#"
            Item _id_ [0-22]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-21]: Tuple:
                    TyDef _id_ [15-15]: Err
                    TyDef _id_ [17-20]: Field:
                        Type _id_ [17-20]: Path: Path _id_ [17-20] (Ident _id_ [17-20] "Int")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 15,
                            hi: 15,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn ty_def_named_duplicate_comma() {
    check(
        parse,
        "newtype Foo = (X : Int,, Int);",
        &expect![[r#"
            Item _id_ [0-30]:
                New Type (Ident _id_ [8-11] "Foo"): TyDef _id_ [14-29]: Tuple:
                    TyDef _id_ [15-22]: Field:
                        Ident _id_ [15-16] "X"
                        Type _id_ [19-22]: Path: Path _id_ [19-22] (Ident _id_ [19-22] "Int")
                    TyDef _id_ [23-23]: Err
                    TyDef _id_ [25-28]: Field:
                        Type _id_ [25-28]: Path: Path _id_ [25-28] (Ident _id_ [25-28] "Int")

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 23,
                            hi: 23,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn function_decl() {
    check(
        parse,
        "function Foo() : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-41]:
                Callable _id_ [0-41] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-14]: Unit
                    output: Type _id_ [17-21]: Path: Path _id_ [17-21] (Ident _id_ [17-21] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [24-39] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn function_decl_doc() {
    check(
        parse,
        "/// This is a
        /// doc comment.
        function Foo() : () {}",
        &expect![[r#"
            Item _id_ [0-69]:
                doc:
                    This is a
                    doc comment.
                Callable _id_ [47-69] (Function):
                    name: Ident _id_ [56-59] "Foo"
                    input: Pat _id_ [59-61]: Unit
                    output: Type _id_ [64-66]: Unit
                    body: Block: Block _id_ [67-69]: <empty>"#]],
    );
}

#[test]
fn doc_between_attr_and_keyword() {
    check(
        parse,
        "@EntryPoint()
        /// doc comment.
        function Foo() : () {}",
        &expect![[r#"
            Item _id_ [0-69]:
                Attr _id_ [0-13] (Ident _id_ [1-11] "EntryPoint"):
                    Expr _id_ [11-13]: Unit
                Callable _id_ [22-69] (Function):
                    name: Ident _id_ [56-59] "Foo"
                    input: Pat _id_ [59-61]: Unit
                    output: Type _id_ [64-66]: Unit
                    body: Block: Block _id_ [67-69]: <empty>"#]],
    );
}

#[test]
fn operation_decl() {
    check(
        parse,
        "operation Foo() : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-42]:
                Callable _id_ [0-42] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [25-40] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn operation_decl_doc() {
    check(
        parse,
        "/// This is a
        /// doc comment.
        operation Foo() : () {}",
        &expect![[r#"
            Item _id_ [0-70]:
                doc:
                    This is a
                    doc comment.
                Callable _id_ [47-70] (Operation):
                    name: Ident _id_ [57-60] "Foo"
                    input: Pat _id_ [60-62]: Unit
                    output: Type _id_ [65-67]: Unit
                    body: Block: Block _id_ [68-70]: <empty>"#]],
    );
}

#[test]
fn function_one_param() {
    check(
        parse,
        "function Foo(x : Int) : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-48]:
                Callable _id_ [0-48] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-21]: Paren:
                        Pat _id_ [13-20]: Bind:
                            Ident _id_ [13-14] "x"
                            Type _id_ [17-20]: Path: Path _id_ [17-20] (Ident _id_ [17-20] "Int")
                    output: Type _id_ [24-28]: Path: Path _id_ [24-28] (Ident _id_ [24-28] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [31-46] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn function_two_params() {
    check(
        parse,
        "function Foo(x : Int, y : Int) : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-57]:
                Callable _id_ [0-57] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-30]: Tuple:
                        Pat _id_ [13-20]: Bind:
                            Ident _id_ [13-14] "x"
                            Type _id_ [17-20]: Path: Path _id_ [17-20] (Ident _id_ [17-20] "Int")
                        Pat _id_ [22-29]: Bind:
                            Ident _id_ [22-23] "y"
                            Type _id_ [26-29]: Path: Path _id_ [26-29] (Ident _id_ [26-29] "Int")
                    output: Type _id_ [33-37]: Path: Path _id_ [33-37] (Ident _id_ [33-37] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [40-55] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn function_one_ty_param() {
    check(
        parse,
        "function Foo<'T>() : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-45]:
                Callable _id_ [0-45] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    generics:
                        'T
                    input: Pat _id_ [16-18]: Unit
                    output: Type _id_ [21-25]: Path: Path _id_ [21-25] (Ident _id_ [21-25] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [28-43] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn function_two_ty_params() {
    check(
        parse,
        "function Foo<'T, 'U>() : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-49]:
                Callable _id_ [0-49] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    generics:
                        'T,
                        'U
                    input: Pat _id_ [20-22]: Unit
                    output: Type _id_ [25-29]: Path: Path _id_ [25-29] (Ident _id_ [25-29] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [32-47] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn function_duplicate_comma_in_ty_param() {
    check(
        parse,
        "function Foo<'T,,>() : Unit { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-47]:
                Callable _id_ [0-47] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    generics:
                        'T,

                    input: Pat _id_ [18-20]: Unit
                    output: Type _id_ [23-27]: Path: Path _id_ [23-27] (Ident _id_ [23-27] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [30-45] (Body): Gen: Intrinsic

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 16,
                            hi: 16,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn function_single_impl() {
    check(
        parse,
        "function Foo(x : Int) : Int { let y = x; y }",
        &expect![[r#"
            Item _id_ [0-44]:
                Callable _id_ [0-44] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-21]: Paren:
                        Pat _id_ [13-20]: Bind:
                            Ident _id_ [13-14] "x"
                            Type _id_ [17-20]: Path: Path _id_ [17-20] (Ident _id_ [17-20] "Int")
                    output: Type _id_ [24-27]: Path: Path _id_ [24-27] (Ident _id_ [24-27] "Int")
                    body: Block: Block _id_ [28-44]:
                        Stmt _id_ [30-40]: Local (Immutable):
                            Pat _id_ [34-35]: Bind:
                                Ident _id_ [34-35] "y"
                            Expr _id_ [38-39]: Path: Path _id_ [38-39] (Ident _id_ [38-39] "x")
                        Stmt _id_ [41-42]: Expr: Expr _id_ [41-42]: Path: Path _id_ [41-42] (Ident _id_ [41-42] "y")"#]],
    );
}

#[test]
fn function_body_missing_semi_between_stmts() {
    check(
        parse,
        "function Foo() : () { f(x) g(y) }",
        &expect![[r#"
            Item _id_ [0-33]:
                Callable _id_ [0-33] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-14]: Unit
                    output: Type _id_ [17-19]: Unit
                    body: Block: Block _id_ [20-33]:
                        Stmt _id_ [22-26]: Expr: Expr _id_ [22-26]: Call:
                            Expr _id_ [22-23]: Path: Path _id_ [22-23] (Ident _id_ [22-23] "f")
                            Expr _id_ [23-26]: Paren: Expr _id_ [24-25]: Path: Path _id_ [24-25] (Ident _id_ [24-25] "x")
                        Stmt _id_ [27-31]: Expr: Expr _id_ [27-31]: Call:
                            Expr _id_ [27-28]: Path: Path _id_ [27-28] (Ident _id_ [27-28] "g")
                            Expr _id_ [28-31]: Paren: Expr _id_ [29-30]: Path: Path _id_ [29-30] (Ident _id_ [29-30] "y")

            [
                Error(
                    MissingSemi(
                        Span {
                            lo: 26,
                            hi: 26,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn operation_body_impl() {
    check(
        parse,
        "operation Foo() : Unit { body (...) { x } }",
        &expect![[r#"
            Item _id_ [0-43]:
                Callable _id_ [0-43] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [25-41] (Body): Impl:
                            Pat _id_ [30-35]: Paren:
                                Pat _id_ [31-34]: Elided
                            Block _id_ [36-41]:
                                Stmt _id_ [38-39]: Expr: Expr _id_ [38-39]: Path: Path _id_ [38-39] (Ident _id_ [38-39] "x")"#]],
    );
}

#[test]
fn operation_body_ctl_impl() {
    check(
        parse,
        "operation Foo() : Unit { body (...) { x } controlled (cs, ...) { y } }",
        &expect![[r#"
            Item _id_ [0-70]:
                Callable _id_ [0-70] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [25-41] (Body): Impl:
                            Pat _id_ [30-35]: Paren:
                                Pat _id_ [31-34]: Elided
                            Block _id_ [36-41]:
                                Stmt _id_ [38-39]: Expr: Expr _id_ [38-39]: Path: Path _id_ [38-39] (Ident _id_ [38-39] "x")
                        SpecDecl _id_ [42-68] (Ctl): Impl:
                            Pat _id_ [53-62]: Tuple:
                                Pat _id_ [54-56]: Bind:
                                    Ident _id_ [54-56] "cs"
                                Pat _id_ [58-61]: Elided
                            Block _id_ [63-68]:
                                Stmt _id_ [65-66]: Expr: Expr _id_ [65-66]: Path: Path _id_ [65-66] (Ident _id_ [65-66] "y")"#]],
    );
}

#[test]
fn operation_impl_and_gen() {
    check(
        parse,
        "operation Foo() : Unit { body (...) { x } adjoint self; }",
        &expect![[r#"
            Item _id_ [0-57]:
                Callable _id_ [0-57] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [25-41] (Body): Impl:
                            Pat _id_ [30-35]: Paren:
                                Pat _id_ [31-34]: Elided
                            Block _id_ [36-41]:
                                Stmt _id_ [38-39]: Expr: Expr _id_ [38-39]: Path: Path _id_ [38-39] (Ident _id_ [38-39] "x")
                        SpecDecl _id_ [42-55] (Adj): Gen: Slf"#]],
    );
}

#[test]
fn operation_is_adj() {
    check(
        parse,
        "operation Foo() : Unit is Adj {}",
        &expect![[r#"
            Item _id_ [0-32]:
                Callable _id_ [0-32] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    functors: Functor Expr _id_ [26-29]: Adj
                    body: Block: Block _id_ [30-32]: <empty>"#]],
    );
}

#[test]
fn operation_is_adj_ctl() {
    check(
        parse,
        "operation Foo() : Unit is Adj + Ctl {}",
        &expect![[r#"
            Item _id_ [0-38]:
                Callable _id_ [0-38] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    input: Pat _id_ [13-15]: Unit
                    output: Type _id_ [18-22]: Path: Path _id_ [18-22] (Ident _id_ [18-22] "Unit")
                    functors: Functor Expr _id_ [26-35]: BinOp Union: (Functor Expr _id_ [26-29]: Adj) (Functor Expr _id_ [32-35]: Ctl)
                    body: Block: Block _id_ [36-38]: <empty>"#]],
    );
}

#[test]
fn function_missing_output_ty() {
    check(
        parse,
        "function Foo() { body intrinsic; }",
        &expect![[r#"
            Item _id_ [0-34]:
                Callable _id_ [0-34] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-14]: Unit
                    output: Type _id_ [15-15]: Err
                    body: Specializations:
                        SpecDecl _id_ [17-32] (Body): Gen: Intrinsic

            [
                Error(
                    Token(
                        Colon,
                        Open(
                            Brace,
                        ),
                        Span {
                            lo: 15,
                            hi: 16,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn internal_ty() {
    check(
        parse,
        "internal newtype Foo = Unit;",
        &expect![[r#"
            Item _id_ [0-28]:
                New Type (Ident _id_ [17-20] "Foo"): TyDef _id_ [23-27]: Field:
                    Type _id_ [23-27]: Path: Path _id_ [23-27] (Ident _id_ [23-27] "Unit")"#]],
    );
}

#[test]
fn internal_function() {
    check(
        parse,
        "internal function Foo() : Unit {}",
        &expect![[r#"
            Item _id_ [0-33]:
                Callable _id_ [9-33] (Function):
                    name: Ident _id_ [18-21] "Foo"
                    input: Pat _id_ [21-23]: Unit
                    output: Type _id_ [26-30]: Path: Path _id_ [26-30] (Ident _id_ [26-30] "Unit")
                    body: Block: Block _id_ [31-33]: <empty>"#]],
    );
}

#[test]
fn internal_function_doc() {
    check(
        parse,
        "/// This is a
        /// doc comment.
        internal function Foo() : () {}",
        &expect![[r#"
            Item _id_ [0-78]:
                doc:
                    This is a
                    doc comment.
                Callable _id_ [56-78] (Function):
                    name: Ident _id_ [65-68] "Foo"
                    input: Pat _id_ [68-70]: Unit
                    output: Type _id_ [73-75]: Unit
                    body: Block: Block _id_ [76-78]: <empty>"#]],
    );
}

#[test]
fn internal_operation() {
    check(
        parse,
        "internal operation Foo() : Unit {}",
        &expect![[r#"
            Item _id_ [0-34]:
                Callable _id_ [9-34] (Operation):
                    name: Ident _id_ [19-22] "Foo"
                    input: Pat _id_ [22-24]: Unit
                    output: Type _id_ [27-31]: Path: Path _id_ [27-31] (Ident _id_ [27-31] "Unit")
                    body: Block: Block _id_ [32-34]: <empty>"#]],
    );
}

#[test]
fn attr_no_args() {
    check(
        parse_attr,
        "@Foo()",
        &expect![[r#"
            Attr _id_ [0-6] (Ident _id_ [1-4] "Foo"):
                Expr _id_ [4-6]: Unit"#]],
    );
}

#[test]
fn attr_single_arg() {
    check(
        parse_attr,
        "@Foo(123)",
        &expect![[r#"
            Attr _id_ [0-9] (Ident _id_ [1-4] "Foo"):
                Expr _id_ [4-9]: Paren: Expr _id_ [5-8]: Lit: Int(123)"#]],
    );
}

#[test]
fn attr_two_args() {
    check(
        parse_attr,
        "@Foo(123, \"bar\")",
        &expect![[r#"
            Attr _id_ [0-16] (Ident _id_ [1-4] "Foo"):
                Expr _id_ [4-16]: Tuple:
                    Expr _id_ [5-8]: Lit: Int(123)
                    Expr _id_ [10-15]: Lit: String("bar")"#]],
    );
}

#[test]
fn open_attr() {
    check(
        parse,
        "@Foo() open Bar;",
        &expect![[r#"
            Item _id_ [0-16]:
                Attr _id_ [0-6] (Ident _id_ [1-4] "Foo"):
                    Expr _id_ [4-6]: Unit
                Open (Path _id_ [12-15] (Ident _id_ [12-15] "Bar"))"#]],
    );
}

#[test]
fn newtype_attr() {
    check(
        parse,
        "@Foo() newtype Bar = Unit;",
        &expect![[r#"
            Item _id_ [0-26]:
                Attr _id_ [0-6] (Ident _id_ [1-4] "Foo"):
                    Expr _id_ [4-6]: Unit
                New Type (Ident _id_ [15-18] "Bar"): TyDef _id_ [21-25]: Field:
                    Type _id_ [21-25]: Path: Path _id_ [21-25] (Ident _id_ [21-25] "Unit")"#]],
    );
}

#[test]
fn operation_one_attr() {
    check(
        parse,
        "@Foo() operation Bar() : Unit {}",
        &expect![[r#"
            Item _id_ [0-32]:
                Attr _id_ [0-6] (Ident _id_ [1-4] "Foo"):
                    Expr _id_ [4-6]: Unit
                Callable _id_ [7-32] (Operation):
                    name: Ident _id_ [17-20] "Bar"
                    input: Pat _id_ [20-22]: Unit
                    output: Type _id_ [25-29]: Path: Path _id_ [25-29] (Ident _id_ [25-29] "Unit")
                    body: Block: Block _id_ [30-32]: <empty>"#]],
    );
}

#[test]
fn operation_two_attrs() {
    check(
        parse,
        "@Foo() @Bar() operation Baz() : Unit {}",
        &expect![[r#"
            Item _id_ [0-39]:
                Attr _id_ [0-6] (Ident _id_ [1-4] "Foo"):
                    Expr _id_ [4-6]: Unit
                Attr _id_ [7-13] (Ident _id_ [8-11] "Bar"):
                    Expr _id_ [11-13]: Unit
                Callable _id_ [14-39] (Operation):
                    name: Ident _id_ [24-27] "Baz"
                    input: Pat _id_ [27-29]: Unit
                    output: Type _id_ [32-36]: Path: Path _id_ [32-36] (Ident _id_ [32-36] "Unit")
                    body: Block: Block _id_ [37-39]: <empty>"#]],
    );
}

#[test]
fn operation_attr_doc() {
    check(
        parse,
        "/// This is a
        /// doc comment.
        @Foo()
        operation Bar() : () {}",
        &expect![[r#"
            Item _id_ [0-85]:
                doc:
                    This is a
                    doc comment.
                Attr _id_ [47-53] (Ident _id_ [48-51] "Foo"):
                    Expr _id_ [51-53]: Unit
                Callable _id_ [62-85] (Operation):
                    name: Ident _id_ [72-75] "Bar"
                    input: Pat _id_ [75-77]: Unit
                    output: Type _id_ [80-82]: Unit
                    body: Block: Block _id_ [83-85]: <empty>"#]],
    );
}

#[test]
fn namespace_function() {
    check_vec(
        parse_namespaces,
        "namespace A { function Foo() : Unit { body intrinsic; } }",
        &expect![[r#"
            Namespace _id_ [0-57] (Ident _id_ [10-11] "A"):
                Item _id_ [14-55]:
                    Callable _id_ [14-55] (Function):
                        name: Ident _id_ [23-26] "Foo"
                        input: Pat _id_ [26-28]: Unit
                        output: Type _id_ [31-35]: Path: Path _id_ [31-35] (Ident _id_ [31-35] "Unit")
                        body: Specializations:
                            SpecDecl _id_ [38-53] (Body): Gen: Intrinsic"#]],
    );
}

#[test]
fn namespace_doc() {
    check_vec(
        parse_namespaces,
        "/// This is a
        /// doc comment.
        namespace A {
            function Foo() : () {}
        }",
        &expect![[r#"
            Namespace _id_ [0-105] (Ident _id_ [57-58] "A"):
                doc:
                    This is a
                    doc comment.
                Item _id_ [73-95]:
                    Callable _id_ [73-95] (Function):
                        name: Ident _id_ [82-85] "Foo"
                        input: Pat _id_ [85-87]: Unit
                        output: Type _id_ [90-92]: Unit
                        body: Block: Block _id_ [93-95]: <empty>"#]],
    );
}

#[test]
fn floating_doc_comments_in_namespace() {
    check_vec(
        parse_namespaces,
        "namespace MyQuantumProgram {
    @EntryPoint()
    function Main() : Unit {}
    /// hi
    /// another doc comment
}
",
        &expect![[r#"
            Namespace _id_ [0-117] (Ident _id_ [10-26] "MyQuantumProgram"):
                Item _id_ [33-76]:
                    Attr _id_ [33-46] (Ident _id_ [34-44] "EntryPoint"):
                        Expr _id_ [44-46]: Unit
                    Callable _id_ [51-76] (Function):
                        name: Ident _id_ [60-64] "Main"
                        input: Pat _id_ [64-66]: Unit
                        output: Type _id_ [69-73]: Path: Path _id_ [69-73] (Ident _id_ [69-73] "Unit")
                        body: Block: Block _id_ [74-76]: <empty>
                Item _id_ [81-115]:
                    Err

            [
                Error(
                    FloatingDocComment(
                        Span {
                            lo: 81,
                            hi: 115,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn floating_attr_in_namespace() {
    check_vec(
        parse_namespaces,
        "namespace MyQuantumProgram { @EntryPoint() }",
        &expect![[r#"
        Namespace _id_ [0-44] (Ident _id_ [10-26] "MyQuantumProgram"):
            Item _id_ [29-42]:
                Err

        [
            Error(
                FloatingAttr(
                    Span {
                        lo: 29,
                        hi: 42,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn floating_visibility_in_namespace() {
    check_vec(
        parse_namespaces,
        "namespace MyQuantumProgram { internal }",
        &expect![[r#"
            Namespace _id_ [0-39] (Ident _id_ [10-26] "MyQuantumProgram"):
                Item _id_ [29-37]:
                    Err

            [
                Error(
                    FloatingVisibility(
                        Span {
                            lo: 29,
                            hi: 37,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn two_namespaces() {
    check_vec(
        parse_namespaces,
        "namespace A {} namespace B {}",
        &expect![[r#"
            Namespace _id_ [0-14] (Ident _id_ [10-11] "A"):,
            Namespace _id_ [15-29] (Ident _id_ [25-26] "B"):"#]],
    );
}

#[test]
fn two_namespaces_docs() {
    check_vec(
        parse_namespaces,
        "/// This is the first namespace.
        namespace A {}
        /// This is the second namespace.
        namespace B {}",
        &expect![[r#"
            Namespace _id_ [0-55] (Ident _id_ [51-52] "A"):
                doc:
                    This is the first namespace.,
            Namespace _id_ [64-120] (Ident _id_ [116-117] "B"):
                doc:
                    This is the second namespace."#]],
    );
}

#[test]
fn two_open_items() {
    check_vec(
        parse_namespaces,
        "namespace A { open B; open C; }",
        &expect![[r#"
            Namespace _id_ [0-31] (Ident _id_ [10-11] "A"):
                Item _id_ [14-21]:
                    Open (Path _id_ [19-20] (Ident _id_ [19-20] "B"))
                Item _id_ [22-29]:
                    Open (Path _id_ [27-28] (Ident _id_ [27-28] "C"))"#]],
    );
}

#[test]
fn two_ty_items() {
    check_vec(
        parse_namespaces,
        "namespace A { newtype B = Unit; newtype C = Unit; }",
        &expect![[r#"
            Namespace _id_ [0-51] (Ident _id_ [10-11] "A"):
                Item _id_ [14-31]:
                    New Type (Ident _id_ [22-23] "B"): TyDef _id_ [26-30]: Field:
                        Type _id_ [26-30]: Path: Path _id_ [26-30] (Ident _id_ [26-30] "Unit")
                Item _id_ [32-49]:
                    New Type (Ident _id_ [40-41] "C"): TyDef _id_ [44-48]: Field:
                        Type _id_ [44-48]: Path: Path _id_ [44-48] (Ident _id_ [44-48] "Unit")"#]],
    );
}

#[test]
fn two_callable_items() {
    check_vec(
        parse_namespaces,
        "namespace A { operation B() : Unit {} function C() : Unit {} }",
        &expect![[r#"
            Namespace _id_ [0-62] (Ident _id_ [10-11] "A"):
                Item _id_ [14-37]:
                    Callable _id_ [14-37] (Operation):
                        name: Ident _id_ [24-25] "B"
                        input: Pat _id_ [25-27]: Unit
                        output: Type _id_ [30-34]: Path: Path _id_ [30-34] (Ident _id_ [30-34] "Unit")
                        body: Block: Block _id_ [35-37]: <empty>
                Item _id_ [38-60]:
                    Callable _id_ [38-60] (Function):
                        name: Ident _id_ [47-48] "C"
                        input: Pat _id_ [48-50]: Unit
                        output: Type _id_ [53-57]: Path: Path _id_ [53-57] (Ident _id_ [53-57] "Unit")
                        body: Block: Block _id_ [58-60]: <empty>"#]],
    );
}

#[test]
fn two_callable_items_docs() {
    check_vec(
        parse_namespaces,
        "namespace A {
            /// This is the first callable.
            function Foo() : () {}
            /// This is the second callable.
            operation Foo() : () {}
        }",
        &expect![[r#"
            Namespace _id_ [0-183] (Ident _id_ [10-11] "A"):
                Item _id_ [26-92]:
                    doc:
                        This is the first callable.
                    Callable _id_ [70-92] (Function):
                        name: Ident _id_ [79-82] "Foo"
                        input: Pat _id_ [82-84]: Unit
                        output: Type _id_ [87-89]: Unit
                        body: Block: Block _id_ [90-92]: <empty>
                Item _id_ [105-173]:
                    doc:
                        This is the second callable.
                    Callable _id_ [150-173] (Operation):
                        name: Ident _id_ [160-163] "Foo"
                        input: Pat _id_ [163-165]: Unit
                        output: Type _id_ [168-170]: Unit
                        body: Block: Block _id_ [171-173]: <empty>"#]],
    );
}

#[test]
fn doc_without_item() {
    check_vec(
        parse_namespaces,
        "namespace A {
            /// This is a doc comment.
        }",
        &expect![[r#"
            Namespace _id_ [0-62] (Ident _id_ [10-11] "A"):
                Item _id_ [26-52]:
                    Err

            [
                Error(
                    FloatingDocComment(
                        Span {
                            lo: 26,
                            hi: 52,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_callable_item() {
    check_vec(
        parse_namespaces,
        "namespace A {
            function Foo() : Int { 5 }
            function Bar() { 10 }
            operation Baz() : Double { 2.0 }
        }",
        &expect![[r#"
            Namespace _id_ [0-141] (Ident _id_ [10-11] "A"):
                Item _id_ [26-52]:
                    Callable _id_ [26-52] (Function):
                        name: Ident _id_ [35-38] "Foo"
                        input: Pat _id_ [38-40]: Unit
                        output: Type _id_ [43-46]: Path: Path _id_ [43-46] (Ident _id_ [43-46] "Int")
                        body: Block: Block _id_ [47-52]:
                            Stmt _id_ [49-50]: Expr: Expr _id_ [49-50]: Lit: Int(5)
                Item _id_ [65-86]:
                    Callable _id_ [65-86] (Function):
                        name: Ident _id_ [74-77] "Bar"
                        input: Pat _id_ [77-79]: Unit
                        output: Type _id_ [80-80]: Err
                        body: Block: Block _id_ [80-86]:
                            Stmt _id_ [82-84]: Expr: Expr _id_ [82-84]: Lit: Int(10)
                Item _id_ [99-131]:
                    Callable _id_ [99-131] (Operation):
                        name: Ident _id_ [109-112] "Baz"
                        input: Pat _id_ [112-114]: Unit
                        output: Type _id_ [117-123]: Path: Path _id_ [117-123] (Ident _id_ [117-123] "Double")
                        body: Block: Block _id_ [124-131]:
                            Stmt _id_ [126-129]: Expr: Expr _id_ [126-129]: Lit: Double(2)

            [
                Error(
                    Token(
                        Colon,
                        Open(
                            Brace,
                        ),
                        Span {
                            lo: 80,
                            hi: 81,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_unclosed_callable_item() {
    check_vec(
        parse_namespaces,
        "namespace A {
            function Foo() : Int {",
        &expect![[r#"
            Namespace _id_ [0-48] (Ident _id_ [10-11] "A"):
                Item _id_ [26-48]:
                    Callable _id_ [26-48] (Function):
                        name: Ident _id_ [35-38] "Foo"
                        input: Pat _id_ [38-40]: Unit
                        output: Type _id_ [43-46]: Path: Path _id_ [43-46] (Ident _id_ [43-46] "Int")
                        body: Block: Block _id_ [47-48]: <empty>

            [
                Error(
                    Token(
                        Close(
                            Brace,
                        ),
                        Eof,
                        Span {
                            lo: 48,
                            hi: 48,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn recover_unclosed_namespace() {
    check_vec(
        parse_namespaces,
        "namespace A {
            function Foo() : Int { 2 }",
        &expect![[r#"
            Namespace _id_ [0-52] (Ident _id_ [10-11] "A"):
                Item _id_ [26-52]:
                    Callable _id_ [26-52] (Function):
                        name: Ident _id_ [35-38] "Foo"
                        input: Pat _id_ [38-40]: Unit
                        output: Type _id_ [43-46]: Path: Path _id_ [43-46] (Ident _id_ [43-46] "Int")
                        body: Block: Block _id_ [47-52]:
                            Stmt _id_ [49-50]: Expr: Expr _id_ [49-50]: Lit: Int(2)

            [
                Error(
                    Token(
                        Close(
                            Brace,
                        ),
                        Eof,
                        Span {
                            lo: 52,
                            hi: 52,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn callable_missing_parens() {
    check_vec(
        parse_namespaces,
        "namespace A {
        function Foo x : Int : Int { x }
        }",
        &expect![[r#"
            Namespace _id_ [0-64] (Ident _id_ [10-11] "A"):
                Item _id_ [22-54]:
                    Err

            [
                Error(
                    MissingParens(
                        Span {
                            lo: 35,
                            hi: 42,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn callable_missing_close_parens() {
    check_vec(
        parse_namespaces,
        "namespace A {
        function Foo (x : Int : Int { x }
        }",
        &expect![[r#"
            Namespace _id_ [0-65] (Ident _id_ [10-11] "A"):
                Item _id_ [22-55]:
                    Err

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Colon,
                        Span {
                            lo: 44,
                            hi: 45,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn callable_missing_open_parens() {
    check_vec(
        parse_namespaces,
        "namespace A {
        function Foo x : Int) : Int { x }
        }",
        &expect![[r#"
            Namespace _id_ [0-65] (Ident _id_ [10-11] "A"):
                Item _id_ [22-55]:
                    Err

            [
                Error(
                    MissingParens(
                        Span {
                            lo: 35,
                            hi: 42,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn disallow_qubit_scoped_block() {
    check_vec_v2_preview(
        parse_namespaces,
        "namespace Foo { operation Main() : Unit { use q1 = Qubit() {  };  } }",
        &expect![[r#"
            Namespace _id_ [0-69] (Ident _id_ [10-13] "Foo"):
                Item _id_ [16-67]:
                    Callable _id_ [16-67] (Operation):
                        name: Ident _id_ [26-30] "Main"
                        input: Pat _id_ [30-32]: Unit
                        output: Type _id_ [35-39]: Path: Path _id_ [35-39] (Ident _id_ [35-39] "Unit")
                        body: Block: Block _id_ [40-67]:
                            Stmt _id_ [42-58]: Qubit (Fresh)
                                Pat _id_ [46-48]: Bind:
                                    Ident _id_ [46-48] "q1"
                                QubitInit _id_ [51-58] Single
                            Stmt _id_ [59-64]: Semi: Expr _id_ [59-63]: Expr Block: Block _id_ [59-63]: <empty>

            [
                Error(
                    Token(
                        Semi,
                        Open(
                            Brace,
                        ),
                        Span {
                            lo: 59,
                            hi: 60,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn reject_nested_namespace_with_items() {
    check_vec(
        parse_namespaces,
        "namespace Outer {
            namespace Inner {
                function NestedFunction() : Unit {}
                newtype NestedType = Int;
            }
        }",
        &expect![[r#"
            Namespace _id_ [0-99] (Ident _id_ [10-15] "Outer"):

            [
                Error(
                    Token(
                        Close(
                            Brace,
                        ),
                        Keyword(
                            Namespace,
                        ),
                        Span {
                            lo: 30,
                            hi: 39,
                        },
                    ),
                ),
                Error(
                    Token(
                        Eof,
                        Keyword(
                            Newtype,
                        ),
                        Span {
                            lo: 116,
                            hi: 123,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn reject_namespace_with_multiple_nested_levels() {
    check_vec(
        parse_namespaces,
        "namespace LevelOne {
            namespace LevelTwo {
                namespace LevelThree {
                    function DeepFunction() : Unit {}
                }
            }
        }",
        &expect![[r#"
            Namespace _id_ [0-146] (Ident _id_ [10-18] "LevelOne"):

            [
                Error(
                    Token(
                        Close(
                            Brace,
                        ),
                        Keyword(
                            Namespace,
                        ),
                        Span {
                            lo: 33,
                            hi: 42,
                        },
                    ),
                ),
                Error(
                    Token(
                        Eof,
                        Close(
                            Brace,
                        ),
                        Span {
                            lo: 163,
                            hi: 164,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn namespace_with_attributes_and_docs() {
    check_vec(
        parse_namespaces,
        "/// Documentation for LevelOne
        namespace LevelOne {
            @ExampleAttr()
            /// Documentation that shouldn't show up, since docstrings go above attrs
            function InnerItem() : Unit {}
        }",
        &expect![[r#"
            Namespace _id_ [0-225] (Ident _id_ [49-57] "LevelOne"):
                doc:
                    Documentation for LevelOne
                Item _id_ [72-215]:
                    Attr _id_ [72-86] (Ident _id_ [73-84] "ExampleAttr"):
                        Expr _id_ [84-86]: Unit
                    Callable _id_ [99-215] (Function):
                        name: Ident _id_ [194-203] "InnerItem"
                        input: Pat _id_ [203-205]: Unit
                        output: Type _id_ [208-212]: Path: Path _id_ [208-212] (Ident _id_ [208-212] "Unit")
                        body: Block: Block _id_ [213-215]: <empty>"#]],
    );
}

#[test]
fn namespace_with_conflicting_names() {
    check_vec(
        parse_namespaces,
        "namespace Conflicts {
            function Item() : Unit {}
            newtype Item = Int;
        }",
        &expect![[r#"
            Namespace _id_ [0-101] (Ident _id_ [10-19] "Conflicts"):
                Item _id_ [34-59]:
                    Callable _id_ [34-59] (Function):
                        name: Ident _id_ [43-47] "Item"
                        input: Pat _id_ [47-49]: Unit
                        output: Type _id_ [52-56]: Path: Path _id_ [52-56] (Ident _id_ [52-56] "Unit")
                        body: Block: Block _id_ [57-59]: <empty>
                Item _id_ [72-91]:
                    New Type (Ident _id_ [80-84] "Item"): TyDef _id_ [87-90]: Field:
                        Type _id_ [87-90]: Path: Path _id_ [87-90] (Ident _id_ [87-90] "Int")"#]],
    );
}

// We technically broke this syntax as of May 2024. Although we don't think anybody was using it,
// we want to make sure we provide a helpful error message.
#[test]
fn helpful_error_on_dotted_alias() {
    check_vec(
        parse_namespaces,
        "namespace A {
            open Microsoft.Quantum.Math as Foo.Bar.Baz;
            operation Main() : Unit {}
        }",
        &expect![[r#"
            Namespace _id_ [0-118] (Ident _id_ [10-11] "A"):
                Item _id_ [26-69]:
                    Err
                Item _id_ [82-108]:
                    Callable _id_ [82-108] (Operation):
                        name: Ident _id_ [92-96] "Main"
                        input: Pat _id_ [96-98]: Unit
                        output: Type _id_ [101-105]: Path: Path _id_ [101-105] (Ident _id_ [101-105] "Unit")
                        body: Block: Block _id_ [106-108]: <empty>

            [
                Error(
                    DotIdentAlias(
                        Span {
                            lo: 60,
                            hi: 61,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn parse_export_basic() {
    check_vec(
        parse_namespaces,
        "namespace Foo {
               operation Bar() : Unit {}
               export Bar;
        }",
        &expect![[r#"
            Namespace _id_ [0-93] (Ident _id_ [10-13] "Foo"):
                Item _id_ [31-56]:
                    Callable _id_ [31-56] (Operation):
                        name: Ident _id_ [41-44] "Bar"
                        input: Pat _id_ [44-46]: Unit
                        output: Type _id_ [49-53]: Path: Path _id_ [49-53] (Ident _id_ [49-53] "Unit")
                        body: Block: Block _id_ [54-56]: <empty>
                Item _id_ [72-83]:
                    Export [72-83]:
                        [79-82] Direct: Path _id_ [79-82] (Ident _id_ [79-82] "Bar")"#]],
    );
}

#[test]
fn parse_export_list() {
    check_vec(
        parse_namespaces,
        "namespace Foo {
               operation Bar() : Unit {}
               export Bar, Baz.Quux, Math.Quantum.Some.Nested, Math.Quantum.Some.Other.Nested;
        }",
        &expect![[r#"
            Namespace _id_ [0-161] (Ident _id_ [10-13] "Foo"):
                Item _id_ [31-56]:
                    Callable _id_ [31-56] (Operation):
                        name: Ident _id_ [41-44] "Bar"
                        input: Pat _id_ [44-46]: Unit
                        output: Type _id_ [49-53]: Path: Path _id_ [49-53] (Ident _id_ [49-53] "Unit")
                        body: Block: Block _id_ [54-56]: <empty>
                Item _id_ [72-151]:
                    Export [72-151]:
                        [79-82] Direct: Path _id_ [79-82] (Ident _id_ [79-82] "Bar")
                        [84-92] Direct: Path _id_ [84-92]:
                            Ident _id_ [84-87] "Baz"
                            Ident _id_ [88-92] "Quux"
                        [94-118] Direct: Path _id_ [94-118]:
                            Ident _id_ [94-98] "Math"
                            Ident _id_ [99-106] "Quantum"
                            Ident _id_ [107-111] "Some"
                            Ident _id_ [112-118] "Nested"
                        [120-150] Direct: Path _id_ [120-150]:
                            Ident _id_ [120-124] "Math"
                            Ident _id_ [125-132] "Quantum"
                            Ident _id_ [133-137] "Some"
                            Ident _id_ [138-143] "Other"
                            Ident _id_ [144-150] "Nested""#]],
    );
}

#[test]
fn parse_single_import() {
    check(
        parse_import_or_export,
        "import Foo;",
        &expect![[r#"
            Import [0-11]:
                [7-10] Direct: Path _id_ [7-10] (Ident _id_ [7-10] "Foo")"#]],
    );
}

#[test]
fn parse_multiple_imports() {
    check(
        parse_import_or_export,
        "import Foo.Bar, Foo.Baz;",
        &expect![[r#"
            Import [0-24]:
                [7-14] Direct: Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"
                [16-23] Direct: Path _id_ [16-23]:
                    Ident _id_ [16-19] "Foo"
                    Ident _id_ [20-23] "Baz""#]],
    );
}

#[test]
fn parse_import_with_alias() {
    check(
        parse_import_or_export,
        "import Foo as Bar;",
        &expect![[r#"
            Import [0-18]:
                [7-17] Direct (alias: Ident _id_ [14-17] "Bar"): Path _id_ [7-10] (Ident _id_ [7-10] "Foo")"#]],
    );
}

#[test]
fn multi_import_with_alias() {
    check(
        parse_import_or_export,
        "import Foo.Bar as Baz, Foo.Quux;",
        &expect![[r#"
            Import [0-32]:
                [7-21] Direct (alias: Ident _id_ [18-21] "Baz"): Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"
                [23-31] Direct: Path _id_ [23-31]:
                    Ident _id_ [23-26] "Foo"
                    Ident _id_ [27-31] "Quux""#]],
    );
}

#[test]
fn empty_import_statement() {
    check(
        parse_import_or_export,
        "import;",
        &expect![[r#"
            Error(
                Token(
                    Ident,
                    Semi,
                    Span {
                        lo: 6,
                        hi: 7,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn parse_export_empty() {
    check_vec(
        parse_namespaces,
        "namespace Foo {
               operation Bar() : Unit {}
               export;
        }",
        &expect![[r#"
            Namespace _id_ [0-89] (Ident _id_ [10-13] "Foo"):
                Item _id_ [31-56]:
                    Callable _id_ [31-56] (Operation):
                        name: Ident _id_ [41-44] "Bar"
                        input: Pat _id_ [44-46]: Unit
                        output: Type _id_ [49-53]: Path: Path _id_ [49-53] (Ident _id_ [49-53] "Unit")
                        body: Block: Block _id_ [54-56]: <empty>
                Item _id_ [72-79]:
                    Err

            [
                Error(
                    Token(
                        Ident,
                        Semi,
                        Span {
                            lo: 78,
                            hi: 79,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn parse_glob_import() {
    check(
        parse_import_or_export,
        "import Foo.*;",
        &expect![[r#"
            Import [0-13]:
                [7-12] Wildcard: Path _id_ [7-10] (Ident _id_ [7-10] "Foo")"#]],
    );
}

#[test]
fn parse_glob_import_in_list() {
    check(
        parse_import_or_export,
        "import Foo.Bar, Foo.Baz.*;",
        &expect![[r#"
            Import [0-26]:
                [7-14] Direct: Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"
                [16-25] Wildcard: Path _id_ [16-23]:
                    Ident _id_ [16-19] "Foo"
                    Ident _id_ [20-23] "Baz""#]],
    );
}

#[test]
fn parse_glob_import_of_parent_in_list() {
    check(
        parse_import_or_export,
        "import Foo.Bar, Foo.Baz, Foo.*;",
        &expect![[r#"
            Import [0-31]:
                [7-14] Direct: Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"
                [16-23] Direct: Path _id_ [16-23]:
                    Ident _id_ [16-19] "Foo"
                    Ident _id_ [20-23] "Baz"
                [25-30] Wildcard: Path _id_ [25-28] (Ident _id_ [25-28] "Foo")"#]],
    );
}

#[test]
fn parse_glob_import_with_alias() {
    check(
        parse_import_or_export,
        "import Foo.* as Foo;",
        &expect![[r#"
            Error(
                WildcardAlias {
                    span: Span {
                        lo: 7,
                        hi: 19,
                    },
                    path: "Path _id_ [7-10] (Ident _id_ [7-10] \"Foo\")",
                    alias: "Ident _id_ [16-19] \"Foo\"",
                },
            )
        "#]],
    );
}

#[test]
fn parse_aliased_glob_import_in_list() {
    check(
        parse_import_or_export,
        "import Foo.Bar, Foo.Baz.* as Quux;",
        &expect![[r#"
            Error(
                WildcardAlias {
                    span: Span {
                        lo: 16,
                        hi: 33,
                    },
                    path: "Path _id_ [16-23]:\n    Ident _id_ [16-19] \"Foo\"\n    Ident _id_ [20-23] \"Baz\"",
                    alias: "Ident _id_ [29-33] \"Quux\"",
                },
            )
        "#]],
    );
}

#[test]
fn invalid_glob_syntax_extra_asterisk() {
    check(
        parse_import_or_export,
        "import Foo.**;",
        &expect![[r#"
            Import [0-12]:
                [7-12] Wildcard: Path _id_ [7-10] (Ident _id_ [7-10] "Foo")

            [
                Error(
                    Token(
                        Semi,
                        ClosedBinOp(
                            Star,
                        ),
                        Span {
                            lo: 12,
                            hi: 13,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_glob_syntax_missing_dot() {
    check(
        parse_import_or_export,
        "import Foo.Bar**;",
        &expect![[r#"
            Import [0-14]:
                [7-14] Direct: Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"

            [
                Error(
                    Token(
                        Semi,
                        ClosedBinOp(
                            Star,
                        ),
                        Span {
                            lo: 14,
                            hi: 15,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_glob_syntax_multiple_asterisks_in_path() {
    check(
        parse_import_or_export,
        "import Foo.Bar.*.*;",
        &expect![[r#"
            Import [0-16]:
                [7-16] Wildcard: Path _id_ [7-14]:
                    Ident _id_ [7-10] "Foo"
                    Ident _id_ [11-14] "Bar"

            [
                Error(
                    Token(
                        Semi,
                        Dot,
                        Span {
                            lo: 16,
                            hi: 17,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_glob_syntax_with_following_ident() {
    check(
        parse_import_or_export,
        "import Foo.*.Bar;",
        &expect![[r#"
            Import [0-12]:
                [7-12] Wildcard: Path _id_ [7-10] (Ident _id_ [7-10] "Foo")

            [
                Error(
                    Token(
                        Semi,
                        Dot,
                        Span {
                            lo: 12,
                            hi: 13,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn invalid_glob_syntax_follows_keyword() {
    check(
        parse_import_or_export,
        "import Foo.in*;",
        &expect![[r#"
            Import [0-13]:
                [7-13] Direct: Err IncompletePath [7-13]:
                    Ident _id_ [7-10] "Foo"

            [
                Error(
                    Rule(
                        "identifier",
                        Keyword(
                            In,
                        ),
                        Span {
                            lo: 11,
                            hi: 13,
                        },
                    ),
                ),
                Error(
                    Token(
                        Semi,
                        ClosedBinOp(
                            Star,
                        ),
                        Span {
                            lo: 13,
                            hi: 14,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn disallow_top_level_recursive_glob() {
    check(
        parse_import_or_export,
        "import *;",
        &expect![[r#"
            Error(
                Token(
                    Ident,
                    ClosedBinOp(
                        Star,
                    ),
                    Span {
                        lo: 7,
                        hi: 8,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn disallow_wildcard_export() {
    check(
        parse_import_or_export,
        "export Foo.*;",
        &expect![[r#"
            Error(
                ExportWildcard(
                    Span {
                        lo: 7,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn incomplete_open_with_semi() {
    check(
        parse_open,
        "open Foo.  ;",
        &expect![[r#"
            Open (Err IncompletePath [5-11]:
                Ident _id_ [5-8] "Foo")

            [
                Error(
                    Rule(
                        "identifier",
                        Semi,
                        Span {
                            lo: 11,
                            hi: 12,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn incomplete_open_no_semi() {
    check(
        parse_open,
        "open Foo. ",
        &expect![[r#"
            Open (Err IncompletePath [5-10]:
                Ident _id_ [5-8] "Foo")

            [
                Error(
                    Rule(
                        "identifier",
                        Eof,
                        Span {
                            lo: 10,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn missing_semi_between_items() {
    check_vec(
        parse_namespaces,
        "namespace Foo { open Foo open Bar. open Baz }",
        &expect![[r#"
            Namespace _id_ [0-45] (Ident _id_ [10-13] "Foo"):
                Item _id_ [16-24]:
                    Open (Path _id_ [21-24] (Ident _id_ [21-24] "Foo"))
                Item _id_ [25-35]:
                    Open (Err IncompletePath [30-35]:
                        Ident _id_ [30-33] "Bar")
                Item _id_ [35-43]:
                    Open (Path _id_ [40-43] (Ident _id_ [40-43] "Baz"))

            [
                Error(
                    Token(
                        Semi,
                        Keyword(
                            Open,
                        ),
                        Span {
                            lo: 25,
                            hi: 29,
                        },
                    ),
                ),
                Error(
                    Rule(
                        "identifier",
                        Keyword(
                            Open,
                        ),
                        Span {
                            lo: 35,
                            hi: 39,
                        },
                    ),
                ),
                Error(
                    Token(
                        Semi,
                        Keyword(
                            Open,
                        ),
                        Span {
                            lo: 35,
                            hi: 39,
                        },
                    ),
                ),
                Error(
                    Token(
                        Semi,
                        Close(
                            Brace,
                        ),
                        Span {
                            lo: 44,
                            hi: 45,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn allow_class_bound_on_type_param() {
    check(
        parse,
        "operation Foo<'T: Eq + Ord, 'E: Eq>() : Unit {}",
        &expect![[r#"
            Item _id_ [0-47]:
                Callable _id_ [0-47] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    generics:
                        'T: Eq + Ord,
                        'E: Eq
                    input: Pat _id_ [35-37]: Unit
                    output: Type _id_ [40-44]: Path: Path _id_ [40-44] (Ident _id_ [40-44] "Unit")
                    body: Block: Block _id_ [45-47]: <empty>"#]],
    );
}

#[test]
fn callable_decl_no_return_type_or_body_recovery() {
    check(
        parse,
        "operation Foo<'T>() : ",
        &expect![[r#"
            Item _id_ [0-22]:
                Callable _id_ [0-22] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    generics:
                        'T
                    input: Pat _id_ [17-19]: Unit
                    output: Type _id_ [22-22]: Err
                    body: Block: Block _id_ [22-22]: <empty>

            [
                Error(
                    Rule(
                        "type",
                        Eof,
                        Span {
                            lo: 22,
                            hi: 22,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn callable_decl_broken_return_type_no_body_recovery() {
    check(
        parse,
        "operation Foo<'T>() : () => ",
        &expect![[r#"
            Item _id_ [0-28]:
                Callable _id_ [0-28] (Operation):
                    name: Ident _id_ [10-13] "Foo"
                    generics:
                        'T
                    input: Pat _id_ [17-19]: Unit
                    output: Type _id_ [22-28]: Arrow (Operation):
                        param: Type _id_ [22-24]: Unit
                        return: Type _id_ [28-28]: Err
                    body: Block: Block _id_ [28-28]: <empty>

            [
                Error(
                    Rule(
                        "type",
                        Eof,
                        Span {
                            lo: 28,
                            hi: 28,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn top_level_nodes() {
    check_vec(
        parse_top_level_nodes,
        "function Foo() : Unit { body intrinsic; } let x = 5;",
        &expect![[r#"
            Stmt _id_ [0-41]: Item: Item _id_ [0-41]:
                Callable _id_ [0-41] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-14]: Unit
                    output: Type _id_ [17-21]: Path: Path _id_ [17-21] (Ident _id_ [17-21] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [24-39] (Body): Gen: Intrinsic,
            Stmt _id_ [42-52]: Local (Immutable):
                Pat _id_ [46-47]: Bind:
                    Ident _id_ [46-47] "x"
                Expr _id_ [50-51]: Lit: Int(5)"#]],
    );
}

#[test]
fn top_level_nodes_error_recovery() {
    check_vec(
        parse_top_level_nodes,
        "function Foo() : Unit { body intrinsic; } 3 + ",
        &expect![[r#"
            Stmt _id_ [0-41]: Item: Item _id_ [0-41]:
                Callable _id_ [0-41] (Function):
                    name: Ident _id_ [9-12] "Foo"
                    input: Pat _id_ [12-14]: Unit
                    output: Type _id_ [17-21]: Path: Path _id_ [17-21] (Ident _id_ [17-21] "Unit")
                    body: Specializations:
                        SpecDecl _id_ [24-39] (Body): Gen: Intrinsic,
            Stmt _id_ [42-45]: Err

            [
                Error(
                    Rule(
                        "expression",
                        Eof,
                        Span {
                            lo: 46,
                            hi: 46,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn test_attribute() {
    check(
        parse,
        "@Test() function Foo() : Unit {}",
        &expect![[r#"
            Item _id_ [0-32]:
                Attr _id_ [0-7] (Ident _id_ [1-5] "Test"):
                    Expr _id_ [5-7]: Unit
                Callable _id_ [8-32] (Function):
                    name: Ident _id_ [17-20] "Foo"
                    input: Pat _id_ [20-22]: Unit
                    output: Type _id_ [25-29]: Path: Path _id_ [25-29] (Ident _id_ [25-29] "Unit")
                    body: Block: Block _id_ [30-32]: <empty>"#]],
    );
}
