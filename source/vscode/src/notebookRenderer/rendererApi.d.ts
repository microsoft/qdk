// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

declare module "vscode-notebook-renderer" {
  export interface RendererContext<TState> {
    readonly workspaceState: TState;
    postMessage?(message: unknown): void | PromiseLike<boolean>;
  }

  export interface OutputItem {
    readonly id: string;
    readonly mime: string;
    readonly data: Uint8Array;
    readonly metadata?: Record<string, unknown>;
    json(): unknown;
    text(): string;
    blob(): Blob;
  }

  export type ActivationFunction<TState> = (
    context: RendererContext<TState>,
  ) => {
    renderOutputItem(
      outputItem: OutputItem,
      element: HTMLElement,
    ): void | Promise<void>;
    /** Called with no id when the host clears every output in the document. */
    disposeOutputItem(id?: string): void;
  };
}
