// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { messageHandler } from "qsharp-lang/compiler-worker";

self.onmessage = messageHandler;
