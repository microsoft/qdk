// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { Worker } from "node:worker_threads";
import type { IWorkerHost } from "./types.js";

export class NodeWorkerHost implements IWorkerHost {
  private worker: Worker;

  constructor(url: string | URL) {
    const workerUrl = typeof url === "string" ? new URL(url) : url;
    const bootstrap = `
      import { parentPort } from 'node:worker_threads';
      globalThis.WorkerSelf = {
        postMessage(msg) { parentPort.postMessage(msg); },
        onMessage(handler) {
          parentPort.on('message', (data) => handler({ data }));
        }
      };
      await import("${workerUrl.href}");
    `;
    this.worker = new Worker(
      new URL(`data:text/javascript,${encodeURIComponent(bootstrap)}`),
    );
  }

  postMessage(msg: unknown): void {
    this.worker.postMessage(msg);
  }

  onMessage(handler: (e: MessageEvent) => void): void {
    this.worker.on("message", (data) => {
      handler({ data } as MessageEvent);
    });
  }

  onError(handler: (e: Event) => void): void {
    this.worker.on("error", handler);
  }

  terminate(): void {
    this.worker.terminate();
  }
}
