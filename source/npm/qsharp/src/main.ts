// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This module is the main entry point for use in Node.js environments. For browser environments,
// the "./browser.js" file is the entry point module.
//
// Most functionality is shared with browser.ts. This module only provides
// Node.js-specific worker creation (using node:worker_threads) and
// re-exports everything else from browser.ts.

import { type ICompilerWorker, compilerProtocol } from "./compiler/compiler.js";
import {
  type IDebugServiceWorker,
  debugServiceProtocol,
} from "./debug-service/debug-service.js";
import {
  type ILanguageServiceWorker,
  languageServiceProtocol,
} from "./language-service/language-service.js";
import { getWasmModule } from "./browser.js";
import { createProxy } from "./workers/node.js";

export {
  loadWasmModule,
  getLibrarySourceContent,
  getCompiler,
  getProjectLoader,
  getDebugService,
  getLanguageService,
  getTargetProfileFromEntryPoint,
} from "./browser.js";

export function getCompilerWorker(worker: string): ICompilerWorker {
  return createProxy(worker, getWasmModule(), compilerProtocol);
}

export function getDebugServiceWorker(worker: string): IDebugServiceWorker {
  return createProxy(worker, getWasmModule(), debugServiceProtocol);
}

export function getLanguageServiceWorker(
  worker: string,
): ILanguageServiceWorker {
  return createProxy(worker, getWasmModule(), languageServiceProtocol);
}

export * from "./common-exports.js";
