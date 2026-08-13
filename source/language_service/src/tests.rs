// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Encoding, LanguageService, Update, UpdateHandler, VersionWaitResult,
    protocol::{DiagnosticUpdate, ErrorKind, TestCallables, WorkspaceConfigurationUpdate},
    push_update,
};
use expect_test::{Expect, expect};
use miette::Diagnostic;
use qsc::{compile, line_column::Position, project};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use test_fs::{FsNode, TestProjectHost, dir, file};

pub(crate) mod test_fs;

#[tokio::test]
async fn single_document() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");

    update_handler.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn single_document_update() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");

    update_handler.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );

    // UPDATE 2
    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { @EntryPoint() operation Bar() : Unit {} }",
        "qsharp",
    );

    update_handler.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { @EntryPoint() operation Bar() : Unit {} }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn document_in_project() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("project/src/this_file.qs", 1, "namespace Foo { }", "qsharp");

    check_errors_and_no_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "project/src/this_file.qs",
        &(expect![[r#"
            []
        "#]]),
    );

    // now process background work
    update_handler.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "project/src/this_file.qs",
        &expect![[r#"
            []
        "#]],
        &expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "project/src/other_file.qs",
                        contents: "namespace OtherFile { operation Other() : Unit {} }",
                        offset: 0,
                    },
                    Source {
                        name: "project/src/this_file.qs",
                        contents: "namespace Foo { }",
                        offset: 52,
                    },
                ],
                common_prefix: Some(
                    "project/src/",
                ),
                entry: None,
            }
        "#]],
    );
}

// the below tests test the asynchronous behavior of the language service.
// we use `get_completions` as a rough analog for all document operations, as
// they all go through the same `document_op` infrastructure.
#[tokio::test]
async fn completions_requested_before_document_load() {
    let errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let _update_handler = create_update_handler(&mut ls, &errors, &test_cases);

    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { open Microsoft.Quantum.Diagnostics; @EntryPoint() operation Main() : Unit { DumpMachine() } }",
        "qsharp"
    );

    // we intentionally don't await work to test how LSP features function when
    // a document hasn't fully loaded

    // this should be empty, because the doc hasn't loaded
    assert!(
        ls.get_completions(
            "foo.qs",
            Position {
                line: 0,
                column: 76
            }
        )
        .items
        .is_empty()
    );
}

#[tokio::test]
async fn completions_requested_after_document_load() {
    let errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &errors, &test_cases);

    // this test is a contrast to `completions_requested_before_document_load`
    // we want to ensure that completions load when the update_document call has been awaited
    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { open Microsoft.Quantum.Diagnostics; @EntryPoint() operation Main() : Unit { DumpMachine() } }",
        "qsharp"
    );

    update_handler.apply_pending().await;

    assert!(
        &ls.get_completions(
            "foo.qs",
            Position {
                line: 0,
                column: 92,
            },
        )
        .items
        .iter()
        .any(|item| item.label == "DumpMachine")
    );
}

