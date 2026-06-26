// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { runMochaTests } from "../runBrowser";

export function run(): Promise<void> {
  return runMochaTests(() => {
    // We can't use any wildcards or dynamically discovered
    // paths here since ESBuild needs these modules to be
    // real paths on disk at bundling time.
    require("./learning.test"); // eslint-disable-line @typescript-eslint/no-require-imports
  });
}
