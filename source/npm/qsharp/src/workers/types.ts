// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { TelemetryEvent } from "../log.js";
type Wasm = typeof import("../../lib/web/qsc_wasm.js");

/**
 * Describes a service that can be run in a worker.
 */
export interface ServiceProtocol<
  TService extends ServiceMethods<TService>,
  TServiceEventMsg extends IServiceEventMessage,
> {
  /** The concrete class that implements the service. */
  class: { new (wasmModule: Wasm): TService };
  /** Methods that can be proxied from the main thread to the worker. @see MethodMap*/
  methods: MethodMap<TService>;
  /** Events that can be received by the main thread from the worker. */
  eventNames: TServiceEventMsg["type"][];
}

/**
 * Used as a type constraint for a "service", i.e. an object
 * we can create proxy methods for. The type shouldn't define
 * any non-method properties.
 */
export type ServiceMethods<T> = { [x in keyof T]: (...args: any[]) => any };

/**
 * Defines the service methods that the proxy will handle and their types.
 *
 * "request" is a normal async method.
 *
 * "requestWithProgress" methods take an `IServiceEventTarget` to
 *   communicate events back to the main thread as they run. They also set
 *   the service state to "busy" while they run.
 *
 * "addEventListener" and "removeEventListener" methods are used to
 *   subscribe to events from the service.
 */
export type MethodMap<T> = {
  [M in keyof T]:
    | "request"
    | "requestWithProgress"
    | "addEventListener"
    | "removeEventListener";
};

/** Methods added to the service when wrapped in a proxy */
export type IServiceProxy = {
  onstatechange: ((state: ServiceState) => void) | null;
  terminate: () => void;
};

/** "requestWithProgress" type methods will set the service state to "busy" */
export type ServiceState = "idle" | "busy";

/** Request message from a main thread to the worker */
export type RequestMessage<T extends ServiceMethods<T>> = {
  [K in keyof T]: { type: K; args: Parameters<T[K]> };
}[keyof T];

/** Response message for a request from the worker to the main thread */
export type ResponseMessage<T extends ServiceMethods<T>> = {
  messageType: "response";
} & {
  [K in keyof T]: {
    type: K;
    result:
      | { success: true; result: Awaited<ReturnType<T[K]>> }
      | { success: false; error: unknown };
  };
}[keyof T];

/** Event message from the worker to the main thread */
export type EventMessage<TEventMsg extends IServiceEventMessage> = {
  messageType: "event";
} & TEventMsg;

/** Used as a constraint for events defined by the service */
export interface IServiceEventMessage {
  type: string;
  detail: unknown;
}

/**
 * Common event types all workers can send.
 */
export type CommonEvent =
  | { type: "telemetry-event"; detail: TelemetryEvent }
  | {
      type: "log";
      detail: { level: number; target: string; data: any[] };
    };
export type CommonEventMessage = CommonEvent & { messageType: "common-event" };

/**
 * Strongly typed EventTarget interface. Used as a constraint for the
 * event target that "requestWithProgress" methods should take in the service.
 */
export interface IServiceEventTarget<TEvents extends IServiceEventMessage> {
  addEventListener<T extends TEvents["type"]>(
    type: T,
    listener: (event: Event & Extract<TEvents, { type: T }>) => void,
  ): void;

  removeEventListener<T extends TEvents["type"]>(
    type: T,
    listener: (event: Event & Extract<TEvents, { type: T }>) => void,
  ): void;

  dispatchEvent(event: Event & TEvents): boolean;
}