#[tokio::test]
async fn package_aware_foreign_fir_transform_diagnostic() {
    let foreign_source = r#"
        namespace ForeignLib {
            struct Config {
                Count : Int,
                Apply : Qubit => Unit,
                Keep : Qubit => Unit,
            }

            operation InvokeDynamic(q : Qubit) : Unit {
                mutable op = H;
                for _ in 0..3 {
                    op = X;
                }
                let config = new Config {
                    Count = 1,
                    Apply = op,
                    Keep = q2 => {
                        H(q2);
                        S(q2);
                    },
                };
                config.Apply(q);
            }

            export InvokeDynamic;
        }
    "#;
    let user_source = r#"
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Foreign.ForeignLib.InvokeDynamic(q);
        }
    "#;
    let fs = Rc::new(RefCell::new(FsNode::Dir(
        [
            dir(
                "project",
                [
                    file(
                        "qsharp.json",
                        r#"{ "targetProfile": "base", "dependencies": { "Foreign": { "path": "../foreign" } } }"#,
                    ),
                    dir("src", [file("main.qs", user_source)]),
                ],
            ),
            dir(
                "foreign",
                [
                    file("qsharp.json", "{}"),
                    dir("src", [file("lib.qs", foreign_source)]),
                ],
            ),
        ]
        .into_iter()
        .collect(),
    )));
    let diagnostics = RefCell::new(Vec::<(String, compile::Error)>::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = ls.create_update_handler(
        |update: DiagnosticUpdate| {
            diagnostics
                .borrow_mut()
                .extend(update.errors.into_iter().filter_map(|error| match error {
                    ErrorKind::Compile(error) => Some((update.uri.clone(), error)),
                    ErrorKind::Project(_)
                    | ErrorKind::DocumentStatus { .. }
                    | ErrorKind::Unnecessary(_) => None,
                }));
        },
        |_| {},
        TestProjectHost { fs },
    );

    ls.update_document("project/src/main.qs", 1, user_source, "qsharp");
    update_handler.apply_pending().await;

    let diagnostics = diagnostics.borrow();
    let [(uri, error)] = diagnostics.as_slice() else {
        panic!("expected one compile diagnostic, got {diagnostics:?}");
    };
    let code = error.code().expect("diagnostic should have a code");
    assert_eq!(code.to_string(), "Qdk.Qsc.Defunctionalize.DynamicCallable");

    let label = error
        .labels()
        .into_iter()
        .flatten()
        .next()
        .expect("diagnostic should have a source label");
    let (source, relative_span) = error.resolve_span(label.inner());
    let span_start = relative_span.offset();
    let span_end = span_start + relative_span.len();

    assert_eq!(uri, "foreign/src/lib.qs");
    assert_eq!(source.name.as_ref(), "foreign/src/lib.qs");
    assert_eq!(&source.contents[span_start..span_end], "config.Apply(q)");
    assert_ne!(source.name.as_ref(), "OutOfBounds");
}

fn check_errors_and_compilation(
    ls: &LanguageService,
    received_errors: &mut Vec<ErrorInfo>,
    uri: &str,
    expected_errors: &Expect,
    expected_compilation: &Expect,
) {
    expected_errors.assert_debug_eq(received_errors);
    assert_compilation(ls, uri, expected_compilation);
    received_errors.clear();
}

fn check_errors_and_no_compilation(
    ls: &LanguageService,
    received_errors: &mut Vec<ErrorInfo>,
    uri: &str,
    expected_errors: &Expect,
) {
    expected_errors.assert_debug_eq(received_errors);
    received_errors.clear();

    let state = ls.state.try_borrow().expect("borrow should succeed");
    assert!(state.get_compilation(uri).is_none());
}

fn assert_compilation(ls: &LanguageService, uri: &str, expected: &Expect) {
    let state = ls.state.try_borrow().expect("borrow should succeed");
    let compilation = state
        .get_compilation(uri)
        .expect("compilation should exist");
    expected.assert_debug_eq(&compilation.user_unit().sources);
}

/// Drives the real `run()` loop to verify that updates delivered by the host while an
/// update is in flight get coalesced into a single compilation.
///
/// The host event loop is simulated by the yield closure: it pushes updates on the
/// iteration where a real host would have delivered queued keystrokes. No timers are
/// involved, so this is deterministic.
#[tokio::test]
async fn run_coalesces_updates_delivered_while_yielding() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    // Unterminated namespace, so every version reports a diagnostic and is therefore
    // observable in `received_errors`.
    ls.update_document("foo.qs", 1, "namespace Foo { ", "qsharp");

    let ls = RefCell::new(ls);
    let yields = Cell::new(0);
    // Mock the function that would be created by createHostYield
    let yield_to_host = || {
        // The first time the update handler yields, simulate two more updates coming in before it resumes
        if yields.replace(yields.get() + 1) == 0 {
            let mut ls = ls.borrow_mut();
            // Two more keystrokes land while the first update is being handled.
            ls.update_document("foo.qs", 2, "namespace Foo { a", "qsharp");
            ls.update_document("foo.qs", 3, "namespace Foo { ab", "qsharp");
            // Nothing further arrives. Closing the channel is what lets `run()` return
            // instead of waiting forever once this batch has been applied.
            ls.stop_updates();
        }
        std::future::ready(())
    };

    update_handler.run(yield_to_host).await;

    let applied: Vec<Option<u32>> = received_errors
        .borrow()
        .iter()
        .map(|(_, version, _, _)| *version)
        .collect();

    // All three collapse into one compilation. Version 1 is included even though it was
    // dequeued first, because yielding happens before it is applied, so there is nothing
    // in flight that has to be finished. The diagnostics that do get published describe
    // the document as it actually stands.
    assert_eq!(applied, vec![Some(3)]);

    // The handler yields exactly once per batch, and everything landed in one batch.
    assert_eq!(yields.get(), 1);
}

