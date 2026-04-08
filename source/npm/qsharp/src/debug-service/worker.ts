// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/worker.js";
import { debugServiceProtocol } from "./debug-service.js";

const messageHandler = createWorker(debugServiceProtocol);
addEventListener("message", messageHandler);
