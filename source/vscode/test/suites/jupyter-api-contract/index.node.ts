// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { runMochaTests } from "../runNode";
import { TEST_TIMEOUT_MS } from "../extensionUtils";

export function run(): Promise<void> {
  return runMochaTests(
    () => {
      require("./jupyter-api-contract.test"); // eslint-disable-line @typescript-eslint/no-require-imports
    },
    { timeout: TEST_TIMEOUT_MS },
  );
}