/// Coalescing must not drop updates that aren't redundant with each other.
#[tokio::test]
async fn run_applies_updates_to_distinct_documents() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    // Deliberately erroneous code so we'll see an error from this file if this code
    // is included in the compilation
    ls.update_document("foo.qs", 1, "namespace Foo { ", "qsharp");

    let ls = RefCell::new(ls);
    let yields = Cell::new(0);
    let yield_to_host = || {
        if yields.replace(yields.get() + 1) == 0 {
            // Simulate the arrival of another update while the handler is yielding
            let mut ls = ls.borrow_mut();
            ls.update_document("bar.qs", 1, "namespace Bar { ", "qsharp");
            ls.stop_updates();
        }
        std::future::ready(())
    };

    update_handler.run(yield_to_host).await;

    // Diagnostics get republished for every compilation on each update, so compare the
    // set of documents that were compiled rather than the exact publish sequence.
    let mut applied: Vec<String> = received_errors
        .borrow()
        .iter()
        .map(|(uri, _, _, _)| uri.clone())
        .collect();
    applied.sort();
    applied.dedup();

    assert_eq!(applied, ["bar.qs", "foo.qs"]);
}

#[tokio::test]
async fn wait_for_document_version_ready_when_already_current() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");
    update_handler.apply_pending().await;

    assert_eq!(
        ls.wait_for_document_version("foo.qs", 1).await,
        VersionWaitResult::Ready
    );
}

/// A caller that parks while its version is still queued is woken once it lands.
#[tokio::test]
async fn wait_for_document_version_resolves_once_applied() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");

    let wait = ls.wait_for_document_version("foo.qs", 1);
    futures_util::pin_mut!(wait);
    assert!(
        futures::poll!(wait.as_mut()).is_pending(),
        "expected the caller to park until the update is applied"
    );

    update_handler.apply_pending().await;

    assert_eq!(wait.await, VersionWaitResult::Ready);
}

/// The case the completion path is built around: the requested version gets merged away
/// before it is ever compiled, so it can never be answered for.
#[tokio::test]
async fn wait_for_document_version_superseded_when_coalesced_away() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { ", "qsharp");

    let wait = ls.wait_for_document_version("foo.qs", 1);
    futures_util::pin_mut!(wait);
    assert!(futures::poll!(wait.as_mut()).is_pending());

    // Version 1 is still queued, so this merges over it and only version 2 is compiled.
    ls.update_document("foo.qs", 2, "namespace Foo { a", "qsharp");
    update_handler.apply_pending().await;

    assert_eq!(wait.await, VersionWaitResult::Superseded);
}

/// A version the state has already moved past is reported without parking at all.
#[tokio::test]
async fn wait_for_document_version_superseded_without_parking() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 2, "namespace Foo { }", "qsharp");
    update_handler.apply_pending().await;

    assert_eq!(
        ls.wait_for_document_version("foo.qs", 1).await,
        VersionWaitResult::Superseded
    );
}

/// A caller parked on a version that can no longer arrive has to be released when the
/// update handler stops, rather than waiting forever.
#[tokio::test]
async fn wait_for_document_version_released_when_handler_stops() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    // The wait doesn't borrow `ls`, so updates can still be stopped while it's alive.
    let wait = ls.wait_for_document_version("foo.qs", 1);
    ls.stop_updates();

    // `join` polls the wait first, so it is parked by the time the handler shuts down.
    let (result, ()) =
        futures::future::join(wait, update_handler.run(|| std::future::ready(()))).await;

    assert_eq!(result, VersionWaitResult::Never);
}

