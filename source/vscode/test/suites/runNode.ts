// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Adapted from the sample at
// https://github.com/microsoft/vscode-test/blob/main/sample/src/test/suite/index.ts
//
//
// This script runs in the VS Code extension host in Electron
//

import Mocha from "mocha";

export function runMochaTests(
  requireTestModules: () => void,
  options?: Mocha.MochaOptions,
): Promise<void> {
  return new Promise((c, e) => {
    const mocha = new Mocha({
      ui: "tdd",
      reporter: undefined,
      ...options,
    });

    // Install tdd globals (suite, test, suiteSetup, etc.) on globalThis
    // before requiring bundled test modules. Normally mocha fires
    // "pre-require" per file inside loadFiles(), but since we're loading
    // bundled modules directly via require(), we trigger it manually.
    (mocha.suite as any).emit("pre-require", globalThis, "bundled", mocha);

    // Load the test suites. This needs to come after the pre-require emit
    // so that the suite() global is defined by mocha.
    requireTestModules();

    try {
      // Run the mocha test
      mocha.run((failures: number) => {
        if (failures > 0) {
          console.error(
            `[error] ${failures} vscode integration test(s) failed. See above for failure details.`,
          );
          e(new Error(`${failures} vscode integration test(s) failed.`));
        } else {
          c();
        }
      });
    } catch (err) {
      console.error(err);
      e(err);
    }
  });
}
