// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as wasm from "../../lib/web/qsc_wasm.js";
import { QdkDiagnostics } from "../diagnostics.js";
import { TelemetryEvent, log } from "../log.js";
import type {
  CommonEventMessage,
  EventMessage,
  IServiceEventMessage,
  IServiceEventTarget,
  MethodMap,
  RequestMessage,
  ResponseMessage,
  ServiceMethods,
  ServiceProtocol,
} from "./types.js";

type Wasm = typeof wasm;

/**
 * Creates and initializes a service, setting it up to receive requests.
 * This function is used in the worker thread. It uses the `WorkerSelf` global,
 * which is bootstrapped by the platform-specific adapter before this code runs.
 *
 * @param serviceProtocol An object that describes the service: its constructor, methods and events
 * @returns The message handler registered on the worker thread.
 */
export function createWorker<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
): (e: MessageEvent) => void {
  let invokeService: ((req: RequestMessage<TService>) => Promise<void>) | null =
    null;

  const messageHandler = (e: MessageEvent) => {
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
            WorkerSelf.postMessage.bind(WorkerSelf),
            serviceProtocol,
            wasm,
            data.qscLogLevel,
          );
        }
        break;
      case "set-log-level":
        log.setLogLevel(data.level);
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

  WorkerSelf.onMessage(messageHandler);
  return messageHandler;
}

/**
 * Serializes an error, if it is a known type, so that it can be sent between threads.
 *
 * By default, browsers can only send certain types of errors between the main thread and a worker.
 * See: https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Structured_clone_algorithm#error_types
 *
 * Serializing our own custom errors allows us to send them between threads.
 */
function serializeIfError(err: unknown) {
  if (err instanceof QdkDiagnostics) {
    err = { name: err.name, data: err.diagnostics };
  } else if (err instanceof WebAssembly.RuntimeError) {
    err = {
      name: "WebAssembly.RuntimeError",
      message: err.message,
      stack: err.stack,
    };
  }
  return err;
}

/**
 * Function to wrap a service in a dispatcher. To be used in the worker thread.
 *
 * @param service The service to be wrapped
 * @param methods A map of method names. Should match the list passed into @see createProxyInternal.
 * @param eventNames The list of event names that the service can emit
 * @param postMessage A function to post messages back to the main thread
 * @returns A function that takes a message and invokes the corresponding
 * method on the service. The caller should then set this method as a message handler.
 */
function createDispatcher<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  postMessage: (
    msg: ResponseMessage<TService> | EventMessage<TServiceEventMsg>,
  ) => void,
  service: TService,
  methods: MethodMap<TService>,
  eventNames: TServiceEventMsg["type"][],
): (req: RequestMessage<TService>) => Promise<void> {
  log.trace("Worker: Constructing WorkerEventHandler");

  function logAndPost(
    msg: ResponseMessage<TService> | EventMessage<TServiceEventMsg>,
  ) {
    log.trace(
      "Worker: Sending %s message from worker: %o",
      msg.messageType,
      msg,
    );
    postMessage(msg);
  }

  const eventTarget =
    new EventTarget() as IServiceEventTarget<TServiceEventMsg>;

  eventNames.forEach((eventName: TServiceEventMsg["type"]) => {
    // Subscribe to all known events and forward them as messages to the main thread.
    eventTarget.addEventListener(eventName, (ev) => {
      logAndPost({
        messageType: "event",
        type: ev.type,
        detail: ev.detail,
      });
    });

    // If there's an addEventListener on the object itself, forward those events as well.
    if ((service as any).addEventListener) {
      (service as any).addEventListener(eventName, (ev: any) => {
        logAndPost({
          messageType: "event",
          type: ev.type,
          detail: ev.detail,
        });
      });
    }
  });

  return function invokeMethod(req: RequestMessage<TService>) {
    // Pass the eventTarget to the methods marked as taking progress
    return service[req.type]
      .call(
        service,
        ...req.args,
        methods[req.type] === "requestWithProgress" ? eventTarget : undefined,
      )
      .then((result: any) =>
        logAndPost({
          messageType: "response",
          type: req.type,
          result: { success: true, result },
        }),
      )
      .catch((err: any) => {
        // Serialize the error if it's a known type.
        err = serializeIfError(err);

        logAndPost({
          // If this happens then the wasm code likely threw an exception/panicked rather than
          // completing gracefully and fulfilling the promise. Communicate to the client
          // that there was an error and it should reject the current request
          messageType: "response",
          type: req.type,
          result: { success: false, error: err },
        });
      });
  };
}

/**
 * Creates and initializes the actual service. To be used in the worker thread.
 *
 * @param postMessage A function to post messages back to the main thread
 * @param serviceProtocol An object that describes the service: its constructor, methods and events
 * @param wasm The wasm module to initialize the service with
 * @param qscLogLevel The log level to initialize the service with
 * @returns A function that takes a message and invokes the corresponding
 * method on the service. The caller should then set this method as a message handler.
 */
function initService<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  postMessage: (
    msg:
      | ResponseMessage<TService>
      | EventMessage<TServiceEventMsg>
      | CommonEventMessage,
  ) => void,
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
  wasm: Wasm,
  qscLogLevel?: number,
): (req: RequestMessage<TService>) => Promise<void> {
  function postTelemetryMessage(telemetry: TelemetryEvent) {
    postMessage({
      messageType: "common-event",
      type: "telemetry-event",
      detail: telemetry,
    });
  }

  function postLogMessage(level: number, target: string, ...args: any) {
    if (log.getLogLevel() < level) {
      return;
    }

    let data = args;
    try {
      // Only structured cloneable objects can be sent in worker messages.
      // Test if this is the case.
      structuredClone(args);
    } catch {
      // Uncloneable object.
      // Use String(args) instead of ${args} to handle all possible values
      // without throwing. See: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String#string_coercion
      data = ["unsupported log data " + String(args)];
    }
    postMessage({
      messageType: "common-event",
      type: "log",
      detail: { level, target, data },
    });
  }

  // Override the global logger
  log.error = (...args) => postLogMessage(1, "worker", ...args);
  log.warn = (...args) => postLogMessage(2, "worker", ...args);
  log.info = (...args) => postLogMessage(3, "worker", ...args);
  log.debug = (...args) => postLogMessage(4, "worker", ...args);
  log.trace = (...args) => postLogMessage(5, "worker", ...args);

  if (qscLogLevel !== undefined) {
    log.setLogLevel(qscLogLevel);
  }

  // Set up logging and telemetry as soon as possible after instantiating
  log.addLevelChangedListener((level) => wasm.setLogLevel(level));
  log.setTelemetryCollector(postTelemetryMessage);
  wasm.initLogging(postLogMessage, log.getLogLevel());

  // Create the actual service and return the dispatcher method
  const service = new serviceProtocol.class(wasm);
  return createDispatcher(
    postMessage,
    service,
    serviceProtocol.methods,
    serviceProtocol.eventNames,
  );
}