/// Once the handler has stopped there is nothing left to wake a new caller, so parking
/// one would hang it until its timeout.
#[tokio::test]
async fn wait_for_document_version_returns_immediately_after_handler_stops() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut update_handler = create_update_handler(&mut ls, &received_errors, &test_cases);

    ls.stop_updates();
    update_handler.run(|| std::future::ready(())).await;

    assert_eq!(
        ls.wait_for_document_version("foo.qs", 1).await,
        VersionWaitResult::Never
    );
}

#[test]
fn push_update_merges_consecutive_updates_to_same_document() {
    let mut updates = Vec::new();
    push_update(&mut updates, document_update("foo.qs", 1));
    push_update(&mut updates, document_update("foo.qs", 2));
    push_update(&mut updates, document_update("foo.qs", 3));

    assert_eq!(update_summaries(&updates), ["Document(foo.qs, 3)"]);
}

#[test]
fn push_update_keeps_updates_to_different_documents() {
    let mut updates = Vec::new();
    push_update(&mut updates, document_update("foo.qs", 1));
    push_update(&mut updates, document_update("bar.qs", 1));
    push_update(&mut updates, document_update("foo.qs", 2));

    assert_eq!(
        update_summaries(&updates),
        [
            "Document(foo.qs, 1)",
            "Document(bar.qs, 1)",
            "Document(foo.qs, 2)"
        ]
    );
}

#[test]
fn push_update_does_not_merge_across_a_configuration_update() {
    let mut updates = Vec::new();
    push_update(&mut updates, document_update("foo.qs", 1));
    push_update(
        &mut updates,
        Update::Configuration {
            changed: WorkspaceConfigurationUpdate::default(),
        },
    );
    push_update(&mut updates, document_update("foo.qs", 2));

    assert_eq!(
        update_summaries(&updates),
        [
            "Document(foo.qs, 1)",
            "Configuration",
            "Document(foo.qs, 2)"
        ]
    );
}

fn document_update(uri: &str, version: u32) -> Update {
    Update::Document {
        uri: uri.into(),
        version,
        text: "namespace Foo { }".into(),
        language_id: "qsharp".into(),
    }
}

fn update_summaries(updates: &[Update]) -> Vec<String> {
    updates.iter().map(Update::summary).collect()
}

type ErrorInfo = (
    String,
    Option<u32>,
    Vec<compile::ErrorKind>,
    Vec<project::Error>,
);

fn create_update_handler<'a>(
    ls: &mut LanguageService,
    received_errors: &'a RefCell<Vec<ErrorInfo>>,
    received_test_cases: &'a RefCell<Vec<TestCallables>>,
) -> UpdateHandler<'a> {
    ls.create_update_handler(
        |update: DiagnosticUpdate| {
            let project_errors = update.errors.iter().filter_map(|error| match error {
                ErrorKind::Project(error) => Some(error.clone()),
                ErrorKind::Compile(_)
                | ErrorKind::DocumentStatus { .. }
                | ErrorKind::Unnecessary(_) => None,
            });
            let compile_errors = update.errors.iter().filter_map(|error| match error {
                ErrorKind::Compile(error) => Some(error.error().clone()),
                ErrorKind::Project(_)
                | ErrorKind::DocumentStatus { .. }
                | ErrorKind::Unnecessary(_) => None,
            });

            let mut v = received_errors.borrow_mut();

            v.push((
                update.uri,
                update.version,
                compile_errors.collect(),
                project_errors.collect(),
            ));
        },
        move |update: TestCallables| {
            let mut v = received_test_cases.borrow_mut();
            v.push(update);
        },
        TestProjectHost {
            fs: TEST_FS.with(Clone::clone),
        },
    )
}

thread_local! { static TEST_FS: Rc<RefCell<FsNode>> = Rc::new(RefCell::new(test_fs())) }

fn test_fs() -> FsNode {
    FsNode::Dir(
        [dir(
            "project",
            [
                file("qsharp.json", "{}"),
                dir(
                    "src",
                    [
                        file(
                            "other_file.qs",
                            "namespace OtherFile { operation Other() : Unit {} }",
                        ),
                        file("this_file.qs", "namespace Foo { }"),
                    ],
                ),
            ],
        )]
        .into_iter()
        .collect(),
    )
}
