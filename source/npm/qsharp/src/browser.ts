// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This module is the entry point for browser environments. For Node.js environment,
// the "./main.js" module is the entry point.

import * as wasm from "../lib/web/qsc_wasm.js";
import initWasm, {
  IProjectHost,
  ProjectType,
  TargetProfile,
} from "../lib/web/qsc_wasm.js";
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
import { callAndTransformExceptions } from "./diagnostics.js";
import {
  ILanguageService,
  ILanguageServiceWorker,
  QSharpLanguageService,
  languageServiceProtocol,
  qsharpGithubUriScheme,
  qsharpLibraryUriScheme,
} from "./language-service/language-service.js";
import { LogLevel, log } from "./log.js";
import { ProjectLoader } from "./project.js";
import { createProxy } from "./workers/browser.js";

export { qsharpGithubUriScheme, qsharpLibraryUriScheme };

// Create once. A module is stateless and can be efficiently passed to WebWorkers.
let wasmModule: WebAssembly.Module | null = null;
let wasmModulePromise: Promise<void> | null = null;

// Used to track if an instance is already instantiated
let wasmInstancePromise: Promise<wasm.InitOutput> | null = null;

async function wasmLoader(uriOrBuffer: string | ArrayBuffer) {
  if (typeof uriOrBuffer === "string") {
    log.info("Fetching wasm module from %s", uriOrBuffer);
    performance.mark("fetch-wasm-start");
    const wasmRequst = await fetch(uriOrBuffer);
    const wasmBuffer = await wasmRequst.arrayBuffer();
    const fetchTiming = performance.measure("fetch-wasm", "fetch-wasm-start");
    log.logTelemetry({
      id: "fetch-wasm",
      data: {
        duration: fetchTiming.duration,
        uri: uriOrBuffer,
      },
    });

    wasmModule = await WebAssembly.compile(wasmBuffer);
  } else {
    log.info("Compiling wasm module from provided buffer");
    wasmModule = await WebAssembly.compile(uriOrBuffer);
  }
}

export function loadWasmModule(
  uriOrBuffer: string | ArrayBuffer,
): Promise<void> {
  // Only initiate if not already in flight, to avoid race conditions
  if (!wasmModulePromise) {
    wasmModulePromise = wasmLoader(uriOrBuffer);
  }
  return wasmModulePromise;
}

async function instantiateWasm() {
  // Ensure loading the module has been initiated, and wait for it.
  if (!wasmModulePromise) throw "Wasm module must be loaded first";
  await wasmModulePromise;
  if (!wasmModule) throw "Wasm module failed to load";

  if (wasmInstancePromise) {
    // Either in flight or already complete. The prior request will do the init,
    // so just wait on that.
    await wasmInstancePromise;
    return;
  }

  // Set the promise to signal this is in flight, then wait on the result.
  wasmInstancePromise = initWasm(wasmModule);
  await wasmInstancePromise;

  // Once ready, set up logging and telemetry as soon as possible after instantiating
  wasm.initLogging(log.logWithLevel, log.getLogLevel());
  log.onLevelChanged = (level) => wasm.setLogLevel(level);
}

export async function getLibrarySourceContent(
  path: string,
): Promise<string | undefined> {
  await instantiateWasm();
  return wasm.get_library_source_content(path);
}

export async function getDebugService(): Promise<IDebugService> {
  await instantiateWasm();
  return new QSharpDebugService(wasm);
}

export async function getProjectLoader(
  host: IProjectHost,
): Promise<ProjectLoader> {
  await instantiateWasm();
  return new ProjectLoader(wasm, host);
}

// Create the debugger inside a WebWorker and proxy requests.
// If the Worker was already created via other means and is ready to receive
// messages, then the worker may be passed in and it will be initialized.
export function getDebugServiceWorker(
  worker: string | Worker,
): IDebugServiceWorker {
  if (!wasmModule) throw "Wasm module must be loaded first";
  return createProxy(worker, wasmModule, debugServiceProtocol);
}

export async function getCompiler(): Promise<ICompiler> {
  await instantiateWasm();
  return new Compiler(wasm);
}

// Create the compiler inside a WebWorker and proxy requests.
// If the Worker was already created via other means and is ready to receive
// messages, then the worker may be passed in and it will be initialized.
export function getCompilerWorker(worker: string | Worker): ICompilerWorker {
  if (!wasmModule) throw "Wasm module must be loaded first";
  return createProxy(worker, wasmModule, compilerProtocol);
}

export async function getLanguageService(
  host?: IProjectHost,
): Promise<ILanguageService> {
  await instantiateWasm();
  return new QSharpLanguageService(wasm, host);
}

// Create the compiler inside a WebWorker and proxy requests.
// If the Worker was already created via other means and is ready to receive
// messages, then the worker may be passed in and it will be initialized.
export function getLanguageServiceWorker(
  worker: string | Worker,
): ILanguageServiceWorker {
  if (!wasmModule) throw "Wasm module must be loaded first";
  return createProxy(worker, wasmModule, languageServiceProtocol);
}

/// Extracts the target profile from a Q# source file's entry point.
/// Scans the provided source code for an EntryPoint argument specifying
/// a profile and returns the corresponding TargetProfile value, if found.
/// Returns undefined if no profile is specified or if the profile is not recognized.
export async function getTargetProfileFromEntryPoint(
  fileName: string,
  source: string,
): Promise<wasm.TargetProfile | undefined> {
  await instantiateWasm();
  return callAndTransformExceptions(
    async () =>
      wasm.get_target_profile_from_entry_point(fileName, source) as
        | wasm.TargetProfile
        | undefined,
  );
}

export { StepResultId } from "../lib/web/qsc_wasm.js";
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
export { type Dump, type ShotResult } from "./compiler/common.js";
export { type CompilerState, type ProgramConfig } from "./compiler/compiler.js";
export { QscEventTarget } from "./compiler/events.js";
export { QdkDiagnostics } from "./diagnostics.js";
export type {
  LanguageServiceDiagnosticEvent,
  LanguageServiceEvent,
  LanguageServiceTestCallablesEvent,
} from "./language-service/language-service.js";
export { default as openqasm_samples } from "./openqasm-samples.generated.js";
export type { ProjectLoader } from "./project.js";
export { default as samples } from "./samples.generated.js";
export type { CircuitGroup as CircuitData } from "./data-structures/circuit.js";
export * as utils from "./utils.js";
export { log, type LogLevel, type ProjectType, type TargetProfile };
export type {
  ICompiler,
  ICompilerWorker,
  IDebugService,
  IDebugServiceWorker,
  ILanguageService,
  ILanguageServiceWorker,
};
