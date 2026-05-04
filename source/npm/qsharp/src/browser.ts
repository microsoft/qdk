// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { BrowserWorkerHost } from "./workers/adapters/browser.js";

globalThis.WorkerHost = BrowserWorkerHost;
export * from "./main.js";
