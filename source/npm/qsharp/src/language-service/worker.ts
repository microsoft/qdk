// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createWorker } from "../workers/worker.js";
import { languageServiceProtocol } from "./language-service.js";

const messageHandler = createWorker(languageServiceProtocol);
addEventListener("message", messageHandler);
