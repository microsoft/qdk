// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::interpret::Debugger;
use crate::line_column::Encoding;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_eval::{StepAction, StepResult, output::CursorReceiver};
use qsc_fir::fir::StmtId;
use std::io::Cursor;

fn get_breakpoint_ids(debugger: &Debugger, path: &str) -> Vec<StmtId> {
    let mut bps = debugger.get_breakpoints(path);
    bps.sort_by_key(|f| f.id);
    bps.iter().map(|f| f.id.into()).collect::<Vec<_>>()
}

fn expect_return(mut debugger: Debugger, expected: &str) {
    let r = step_next(&mut debugger, &[]);
    match r.0 {
        Ok(StepResult::Return(value)) => assert_eq!(value.to_string(), expected),
        Ok(v) => panic!("Expected Return, got {v:?}"),
        Err(e) => panic!("Expected Return, got {e:?}"),
    }
}

fn expect_bp(debugger: &mut Debugger, ids: &[StmtId], expected_id: StmtId) {
    let r = step_next(debugger, ids);
    match r.0 {
        Ok(StepResult::BreakpointHit(actual_id)) => assert!(actual_id == expected_id),
        Ok(v) => panic!("Expected BP, got {v:?}"),
        Err(e) => panic!("Expected BP, got {e:?}"),
    }
}

fn step_in(
    debugger: &mut Debugger,
    breakpoints: &[StmtId],
) -> (Result<StepResult, Vec<crate::interpret::Error>>, String) {
    step(debugger, breakpoints, qsc_eval::StepAction::In)
}

fn step_next(
    debugger: &mut Debugger,
    breakpoints: &[StmtId],
) -> (Result<StepResult, Vec<crate::interpret::Error>>, String) {
    step(debugger, breakpoints, qsc_eval::StepAction::Next)
}

fn step_out(
    debugger: &mut Debugger,
    breakpoints: &[StmtId],
) -> (Result<StepResult, Vec<crate::interpret::Error>>, String) {
    step(debugger, breakpoints, qsc_eval::StepAction::Out)
}

fn step(
    debugger: &mut Debugger,
    breakpoints: &[StmtId],
    step: StepAction,
) -> (Result<StepResult, Vec<crate::interpret::Error>>, String) {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    (
        debugger.eval_step(&mut receiver, breakpoints, step),
        receiver.dump(),
    )
}

fn expect_next(debugger: &mut Debugger) {
    let result = step_next(debugger, &[]);
    match result.0 {
        Ok(StepResult::Next) => (),
        Ok(v) => panic!("Expected Next, got {v:?}"),
        Err(e) => panic!("Expected Next, got {e:?}"),
    }
}

fn expect_in(debugger: &mut Debugger) {
    let result = step_in(debugger, &[]);
    match result.0 {
        Ok(StepResult::StepIn) => (),
        Ok(v) => panic!("Expected StepIn, got {v:?}"),
        Err(e) => panic!("Expected StepIn, got {e:?}"),
    }
}

fn expect_out(debugger: &mut Debugger) {
    let result = step_out(debugger, &[]);
    match result.0 {
        Ok(StepResult::StepOut) => (),
        Ok(v) => panic!("Expected StepOut, got {v:?}"),
        Err(e) => panic!("Expected StepOut, got {e:?}"),
    }
}

/// Converts a line/column position to a byte offset in `source`. The test
/// sources here are ASCII, so a UTF-8 column equals a byte column.
fn offset_of(source: &str, pos: crate::line_column::Position) -> usize {
    let target_line = pos.line as usize;
    let mut offset = 0;
    for (line_idx, line) in source.split_inclusive('\n').enumerate() {
        if line_idx == target_line {
            return offset + pos.column as usize;
        }
        offset += line.len();
    }
    offset
}

/// Renders a breakpoint as `startLine:startCol-endLine:endCol "<source text>"`
/// so a snapshot shows exactly which user statement each breakpoint maps to.
/// Any synthetic desugar node would surface here as a `0:0-0:0` entry, so the
/// snapshot doubles as an assertion that no synthetic guard is breakpointable.
fn format_breakpoint(source: &str, bp: &crate::interpret::BreakpointSpan) -> String {
    let start = offset_of(source, bp.range.start);
    let end = offset_of(source, bp.range.end);
    let snippet = source[start..end].replace('\n', " ");
    format!(
        "{}:{}-{}:{} {snippet:?}",
        bp.range.start.line, bp.range.start.column, bp.range.end.line, bp.range.end.column,
    )
}

