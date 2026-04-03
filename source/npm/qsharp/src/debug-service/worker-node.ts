// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/node.js";
import { debugServiceProtocol } from "./debug-service.js";

export const messageHandler = createWorker(debugServiceProtocol);
