// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  ICompilerWorker,
  ILanguageService,
  ITestDescriptor,
  log,
} from "qsharp-lang";
import * as vscode from "vscode";
import { loadCompilerWorker, toVsCodeLocation, toVsCodeRange } from "../common";
import { getProgramForDocument } from "../programConfig";
import { createDebugConsoleEventTarget } from "../debugger/output";

let worker: ICompilerWorker | null = null;
/**
 * Returns a singleton instance of the compiler worker.
 * @param context The extension context.
 * @returns The compiler worker.
 **/
function getLocalCompilerWorker(extensionUri: vscode.Uri): ICompilerWorker {
  if (worker !== null) {
    return worker;
  }

  worker = loadCompilerWorker(extensionUri);

  return worker;
}

export function startTestDiscovery(
  languageService: ILanguageService,
  context: vscode.ExtensionContext,
): vscode.Disposable[] {
  // test explorer features
  const testController: vscode.TestController =
    vscode.tests.createTestController("qsharpTestController", "Q# Tests");
  const runHandler = (request: vscode.TestRunRequest) => {
    if (!request.continuous) {
      return startTestRun(request);
    }
  };

  // runs an individual test run
  // or test group (a test run where there are child tests)
  const startTestRun = async (request: vscode.TestRunRequest) => {
    // use the compiler worker to run the test in the interpreter

    log.trace("Starting test run, request was", JSON.stringify(request));

    const worker = getLocalCompilerWorker(context.extensionUri);

    // request.include is an array of test cases to run, and it is only provided if a specific set of tests were selected.
    if (request.include !== undefined) {
      for (const testCase of request.include || []) {
        await runTestCase(testController, testCase, request, worker);
      }
    } else {
      // alternatively, if there is no include specified, we run all tests that are not in the exclude list
      for (const [, testCase] of testController.items) {
        if (request.exclude && request.exclude.includes(testCase)) {
          continue;
        }
        await runTestCase(testController, testCase, request, worker);
      }
    }
  };

  /**
   * Given a single test case, run it in the worker (which runs the interpreter) and report results back to the
   * `TestController` as a side effect.
   *
   * This function manages its own event target for the results of the test run and uses the controller to render the output in the VS Code UI.
   **/
  async function runTestCase(
    ctrl: vscode.TestController,
    testCase: vscode.TestItem,
    request: vscode.TestRunRequest,
    worker: ICompilerWorker,
  ): Promise<void> {
    log.trace("Running Q# test: ", testCase.id);
    if (testCase.children.size > 0) {
      for (const childTestCase of testCase.children) {
        await runTestCase(ctrl, childTestCase[1], request, worker);
      }
      return;
    }
    const run = ctrl.createTestRun(request);
    const evtTarget = createDebugConsoleEventTarget((msg) => {
      run.appendOutput(`${msg}\n`);
    });
    evtTarget.addEventListener("Result", (msg) => {
      if (msg.detail.success) {
        run.passed(testCase);
      } else {
        const failureLocationUri =
          msg.detail?.value?.uri ||
          (msg.detail?.value?.related &&
            msg.detail.value.related[0].location?.source) ||
          null;
        // If there was no 'uri' on 'value', then the 'range' on 'value' tends to be unreliable too,
        // so use the span on the first related location if available.
        const failureLocationRange =
          !msg.detail?.value?.uri && msg.detail?.value?.related?.[0]
            ? msg.detail.value.related[0].location.span
            : msg.detail.value.range;

        const message: vscode.TestMessage = {
          message: msg.detail.value.message,
          location:
            failureLocationUri === null
              ? undefined
              : {
                  range: toVsCodeRange(failureLocationRange),
                  uri: vscode.Uri.parse(failureLocationUri),
                },
        };
        run.failed(testCase, message);
      }
      run.end();
    });

    const callableExpr = `${testCase.id}()`;
    const uri = testCase.uri;
    if (!uri) {
      log.error(`No compilation URI for test ${testCase.id}`);
      run.appendOutput(`No compilation URI for test ${testCase.id}\r\n`);
      return;
    }
    const file = await vscode.workspace.openTextDocument(uri);
    const programResult = await getProgramForDocument(file);

    try {
      if (!programResult.success) {
        throw new Error(programResult.errorMsg);
      }

      await worker.run(programResult.programConfig, callableExpr, 1, evtTarget);
    } catch (error) {
      log.error(`Error running test ${testCase.id}:`, error);
      run.appendOutput(`Error running test ${testCase.id}: ${error}\r\n`);
      run.end();
    }
    log.trace("ran test:", testCase.id);
  }

  testController.createRunProfile(
    "Interpreter",
    vscode.TestRunProfileKind.Run,
    runHandler,
    true,
    undefined,
    false,
  );

  const testVersions = new WeakMap<vscode.TestItem, number>();
  async function onTestCallables(evt: {
    detail: {
      callables: ITestDescriptor[];
    };
  }) {
    let currentVersion = 0;
    for (const [, testItem] of testController.items) {
      currentVersion = (testVersions.get(testItem) || 0) + 1;
      break;
    }

    for (const { callableName, location, friendlyName } of evt.detail
      .callables) {
      const vscLocation = toVsCodeLocation(location);
      // below, we transform `parts` into a tree structure for the test explorer
      // e.g. if we have the following callables:
      // - "TestSuite.Test1"
      // - "TestSuite.Test2"
      // they will be turned into the parts:
      // - ["FriendlyName", "TestSuite", "Test1"]
      // - ["FriendlyName", "TestSuite", "Test2"]
      // and then into a tree structure:
      // - FriendlyName
      //   - TestSuite
      //     - Test1
      //     - Test2
      const parts = [friendlyName, ...callableName.split(".")];

      let rover = testController.items;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        // the `id` is used to actually call the test item
        // it is constructed in the test runner via: callableExpr = `${testCase.id}()`;
        // so it should be the full path to the test item, not including the "friendly name" (since that isn't in the callable expr),
        // if and only if it is a "leaf" (an actual test case)
        // note that leaves are test cases and internal nodes are not test cases
        // in the above example, TestSuite would have the id `FriendlyName.TestSuite`, and Test1 would have the id `TestSuite.Test1`
        const id =
          i === parts.length - 1
            ? callableName
            : parts.slice(0, i + 1).join(".");
        // this test item may have already existed from a previous scan, so fetch it
        let testItem = rover.get(id);

        // if it doesn't exist, create it
        if (!testItem) {
          testItem = testController.createTestItem(id, part, vscLocation.uri);
          // if this is the actual test item, give it a range and a compilation uri
          // this triggers the little "green play button" in the test explorer and in the left
          // gutter of the editor
          if (i === parts.length - 1) {
            testItem.range = vscLocation.range;
          }
          rover.add(testItem);
        }
        testVersions.set(testItem, currentVersion);
        rover = testItem.children;
      }
    }

    // delete old items from previous versions that were not updated
    deleteItemsNotOfVersion(
      currentVersion,
      testController.items,
      testController,
    );
  }

  function deleteItemsNotOfVersion(
    version: number,
    items: vscode.TestItemCollection,
    testController: vscode.TestController,
  ) {
    for (const [id, testItem] of items) {
      deleteItemsNotOfVersion(version, testItem.children, testController);
      if (testVersions.get(testItem) !== version) {
        items.delete(id);
      }
    }
  }

  languageService.addEventListener("testCallables", onTestCallables);

  return [
    {
      dispose: () => {
        languageService.removeEventListener("testCallables", onTestCallables);
      },
    },
    testController,
  ];
}
