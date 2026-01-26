// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export type CircuitHelloWorkerRequest =
  | {
      command: "hello";
      reason?: string;
    }
  | {
      command: "ping";
    };

export type CircuitHelloWorkerResponse =
  | {
      command: "hello";
      message: string;
      reason?: string;
    }
  | {
      command: "pong";
    }
  | {
      command: "error";
      error: { name: string; message: string };
    };

const respondError = (err: unknown) => {
  const error =
    err instanceof Error
      ? { name: err.name, message: err.message }
      : { name: "Error", message: String(err) };
  (self as any).postMessage({
    command: "error",
    error,
  } satisfies CircuitHelloWorkerResponse);
};

(self as any).onmessage = (ev: MessageEvent<CircuitHelloWorkerRequest>) => {
  try {
    const msg = ev.data as any;
    if (!msg || typeof msg !== "object") return;

    switch (msg.command) {
      case "ping":
        (self as any).postMessage({
          command: "pong",
        } satisfies CircuitHelloWorkerResponse);
        return;

      case "hello": {
        const reason = typeof msg.reason === "string" ? msg.reason : undefined;
        const message = "Hello from the circuit webview worker.";
        const delayMs = 2000;
        setTimeout(() => {
          try {
            (self as any).postMessage({
              command: "hello",
              message,
              reason,
            } satisfies CircuitHelloWorkerResponse);
          } catch (err) {
            respondError(err);
          }
        }, delayMs);
        return;
      }
    }
  } catch (err) {
    respondError(err);
  }
};
