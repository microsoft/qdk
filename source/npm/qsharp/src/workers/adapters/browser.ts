// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { IWorkerHost } from "./types.js";

export class BrowserWorkerHost implements IWorkerHost {
  private worker: Worker;

  constructor(url: string | URL) {
    // Resolve to an absolute URL because importScripts inside a blob worker
    // cannot resolve relative URLs (there is no base URL to resolve against).
    // Note: import.meta.url is replaced with document.URL by the bundler.
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