/// Finds the breakpoint whose trimmed source text matches `text` and returns
/// its statement id, so a test can set a breakpoint on a specific user
/// statement, such as the `break;`, and verify it is hittable.
fn breakpoint_id_for_text(debugger: &Debugger, path: &str, source: &str, text: &str) -> StmtId {
    debugger
        .get_breakpoints(path)
        .into_iter()
        .find(|bp| {
            let start = offset_of(source, bp.range.start);
            let end = offset_of(source, bp.range.end);
            source[start..end].trim() == text
        })
        .map_or_else(
            || panic!("no breakpoint matching {text:?}"),
            |bp| bp.id.into(),
        )
}

#[cfg(test)]
mod given_debugger {
    use super::*;

    static STEPPING_SOURCE: &str = r#"
        namespace Test {
            @EntryPoint()
            operation A() : Int {
                let d = B();
                let e = d / 1;
                e
            }
            operation B() : Int {
                let g = 10;
                let h = 20;
                let l = C(g, h);
                42
            }
            operation C(m: Int, n: Int) : Int {
                let o = 42 - (m + n);
                let p = (m + n) + o;
                p
            }
        }"#;

    static DUPLICATE_RANGE_SOURCE: &str = r#"
        namespace Sample {
            @EntryPoint()
            operation Main() : Result[] {
                use q1 = Qubit();
                Y(q1);
                let m1 = M(q1);
                return [m1];
            }
        }"#;

    #[cfg(test)]
    mod step {
        use qsc_data_structures::{source::SourceMap, target::TargetCapabilityFlags};
        use rustc_hash::FxHashSet;

        use super::*;

