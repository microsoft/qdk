// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export interface IWorkerHost {
  postMessage(msg: unknown): void;
  onMessage(handler: (e: MessageEvent) => void): void;
  onError(handler: (e: Event) => void): void;
  terminate(): void;
}

export interface IWorkerSelf {
  postMessage(msg: unknown): void;
  onMessage(handler: (e: MessageEvent) => void): void;
}

declare global {
  var WorkerHost: new (url: string | URL) => IWorkerHost;
  var WorkerSelf: IWorkerSelf;
}
