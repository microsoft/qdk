// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod given_interpreter {
    use crate::interpret::{InterpretResult, Interpreter};
    use expect_test::Expect;
    use miette::Diagnostic;
    use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
    use qsc_eval::{output::CursorReceiver, val::Value};
    use qsc_frontend::compile::SourceMap;
    use qsc_passes::PackageType;
    use std::{fmt::Write, io::Cursor, iter, str::from_utf8};

    fn line(interpreter: &mut Interpreter, line: &str) -> (InterpretResult, String) {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let mut receiver = CursorReceiver::new(&mut cursor);
        (
            interpreter.eval_fragments(&mut receiver, line),
            receiver.dump(),
        )
    }

    fn run(interpreter: &mut Interpreter, expr: &str) -> (InterpretResult, String) {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let mut receiver = CursorReceiver::new(&mut cursor);
        let res = interpreter.run(&mut receiver, Some(expr), None, None);
        (res, receiver.dump())
    }

    fn entry(interpreter: &mut Interpreter) -> (InterpretResult, String) {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let mut receiver = CursorReceiver::new(&mut cursor);
        (interpreter.eval_entry(&mut receiver), receiver.dump())
    }

    fn fragment(
        interpreter: &mut Interpreter,
        fragments: &str,
        package: crate::ast::Package,
    ) -> (InterpretResult, String) {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let mut receiver = CursorReceiver::new(&mut cursor);
        let result = interpreter.eval_ast_fragments(&mut receiver, fragments, package);
        (result, receiver.dump())
    }

    fn invoke(
        interpreter: &mut Interpreter,
        callable: &str,
        args: Value,
    ) -> (InterpretResult, String) {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let mut receiver = CursorReceiver::new(&mut cursor);
        let callable = match interpreter.eval_fragments(&mut receiver, callable) {
            Ok(val) => val,
            Err(e) => return (Err(e), receiver.dump()),
        };
        let result = interpreter.invoke(&mut receiver, callable, args);
        (result, receiver.dump())
    }

    mod without_sources {
        use std::rc::Rc;

        use expect_test::expect;
        use indoc::indoc;

        use super::*;

        mod without_stdlib {
            use qsc_frontend::compile::SourceMap;
            use qsc_passes::PackageType;

            use super::*;

            #[test]
            fn stdlib_members_should_be_unavailable() {
                let store = crate::PackageStore::new(crate::compile::core());
                let mut interpreter = Interpreter::new(
                    SourceMap::default(),
                    PackageType::Lib,
                    TargetCapabilityFlags::all(),
                    LanguageFeatures::default(),
                    store,
                    &[],
                )
                .expect("interpreter should be created");

                let (result, output) = line(&mut interpreter, "Message(\"_\")");
                is_only_error(
                    &result,
                    &output,
                    &expect![[r#"
                        name error: `Message` not found
                           [line_0] [Message]
                        type error: insufficient type information to infer type
                           [line_0] [Message("_")]
                    "#]],
                );
            }
        }

        #[test]
        fn stdlib_members_should_be_available() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "Message(\"_\")");
            is_unit_with_output(&result, &output, "_");
        }

        #[test]
        fn core_members_should_be_available() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "Length([1, 2, 3])");
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn let_bindings_update_interpreter() {
            let mut interpreter = get_interpreter();
            line(&mut interpreter, "let y = 7;")
                .0
                .expect("line should succeed");
            let (result, output) = line(&mut interpreter, "y");
            is_only_value(&result, &output, &Value::Int(7));
        }

        #[test]
        fn let_bindings_can_be_shadowed() {
            let mut interpreter = get_interpreter();

            let (result, output) = line(&mut interpreter, "let y = 7;");
            is_only_value(&result, &output, &Value::unit());

            let (result, output) = line(&mut interpreter, "y");
            is_only_value(&result, &output, &Value::Int(7));

            let (result, output) = line(&mut interpreter, "let y = \"Hello\";");
            is_only_value(&result, &output, &Value::unit());

            let (result, output) = line(&mut interpreter, "y");
            is_only_value(&result, &output, &Value::String("Hello".into()));
        }

        #[test]
        fn invalid_statements_return_error() {
            let mut interpreter = get_interpreter();

            let (result, output) = line(&mut interpreter, "let y = 7");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    syntax error: expected `;`, found EOF
                       [line_0] []
                "#]],
            );

            let (result, output) = line(&mut interpreter, "y");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `y` not found
                       [line_1] [y]
                "#]],
            );
        }

        #[test]
        fn invalid_statements_and_unbound_vars_return_error_on_immutable_usage() {
            let mut interpreter = get_interpreter();

            let (result, output) = line(&mut interpreter, "let y = x;");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `x` not found
                       [line_0] [x]
                    type error: insufficient type information to infer type
                       [line_0] [y]
                "#]],
            );

            let (result, output) = line(&mut interpreter, "y");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: name is not bound
                       [line_1] [y]
                "#]],
            );
        }

        #[test]
        fn invalid_statements_and_unbound_vars_return_error_on_mutable_update() {
            let mut interpreter = get_interpreter();

            let (result, output) = line(&mut interpreter, "mutable y = x;");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `x` not found
                       [line_0] [x]
                    type error: insufficient type information to infer type
                       [line_0] [y]
                "#]],
            );

            let (result, output) = line(&mut interpreter, "y = 3");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    cannot update immutable variable
                       [line_1] [y]
                "#]],
            );
        }

        #[test]
        fn invalid_statements_and_unbound_vars_return_error_on_immutable_usage_with_rca() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());

            let (result, output) = line(&mut interpreter, "let y = x;");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `x` not found
                       [line_0] [x]
                    type error: insufficient type information to infer type
                       [line_0] [y]
                "#]],
            );

            let (result, output) = line(&mut interpreter, "y");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: name is not bound
                       [line_1] [y]
                "#]],
            );
        }

        #[test]
        fn invalid_statements_and_unbound_vars_return_error_on_mutable_update_with_rca() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());

            let (result, output) = line(&mut interpreter, "mutable y = x;");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `x` not found
                       [line_0] [x]
                    type error: insufficient type information to infer type
                       [line_0] [y]
                "#]],
            );

            let (result, output) = line(&mut interpreter, "y = 3");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    cannot update immutable variable
                       [line_1] [y]
                "#]],
            );
        }

        #[test]
        fn failing_statements_return_early_error() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "let y = 7;y/0;y");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: division by zero
                      cannot divide by zero [line_0] [0]
                "#]],
            );
        }

        #[test]
        fn passes_are_run_on_incremental() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "within {Message(\"A\");} apply {Message(\"B\");}",
            );
            is_unit_with_output(&result, &output, "A\nB\nA");
        }

        #[test]
        fn declare_function() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "function Foo() : Int { 2 }");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Foo()");
            is_only_value(&result, &output, &Value::Int(2));
        }

        #[test]
        fn invalid_declare_function_and_unbound_call_return_error() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "function Foo() : Int { invalid }");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `invalid` not found
                       [line_0] [invalid]
                "#]],
            );
            let (result, output) = line(&mut interpreter, "Foo()");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: name is not bound
                       [line_1] [Foo]
                "#]],
            );
        }

        #[test]
        fn declare_function_call_same_line() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "function Foo() : Int { 2 }; Foo()");
            is_only_value(&result, &output, &Value::Int(2));
        }

        #[test]
        fn let_binding_function_declaration_call_same_line() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "let x = 1; function Foo() : Int { 2 }; Foo() + 1",
            );
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn nested_function() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "function Foo() : Int { function Bar() : Int { 1 }; Bar() + 1 }; Foo() + 1",
            );
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn open_namespace() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "import Std.Diagnostics.*;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "DumpMachine()");
            is_unit_with_output(&result, &output, "STATE:\nNo qubits allocated");
        }

        #[test]
        fn open_namespace_call_same_line() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "open Microsoft.Quantum.Diagnostics; DumpMachine()",
            );
            is_unit_with_output(&result, &output, "STATE:\nNo qubits allocated");
        }

        #[test]
        fn declare_namespace_call() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "namespace Foo { function Bar() : Int { 5 } }",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Foo.Bar()");
            is_only_value(&result, &output, &Value::Int(5));
        }

        #[test]
        fn declare_namespace_open_call() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "namespace Foo { function Bar() : Int { 5 } }",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "open Foo;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Bar()");
            is_only_value(&result, &output, &Value::Int(5));
        }

        #[test]
        fn declare_namespace_open_call_same_line() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "namespace Foo { function Bar() : Int { 5 } } open Foo; Bar()",
            );
            is_only_value(&result, &output, &Value::Int(5));
        }

        #[test]
        fn mix_stmts_and_namespace_same_line() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "Message(\"before\"); namespace Foo { function Bar() : Int { 5 } } Message(\"after\")",
            );
            is_unit_with_output(&result, &output, "before\nafter");
        }

        #[test]
        fn assign_array_index_expr_eval_in_order() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "mutable arr = [[[0, 1], [2, 3]], [[4, 5], [6, 7]]];",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(
                &mut interpreter,
                "arr[{ Message(\"First Index\"); 0 }][{ Message(\"Second Index\"); 1 }][{ Message(\"Third Index\"); 1 }] = 13;",
            );
            is_unit_with_output(&result, &output, "First Index\nSecond Index\nThird Index");
            let (result, output) = line(&mut interpreter, "arr");
            is_only_value(
                &result,
                &output,
                &Value::Array(Rc::new(vec![
                    Value::Array(Rc::new(vec![
                        Value::Array(Rc::new(vec![Value::Int(0), Value::Int(1)])),
                        Value::Array(Rc::new(vec![Value::Int(2), Value::Int(13)])),
                    ])),
                    Value::Array(Rc::new(vec![
                        Value::Array(Rc::new(vec![Value::Int(4), Value::Int(5)])),
                        Value::Array(Rc::new(vec![Value::Int(6), Value::Int(7)])),
                    ])),
                ])),
            );
        }

        #[test]
        fn global_qubits() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "import Std.Diagnostics.*;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "DumpMachine()");
            is_unit_with_output(&result, &output, "STATE:\nNo qubits allocated");
            let (result, output) = line(&mut interpreter, "use (q0, qs) = (Qubit(), Qubit[3]);");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "DumpMachine()");
            is_unit_with_output(&result, &output, "STATE:\n|0000⟩: 1+0i");
            let (result, output) = line(&mut interpreter, "X(q0); X(qs[1]);");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "DumpMachine()");
            is_unit_with_output(&result, &output, "STATE:\n|1010⟩: 1+0i");
        }

        #[test]
        fn ambiguous_type_error_in_top_level_stmts() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "let x = [];");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    type error: insufficient type information to infer type
                       [line_0] [[]]
                "#]],
            );
            let (result, output) = line(&mut interpreter, "let x = []; let y = [0] + x;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "function Foo() : Unit { let x = []; }");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    type error: insufficient type information to infer type
                       [line_2] [[]]
                "#]],
            );
        }

        #[test]
        fn resolved_type_persists_across_stmts() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "let x = []; let y = [0] + x;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "let z = [0.0] + x;");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    type error: expected Double, found Int
                       [line_1] [x]
                "#]],
            );
        }

        #[test]
        fn incremental_lambas_work() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "let x = 1; let f = (y) -> x + y;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "f(1)");
            is_only_value(&result, &output, &Value::Int(2));
        }

        #[test]
        fn mutability_persists_across_stmts() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "mutable x : Int[] = []; let y : Int[] = [];",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "set x += [0];");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "set y += [0];");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    cannot update immutable variable
                       [line_2] [y]
                "#]],
            );
            let (result, output) = line(&mut interpreter, "let lam = () -> y + [0];");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "let lam = () -> x + [0];");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    lambdas cannot close over mutable variables
                       [line_4] [() -> x + [0]]
                "#]],
            );
        }

        #[test]
        fn runtime_error_across_lines() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "operation Main() : Unit { Microsoft.Quantum.Random.DrawRandomInt(2,1); }",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Main()");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: empty range
                      the range cannot be empty [line_0] [(2,1)]
                "#]],
            );
        }

        #[test]
        fn compiler_error_across_lines() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "namespace Other { operation DumpMachine() : Unit { } }",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "open Other;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "import Std.Diagnostics.*;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "DumpMachine();");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `DumpMachine` could refer to the item in `Other` or `Std.Diagnostics`
                      ambiguous name [line_3] [DumpMachine]
                      found in this namespace [line_1] [Other]
                      and also in this namespace [line_2] [Std.Diagnostics]
                    type error: insufficient type information to infer type
                       [line_3] [DumpMachine()]
                "#]],
            );
        }

        #[test]
        fn runtime_error_from_stdlib() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "use q = Qubit(); CNOT(q,q)");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: qubits in invocation are not unique
                       [qsharp-library-source:Std/Intrinsic.qs] [(control, target)]
                "#]],
            );
        }

        #[test]
        fn items_usable_before_definition() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    function A() : Unit {
                        B();
                    }
                    function B() : Unit {}
                    A()
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
        }

        #[test]
        fn items_usable_before_definition_top_level() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    B();
                    function B() : Unit {}
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
        }

        #[test]
        fn interpreter_without_sources_has_no_items() {
            let interpreter = get_interpreter();
            let items = interpreter.source_globals();
            assert!(items.is_empty());
        }

        #[test]
        fn fragment_without_items_has_no_items() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "()");
            is_only_value(&result, &output, &Value::unit());
            let items = interpreter.user_globals();
            assert!(items.is_empty());
        }

        #[test]
        fn fragment_defining_items_has_items() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    function Foo() : Int { 2 }
                    function Bar() : Int { 3 }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
            let items = interpreter.user_globals();
            assert_eq!(items.len(), 2);
            // No namespace for top-level items
            assert!(items[0].0.is_empty());
            expect![[r#"
                "Foo"
            "#]]
            .assert_debug_eq(&items[0].1);
            // No namespace for top-level items
            assert!(items[1].0.is_empty());
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[1].1);
        }

        #[test]
        fn fragment_defining_items_with_namespace_has_items() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    namespace Foo {
                        function Bar() : Int { 3 }
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
            let items = interpreter.user_globals();
            assert_eq!(items.len(), 1);
            expect![[r#"
                [
                    "Foo",
                ]
            "#]]
            .assert_debug_eq(&items[0].0);
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[0].1);
        }

        #[test]
        fn fragments_defining_items_add_to_existing_items() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    function Foo() : Int { 2 }
                    function Bar() : Int { 3 }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
            let items = interpreter.user_globals();
            assert_eq!(items.len(), 2);
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    function Baz() : Int { 4 }
                    function Qux() : Int { 5 }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
            let items = interpreter.user_globals();
            assert_eq!(items.len(), 4);
            // No namespace for top-level items
            assert!(items[0].0.is_empty());
            expect![[r#"
                "Foo"
            "#]]
            .assert_debug_eq(&items[0].1);
            // No namespace for top-level items
            assert!(items[1].0.is_empty());
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[1].1);
            // No namespace for top-level items
            assert!(items[2].0.is_empty());
            expect![[r#"
                "Baz"
            "#]]
            .assert_debug_eq(&items[2].1);
            // No namespace for top-level items
            assert!(items[3].0.is_empty());
            expect![[r#"
                "Qux"
            "#]]
            .assert_debug_eq(&items[3].1);
        }

        #[test]
        fn invoke_callable_without_args_succeeds() {
            let mut interpreter = get_interpreter();
            let (result, output) = invoke(
                &mut interpreter,
                "Std.Diagnostics.DumpMachine",
                Value::unit(),
            );
            is_unit_with_output(&result, &output, "STATE:\nNo qubits allocated");
        }

        #[test]
        fn invoke_callable_with_args_succeeds() {
            let mut interpreter = get_interpreter();
            let (result, output) = invoke(
                &mut interpreter,
                "Message",
                Value::String("Hello, World!".into()),
            );
            is_unit_with_output(&result, &output, "Hello, World!");
        }

        #[test]
        fn invoke_lambda_with_capture_succeeds() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "let x = 1; let f = y -> x + y;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = invoke(&mut interpreter, "f", Value::Int(2));
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn invoke_lambda_with_capture_in_callable_expr_succeeds() {
            let mut interpreter = get_interpreter();
            let (result, output) = invoke(
                &mut interpreter,
                "{let x = 1; let f = y -> x + y; f}",
                Value::Int(2),
            );
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn callables_failing_profile_validation_are_not_registered() {
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    operation Foo() : Int { use q = Qubit(); mutable x = 1; if MResetZ(q) == One { set x = 2; } x }
                "#},
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                cannot use a dynamic integer value
                   [line_0] [set x = 2]
                cannot use a dynamic integer value
                   [line_0] [x]
            "#]],
            );
            // do something innocuous
            let (result, output) = line(&mut interpreter, indoc! {r#"Foo()"#});
            // since the callable wasn't registered, this will return an unbound name error.
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                runtime error: name is not bound
                   [line_1] [Foo]
            "#]],
            );
        }

        #[test]
        fn callables_failing_profile_validation_also_fail_qir_generation() {
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    operation Foo() : Int { use q = Qubit(); mutable x = 1; if MResetZ(q) == One { set x = 2; } x }
                "#},
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                cannot use a dynamic integer value
                   [line_0] [set x = 2]
                cannot use a dynamic integer value
                   [line_0] [x]
            "#]],
            );
            let res = interpreter.qirgen("{Foo();}");
            expect![[r#"
                Err(
                    [
                        PartialEvaluation(
                            WithSource {
                                sources: [
                                    Source {
                                        name: "<entry>",
                                        contents: "{Foo();}",
                                        offset: 97,
                                    },
                                ],
                                error: EvaluationFailed(
                                    "name is not bound",
                                    PackageSpan {
                                        package: PackageId(
                                            3,
                                        ),
                                        span: Span {
                                            lo: 98,
                                            hi: 101,
                                        },
                                    },
                                ),
                            },
                        ),
                    ],
                )
            "#]]
            .assert_debug_eq(&res);
        }

        #[test]
        fn once_rca_validation_fails_following_calls_do_not_fail() {
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    operation Foo() : Int { use q = Qubit(); mutable x = 1; if MResetZ(q) == One { set x = 2; } x }
                "#},
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                cannot use a dynamic integer value
                   [line_0] [set x = 2]
                cannot use a dynamic integer value
                   [line_0] [x]
            "#]],
            );
            // do something innocuous
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    let y = 7;
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
        }

        #[test]
        fn namespace_usable_before_definition() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    A.B();
                    namespace A {
                        function B() : Unit {}
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
        }

        #[test]
        fn mutually_recursive_namespaces_work() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    A.B();
                    namespace A {
                        open C;
                        function B() : Unit {
                            D();
                        }
                        function E() : Unit {}
                    }
                    namespace C {
                        open A;
                        function D() : Unit {
                            E();
                        }
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());
        }

        #[test]
        fn local_var_valid_after_item_definition() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(&mut interpreter, "let a = 1;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "a");
            is_only_value(&result, &output, &Value::Int(1));
            let (result, output) = line(
                &mut interpreter,
                "function B() : Int { let inner_b = 3; inner_b }",
            );
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "B()");
            is_only_value(&result, &output, &Value::Int(3));
            let (result, output) = line(&mut interpreter, "let b = 2;");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "b");
            is_only_value(&result, &output, &Value::Int(2));
            let (result, output) = line(&mut interpreter, "a");
            is_only_value(&result, &output, &Value::Int(1));
            let (result, output) = line(&mut interpreter, "B()");
            is_only_value(&result, &output, &Value::Int(3));
        }

        #[test]
        fn base_qirgen() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter.qirgen("Foo()").expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);
        }

        #[test]
        fn adaptive_qirgen() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive
                    | TargetCapabilityFlags::QubitReset
                    | TargetCapabilityFlags::IntegerComputations,
            );
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                namespace Test {
                    import Std.Math.*;
                    open QIR.Intrinsic;
                    @EntryPoint()
                    operation Main() : Result {
                        use q = Qubit();
                        let pi_over_2 = 4.0 / 2.0;
                        __quantum__qis__rz__body(pi_over_2, q);
                        mutable some_angle = ArcSin(0.0);
                        __quantum__qis__rz__body(some_angle, q);
                        set some_angle = ArcCos(-1.0) / PI();
                        __quantum__qis__rz__body(some_angle, q);
                        __quantum__qis__mresetz__body(q)
                    }
                }"#
                },
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter.qirgen("Test.Main()").expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__rz__body(double, %Qubit*)

                declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3, !4}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
                !4 = !{i32 1, !"int_computations", !"i64"}
            "#]]
            .assert_eq(&res);
        }

        #[test]
        fn adaptive_qirgen_nested_output_types() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::QubitReset,
            );
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                namespace Test {
                    open QIR.Intrinsic;
                    @EntryPoint()
                    operation Main() : (Result, (Bool, Bool)) {
                        use q = Qubit();
                        let r = __quantum__qis__mresetz__body(q);
                        (r, (r == One, r == Zero))
                    }
                }"#
                },
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter.qirgen("Test.Main()").expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
                  %var_2 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
                  %var_3 = icmp eq i1 %var_2, false
                  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
                  call void @__quantum__rt__bool_record_output(i1 %var_0, i8* null)
                  call void @__quantum__rt__bool_record_output(i1 %var_3, i8* null)
                  ret void
                }

                declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

                declare i1 @__quantum__qis__read_result__body(%Result*)

                declare void @__quantum__rt__tuple_record_output(i64, i8*)

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__rt__bool_record_output(i1, i8*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]]
            .assert_eq(&res);
        }

        #[test]
        fn adaptive_qirgen_fails_when_entry_expr_does_not_match_profile() {
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                use q = Qubit();
                mutable x = 1;
                "#
                },
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .qirgen("if M(q) == One { set x = 2; }")
                .expect_err("expected error");
            is_error(
                &res,
                &expect![[r#"
                    cannot use a dynamic integer value
                       [<entry>] [set x = 2]
                "#]],
            );
        }

        #[test]
        fn qirgen_entry_expr_in_block() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter.qirgen("{Foo()}").expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);
        }

        #[test]
        fn qirgen_entry_expr_defines_operation() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());

            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .qirgen("{operation Bar() : Unit {}; Foo()}")
                .expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);

            // Operation should not be visible from global scope
            let (result, output) = line(&mut interpreter, indoc! {"Bar()"});
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `Bar` not found
                       [line_1] [Bar]
                    type error: insufficient type information to infer type
                       [line_1] [Bar()]
                "#]],
            );
        }

        #[test]
        fn qirgen_multiple_exprs_parse_fail() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .qirgen("Foo(); operation Bar() : Unit {}; Foo()")
                .expect_err("expected error");
            is_error(
                &res,
                &expect![[r#"
                syntax error: expected EOF, found `;`
                   [<entry>] [;]
            "#]],
            );
        }

        #[test]
        fn qirgen_entry_expr_defines_operation_then_more_operations() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .qirgen("{operation Bar() : Unit {}; Foo()}")
                .expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);

            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Baz() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());

            let (result, output) = line(&mut interpreter, indoc! {"Bar()"});
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `Bar` not found
                       [line_2] [Bar]
                    type error: insufficient type information to infer type
                       [line_2] [Bar()]
                "#]],
            );
        }

        #[test]
        fn qirgen_define_operation_use_it() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let res = interpreter
                .qirgen("{ operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; }; Foo() }")
                .expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
                  ret void
                }

                declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

                declare void @__quantum__rt__result_record_output(%Result*, i8*)

                declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);
        }

        #[test]
        fn qirgen_entry_expr_profile_incompatible() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let res = interpreter
                .qirgen("1")
                .expect_err("expected qirgen to fail");
            is_error(
                &res,
                &expect![[r#"
                    cannot use an integer value as an output
                       [<entry>] [1]
                "#]],
            );
        }

        #[test]
        fn adaptive_qirgen_custom_intrinsic_returning_bool() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::QubitReset,
            );
            let res = interpreter
                .qirgen("{ operation check_result(r : Result) : Bool { body intrinsic; }; operation Foo() : Bool { use q = Qubit(); let r = MResetZ(q); check_result(r) } Foo() }")
                .expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                define void @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call i1 @check_result(%Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__bool_record_output(i1 %var_0, i8* null)
                  ret void
                }

                declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

                declare i1 @check_result(%Result*)

                declare void @__quantum__rt__bool_record_output(i1, i8*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]].assert_eq(&res);
        }

        #[test]
        fn run_with_shots() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                "operation Foo(qs : Qubit[]) : Unit { Microsoft.Quantum.Diagnostics.DumpMachine(); }",
            );
            is_only_value(&result, &output, &Value::unit());
            for _ in 0..4 {
                let (results, output) = run(&mut interpreter, "{use qs = Qubit[2]; Foo(qs)}");
                is_unit_with_output(&results, &output, "STATE:\n|00⟩: 1+0i");
            }
        }

        #[test]
        fn run_parse_error() {
            let mut interpreter = get_interpreter();
            let (results, _) = run(&mut interpreter, "Foo)");
            results.expect_err("run() should fail");
        }

        #[test]
        fn run_compile_error() {
            let mut interpreter = get_interpreter();
            let (results, _) = run(&mut interpreter, "Foo()");
            results.expect_err("run() should fail");
        }

        #[test]
        fn run_multiple_statements_with_return_value() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(&mut interpreter, "operation Foo() : Int { 1 }");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "operation Bar() : Int { 2 }");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = run(&mut interpreter, "{ Foo(); Bar() }");
            is_only_value(&result, &output, &Value::Int(2));
        }

        #[test]
        fn run_runtime_failure() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                r#"operation Foo() : Int { fail "failed" }"#,
            );
            is_only_value(&result, &output, &Value::unit());
            for _ in 0..1 {
                let (result, output) = run(&mut interpreter, "Foo()");
                is_only_error(
                    &result,
                    &output,
                    &expect![[r#"
                        runtime error: program failed: failed
                          explicit fail [line_0] [fail "failed"]
                    "#]],
                );
            }
        }

        #[test]
        fn run_output_merged() {
            let mut interpreter = get_interpreter();
            let (result, output) = line(
                &mut interpreter,
                r#"operation Foo() : Unit { Message("hello!") }"#,
            );
            is_only_value(&result, &output, &Value::unit());
            for _ in 0..4 {
                let (result, output) = run(&mut interpreter, "Foo()");
                is_unit_with_output(&result, &output, "hello!");
            }
        }

        #[test]
        fn base_prof_non_result_return() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(&mut interpreter, "123");
            is_only_value(&result, &output, &Value::Int(123));
        }
    }

    fn get_interpreter() -> Interpreter {
        let (std_id, store) =
            crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
        let dependencies = &[(std_id, None)];
        Interpreter::new(
            SourceMap::default(),
            PackageType::Lib,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
            store,
            dependencies,
        )
        .expect("interpreter should be created")
    }

    fn get_interpreter_with_capabilities(capabilities: TargetCapabilityFlags) -> Interpreter {
        let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
        let dependencies = &[(std_id, None)];
        Interpreter::new(
            SourceMap::default(),
            PackageType::Lib,
            capabilities,
            LanguageFeatures::default(),
            store,
            dependencies,
        )
        .expect("interpreter should be created")
    }

    fn is_only_value(result: &InterpretResult, output: &str, value: &Value) {
        assert_eq!("", output);

        match result {
            Ok(v) => assert_eq!(value, v),
            Err(e) => panic!("Expected {value:?}, got {e:?}"),
        }
    }

    fn is_unit_with_output_eval_entry(
        result: &InterpretResult,
        output: &str,
        expected_output: &str,
    ) {
        assert_eq!(expected_output, output);

        match result {
            Ok(value) => assert_eq!(Value::unit(), *value),
            Err(e) => panic!("Expected unit value, got {e:?}"),
        }
    }

    fn is_unit_with_output(result: &InterpretResult, output: &str, expected_output: &str) {
        match result {
            Ok(value) => assert_eq!(Value::unit(), *value),
            Err(e) => panic!("Expected unit value, got {e:?}"),
        }
        assert_eq!(expected_output, output);
    }

    fn is_only_error<E>(result: &Result<Value, Vec<E>>, output: &str, expected_errors: &Expect)
    where
        E: Diagnostic,
    {
        assert_eq!("", output);

        match result {
            Ok(value) => panic!("Expected error , got {value:?}"),
            Err(errors) => is_error(errors, expected_errors),
        }
    }

    fn is_error<E>(errors: &Vec<E>, expected_errors: &Expect)
    where
        E: Diagnostic,
    {
        let mut actual = String::new();
        for error in errors {
            write!(actual, "{error}").expect("writing should succeed");
            for s in iter::successors(error.source(), |&s| s.source()) {
                write!(actual, ": {s}").expect("writing should succeed");
            }
            for label in error.labels().into_iter().flatten() {
                let span = error
                    .source_code()
                    .expect("expected valid source code")
                    .read_span(label.inner(), 0, 0)
                    .expect("expected to be able to read span");

                write!(
                    actual,
                    "\n  {} [{}] [{}]",
                    label.label().unwrap_or(""),
                    span.name().expect("expected source file name"),
                    from_utf8(span.data()).expect("expected valid utf-8 string"),
                )
                .expect("writing should succeed");
            }
            writeln!(actual).expect("writing should succeed");
        }

        expected_errors.assert_eq(&actual);
    }

    #[cfg(test)]
    mod with_sources {
        use std::{sync::Arc, vec};

        use super::*;
        use crate::interpret::Debugger;
        use crate::line_column::Encoding;
        use expect_test::expect;
        use indoc::indoc;

        use qsc_ast::ast::{
            Expr, ExprKind, NodeId, Package, Path, PathKind, Stmt, StmtKind, TopLevelNode,
        };
        use qsc_data_structures::span::Span;
        use qsc_frontend::compile::SourceMap;
        use qsc_passes::PackageType;

        #[test]
        fn entry_expr_is_executed() {
            let source = indoc! { r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    Message("hello there...")
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Exe,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            let (result, output) = entry(&mut interpreter);
            is_unit_with_output_eval_entry(&result, &output, "hello there...");
        }

        #[test]
        fn invalid_partial_application_should_fail_not_panic() {
            // Found via fuzzing, see #2363
            let source = "operation e(oracle:(w=>)){oracle=i(_)";
            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            assert!(
                Interpreter::new(
                    sources,
                    PackageType::Exe,
                    TargetCapabilityFlags::all(),
                    LanguageFeatures::default(),
                    store,
                    &[(std_id, None)],
                )
                .is_err(),
                "interpreter should fail with error"
            );
        }

        #[test]
        fn errors_returned_if_sources_do_not_match_profile() {
            let source = indoc! { r#"
            namespace A { operation Test() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x } }"#};

            let sources = SourceMap::new([("test".into(), source.into())], Some("A.Test()".into()));
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let result = Interpreter::new(
                sources,
                PackageType::Exe,
                TargetCapabilityFlags::Adaptive
                    | TargetCapabilityFlags::IntegerComputations
                    | TargetCapabilityFlags::QubitReset,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            );

            match result {
                Ok(_) => panic!("Expected error, got interpreter."),
                Err(errors) => is_error(
                    &errors,
                    &expect![[r#"
                        cannot use a dynamic double value
                           [<entry>] [A.Test()]
                        cannot use a double value as an output
                           [<entry>] [A.Test()]
                        cannot use a dynamic double value
                           [test] [set x = 2.0]
                        cannot use a dynamic double value
                           [test] [x]
                    "#]],
                ),
            }
        }

        #[test]
        fn stdlib_members_can_be_accessed_from_sources() {
            let source = indoc! { r#"
            namespace Test {
                operation Main() : Unit {
                    Message("hello there...")
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let dependencies = &[(std_id, None)];
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                dependencies,
            )
            .expect("interpreter should be created");

            let (result, output) = line(&mut interpreter, "Test.Main()");
            is_unit_with_output(&result, &output, "hello there...");
        }

        #[test]
        fn members_from_namespaced_sources_are_in_context() {
            let source = indoc! { r#"
            namespace Test {
                function Hello() : String {
                    "hello there..."
                }

                operation Main() : String {
                    Hello()
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let store = crate::PackageStore::new(crate::compile::core());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[],
            )
            .expect("interpreter should be created");

            let (result, output) = line(&mut interpreter, "Test.Hello()");
            is_only_value(&result, &output, &Value::String("hello there...".into()));
            let (result, output) = line(&mut interpreter, "Test.Main()");
            is_only_value(&result, &output, &Value::String("hello there...".into()));
        }

        #[test]
        fn multiple_files_are_loaded_from_sources_into_eval_context() {
            let sources: [(Arc<str>, Arc<str>); 2] = [
                (
                    "a.qs".into(),
                    r#"
            namespace Test {
                function Hello() : String {
                    "hello there..."
                }
            }"#
                    .into(),
                ),
                (
                    "b.qs".into(),
                    r#"
            namespace Test2 {
                open Test;
                @EntryPoint()
                operation Main() : String {
                    Hello();
                    Hello()
                }
            }"#
                    .into(),
                ),
            ];

            let sources = SourceMap::new(sources, None);
            let store = crate::PackageStore::new(crate::compile::core());
            let debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[],
            )
            .expect("debugger should be created");
            let bps = debugger.get_breakpoints("a.qs");
            assert_eq!(1, bps.len());
            let bps = debugger.get_breakpoints("b.qs");
            assert_eq!(2, bps.len());
        }

        #[test]
        fn debugger_simple_execution_succeeds() {
            let source = indoc! { r#"
            namespace Test {
                function Hello() : Unit {
                    Message("hello there...");
                }

                @EntryPoint()
                operation Main() : Unit {
                    Hello()
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("debugger should be created");
            let (result, output) = entry(&mut debugger.interpreter);
            is_unit_with_output_eval_entry(&result, &output, "hello there...");
        }

        #[test]
        fn debugger_execution_with_call_to_library_succeeds() {
            let source = indoc! { r#"
            namespace Test {
                import Std.Math.*;
                @EntryPoint()
                operation Main() : Int {
                    Binom(31, 7)
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("debugger should be created");
            let (result, output) = entry(&mut debugger.interpreter);
            is_only_value(&result, &output, &Value::Int(2_629_575));
        }

        #[test]
        fn debugger_execution_with_early_return_succeeds() {
            let source = indoc! { r#"
            namespace Test {
                import Std.Arrays.*;

                operation Max20(i : Int) : Int {
                    if (i > 20) {
                        return 20;
                    }
                    return i;
                }

                @EntryPoint()
                operation Main() : Int[] {
                    ForEach(Max20, [10, 20, 30, 40, 50])
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("debugger should be created");

            let (result, output) = entry(&mut debugger.interpreter);
            is_only_value(
                &result,
                &output,
                &Value::Array(
                    vec![
                        Value::Int(10),
                        Value::Int(20),
                        Value::Int(20),
                        Value::Int(20),
                        Value::Int(20),
                    ]
                    .into(),
                ),
            );
        }

        #[test]
        fn multiple_namespaces_are_loaded_from_sources_into_eval_context() {
            let source = indoc! { r#"
            namespace Test {
                function Hello() : String {
                    "hello there..."
                }
            }
            namespace Test2 {
                open Test;
                operation Main() : String {
                    Hello()
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let store = crate::PackageStore::new(crate::compile::core());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[],
            )
            .expect("interpreter should be created");
            let (result, output) = line(&mut interpreter, "Test.Hello()");
            is_only_value(&result, &output, &Value::String("hello there...".into()));
            let (result, output) = line(&mut interpreter, "Test2.Main()");
            is_only_value(&result, &output, &Value::String("hello there...".into()));
        }

        #[test]
        fn runtime_error_from_stdlib() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    "namespace Foo {
                        operation Bar(): Unit {
                            let x = -1;
                            use qs = Qubit[x];
                        }
                    }
                    "
                    .into(),
                )],
                Some("Foo.Bar()".into()),
            );

            let store = crate::PackageStore::new(crate::compile::core());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[],
            )
            .expect("interpreter should be created");

            let (result, output) = entry(&mut interpreter);
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: program failed: Cannot allocate qubit array with a negative length
                      explicit fail [qsharp-library-source:core/qir.qs] [fail "Cannot allocate qubit array with a negative length"]
                "#]],
            );
        }

        #[test]
        fn interpreter_returns_items_from_source() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    "namespace A {
                        operation B(): Unit { }
                    }
                    "
                    .into(),
                )],
                Some("A.B()".into()),
            );

            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            let items = interpreter.source_globals();
            assert_eq!(1, items.len());
            expect![[r#"
                [
                    "A",
                ]
            "#]]
            .assert_debug_eq(&items[0].0);
            expect![[r#"
                "B"
            "#]]
            .assert_debug_eq(&items[0].1);
        }

        #[test]
        fn interpreter_can_be_created_from_ast() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    "namespace A {
                        operation B(): Result {
                            use qs = Qubit[2];
                            X(qs[0]);
                            CNOT(qs[0], qs[1]);
                            let res = Measure([PauliZ, PauliZ], qs[...1]);
                            ResetAll(qs);
                            res
                        }
                    }
                    "
                    .into(),
                )],
                Some("A.B()".into()),
            );

            let (package_type, capabilities, language_features) = (
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
            );

            let mut store = crate::PackageStore::new(crate::compile::core());
            let dependencies = vec![(
                store.insert(crate::compile::std(&store, capabilities)),
                None,
            )];

            let (mut unit, errors) = crate::compile::compile(
                &store,
                &dependencies,
                sources,
                package_type,
                capabilities,
                language_features,
            );
            unit.expose();
            for e in &errors {
                eprintln!("{e:?}");
            }
            assert!(errors.is_empty(), "compilation failed: {}", errors[0]);
            let package_id = store.insert(unit);

            let mut interpreter = Interpreter::from(
                false,
                store,
                package_id,
                capabilities,
                language_features,
                &dependencies,
            )
            .expect("interpreter should be created");
            let (result, output) = entry(&mut interpreter);
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );
        }

        #[test]
        fn ast_fragments_can_be_evaluated() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    "namespace A {
                        operation B(): Result {
                            use qs = Qubit[2];
                            X(qs[0]);
                            CNOT(qs[0], qs[1]);
                            let res = Measure([PauliZ, PauliZ], qs[...1]);
                            ResetAll(qs);
                            res
                        }
                    }
                    "
                    .into(),
                )],
                None,
            );
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            let package = get_package_for_call("A", "B");
            let (result, output) = fragment(&mut interpreter, "A.B()", package);
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );
        }

        #[test]
        fn ast_fragments_evaluation_returns_runtime_errors() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    "namespace A {
                        operation B(): Int {
                            42 / 0
                        }
                    }
                    "
                    .into(),
                )],
                None,
            );
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            let package = get_package_for_call("A", "B");
            let (result, output) = fragment(&mut interpreter, "A.B()", package);
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    runtime error: division by zero
                      cannot divide by zero [test] [0]
                "#]],
            );
        }

        fn get_package_for_call(ns: &str, name: &str) -> crate::ast::Package {
            let args = Expr {
                id: NodeId::default(),
                span: Span::default(),
                kind: Box::new(ExprKind::Tuple(Box::new([]))),
            };
            let path = Path {
                id: NodeId::default(),
                span: Span::default(),
                segments: Some(
                    std::iter::once(qsc_ast::ast::Ident {
                        id: NodeId::default(),
                        span: Span::default(),
                        name: ns.into(),
                    })
                    .collect(),
                ),
                name: Box::new(qsc_ast::ast::Ident {
                    id: NodeId::default(),
                    span: Span::default(),
                    name: name.into(),
                }),
            };
            let path_expr = Expr {
                id: NodeId::default(),
                span: Span::default(),
                kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(path)))),
            };
            let expr = Expr {
                id: NodeId::default(),
                span: Span::default(),
                kind: Box::new(ExprKind::Call(Box::new(path_expr), Box::new(args))),
            };
            let stmt = Stmt {
                id: NodeId::default(),
                span: Span::default(),
                kind: Box::new(StmtKind::Expr(Box::new(expr))),
            };
            let top_level = TopLevelNode::Stmt(Box::new(stmt));
            Package {
                id: NodeId::default(),
                nodes: vec![top_level].into_boxed_slice(),
                entry: None,
            }
        }

        #[test]
        fn name_resolution_from_source_named_main_should_succeed() {
            let sources = SourceMap::new(
                [(
                    "Main".into(),
                    r#"function Foo() : Unit { Message("hello there..."); }"#.into(),
                )],
                None,
            );
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            // Operations defined in Main.qs should also be visible with Main qualifier.
            let (result, output) = line(&mut interpreter, "Main.Foo()");
            is_unit_with_output(&result, &output, "hello there...");

            // Operations defined in Main.qs should be importable with fully qualified name.
            let (result, output) = line(&mut interpreter, "import Main.Foo;");
            is_only_value(&result, &output, &Value::unit());

            // After import the operation can be invoked without Main qualifier.
            let (result, output) = line(&mut interpreter, "Foo()");
            is_unit_with_output(&result, &output, "hello there...");
        }

        #[test]
        fn name_resolution_from_source_named_main_without_full_path_or_import_should_fail() {
            let sources = SourceMap::new(
                [(
                    "Main".into(),
                    r#"function Foo() : Unit { Message("hello there..."); }"#.into(),
                )],
                None,
            );
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created");

            // Operations defined in Main.qs should also be visible with Main qualifier.
            let (errors, _) = line(&mut interpreter, "Foo()");
            is_error(
                &errors.expect_err("line invocation should fail with error"),
                &expect![[r#"
                    name error: `Foo` not found
                       [line_0] [Foo]
                    type error: insufficient type information to infer type
                       [line_0] [Foo()]
                "#]],
            );
        }

        /// Found via fuzzing, see #2426 <https://github.com/microsoft/qsharp/issues/2426>
        #[test]
        fn recursive_type_constraint_should_fail() {
            let sources = SourceMap::new(
                [(
                    "test".into(),
                    r#"operation a(){(foo,bar)->foo+bar=foo->foo"#.into(),
                )],
                None,
            );
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            match Interpreter::new(
                sources,
                PackageType::Lib,
                TargetCapabilityFlags::all(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            ) {
                Ok(_) => panic!("interpreter should fail with error"),
                Err(errors) => {
                    is_error(
                        &errors,
                        &expect![[r#"
                            syntax error: expected `:`, found `{`
                               [test] [{]
                            syntax error: expected `}`, found EOF
                               [test] []
                            type error: unsupported recursive type constraint
                               [test] [(foo,bar)->foo+bar]
                            type error: insufficient type information to infer type
                               [test] [foo+bar]
                        "#]],
                    );
                }
            }
        }
    }
}