        #[test]
        fn in_one_level_operation_works() -> Result<(), Vec<crate::interpret::Error>> {
            use qsc_data_structures::language_features::LanguageFeatures;
            let sources = SourceMap::new([("test".into(), STEPPING_SOURCE.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )?;
            let ids = get_breakpoint_ids(&debugger, "test");
            let expected_id = ids[0];
            expect_bp(&mut debugger, &ids, expected_id);
            expect_in(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            let expected = "42";
            expect_return(debugger, expected);
            Ok(())
        }

        #[test]
        fn next_crosses_operation_works() -> Result<(), Vec<crate::interpret::Error>> {
            let sources = SourceMap::new([("test".into(), STEPPING_SOURCE.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )?;
            let ids = get_breakpoint_ids(&debugger, "test");
            let expected_id = ids[0];
            expect_bp(&mut debugger, &ids, expected_id);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            let expected = "42";
            expect_return(debugger, expected);
            Ok(())
        }

        #[test]
        fn in_multiple_operations_works() -> Result<(), Vec<crate::interpret::Error>> {
            let sources = SourceMap::new([("test".into(), STEPPING_SOURCE.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )?;
            let ids = get_breakpoint_ids(&debugger, "test");
            let expected_id = ids[0];
            expect_bp(&mut debugger, &ids, expected_id);
            expect_in(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_in(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            let expected = "42";
            expect_return(debugger, expected);
            Ok(())
        }

        #[test]
        fn out_multiple_operations_works() -> Result<(), Vec<crate::interpret::Error>> {
            let sources = SourceMap::new([("test".into(), STEPPING_SOURCE.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )?;
            let ids = get_breakpoint_ids(&debugger, "test");
            let expected_id = ids[0];
            expect_bp(&mut debugger, &ids, expected_id);
            expect_in(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            expect_in(&mut debugger);
            expect_out(&mut debugger);
            expect_out(&mut debugger);
            expect_next(&mut debugger);
            expect_next(&mut debugger);
            let expected = "42";
            expect_return(debugger, expected);
            Ok(())
        }

        #[test]
        fn duplicate_source_ranges_collapse_to_one_hittable_breakpoint()
        -> Result<(), Vec<crate::interpret::Error>> {
            let sources = SourceMap::new([("test.qs".into(), DUPLICATE_RANGE_SOURCE.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            let mut debugger = Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )?;

            let breakpoints = debugger.get_breakpoints("test.qs");
            assert_eq!(breakpoints.len(), 4);

            let unique_ranges: FxHashSet<_> = breakpoints.iter().map(|bp| bp.range).collect();
            assert_eq!(unique_ranges.len(), breakpoints.len());

            let return_breakpoint_id = breakpoints
                .last()
                .expect("expected a return breakpoint")
                .id
                .into();

            expect_bp(&mut debugger, &[return_breakpoint_id], return_breakpoint_id);
            Ok(())
        }
    }

    // Stepping and breakpoint behavior for loops, including those carrying
    // `break`/`continue`. The desugar runs in `loop_unification`, so its
    // synthetic nodes are debugger-visible; these tests confirm the steppable-span
    // discipline: the flag-set replacing `break;`/`continue;` keeps the keyword
    // span and is breakpointable, guarded user statements keep their own spans, and
    // the synthetic guard `if`s (which carry `Span::default()`) are never surfaced
    // as breakpoints.
    #[cfg(test)]
    mod loops_with_break_continue {
        use super::*;
        use expect_test::expect;
        use qsc_data_structures::{source::SourceMap, target::TargetCapabilityFlags};

        static FOR_BREAK_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable total = 0;
        for i in 0..10 {
            if i == 3 {
                break;
            }
            set total = total + i;
        }
        total
    }
}"#;

        static WHILE_BREAK_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        while i < 10 {
            if i == 4 {
                break;
            }
            set i = i + 1;
        }
        i
    }
}"#;

        static REPEAT_CONTINUE_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        mutable total = 0;
        repeat {
            set i = i + 1;
            if i % 2 == 0 {
                continue;
            }
            set total = total + i;
        } until i >= 5;
        total
    }
}"#;

        static FOR_CONTINUE_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable total = 0;
        for i in 0..5 {
            if i % 2 == 0 {
                continue;
            }
            set total = total + i;
        }
        total
    }
}"#;

        static NESTED_CONTROL_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable outer = 0;
        while outer < 2 {
            set outer = outer + 1;
            for inner in 1..3 {
                if inner == 2 {
                    break;
                }
            }
            continue;
        }
        outer
    }
}"#;

        static PLAIN_REPEAT_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        repeat {
            set i = i + 1;
            set i = i + 10;
        } until i >= 5;
        i
    }
}"#;

        fn make_debugger(source: &str) -> Debugger {
            let sources = SourceMap::new([("test".into(), source.into())], None);
            let (std_id, store) =
                crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
            Debugger::new(
                sources,
                TargetCapabilityFlags::all(),
                Encoding::Utf8,
                LanguageFeatures::default(),
                store,
                &[(std_id, None)],
            )
            .expect("debugger should be created")
        }

        fn rendered_breakpoints(debugger: &Debugger, source: &str) -> String {
            debugger
                .get_breakpoints("test")
                .iter()
                .map(|bp| format_breakpoint(source, bp))
                .collect::<Vec<_>>()
                .join("\n")
        }

        #[test]
        fn for_break_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(FOR_BREAK_SOURCE);
            // Every breakpoint maps to a real user statement, including `break;`.
            // No `0:0-0:0` entry appears, so no synthetic guard is breakpointable.
            expect![[r#"
                3:8-3:26 "mutable total = 0;"
                4:8-9:9 "for i in 0..10 {             if i == 3 {                 break;             }             set total = total + i;         }"
                4:12-4:13 "i"
                4:17-4:22 "0..10"
                5:12-7:13 "if i == 3 {                 break;             }"
                6:16-6:21 "break"
                8:12-8:34 "set total = total + i;"
                10:8-10:13 "total""#]]
            .assert_eq(&rendered_breakpoints(&debugger, FOR_BREAK_SOURCE));
        }

        #[test]
        fn for_break_stepping_lands_on_break_statement() {
            let mut debugger = make_debugger(FOR_BREAK_SOURCE);
            // Setting only the `break;` breakpoint and running hits it, proving the
            // flag-set replacing `break;` is a reachable, steppable location. The
            // steppable span is the `break` keyword itself, with no trailing `;`.
            let break_id = breakpoint_id_for_text(&debugger, "test", FOR_BREAK_SOURCE, "break");
            expect_bp(&mut debugger, &[break_id], break_id);
        }

        /// Compiler-generated variables from loop unification/normalization/desugaring should not be visible in the locals list.
        #[test]
        fn for_break_locals_hide_compiler_generated_variables() {
            let mut debugger = make_debugger(FOR_BREAK_SOURCE);
            let break_id = breakpoint_id_for_text(&debugger, "test", FOR_BREAK_SOURCE, "break");
            expect_bp(&mut debugger, &[break_id], break_id);

            let mut names = debugger
                .get_locals(1)
                .into_iter()
                .map(|variable| variable.name.to_string())
                .collect::<Vec<_>>();
            names.sort();
            assert_eq!(names, ["i", "total"]);
        }

        #[test]
        fn next_skips_synthetic_block_exits() {
            let mut debugger = make_debugger(FOR_BREAK_SOURCE);
            // Synthetic guard blocks carry `Span::default()` and must not surface
            // as step locations. Every `Next` result should point to a nonempty
            // range within the user's source.
            let ids = get_breakpoint_ids(&debugger, "test");
            expect_bp(&mut debugger, &ids, ids[0]);
            let mut next_steps = 0;
            loop {
                let (result, _) = step_next(&mut debugger, &[]);
                match result {
                    Ok(StepResult::Next) => {
                        next_steps += 1;
                        let frames = debugger.get_stack_frames();
                        let frame = frames
                            .last()
                            .expect("a next step should have a stack frame");
                        let range = &frame.location.range;
                        let start = offset_of(FOR_BREAK_SOURCE, range.start);
                        let end = offset_of(FOR_BREAK_SOURCE, range.end);
                        assert!(
                            start < end && end <= FOR_BREAK_SOURCE.len(),
                            "next step should point within user source, got {:?}..{:?}",
                            range.start,
                            range.end
                        );
                    }
                    Ok(StepResult::Return(value)) => {
                        assert_eq!(value.to_string(), "3");
                        break;
                    }
                    Ok(other) => panic!("unexpected step result: {other:?}"),
                    Err(error) => panic!("unexpected error while stepping: {error:?}"),
                }
            }
            assert!(next_steps > 0, "expected at least one next step");
        }

        #[test]
        fn for_break_guarded_statement_retains_breakpoint() {
            let mut debugger = make_debugger(FOR_BREAK_SOURCE);
            // The statement after `break;` is wrapped in a synthetic guard, yet it
            // keeps its own breakpoint and is hit on the pre-break iterations.
            let guarded_id = breakpoint_id_for_text(
                &debugger,
                "test",
                FOR_BREAK_SOURCE,
                "set total = total + i;",
            );
            expect_bp(&mut debugger, &[guarded_id], guarded_id);
        }

        #[test]
        fn while_break_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(WHILE_BREAK_SOURCE);
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-9:9 "while i < 10 {             if i == 4 {                 break;             }             set i = i + 1;         }"
                5:12-7:13 "if i == 4 {                 break;             }"
                6:16-6:21 "break"
                8:12-8:26 "set i = i + 1;"
                10:8-10:9 "i""#]]
            .assert_eq(&rendered_breakpoints(&debugger, WHILE_BREAK_SOURCE));
        }

        #[test]
        fn while_break_stepping_lands_on_break_statement() {
            let mut debugger = make_debugger(WHILE_BREAK_SOURCE);
            let break_id = breakpoint_id_for_text(&debugger, "test", WHILE_BREAK_SOURCE, "break");
            expect_bp(&mut debugger, &[break_id], break_id);
        }

        #[test]
        fn repeat_continue_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(REPEAT_CONTINUE_SOURCE);
            // Even though a `repeat ... until cond;` is a `Semi` statement, its
            // body statements are individually breakpointable, mirroring the
            // `for`/`while` cases. The `continue;` maps to the `continue`
            // keyword span, the statement after it keeps its own breakpoint, and
            // no synthetic guard surfaces as a `0:0-0:0` breakpoint.
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-4:26 "mutable total = 0;"
                5:8-11:23 "repeat {             set i = i + 1;             if i % 2 == 0 {                 continue;             }             set total = total + i;         } until i >= 5;"
                6:12-6:26 "set i = i + 1;"
                7:12-9:13 "if i % 2 == 0 {                 continue;             }"
                8:16-8:24 "continue"
                10:12-10:34 "set total = total + i;"
                11:16-11:22 "i >= 5"
                12:8-12:13 "total""#]]
            .assert_eq(&rendered_breakpoints(&debugger, REPEAT_CONTINUE_SOURCE));
        }

        #[test]
        fn repeat_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(PLAIN_REPEAT_SOURCE);
            // A plain `repeat ... until cond;` with no `break`/`continue` is a `Semi`
            // statement, yet each body statement is individually breakpointable,
            // just like the `for`/`while` cases. The synthetic `while` wrapper
            // introduced by the loop desugar carries `Span::default()`, so it never
            // surfaces as a `0:0-0:0` breakpoint.
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-7:23 "repeat {             set i = i + 1;             set i = i + 10;         } until i >= 5;"
                5:12-5:26 "set i = i + 1;"
                6:12-6:27 "set i = i + 10;"
                7:16-7:22 "i >= 5"
                8:8-8:9 "i""#]].assert_eq(&rendered_breakpoints(&debugger, PLAIN_REPEAT_SOURCE));
        }

