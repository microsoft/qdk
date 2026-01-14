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
    #[cfg(test)]
    mod step {
        use qsc_data_structures::{source::SourceMap, target::TargetCapabilityFlags};

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
    }
}
