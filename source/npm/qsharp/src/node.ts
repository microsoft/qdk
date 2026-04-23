// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { NodeMainThreadAdapter } from "./workers/adapters/node.js";

globalThis.WorkerMain = NodeMainThreadAdapter;

export * from "./main.js";
