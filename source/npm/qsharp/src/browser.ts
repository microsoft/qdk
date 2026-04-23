// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { BrowserMainThreadAdapter } from "./workers/adapters/browser.js";

globalThis.WorkerMain = BrowserMainThreadAdapter;
export * from "./main.js";
