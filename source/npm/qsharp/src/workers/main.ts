// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "../log.js";
import {
  IServiceEventMessage,
  IServiceProxy,
  ServiceMethods,
  ServiceProtocol,
  createProxyInternal,
} from "./common.js";

export const isBrowser = typeof Worker !== "undefined";

if (!isBrowser) {
  // In CJS (esbuild bundle), require is available directly.
  // In ESM (e.g. node --test), we use dynamic import.
  if (typeof require === "function") {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    globalThis.Worker = require("web-worker");
  } else {
    // Dynamic import for ESM - this is lazy, Worker will be available
    // by the time it's actually needed.
    import("web-worker").then((mod) => {
      globalThis.Worker = mod.default;
    });
  }
  log.debug(
    "Running in Node.js environment, using web-worker package for Worker support.",
  );
}
/**
 * Creates and initializes a service in a web worker, and returns a proxy for the service
 * to be used from the main thread.
 *
 * @param workerArg The service web worker or the URL of the web worker script.
 * @param wasmModule The wasm module to initialize the service with
 * @param serviceProtocol An object that describes the service: its constructor, methods and events
 * @returns A proxy object that implements the service interface.
 *   This interface can now be used as if calling into the real service,
 *   and the calls will be proxied to the web worker.
 */
export function createProxy<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  workerArg: string | Worker,
  wasmModule: WebAssembly.Module,
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
): TService & IServiceProxy {
  // Create or use the WebWorker
  const useModuleWorker: WorkerOptions = isBrowser
    ? { type: "classic" }
    : { type: "module" };

  const worker =
    typeof workerArg === "string"
      ? new Worker(workerArg, useModuleWorker)
      : workerArg;

  // Log any errors from the worker
  worker.addEventListener("error", (ev: Event) => {
    log.error("Worker error:", ev);
  });

  // Send it the Wasm module to instantiate
  worker.postMessage({
    type: "init",
    wasmModule,
    qscLogLevel: log.getLogLevel(),
  });

  // If you lose the 'this' binding, some environments have issues
  const postMessage = worker.postMessage.bind(worker);
  const onTerminate = () => worker.terminate();

  // Create the proxy which will forward method calls to the worker
  const proxy = createProxyInternal<TService, TServiceEventMsg>(
    postMessage,
    onTerminate,
    serviceProtocol.methods,
  );

  // Let proxy handle response and event messages from the worker
  worker.addEventListener("message", (ev: MessageEvent) => {
    proxy.onMsgFromWorker(ev.data);
  });
  return proxy;
}
