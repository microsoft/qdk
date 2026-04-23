export interface MainThreadWorkerAdapter {
  postMessage(msg: unknown): void;
  onMessage(handler: (e: MessageEvent) => void): void;
  onError(handler: (e: ErrorEvent) => void): void;
  terminate(): void;
}

export interface WorkerThreadAdapter {
  postMessage(msg: unknown): void;
  onMessage(handler: (e: MessageEvent) => void): void;
}

declare global {
  var WorkerMain: new (url: string | URL) => MainThreadWorkerAdapter;
  var WorkerSelf: WorkerThreadAdapter;
}
