// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as wasm from "../../lib/web/qsc_wasm.js";
import { log } from "../log.js";
import {
  IServiceEventMessage,
  RequestMessage,
  ServiceMethods,
  ServiceProtocol,
  initService,
} from "./common.js";

/**
 * Creates an initializes a service, setting it up to receive requests.
 * This function to be is used in the worker.
 *
 * @param serviceProtocol An object that describes the service: its constructor, methods and events
 * @returns A message handler to be assigned to the `self.onmessage` handler in a web worker
 */
export function createWorker<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
): (e: MessageEvent) => void {
  let invokeService: ((req: RequestMessage<TService>) => Promise<void>) | null =
    null;

  // This export should be assigned to 'self.onmessage' in a WebWorker
  return function messageHandler(e: MessageEvent) {
    const data = e.data;

    if (!data.type || typeof data.type !== "string") {
      log.error(`Unrecognized msg: ${data}`);
      return;
    }

    switch (data.type) {
      case "init":
        {
          wasm.initSync({ module: data.wasmModule });

          invokeService = initService<TService, TServiceEventMsg>(
            self.postMessage.bind(self),
            serviceProtocol,
            wasm,
            data.qscLogLevel,
          );
        }
        break;
      default:
        if (!invokeService) {
          log.error(
            `Received message before the service was initialized: %o`,
            data,
          );
        } else {
          invokeService(data);
        }
    }
  };
}
