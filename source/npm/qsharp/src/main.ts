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
  qsharpGithubUriScheme,
  qsharpLibraryUriScheme,
} from "./language-service/language-service.js";
import { log } from "./log.js";
import { createProxy } from "./workers/node.js";
import { ProjectLoader } from "./project.js";
import { callAndTransformExceptions, QdkDiagnostics } from "./diagnostics.js";
import type { IProjectHost } from "./browser.js";

export { qsharpGithubUriScheme, qsharpLibraryUriScheme };
export { QdkDiagnostics };
export { QscEventTarget } from "./compiler/events.js";
export { default as samples } from "./samples.generated.js";
export { default as openqasm_samples } from "./openqasm-samples.generated.js";
export { log };
export type { LogLevel } from "./log.js";

// Node.js loads WASM lazily via require(), so loadWasmModule is a no-op.
export function loadWasmModule(
  uriOrBuffer?: string | ArrayBuffer,
): Promise<void> {
  void uriOrBuffer;
  return Promise.resolve();
}

// StepResultId enum values matching the WASM-generated enum.
// Defined inline to avoid eagerly loading the WASM binary on module import.
export enum StepResultId {
  BreakpointHit = 0,
  Next = 1,
  StepIn = 2,
  StepOut = 3,
  Return = 4,
  Fail = 5,
}

// Only load the Wasm module when first needed, as it may only be used in a Worker,
// and not in the main thread.

// Use the types from the web version for... reasons.
type Wasm = typeof import("../lib/web/qsc_wasm.js");
let wasm: Wasm | null = null;

function ensureWasm() {
  if (!wasm) {
    wasm = require("../lib/nodejs/qsc_wasm.cjs") as Wasm;
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

export function getCompiler(): ICompiler {
  ensureWasm();
  return new Compiler(wasm!);
}

export function getProjectLoader(host: IProjectHost): ProjectLoader {
  ensureWasm();
  return new ProjectLoader(wasm!, host);
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

export function getLanguageService(host?: IProjectHost): ILanguageService {
  ensureWasm();
  return new QSharpLanguageService(wasm!, host);
}

export function getLanguageServiceWorker(): ILanguageServiceWorker {
  return createProxy(
    "../language-service/worker-node.js",
    languageServiceProtocol,
  );
}

export async function getTargetProfileFromEntryPoint(
  fileName: string,
  source: string,
): Promise<import("../lib/web/qsc_wasm.js").TargetProfile | undefined> {
  ensureWasm();
  return callAndTransformExceptions(
    async () =>
      wasm!.get_target_profile_from_entry_point(fileName, source) as
        | import("../lib/web/qsc_wasm.js").TargetProfile
        | undefined,
  );
}

export * as utils from "./utils.js";

// Type re-exports to match browser.ts API surface
export type {
  IBreakpointSpan,
  ICodeAction,
  ICodeLens,
  IDocFile,
  ILocation,
  IOperationInfo,
  IPosition,
  IProjectConfig,
  IProjectHost,
  IQSharpError,
  IRange,
  IStackFrame,
  IStructStepResult,
  ITestDescriptor,
  IVariable,
  IVariableChild,
  IWorkspaceEdit,
  VSDiagnostic,
} from "../lib/web/qsc_wasm.js";
export type { Dump, ShotResult } from "./compiler/common.js";
export type { CompilerState, ProgramConfig } from "./compiler/compiler.js";
export type {
  LanguageServiceDiagnosticEvent,
  LanguageServiceEvent,
  LanguageServiceTestCallablesEvent,
} from "./language-service/language-service.js";
export type { ProjectLoader } from "./project.js";
export type { CircuitGroup as CircuitData } from "./data-structures/circuit.js";
export type {
  ICompiler,
  ICompilerWorker,
  IDebugService,
  IDebugServiceWorker,
  ILanguageService,
  ILanguageServiceWorker,
};
export type { TargetProfile, ProjectType } from "../lib/web/qsc_wasm.js";
