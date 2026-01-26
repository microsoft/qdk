// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This module exists to satisfy TypeScript during `tsc -p src/webview/tsconfig.json`.
// During bundling, `source/vscode/build.mjs` replaces this module with an inlined
// (bundled) string containing the contents of `stateComputeWorker.ts`.

const placeholderWorkerSource = "";
export default placeholderWorkerSource;
