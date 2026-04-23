// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { IQSharpError } from "../../lib/web/qsc_wasm.js";
import type { CancellationToken } from "../cancellation.js";
import { QdkDiagnostics } from "../diagnostics.js";
import { log } from "../log.js";
import type { MainThreadWorkerAdapter } from "./adapters/types.js";
import type {
  CommonEventMessage,
  EventMessage,
  IServiceEventMessage,
  IServiceEventTarget,
  IServiceProxy,
  MethodMap,
  RequestMessage,
  ResponseMessage,
  ServiceMethods,
  ServiceProtocol,
  ServiceState,
} from "./types.js";

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
  workerArg: string | MainThreadWorkerAdapter,
  wasmModule: WebAssembly.Module,
  serviceProtocol: ServiceProtocol<TService, TServiceEventMsg>,
): TService & IServiceProxy {
  const worker =
    typeof workerArg === "string" ? new WorkerMain(workerArg) : workerArg;

  // Log any errors from the worker
  worker.onError((ev: Event) => {
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
  worker.onMessage((ev: MessageEvent) => {
    proxy.onMsgFromWorker(ev.data);
  });
  return proxy;
}

/** Holds state for a single request received by the proxy */
type RequestState<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
> = RequestMessage<TService> & {
  resolve: (val: any) => void;
  reject: (err: any) => void;
  requestEventTarget?: IServiceEventTarget<TServiceEventMsg>;
  cancellationToken?: CancellationToken;
};

/*
The WorkerProxy works by queuing up requests to send over to the Worker, only
ever having one in flight at a time. By queuing on the caller side, this allows
for cancellation (it checks if a request is cancelled before sending to the worker).

The queue contains an entry for each request with the data to send, the promise
to resolve, the event handler, and the cancellation token. When a request completes
the next one (if present) is fetched from the queue. If it is marked as cancelled,
it is resolved immediately, else it is marked as the current request and the command
sent to the worker. As events occurs on the current request the event handler is
invoked. When the response is received this is used to resolve the promise and
complete the request.
*/

/**
 * Function to create the proxy for a type. To be used from the main thread.
 *
 * @param postMessage A function to post messages to the worker
 * @param terminator A function to call to tear down the worker thread
 * @param methods A map of method names to be proxied and some metadata @see MethodMap
 * @returns The proxy object. The caller should then set the onMsgFromWorker
 * property to a callback that will receive messages from the worker.
 */
function createProxyInternal<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
>(
  postMessage: (msg: RequestMessage<TService>) => void,
  terminator: () => void,
  methods: MethodMap<TService>,
): TService &
  IServiceProxy & {
    onMsgFromWorker: (
      msg: ResponseMessage<TService> | EventMessage<TServiceEventMsg>,
    ) => void;
  } {
  const queue: RequestState<TService, TServiceEventMsg>[] = [];
  const eventTarget = new EventTarget();
  let curr: RequestState<TService, TServiceEventMsg> | undefined;
  let state: ServiceState = "idle";

  function setState(newState: ServiceState) {
    if (state === newState) return;
    state = newState;
    if (proxy.onstatechange) proxy.onstatechange(state);
  }

  type ResultOf<TRespMsg> = TRespMsg extends { result: infer R } ? R : never;

  function queueRequest(
    msg: RequestMessage<TService>,
    requestEventTarget?: IServiceEventTarget<TServiceEventMsg>,
    cancellationToken?: CancellationToken,
  ): Promise<ResultOf<ResponseMessage<TService>>> {
    return new Promise((resolve, reject) => {
      queue.push({
        type: msg.type,
        args: msg.args,
        resolve,
        reject,
        requestEventTarget,
        cancellationToken,
      } as RequestState<TService, TServiceEventMsg>);

      // If nothing was running when this got added, kick off processing
      if (queue.length === 1) doNextRequest();
    });
  }

  function doNextRequest() {
    if (curr) return;

    while ((curr = queue.shift())) {
      if (curr.cancellationToken?.isCancellationRequested) {
        curr.reject("cancelled");
        continue;
      } else {
        break;
      }
    }
    if (!curr) {
      // Nothing else queued, signal that we're now idle and exit.
      log.trace("Proxy: Worker queue is empty");
      setState("idle");
      return;
    }

    const msg = { type: curr.type, args: curr.args };
    if (methods[curr.type] === "requestWithProgress") {
      setState("busy");
    }

    log.trace("Proxy: Posting message to worker: %o", msg);
    postMessage(msg);
  }

  function onMsgFromWorker(
    msg:
      | ResponseMessage<TService>
      | EventMessage<TServiceEventMsg>
      | CommonEventMessage,
  ) {
    if (log.getLogLevel() >= 4)
      log.trace("Proxy: Received message from worker: %s", JSON.stringify(msg));

    if (msg.messageType === "common-event") {
      const commonEvent = msg; // assignment is necessary here for TypeScript to narrow the type
      switch (commonEvent.type) {
        case "telemetry-event":
          {
            const detail = commonEvent.detail;
            log.logTelemetry(detail);
          }
          break;
        case "log":
          {
            const detail = commonEvent.detail;
            log.logWithLevel(detail.level, detail.target, ...detail.data);
          }
          break;
      }
    } else if (msg.messageType === "event") {
      const event = new Event(msg.type) as Event & TServiceEventMsg;
      event.detail = msg.detail;

      log.trace("Proxy: Posting event: %o", msg);
      // Post to a currently attached event target if there's a "requestWithProgress"
      // in progress
      curr?.requestEventTarget?.dispatchEvent(event);
      // Also post to the general event target
      eventTarget.dispatchEvent(event);
    } else if (msg.messageType === "response") {
      if (!curr) {
        log.error("Proxy: No active request when message received: %o", msg);
        return;
      }
      const result = {
        success: msg.result.success,
        data: msg.result.success ? msg.result.result : msg.result.error,
      };
      if (result.success) {
        curr.resolve(result.data);
        curr = undefined;
        doNextRequest();
      } else {
        let err = result.data;

        // The error may be a serialized error object.
        err = deserializeIfError(err);

        curr.reject(err);
        curr = undefined;
        doNextRequest();
      }
    }
  }

  // Create the proxy object to be returned
  const proxy = {} as TService &
    IServiceProxy & { onMsgFromWorker: typeof onMsgFromWorker };

  // Assign each method with the desired proxying behavior
  for (const methodName of Object.keys(methods) as (keyof TService &
    string)[]) {
    // @ts-expect-error - tricky to derive the type of the actual method here
    proxy[methodName] = (...args: any[]) => {
      let requestEventTarget:
        | IServiceEventTarget<TServiceEventMsg>
        | undefined = undefined;

      switch (methods[methodName]) {
        case "addEventListener":
          {
            // @ts-expect-error - can't get the typing of the rest parameters quite right
            eventTarget.addEventListener(...args);
          }
          break;
        case "removeEventListener":
          {
            // @ts-expect-error - can't get the typing of the rest parameters quite right
            eventTarget.removeEventListener(...args);
          }
          break;
        case "requestWithProgress": {
          // For progress methods, the last argument is the event target
          requestEventTarget = args[args.length - 1];
          args = args.slice(0, args.length - 1);
        }
        // fallthrough
        case "request": {
          return queueRequest(
            { type: methodName, args } as RequestMessage<TService>,
            requestEventTarget,
          );
        }
      }
    };
  }

  proxy.onstatechange = null;
  proxy.terminate = () => {
    // Kill the worker without a chance to shutdown. May be needed if it is not responding.
    log.info("Proxy: Terminating the worker");
    if (curr) {
      log.trace(
        "Proxy: Terminating running worker item of type: %s",
        curr.type,
      );
      curr.reject("terminated");
    }
    // Reject any outstanding items
    while (queue.length) {
      const item = queue.shift();
      log.trace(
        "Proxy: Terminating outstanding work item of type: %s",
        item?.type,
      );
      item?.reject("terminated");
    }
    terminator();
  };
  proxy.onMsgFromWorker = onMsgFromWorker;

  return proxy;
}

/**
 * Deserializes an error if it is a known type.
 *
 * By default, browsers can only send certain types of errors between the main thread and a worker.
 * See: https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Structured_clone_algorithm#error_types
 *
 * Serializing our own custom errors allows us to send them between threads.
 */
function deserializeIfError(err: unknown) {
  if (err !== null && typeof err === "object" && "name" in err) {
    if (err.name === "QdkDiagnostics" && "data" in err) {
      err = new QdkDiagnostics(err.data as IQSharpError[]);
    } else if (
      err.name === "WebAssembly.RuntimeError" &&
      "message" in err &&
      (typeof err.message === "string" || typeof err.message === "undefined") &&
      "stack" in err &&
      (typeof err.stack === "string" || typeof err.stack === "undefined")
    ) {
      const newErr = new WebAssembly.RuntimeError(err.message);
      newErr.stack = err.stack;
      err = newErr;
    }
  }
  return err;
}
