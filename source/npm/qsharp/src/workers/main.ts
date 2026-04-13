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
import Worker from "web-worker";

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
  const worker =
    typeof workerArg === "string"
      ? new Worker(workerArg, { type: "module" })
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
