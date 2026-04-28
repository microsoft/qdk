// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { IWorkerHost } from "./types.js";

export class BrowserWorkerHost implements IWorkerHost {
  private worker: Worker;

  constructor(url: string | URL) {
    // Make sure the URL is absolute so that importScripts works inside the blob
    const scriptUrl =
      typeof url === "string" ? new URL(url, import.meta.url).href : url.href;
    const bootstrap = `
      self.WorkerSelf = {
        postMessage(msg) { self.postMessage(msg); },
        onMessage(handler) { self.addEventListener("message", handler); }
      };
      importScripts("${scriptUrl}");
    `;
    const blob = new Blob([bootstrap], { type: "application/javascript" });
    this.worker = new Worker(URL.createObjectURL(blob));
  }

  postMessage(msg: unknown): void {
    this.worker.postMessage(msg);
  }

  onMessage(handler: (e: MessageEvent) => void): void {
    this.worker.onmessage = handler;
  }

  onError(handler: (e: Event) => void): void {
    this.worker.onerror = handler;
  }

  terminate(): void {
    this.worker.terminate();
  }
}
