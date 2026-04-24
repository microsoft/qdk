// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { NodeWorkerHost } from "./workers/adapters/node.js";

globalThis.WorkerHost = NodeWorkerHost;

export * from "./main.js";
