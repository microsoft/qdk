// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/worker.js";
import { debugServiceProtocol } from "./debug-service.js";

// message handler exported for backwards compatibility
export const messageHandler = createWorker(debugServiceProtocol);
