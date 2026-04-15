// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Node.js entrypoint. Polyfills the Worker global before loading the main module.

import worker from "web-worker";
import { setWorkerType } from "./main.js";

if (typeof globalThis.Worker === "undefined") {
  globalThis.Worker = worker;
}
setWorkerType("module");

export * from "./main.js";
