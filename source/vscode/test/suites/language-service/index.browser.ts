// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { runMochaTests } from "../runBrowser";
import { setTestGithubEndpoint } from "../extensionUtils";

export function run(): Promise<void> {
  setTestGithubEndpoint("http://localhost:3000/static/mount/web/github");
  return runMochaTests(() => {
    // We can't use any wildcards or dynamically discovered
    // paths here since ESBuild needs these modules to be
    // real paths on disk at bundling time.
    require("./language-service.test"); // eslint-disable-line @typescript-eslint/no-require-imports
    require("./notebook.test"); // eslint-disable-line @typescript-eslint/no-require-imports
  });
}
