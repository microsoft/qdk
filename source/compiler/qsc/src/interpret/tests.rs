// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod given_interpreter {
    use crate::interpret::{InterpretResult, Interpreter};
    use expect_test::Expect;
    use miette::Diagnostic;
    use qsc_data_structures::source::SourceMap;
    use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
    use qsc_eval::{output::CursorReceiver, val::Value};
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
        let res = interpreter.run(
            &mut receiver,
            Some(expr),
            None,
            None,
            None,
            None,
            Default::default(),
        );
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

        use crate::interpret::PackageGlobal;

        use super::*;

        mod without_stdlib {
            use qsc_data_structures::source::SourceMap;
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
        fn config_values_are_available_via_get_config() {
            let mut interpreter = get_interpreter();
            interpreter.set_qsharp_config_value("int_config", Value::Int(123));
            interpreter.set_qsharp_config_value("bool_config", Value::Bool(true));
            interpreter.set_qsharp_config_value("string_config", Value::String("value".into()));
            interpreter.set_qsharp_config_value("double_config", Value::Double(124.1));

            // Integer config.
            let (result, output) =
                line(&mut interpreter, "Std.Core.ConfigValue(\"int_config\", 0)");
            is_only_value(&result, &output, &Value::Int(123));

            // Boolean config.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"bool_config\", false)",
            );
            is_only_value(&result, &output, &Value::Bool(true));

            // String config.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"string_config\", \"\")",
            );
            is_only_value(&result, &output, &Value::String("value".into()));

            // Double config.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"double_config\", 0.0)",
            );
            is_only_value(&result, &output, &Value::Double(124.1));

            // Default value.
            let (result, output) = line(
                &mut interpreter,
                "Std.Math.MaxI(1, Std.Core.ConfigValue(\"int_config\", 0))",
            );
            is_only_value(&result, &output, &Value::Int(123));

            // GetConfig can be used as argument to another function.
            let (result, output) = line(&mut interpreter, "Std.Core.ConfigValue(\"unknown\", 15)");
            is_only_value(&result, &output, &Value::Int(15));
        }

        #[test]
        #[allow(clippy::too_many_lines)]
        fn get_config_errors() {
            let mut interpreter = get_interpreter();
            interpreter.set_qsharp_config_value("int_config", Value::Int(123));
            // Error when default type doesn't match stored config value.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"int_config\", 20.0)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    configuration value type does not match ConfigValue default value type
                       [line_0] [20.0]
                "#]],
            );

            // Error when key is not literal.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"int_\" + \"config\", 20)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    ConfigValue arguments must be literals
                       [line_1] ["int_" + "config"]
                "#]],
            );

            // Error when value is not literal.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"int_config\", 10+10)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    ConfigValue arguments must be literals
                       [line_2] [10+10]
                "#]],
            );

            // Error when value is a variable.
            let (result, output) = line(
                &mut interpreter,
                "let default=10; Std.Core.ConfigValue(\"int_config\", default)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    ConfigValue arguments must be literals
                       [line_3] [default]
                "#]],
            );

            // Error when config contains value of unsupported type (same as default type).
            interpreter.set_qsharp_config_value(
                "result_config",
                Value::Result(qsc_eval::val::Result::Loss),
            );
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"result_config\", Zero)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    unsupported configuration type
                       [line_4] [Zero]
                "#]],
            );

            // Error when config contains value of unsupported type (defferent than default type)
            interpreter.set_qsharp_config_value(
                "result_config",
                Value::Result(qsc_eval::val::Result::Loss),
            );
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"result_config\", 0.5)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    unsupported configuration type
                       [line_5] [0.5]
                "#]],
            );

            // Error when config is missing and default type is unsupported.
            let (result, output) = line(
                &mut interpreter,
                "Std.Core.ConfigValue(\"bigint_config\", 10L)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    unsupported configuration type
                       [line_6] [10L]
                "#]],
            );

            // Error when using ConfigValue not in a call.
            let (result, output) = line(
                &mut interpreter,
                "let f = Std.Core.ConfigValue; f(\"key\", 5)",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    ConfigValue must be called directly
                       [line_7] [Std.Core.ConfigValue]
                "#]],
            );
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
            assert!(items[0].namespace.is_empty());
            expect![[r#"
                "Foo"
            "#]]
            .assert_debug_eq(&items[0].name);
            // No namespace for top-level items
            assert!(items[1].namespace.is_empty());
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[1].name);
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
            .assert_debug_eq(&items[0].namespace);
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[0].name);
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
            assert!(items[0].namespace.is_empty());
            expect![[r#"
                "Foo"
            "#]]
            .assert_debug_eq(&items[0].name);
            // No namespace for top-level items
            assert!(items[1].namespace.is_empty());
            expect![[r#"
                "Bar"
            "#]]
            .assert_debug_eq(&items[1].name);
            // No namespace for top-level items
            assert!(items[2].namespace.is_empty());
            expect![[r#"
                "Baz"
            "#]]
            .assert_debug_eq(&items[2].name);
            // No namespace for top-level items
            assert!(items[3].namespace.is_empty());
            expect![[r#"
                "Qux"
            "#]]
            .assert_debug_eq(&items[3].name);
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
        fn export_and_namespaces_round_trip_and_survive_revert() {
            // The lowerer no longer emits namespace or export items into FIR, so
            // an incremental compile tracks fewer item ids. Declaring an export
            // and multiple namespaces across fragments must still round-trip,
            // and reverting a later increment must leave the earlier
            // declarations intact and callable.
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);

            // Fragment 0: a namespace that exports one of its callables.
            let (result, output) = line(
                &mut interpreter,
                "namespace Foo { function Bar() : Int { 5 } export Bar; }",
            );
            is_only_value(&result, &output, &Value::unit());

            // Fragment 1: a second namespace that calls into the first.
            let (result, output) = line(
                &mut interpreter,
                "namespace Baz { function Qux() : Int { Foo.Bar() + 1 } }",
            );
            is_only_value(&result, &output, &Value::unit());

            // Both namespaces and the exported name resolve.
            let (result, output) = line(&mut interpreter, "Foo.Bar()");
            is_only_value(&result, &output, &Value::Int(5));
            let (result, output) = line(&mut interpreter, "Baz.Qux()");
            is_only_value(&result, &output, &Value::Int(6));

            // Fragment that fails profile validation, forcing the FIR increment
            // to be reverted.
            let (result, output) = line(
                &mut interpreter,
                "operation Dyn() : Int { use q = Qubit(); mutable x = 1; if MResetZ(q) == One { set x = 2; } x }",
            );
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    cannot use a dynamic integer value
                       [line_4] [set x = 2]
                    cannot use a dynamic integer value
                       [line_4] [x]
                "#]],
            );

            // After the revert, the earlier namespace/export declarations remain
            // consistent and callable.
            let (result, output) = line(&mut interpreter, "Foo.Bar()");
            is_only_value(&result, &output, &Value::Int(5));
            let (result, output) = line(&mut interpreter, "Baz.Qux()");
            is_only_value(&result, &output, &Value::Int(6));
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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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

        fn assert_qir_has_three_h_gates(qir: &str) {
            assert!(
                qir.contains("define i64 @ENTRYPOINT__main()"),
                "expected entry point in generated QIR, got:\n{qir}"
            );
            assert!(
                qir.contains(r#""required_num_qubits"="3""#),
                "expected three qubits in generated QIR, got:\n{qir}"
            );
            assert_eq!(
                qir.matches("call void @__quantum__qis__h__body").count(),
                3,
                "expected three H applications in generated QIR, got:\n{qir}"
            );
        }

        fn user_global(interpreter: &Interpreter, name: &str) -> Value {
            interpreter
                .user_globals()
                .into_iter()
                .find_map(
                    |PackageGlobal {
                         name: global_name,
                         value,
                         ..
                     }| { (global_name.as_ref() == name).then_some(value) },
                )
                .unwrap_or_else(|| panic!("{name} should be present in user globals"))
        }

        #[test]
        fn qirgen_does_not_corrupt_later_interpreter_eval_or_recompilation() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());

            interpreter.qirgen("Foo()").expect("expected success");

            let (result, output) = line(&mut interpreter, "Foo()");
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );

            let (result, output) = line(&mut interpreter, "operation Bar() : Result { Foo() }");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Bar()");
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );
        }

        #[test]
        fn ordinary_restricted_entry_failure_reverts_increment() {
            let mut interpreter = get_interpreter_with_capabilities(
                qsc_data_structures::target::Profile::AdaptiveRI.into(),
            );
            let (result, output) = line(&mut interpreter, "function Prior() : Int { 7 }");
            is_only_value(&result, &output, &Value::unit());

            let errors = interpreter
                .compile_entry_expr(indoc! {r#"
                    {
                        operation Rejected() : Int {
                            use q = Qubit();
                            H(q);
                            if MResetZ(q) == One {
                                return 1;
                            }
                            return 2;
                        }
                        Rejected()
                    }
                "#})
                .expect_err("ordinary restricted entry should fail raw-FIR capability validation");
            assert!(
                errors
                    .iter()
                    .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
                "expected capability-check pass errors, got {errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .any(|error| format!("{error:?}").contains("ReturnWithinDynamicScope")),
                "expected a return-within-dynamic-scope diagnostic, got {errors:?}"
            );

            let (result, output) = line(&mut interpreter, "Prior()");
            is_only_value(&result, &output, &Value::Int(7));

            let (result, output) = line(&mut interpreter, "Rejected()");
            is_only_error(
                &result,
                &output,
                &expect![[r#"
                    name error: `Rejected` not found
                       [line_2] [Rejected]
                "#]],
            );
        }

        #[test]
        fn qirgen_twice_on_shared_interpreter_store_is_byte_identical() {
            // The FIR transform pipeline mutates every reachable package in
            // place, including std, so codegen must run on a throwaway clone of
            // the interpreter's long-lived `fir_store`. This entry point calls
            // the std operation `ApplyToEach` cross-package, so the first
            // `qirgen` destructively transforms std in its clone. If a future
            // change ran the pipeline on the shared store instead, the second
            // `qirgen` would see an already-transformed std and diverge or
            // panic. Two identical calls must produce identical QIR.
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"
                    operation Foo() : Result {
                        use qs = Qubit[3];
                        Std.Canon.ApplyToEach(H, qs);
                        let r = M(qs[0]);
                        for q in qs {
                            Reset(q);
                        }
                        return r;
                    }
                "},
            );
            is_only_value(&result, &output, &Value::unit());

            let first = interpreter
                .qirgen("Foo()")
                .expect("first qirgen should succeed");
            let second = interpreter
                .qirgen("Foo()")
                .expect("second qirgen should succeed");
            assert_eq!(
                first, second,
                "two qirgen calls on the same interpreter must produce byte-identical \
                 QIR; divergence means the FIR transform pipeline corrupted the shared \
                 (non-disposable) store on the first call"
            );
        }

        #[test]
        fn qirgen_from_callable_user_global_succeeds_after_fresh_lowering() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());

            let callable = user_global(&interpreter, "Foo");

            let res = interpreter
                .qirgen_from_callable(&callable, Value::unit())
                .expect("expected success");

            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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
            "#]]
            .assert_eq(&res);
        }

        #[test]
        fn qirgen_from_callable_with_global_callable_arg_succeeds() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    open Std.Canon;

                    operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
                        use qs = Qubit[nQubits];
                        f(qs);
                    }

                    operation AllH(qs : Qubit[]) : Unit {
                        struct Point3d { X : Double, Y : Double, Z : Double }

                        let point = new Point3d { X = 1.0, Y = 2.0, Z = 3.0 };
                        let point2 = new Point3d { ...point, Z = 4.0 };
                        let should_apply = point2.X == 1.0;
                        if should_apply {
                            ApplyToEach(H, qs);
                        }
                    }

                    operation UnusedIntOutput() : Int {
                        1
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());

            let invoke_with_qubits = user_global(&interpreter, "InvokeWithQubits");
            let all_h = user_global(&interpreter, "AllH");

            let qir = interpreter
                .qirgen_from_callable(
                    &invoke_with_qubits,
                    Value::Tuple(vec![Value::Int(3), all_h].into(), None),
                )
                .expect("expected success");

            assert_qir_has_three_h_gates(&qir);
        }

        #[test]
        fn qirgen_from_callable_with_closure_arg_succeeds() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    open Std.Canon;

                    operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
                        use qs = Qubit[nQubits];
                        f(qs);
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());

            let invoke_with_qubits = user_global(&interpreter, "InvokeWithQubits");

            let (closure_result, closure_output) = line(&mut interpreter, "ApplyToEach(H, _)");
            assert!(
                closure_output.is_empty(),
                "unexpected output while creating closure: {closure_output}"
            );
            let apply_h = closure_result.expect("expected closure value");

            let qir = interpreter
                .qirgen_from_callable(
                    &invoke_with_qubits,
                    Value::Tuple(vec![Value::Int(3), apply_h].into(), None),
                )
                .expect("expected success");

            assert_qir_has_three_h_gates(&qir);
        }

        #[test]
        fn qirgen_from_callable_with_nested_closure_arg_generates_inner_effect() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    operation InvokeOne(op : Qubit => Unit) : Unit {
                        use q = Qubit();
                        op(q);
                    }

                    function MakeRz(theta : Double) : Qubit => Unit {
                        Rz(theta, _)
                    }

                    function MakeOuter(inner : Qubit => Unit) : Qubit => Unit {
                        inner(_)
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());

            let invoke_one = user_global(&interpreter, "InvokeOne");

            let (closure_result, closure_output) = line(
                &mut interpreter,
                "let inner = MakeRz(4.0); MakeOuter(inner)",
            );
            assert!(
                closure_output.is_empty(),
                "unexpected output while creating nested closure: {closure_output}"
            );
            let outer = closure_result.expect("expected nested closure value");

            let qir = interpreter
                .qirgen_from_callable(&invoke_one, outer)
                .expect("expected success");

            assert_eq!(
                qir.matches("call void @__quantum__qis__rz__body(double 4.0,")
                    .count(),
                1,
                "expected one inner captured rotation in QIR:\n{qir}"
            );
        }

        #[test]
        fn qirgen_from_callable_with_arrow_input_reports_runtime_capability_errors() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
            );
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    import Std.Convert.*;

                    operation InvokeWithMeasuredInt(f : (Int, Qubit) => Unit) : Unit {
                        use q = Qubit();
                        let i = if MResetZ(q) == One { 1 } else { 0 };
                        f(i, q);
                    }

                    operation RotateByInt(i : Int, q : Qubit) : Unit {
                        Rx(IntAsDouble(i), q);
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());

            let invoke_with_measured_int = user_global(&interpreter, "InvokeWithMeasuredInt");
            let rotate_by_int = user_global(&interpreter, "RotateByInt");

            let errors = interpreter
                .qirgen_from_callable(&invoke_with_measured_int, rotate_by_int)
                .expect_err("expected runtime capability error");

            assert!(
                errors
                    .iter()
                    .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
                "expected capability-check pass errors, got {errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .any(|error| format!("{error:?}").contains("UseOfDynamicDouble")),
                "expected a dynamic double capability diagnostic, got {errors:?}"
            );
        }

        #[test]
        fn qirgen_from_callable_profile_incompatible_outputs_report_callable_scoped_errors() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                    operation ReturnInt() : Int {
                        1
                    }

                    operation ReturnDouble() : Double {
                        1.0
                    }

                    operation ReturnBool() : Bool {
                        true
                    }

                    operation ReturnString() : String {
                        "hello"
                    }
                "#},
            );
            is_only_value(&result, &output, &Value::unit());

            let int_errors = interpreter
                .qirgen_from_callable(&user_global(&interpreter, "ReturnInt"), Value::unit())
                .expect_err("expected integer output rejection");
            is_error(
                &int_errors,
                &expect![[r#"
                    cannot use an integer value as an output
                       [line_0] [ReturnInt]
                "#]],
            );

            let double_errors = interpreter
                .qirgen_from_callable(&user_global(&interpreter, "ReturnDouble"), Value::unit())
                .expect_err("expected double output rejection");
            is_error(
                &double_errors,
                &expect![[r#"
                    cannot use a double value as an output
                       [line_0] [ReturnDouble]
                "#]],
            );

            let bool_errors = interpreter
                .qirgen_from_callable(&user_global(&interpreter, "ReturnBool"), Value::unit())
                .expect_err("expected bool output rejection");
            is_error(
                &bool_errors,
                &expect![[r#"
                    cannot use a bool value as an output
                       [line_0] [ReturnBool]
                "#]],
            );

            let advanced_errors = interpreter
                .qirgen_from_callable(&user_global(&interpreter, "ReturnString"), Value::unit())
                .expect_err("expected advanced output rejection");
            is_error(
                &advanced_errors,
                &expect![[r#"
                    cannot use value with advanced type as an output
                       [line_0] [ReturnString]
                "#]],
            );
        }

        #[test]
        fn qirgen_from_callable_does_not_corrupt_later_interpreter_eval_or_recompilation() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());

            let callable = user_global(&interpreter, "Foo");

            interpreter
                .qirgen_from_callable(&callable, Value::unit())
                .expect("expected success");

            let mut cursor = Cursor::new(Vec::<u8>::new());
            let mut receiver = CursorReceiver::new(&mut cursor);
            let result = interpreter.invoke(&mut receiver, callable.clone(), Value::unit());
            let output = receiver.dump();
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );

            let (result, output) = line(&mut interpreter, "operation Bar() : Result { Foo() }");
            is_only_value(&result, &output, &Value::unit());
            let (result, output) = line(&mut interpreter, "Bar()");
            is_only_value(
                &result,
                &output,
                &Value::Result(qsc_eval::val::Result::Val(false)),
            );
        }

        #[test]
        fn qirgen_folds_get_config_values() {
            let source = indoc! {"
                operation Main() : Unit {
                    use q = Qubit[3];
                    Rx(Std.Core.ConfigValue(\"angle1\", 0.1), q[0]);
                    Ry(Std.Core.ConfigValue(\"angle2\", 0.2), q[1]);
                    let loop_iterations = Std.Core.ConfigValue(\"loop_iterations\", 0);
                    for i in 1..loop_iterations {
                        X(q[2]);
                    }
                }
            "};

            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            interpreter.set_qsharp_config_value("angle1", Value::Double(0.6));
            interpreter.set_qsharp_config_value("loop_iterations", Value::Int(3));
            _ = line(&mut interpreter, source);

            let qir = interpreter.qirgen("Main()").expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                @0 = internal constant [4 x i8] c"0_t\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__rx__body(double 0.6, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__ry__body(double 0.2, %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
                  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
                  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
                  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

                declare void @__quantum__qis__rx__body(double, %Qubit*)

                declare void @__quantum__qis__ry__body(double, %Qubit*)

                declare void @__quantum__qis__x__body(%Qubit*)

                declare void @__quantum__rt__tuple_record_output(i64, i8*)

                attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
                attributes #1 = { "irreversible" }

                ; module flags

                !llvm.module.flags = !{!0, !1, !2, !3}

                !0 = !{i32 1, !"qir_major_version", i32 1}
                !1 = !{i32 7, !"qir_minor_version", i32 0}
                !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
                !3 = !{i32 1, !"dynamic_result_management", i1 false}
            "#]]
            .assert_eq(&qir);
        }

        #[test]
        fn adaptive_qirgen() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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
                !4 = !{i32 5, !"int_computations", !{!"i64"}}
            "#]]
            .assert_eq(&res);
        }

        #[test]
        fn base_get_rir() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::empty());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter.get_rir("Foo()").expect("expected success");
            // get_rir returns the raw RIR and the SSA-transformed RIR. The full
            // dump embeds source offsets in its debug metadata, so assert on the
            // stable structure rather than snapshotting the whole program.
            assert_eq!(res.len(), 2);
            let ssa = &res[1];
            assert!(ssa.contains("Program:"), "{ssa}");
            assert!(ssa.contains("capabilities: Base"), "{ssa}");
            assert!(ssa.contains("num_results: 1"), "{ssa}");
            assert!(ssa.contains("call_type: Measurement"), "{ssa}");
        }

        #[test]
        fn adaptive_get_rir() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
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
                        __quantum__qis__mresetz__body(q)
                    }
                }"#
                },
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .get_rir("Test.Main()")
                .expect("expected success");
            assert_eq!(res.len(), 2);
            let ssa = &res[1];
            assert!(ssa.contains("Program:"), "{ssa}");
            assert!(ssa.contains("Adaptive"), "{ssa}");
            assert!(ssa.contains("num_results: 1"), "{ssa}");
            assert!(ssa.contains("call_type: Measurement"), "{ssa}");
        }

        #[test]
        fn get_rir_fails_for_unrestricted_profile() {
            let mut interpreter = get_interpreter_with_capabilities(TargetCapabilityFlags::all());
            let (result, output) = line(
                &mut interpreter,
                indoc! {"operation Foo() : Result { use q = Qubit(); let r = M(q); Reset(q); return r; } "},
            );
            is_only_value(&result, &output, &Value::unit());
            let res = interpreter
                .get_rir("Foo()")
                .expect_err("expected get_rir to fail for the unrestricted profile");
            expect!["[UnsupportedRuntimeCapabilities]"].assert_eq(&format!("{res:?}"));
        }

        #[test]
        fn adaptive_qirgen_source_entrypoint_uses_fresh_lowering() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
            );
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                namespace Test {
                    import Std.Intrinsic.*;
                    import Std.Math.*;
                    import Std.Measurement.*;

                    @EntryPoint()
                    operation Main() : ((Result[], Int), Bool) {
                        use registerA = Qubit[3];
                        if true {
                            X(registerA[0]);
                            if true {
                                X(registerA[1]);
                                if false {
                                    X(registerA[2]);
                                }
                            }
                        }
                        let registerAMeasurements = MeasureEachZ(registerA);

                        mutable a = 0;
                        if registerAMeasurements[0] == Zero {
                            if registerAMeasurements[1] == Zero and registerAMeasurements[2] == Zero {
                                set a = 0;
                            } elif registerAMeasurements[1] == Zero and registerAMeasurements[2] == One {
                                set a = 1;
                            } elif registerAMeasurements[1] == One and registerAMeasurements[2] == Zero {
                                set a = 2;
                            } else {
                                set a = 3;
                            }
                        } else {
                            if registerAMeasurements[1] == Zero and registerAMeasurements[2] == Zero {
                                set a = 4;
                            } elif registerAMeasurements[1] == Zero and registerAMeasurements[2] == One {
                                set a = 5;
                            } elif registerAMeasurements[1] == One and registerAMeasurements[2] == Zero {
                                set a = 6;
                            } else {
                                set a = 7;
                            }
                        }
                        ResetAll(registerA);

                        use q = Qubit();
                        ((registerAMeasurements, a), MResetZ(q) == One)
                    }
                }"#
                },
            );
            is_only_value(&result, &output, &Value::unit());

            let qir = interpreter.qirgen("Test.Main()").expect("expected success");

            assert!(
                qir.contains("call void @__quantum__rt__int_record_output(i64 %var_"),
                "expected dynamic integer output to be recorded from an SSA value, got:\n{qir}"
            );
            assert!(
                !qir.contains("call void @__quantum__rt__int_record_output(i64 0,"),
                "expected source entrypoint QIR generation to avoid stale literal outputs, got:\n{qir}"
            );
        }

        #[test]
        fn adaptive_qirgen_source_entrypoint_supports_measurement_comparisons() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
            );
            let (result, output) = line(
                &mut interpreter,
                indoc! {r#"
                namespace Test {
                    import Std.Intrinsic.*;

                    @EntryPoint()
                    operation Main() : (Bool, Bool, Bool, Bool) {
                        use (q0, q1) = (Qubit(), Qubit());
                        X(q0);
                        CNOT(q0, q1);
                        let (r0, r1) = (M(q0), M(q1));
                        Reset(q0);
                        Reset(q1);
                        return (r0 == One, r1 == Zero, r0 == r1, r0 == Zero ? false | true);
                    }
                }"#
                },
            );
            is_only_value(&result, &output, &Value::unit());

            let qir = interpreter.qirgen("Test.Main()").expect("expected success");

            assert!(
                qir.contains(
                    "call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))"
                ),
                "expected measurement comparisons to lower through read_result, got:\n{qir}"
            );
            assert!(
                qir.contains("icmp eq i1 %var_5, %var_6"),
                "expected result-to-result equality to lower to an i1 comparison, got:\n{qir}"
            );
        }

        #[test]
        fn adaptive_qirgen_nested_output_types() {
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
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

                @0 = internal constant [4 x i8] c"0_t\00"
                @1 = internal constant [6 x i8] c"1_t0r\00"
                @2 = internal constant [6 x i8] c"2_t1t\00"
                @3 = internal constant [8 x i8] c"3_t1t0b\00"
                @4 = internal constant [8 x i8] c"4_t1t1b\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  %var_2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  %var_3 = icmp eq i1 %var_2, false
                  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
                  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
                  call void @__quantum__rt__bool_record_output(i1 %var_0, i8* getelementptr inbounds ([8 x i8], [8 x i8]* @3, i64 0, i64 0))
                  call void @__quantum__rt__bool_record_output(i1 %var_3, i8* getelementptr inbounds ([8 x i8], [8 x i8]* @4, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

                declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

                declare i1 @__quantum__rt__read_result(%Result*)

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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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
        fn adaptive_rif_qirgen_entry_expr_apply_to_each_sx() {
            let mut interpreter = get_interpreter_with_capabilities(
                TargetCapabilityFlags::Adaptive
                    | TargetCapabilityFlags::IntegerComputations
                    | TargetCapabilityFlags::FloatingPointComputations,
            );
            let (result, output) = line(&mut interpreter, indoc! {"open Std.Canon;"});
            is_only_value(&result, &output, &Value::unit());

            let res = interpreter
                .qirgen("{ use qs = Qubit[4]; ApplyToEach(SX, qs); }")
                .expect("expected success");

            assert!(
                res.contains("declare void @__quantum__qis__sx__body(%Qubit*)"),
                "expected ApplyToEach(SX, qs) to generate SX calls, got:\n{res}"
            );
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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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

                @0 = internal constant [4 x i8] c"0_r\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
                  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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
            let mut interpreter =
                get_interpreter_with_capabilities(TargetCapabilityFlags::Adaptive);
            let res = interpreter
                .qirgen("{ operation check_result(r : Result) : Bool { body intrinsic; }; operation Foo() : Bool { use q = Qubit(); let r = MResetZ(q); check_result(r) } Foo() }")
                .expect("expected success");
            expect![[r#"
                %Result = type opaque
                %Qubit = type opaque

                @0 = internal constant [4 x i8] c"0_b\00"

                define i64 @ENTRYPOINT__main() #0 {
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call i1 @check_result(%Result* inttoptr (i64 0 to %Result*))
                  call void @__quantum__rt__bool_record_output(i1 %var_0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0
                }

                declare void @__quantum__rt__initialize(i8*)

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
        use qsc_data_structures::source::SourceMap;
        use qsc_data_structures::span::Span;
        use qsc_data_structures::target::Profile;
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
                TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
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
        fn restricted_profile_struct_output_runs_after_fir_transforms() {
            // A struct/UDT output is only a valid restricted-profile output after
            // the FIR transform pipeline erases the UDT and decomposes it into a
            // tuple. The construction-time gate now validates the transformed
            // program (as codegen does), so this program must construct and run
            // without a false-positive `UseOfAdvancedOutput` rejection.
            let source = indoc! { r#"
            namespace Test {
                import Std.Intrinsic.*;
                import Std.Measurement.*;

                struct Data {
                    a : Result,
                    b : Int,
                }

                @EntryPoint()
                operation Main() : Data {
                    use q = Qubit();
                    H(q);
                    new Data { a = MResetZ(q), b = 3 }
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut interpreter = Interpreter::new(
                sources,
                PackageType::Exe,
                Profile::AdaptiveRI.into(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("interpreter should be created without a false-positive capability rejection");

            let (result, _output) = entry(&mut interpreter);
            match result {
                Ok(_) => {}
                Err(errors) => {
                    panic!("expected entry to run without a capability error, got {errors:?}")
                }
            }
        }

        #[test]
        fn restricted_profile_string_output_still_rejected() {
            // A String output is genuinely advanced and remains so after the FIR
            // transforms, so the construction-time gate must still reject it.
            let source = indoc! { r#"
            namespace Test {
                @EntryPoint()
                operation Main() : String {
                    "hello"
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let result = Interpreter::new(
                sources,
                PackageType::Exe,
                Profile::AdaptiveRI.into(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            );

            match result {
                Ok(_) => panic!("expected a capability rejection for advanced String output"),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
                        "expected capability-check pass errors, got {errors:?}"
                    );
                    assert!(
                        errors
                            .iter()
                            .any(|error| format!("{error:?}").contains("UseOfAdvancedOutput")),
                        "expected an advanced-output capability diagnostic, got {errors:?}"
                    );
                }
            }
        }

        #[test]
        fn restricted_profile_fatal_transform_error_reported_as_fir_transform() {
            // A local callable forced to `Dynamic` by a loop reassignment cannot be
            // resolved statically, so the defunctionalize stage of the FIR transform
            // pipeline emits a fatal diagnostic. The construction-time gate must
            // surface this as `Error::FirTransform` (mirroring codegen), not as a
            // capability `Error::Pass`.
            let source = indoc! { r#"
            namespace Test {
                operation Foo(q : Qubit) : Unit {}
                operation Bar(q : Qubit) : Unit {}

                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    mutable f = Foo;
                    for _ in 0..2 {
                        f = Bar;
                    }
                    f(q);
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let result = Interpreter::new(
                sources,
                PackageType::Exe,
                Profile::AdaptiveRI.into(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            );

            match result {
                Ok(_) => panic!("expected a fatal FIR transform error for a dynamic callable"),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| matches!(error, crate::interpret::Error::FirTransform(_))),
                        "expected FIR transform errors, got {errors:?}"
                    );
                    assert!(
                        errors
                            .iter()
                            .any(|error| format!("{error:?}").contains("DynamicCallable")),
                        "expected a dynamic-callable transform diagnostic, got {errors:?}"
                    );
                }
            }
        }

        #[test]
        fn restricted_profile_entryless_library_does_not_panic() {
            // A PackageType::Lib package with no @EntryPoint/Main has no entry
            // expression. The FIR transform pipeline asserts (panics) on a missing
            // entry, so the construction-time gate must skip transforms and run RCA
            // on the original store. This path is reachable via the Python interop
            // layer, which constructs library interpreters under restricted targets.
            let source = indoc! { r#"
            namespace Test {
                import Std.Intrinsic.*;

                operation DoNothing() : Unit {
                    use q = Qubit();
                    H(q);
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let interpreter = Interpreter::new(
                sources,
                PackageType::Lib,
                Profile::AdaptiveRI.into(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            );

            assert!(
                interpreter.is_ok(),
                "entry-less library construction should not panic and should pass RCA, got {:?}",
                interpreter.err()
            );
        }

        #[test]
        fn restricted_profile_entryless_library_reports_transform_removable_violation() {
            // Documents a known limitation of the entry-less path. An early `return`
            // inside a measurement-conditioned scope is a profile violation
            // (`ReturnWithinDynamicScope`) only before return-unification runs; once
            // an entry expression reaches this operation, the FIR transform pipeline
            // removes the violation and codegen accepts it. But an entry-less library
            // has no entry expression, so the construction-time gate cannot run the
            // transforms and instead runs RCA on the original store. The violation is
            // therefore reported as `Error::Pass` even though it would be removed once
            // the code is actually reachable.
            let source = indoc! { r#"
            namespace Test {
                import Std.Intrinsic.*;
                import Std.Measurement.*;

                operation DynBranch(q : Qubit) : Int {
                    use a = Qubit();
                    H(a);
                    if MResetZ(a) == One {
                        return 1;
                    }
                    return 2;
                }
            }"#};

            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let result = Interpreter::new(
                sources,
                PackageType::Lib,
                Profile::AdaptiveRI.into(),
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            );

            match result {
                Ok(_) => panic!(
                    "expected the entry-less library to report the untransformed RCA violation"
                ),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
                        "expected capability-check pass errors, got {errors:?}"
                    );
                    assert!(
                        errors
                            .iter()
                            .any(|error| format!("{error:?}").contains("ReturnWithinDynamicScope")),
                        "expected a return-within-dynamic-scope diagnostic, got {errors:?}"
                    );
                }
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
            .assert_debug_eq(&items[0].namespace);
            expect![[r#"
                "B"
            "#]]
            .assert_debug_eq(&items[0].name);
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

            let mut interpreter = Interpreter::with_package_store(
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
                "#]],
            );
        }

        /// Found via fuzzing, see #2426 <https://github.com/microsoft/qdk/issues/2426>
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
