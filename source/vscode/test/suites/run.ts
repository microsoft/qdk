// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Adapted from the sample at
// https://github.com/microsoft/vscode-test-web/blob/3f0f858ab15cb65ef3c19564b0f5a6910ea9414e/sample/src/web/test/suite/index.ts
//
//
// This script runs in the VS Code for the Web extension host.
//

// imports mocha for the browser, defining the `mocha` global.
require("mocha/mocha"); // eslint-disable-line @typescript-eslint/no-require-imports

export function runMochaTests(
  requireTestModules: () => void,
  options?: Mocha.MochaOptions
): Promise<void> {
  return new Promise((c, e) => {
    mocha.setup({
      ui: "tdd",
      reporter: undefined,
      ...options,
    });

    // Load the test suites. This needs to come
    // after the call to mocha.setup() so that the
    // suite() global is defined by mocha.
    requireTestModules();

    try {
      // Run the mocha test
      mocha.run((failures) => {
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
