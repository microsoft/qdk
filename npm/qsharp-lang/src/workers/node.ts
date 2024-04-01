// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  Worker,
  isMainThread,
  parentPort,
  workerData,
} from "node:worker_threads";
import * as wasm from "../../lib/node/qsc_wasm.cjs";
import { log } from "../log.js";
import {
  IServiceEventMessage,
  IServiceProxy,
  ServiceMethods,
  ServiceProtocol,
  createProxyInternal,
  initService,
} from "./common.js";

/**
 * Creates an initializes a service, setting it up to receive requests.
 * This function to be is used in the worker.
 *
 * @param serviceProtocol An object that describes the service: its constructor, methods and events
 */
export function createWorker<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(protocol: ServiceProtocol<TService, TServiceEventMsg>): void {
  if (isMainThread)
    throw "Worker script should be loaded in a Worker thread only";

  const port = parentPort!; // eslint-disable-line @typescript-eslint/no-non-null-assertion

  const postMessage = port.postMessage.bind(port);

  const invokeService = initService<TService, TServiceEventMsg>(
    postMessage,
    protocol,
    wasm as any, // Need to cast due to difference in web and node wasm types
    workerData && typeof workerData.qscLogLevel === "number"
      ? workerData.qscLogLevel
      : undefined,
  );

  function messageHandler(data: any) {
    if (!data.type || typeof data.type !== "string") {
      log.error(`Unrecognized msg: %O"`, data);
      return;
    }

    invokeService(data);
  }

  port.addListener("message", messageHandler);
}

/**
 * Creates and initializes a service in a worker thread, and returns a proxy for the service
 * to be used from the main thread.
 *
 * @param workerArg The the URL of the service web worker script.
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
  workerArg: string,
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
): TService & IServiceProxy {
  const thisDir = dirname(fileURLToPath(import.meta.url));
  const worker = new Worker(join(thisDir, workerArg), {
    workerData: { qscLogLevel: log.getLogLevel() },
  });

  // Create the proxy which will forward method calls to the worker
  const proxy = createProxyInternal<TService, TServiceEventMsg>(
    // If you lose the 'this' binding, some environments have issues.
    worker.postMessage.bind(worker),
    () => worker.terminate(),
    serviceProtocol.methods,
  );

  // Let proxy handle response and event messages from the worker
  worker.addListener("message", proxy.onMsgFromWorker);

  return proxy;
}
