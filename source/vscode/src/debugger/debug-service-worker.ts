// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

declare const __PLATFORM_DIR__: string;

import { messageHandler } from "qsharp-lang/debug-service-worker";

if (__PLATFORM_DIR__ === "browser") {
  self.onmessage = messageHandler;
}
