// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { messageHandler } from "qsharp-lang/language-service-worker";

self.onmessage = messageHandler;