        #[test]
        fn for_continue_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(FOR_CONTINUE_SOURCE);
            // The `continue;` maps to the `continue` keyword span and the statement
            // after it keeps its own breakpoint; no synthetic guard surfaces.
            expect![[r#"
                3:8-3:26 "mutable total = 0;"
                4:8-9:9 "for i in 0..5 {             if i % 2 == 0 {                 continue;             }             set total = total + i;         }"
                4:12-4:13 "i"
                4:17-4:21 "0..5"
                5:12-7:13 "if i % 2 == 0 {                 continue;             }"
                6:16-6:24 "continue"
                8:12-8:34 "set total = total + i;"
                10:8-10:13 "total""#]]
                .assert_eq(&rendered_breakpoints(&debugger, FOR_CONTINUE_SOURCE));
        }

        #[test]
        fn for_continue_stepping_lands_on_continue_statement() {
            let mut debugger = make_debugger(FOR_CONTINUE_SOURCE);
            // Setting only the `continue;` breakpoint and running hits it. The
            // steppable span is the `continue` keyword itself, with no trailing `;`.
            let continue_id =
                breakpoint_id_for_text(&debugger, "test", FOR_CONTINUE_SOURCE, "continue");
            expect_bp(&mut debugger, &[continue_id], continue_id);
        }

        #[test]
        fn nested_control_breakpoints_keep_distinct_keyword_spans() {
            let debugger = make_debugger(NESTED_CONTROL_SOURCE);
            expect![[r#"
                3:8-3:26 "mutable outer = 0;"
                4:8-12:9 "while outer < 2 {             set outer = outer + 1;             for inner in 1..3 {                 if inner == 2 {                     break;                 }             }             continue;         }"
                5:12-5:34 "set outer = outer + 1;"
                6:12-10:13 "for inner in 1..3 {                 if inner == 2 {                     break;                 }             }"
                6:16-6:21 "inner"
                6:25-6:29 "1..3"
                7:16-9:17 "if inner == 2 {                     break;                 }"
                8:20-8:25 "break"
                11:12-11:20 "continue"
                13:8-13:13 "outer""#]].assert_eq(&rendered_breakpoints(&debugger, NESTED_CONTROL_SOURCE));
        }

        #[test]
        fn for_continue_guarded_statement_retains_breakpoint() {
            let mut debugger = make_debugger(FOR_CONTINUE_SOURCE);
            // The statement after `continue;` runs on the odd iterations, so its
            // retained breakpoint is hittable.
            let guarded_id = breakpoint_id_for_text(
                &debugger,
                "test",
                FOR_CONTINUE_SOURCE,
                "set total = total + i;",
            );
            expect_bp(&mut debugger, &[guarded_id], guarded_id);
        }

        static OPERAND_BREAK_SOURCE: &str = r#"namespace Test {
    operation Foo(q : Qubit) : Int { 5 }
    @EntryPoint()
    operation Main() : Int {
        use q = Qubit();
        mutable total = 0;
        for i in 0..10 {
            set total = total + Foo(if i == 2 { break } else { q });
        }
        total
    }
}"#;

        #[test]
        fn operand_break_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(OPERAND_BREAK_SOURCE);
            // The `break` sits in operand position inside `Foo(if i == 2 { break }
            // else { q })`. Because the operand is `Qubit`-typed with no classical
            // default, the desugar lifts it to a synthetic array-backed temp and
            // guards the consuming `set` statement. Every synthetic node the lift
            // and guard introduce carries `Span::default()`, so none surfaces as a
            // `0:0-0:0` breakpoint; only real user statements are breakpointable,
            // including the `break` keyword at its own span.
            expect![[r#"
                1:37-1:38 "5"
                4:8-4:24 "use q = Qubit();"
                5:8-5:26 "mutable total = 0;"
                6:8-8:9 "for i in 0..10 {             set total = total + Foo(if i == 2 { break } else { q });         }"
                6:12-6:13 "i"
                6:17-6:22 "0..10"
                7:12-7:68 "set total = total + Foo(if i == 2 { break } else { q });"
                7:48-7:53 "break"
                7:63-7:64 "q"
                9:8-9:13 "total""#]]
            .assert_eq(&rendered_breakpoints(&debugger, OPERAND_BREAK_SOURCE));
        }

        #[test]
        fn operand_break_stepping_lands_on_break_statement() {
            let mut debugger = make_debugger(OPERAND_BREAK_SOURCE);
            // Setting only the operand-position `break` breakpoint and running hits
            // it, proving the lifted, guarded desugar keeps the user's `break`
            // keyword as a reachable, steppable location; the synthetic temp and
            // guard nodes carry `Span::default()` and are never stepped onto.
            let break_id = breakpoint_id_for_text(&debugger, "test", OPERAND_BREAK_SOURCE, "break");
            expect_bp(&mut debugger, &[break_id], break_id);
        }

        static REPEAT_FIXUP_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        repeat {
            set i = i + 1;
        } until i >= 5
        fixup {
            set i = i * 2;
        }
        i
    }
}"#;

        #[test]
        fn repeat_fixup_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(REPEAT_FIXUP_SOURCE);
            // A `repeat ... until cond fixup { ... }` with no break/continue desugars
            // to a `while` whose tail runs the `until` update and a synthetic
            // `if .continue_cond_<id> { fixup }`. The tail update carries the `until`
            // condition span and the fixup guard carries the fixup block span, so
            // both surface at real user spans; every other synthetic node the
            // desugar introduces carries `Span::default()` and never surfaces as a
            // `0:0-0:0` breakpoint.
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-9:9 "repeat {             set i = i + 1;         } until i >= 5         fixup {             set i = i * 2;         }"
                5:12-5:26 "set i = i + 1;"
                6:16-6:22 "i >= 5"
                7:14-9:9 "{             set i = i * 2;         }"
                8:12-8:26 "set i = i * 2;"
                10:8-10:9 "i""#]].assert_eq(&rendered_breakpoints(&debugger, REPEAT_FIXUP_SOURCE));
        }

