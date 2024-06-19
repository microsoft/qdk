// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This module is the main entry point for use in Node.js environments. For browser environments,
// the "./browser.js" file is the entry point module.

import { createRequire } from "node:module";
import {
  Compiler,
  ICompiler,
  ICompilerWorker,
  compilerProtocol,
} from "./compiler/compiler.js";
import {
  IDebugService,
  IDebugServiceWorker,
  QSharpDebugService,
  debugServiceProtocol,
} from "./debug-service/debug-service.js";
import {
  ILanguageService,
  ILanguageServiceWorker,
  QSharpLanguageService,
  languageServiceProtocol,
  qsharpLibraryUriScheme,
} from "./language-service/language-service.js";
import { log } from "./log.js";
import { createProxy } from "./workers/node.js";
import type { IProjectConfig, ProjectLoader } from "../lib/web/qsc_wasm.js";

export { qsharpLibraryUriScheme };

// Only load the Wasm module when first needed, as it may only be used in a Worker,
// and not in the main thread.

// Use the types from the web version for... reasons.
type Wasm = typeof import("../lib/web/qsc_wasm.js");
let wasm: Wasm | null = null;

function ensureWasm() {
  if (!wasm) {
    wasm = require("../lib/node/qsc_wasm.cjs") as Wasm;
    // Set up logging and telemetry as soon as possible after instantiating
    wasm.initLogging(log.logWithLevel, log.getLogLevel());
    log.onLevelChanged = (level) => wasm?.setLogLevel(level);
  }
}

const require = createRequire(import.meta.url);

export async function getLibrarySourceContent(
  path: string,
): Promise<string | undefined> {
  ensureWasm();
  return wasm!.get_library_source_content(path);
}

export async function getGithubSourceContent(
  path: string,
): Promise<string | undefined> {
  ensureWasm();
  return wasm!.get_github_source_content(path);
}

export function getCompiler(): ICompiler {
  ensureWasm();
  return new Compiler(wasm!);
}

export function getProjectLoader(
  readFile: (path: string) => Promise<string | null>,
  loadDirectory: (path: string) => Promise<[string, number][]>,
  resolvePath: (base: string, relative: string) => Promise<string>,
  fetchGithub: (
    owner: string,
    repo: string,
    ref: string,
    path: string,
  ) => Promise<string | null>,
): ProjectLoader {
  ensureWasm();
  return new wasm!.ProjectLoader(readFile, loadDirectory, resolvePath, (args) =>
    fetchGithub(args[0], args[1], args[2], args[3]),
  );
}

export function getCompilerWorker(): ICompilerWorker {
  return createProxy("../compiler/worker-node.js", compilerProtocol);
}

export function getDebugService(): IDebugService {
  ensureWasm();
  return new QSharpDebugService(wasm!);
}

export function getDebugServiceWorker(): IDebugServiceWorker {
  return createProxy("../debug-service/worker-node.js", debugServiceProtocol);
}

export function getLanguageService(
  loadProject?: (uri: string) => Promise<IProjectConfig | null>,
): ILanguageService {
  ensureWasm();
  return new QSharpLanguageService(wasm!, loadProject);
}

export function getLanguageServiceWorker(): ILanguageServiceWorker {
  return createProxy(
    "../language-service/worker-node.js",
    languageServiceProtocol,
  );
}

export * as utils from "./utils.js";
