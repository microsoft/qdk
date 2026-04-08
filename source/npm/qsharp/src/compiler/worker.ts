// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/worker.js";
import { compilerProtocol } from "./compiler.js";

const messageHandler = createWorker(compilerProtocol);
addEventListener("message", messageHandler);
