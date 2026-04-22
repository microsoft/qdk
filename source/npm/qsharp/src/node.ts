// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Node.js entrypoint. Polyfills the Worker global before loading the main module.

import { Worker as NodeWorker } from "node:worker_threads";

const bootstrapCode = `
import { parentPort, workerData } from "node:worker_threads";
const et = new EventTarget();
const g = globalThis;
g.self = globalThis;
g.addEventListener = et.addEventListener.bind(et);
g.removeEventListener = et.removeEventListener.bind(et);
g.dispatchEvent = et.dispatchEvent.bind(et);
g.postMessage = (msg, transfer) => parentPort.postMessage(msg, transfer);
await import(workerData.scriptUrl);
parentPort.on("message", (data) =>
  et.dispatchEvent(new MessageEvent("message", { data }))
);
`;
const bootstrapUrl = new URL(
  `data:text/javascript;base64,${Buffer.from(bootstrapCode).toString("base64")}`,
);

class Worker extends EventTarget {
  private worker: NodeWorker;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onmessageerror: ((ev: MessageEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  constructor(url: URL | string) {
    super();
    const workerUrl = typeof url === "string" ? new URL(url) : url;
    this.worker = new NodeWorker(bootstrapUrl, {
      workerData: { scriptUrl: workerUrl.href },
    } as import("node:worker_threads").WorkerOptions);

    this.worker.on("message", (data) => {
      this.dispatchEvent(new MessageEvent("message", { data }));
    });

    this.worker.on("error", (data) => {
      console.log("WORKER: received error event from worker:", data);
      this.dispatchEvent(new MessageEvent("error", { data }));
    });
  }
  // biome-ignore lint/suspicious/noExplicitAny: <explanation>
  postMessage(data: any) {
    this.worker.postMessage(data);
  }

  terminate() {
    this.worker.terminate();
  }
}

if (typeof globalThis.Worker === "undefined") {
  globalThis.Worker = Worker;
}

import { setWorkerType } from "./main.js";
setWorkerType("module");

export * from "./main.js";
