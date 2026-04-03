// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/node.js";
import { compilerProtocol } from "./compiler.js";

export const messageHandler = createWorker(compilerProtocol);