        static REPEAT_BREAK_FIXUP_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        repeat {
            set i = i + 1;
            if i == 3 {
                break;
            }
        } until i >= 5
        fixup {
            set i = i * 2;
        }
        i
    }
}"#;

        #[test]
        fn repeat_break_fixup_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(REPEAT_BREAK_FIXUP_SOURCE);
            // A `repeat` carrying a `break` and a `fixup`: the tail, the `until`
            // update and the `if .continue_cond_<id> { fixup }`, is wrapped in a
            // `break`-guarded block so it is skipped on the breaking iteration.
            // The `break` keyword, the `until` update span, and the fixup block
            // span all remain breakpointable; the `break` guard and flag scaffold
            // carry `Span::default()` and never surface.
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-12:9 "repeat {             set i = i + 1;             if i == 3 {                 break;             }         } until i >= 5         fixup {             set i = i * 2;         }"
                5:12-5:26 "set i = i + 1;"
                6:12-8:13 "if i == 3 {                 break;             }"
                7:16-7:21 "break"
                9:16-9:22 "i >= 5"
                10:14-12:9 "{             set i = i * 2;         }"
                11:12-11:26 "set i = i * 2;"
                13:8-13:9 "i""#]].assert_eq(&rendered_breakpoints(&debugger, REPEAT_BREAK_FIXUP_SOURCE));
        }

        static FOR_ARRAY_BREAK_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable total = 0;
        for x in [1, 2, 3, 4, 5] {
            if x == 3 {
                break;
            }
            set total = total + x;
        }
        total
    }
}"#;

        #[test]
        fn for_array_break_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(FOR_ARRAY_BREAK_SOURCE);
            // Iterating an array desugars through a distinct shape from a range
            // `for`: `.array_id_<id>`/`.len_id_<id>` captures and an index-driven `while`. The
            // loop variable pattern and the iterable keep their spans, the `break`
            // keyword keeps its span, and the array/length captures and guard
            // scaffold carry `Span::default()`, so no synthetic `0:0-0:0`
            // breakpoint surfaces.
            expect![[r#"
                3:8-3:26 "mutable total = 0;"
                4:8-9:9 "for x in [1, 2, 3, 4, 5] {             if x == 3 {                 break;             }             set total = total + x;         }"
                4:12-4:13 "x"
                4:17-4:32 "[1, 2, 3, 4, 5]"
                5:12-7:13 "if x == 3 {                 break;             }"
                6:16-6:21 "break"
                8:12-8:34 "set total = total + x;"
                10:8-10:13 "total""#]].assert_eq(&rendered_breakpoints(&debugger, FOR_ARRAY_BREAK_SOURCE));
        }

        static WHILE_CONTINUE_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable i = 0;
        mutable total = 0;
        while i < 10 {
            set i = i + 1;
            if i % 2 == 0 {
                continue;
            }
            set total = total + i;
        }
        total
    }
}"#;

        #[test]
        fn while_continue_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(WHILE_CONTINUE_SOURCE);
            // A continue-only `while` needs no `.broke_<id>` flag, so the desugar keeps
            // it as a bare `while` (no wrapping block) and simply resets `.cont_<id>`
            // per iteration. The `while`, its body statements, and the `continue`
            // keyword keep their spans; the `.cont_<id>` reset and the guard `if` carry
            // `Span::default()`, so no synthetic breakpoint surfaces.
            expect![[r#"
                3:8-3:22 "mutable i = 0;"
                4:8-4:26 "mutable total = 0;"
                5:8-11:9 "while i < 10 {             set i = i + 1;             if i % 2 == 0 {                 continue;             }             set total = total + i;         }"
                6:12-6:26 "set i = i + 1;"
                7:12-9:13 "if i % 2 == 0 {                 continue;             }"
                8:16-8:24 "continue"
                10:12-10:34 "set total = total + i;"
                12:8-12:13 "total""#]].assert_eq(&rendered_breakpoints(&debugger, WHILE_CONTINUE_SOURCE));
        }

        static FOR_BREAK_CONTINUE_SOURCE: &str = r#"namespace Test {
    @EntryPoint()
    operation Main() : Int {
        mutable total = 0;
        for i in 0..10 {
            if i % 2 == 0 {
                continue;
            }
            if i == 7 {
                break;
            }
            set total = total + i;
        }
        total
    }
}"#;

        #[test]
        fn for_break_continue_breakpoints_map_to_user_statements() {
            let debugger = make_debugger(FOR_BREAK_CONTINUE_SOURCE);
            // A `for` carrying both a `break` and a `continue` mints both flags and
            // guards trailing statements with `not .broke_<id> and not .cont_<id>`. Both
            // keywords, both `if`s, and the trailing `set` keep their spans; every
            // two-flag guard node carries `Span::default()`, so no synthetic
            // breakpoint surfaces.
            expect![[r#"
                3:8-3:26 "mutable total = 0;"
                4:8-12:9 "for i in 0..10 {             if i % 2 == 0 {                 continue;             }             if i == 7 {                 break;             }             set total = total + i;         }"
                4:12-4:13 "i"
                4:17-4:22 "0..10"
                5:12-7:13 "if i % 2 == 0 {                 continue;             }"
                6:16-6:24 "continue"
                8:12-10:13 "if i == 7 {                 break;             }"
                9:16-9:21 "break"
                11:12-11:34 "set total = total + i;"
                13:8-13:13 "total""#]]
                .assert_eq(&rendered_breakpoints(&debugger, FOR_BREAK_CONTINUE_SOURCE));
        }
    }
}
