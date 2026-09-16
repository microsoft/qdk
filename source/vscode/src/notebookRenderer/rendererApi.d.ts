// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * The slice of VS Code's notebook renderer API this renderer calls.
 *
 * Deliberately narrower than the published `@types/vscode-notebook-renderer`.
 * Declaring members we never call would make this a second, unowned copy of an
 * API nobody here maintains, and a wrong declaration of something unused is
 * worse than no declaration because it reads as fact. Widen it by adding the
 * member you need, checked against the published types.
 */
declare module "vscode-notebook-renderer" {
  export interface RendererContext {
    /** Absent when no extension host is listening, as in an HTML export. */
    postMessage?(message: unknown): void | PromiseLike<boolean>;
  }

  export interface OutputItem {
    readonly id: string;
    readonly mime: string;
    json(): unknown;
  }

  export type ActivationFunction = (context: RendererContext) => {
    renderOutputItem(
      outputItem: OutputItem,
      element: HTMLElement,
    ): void | Promise<void>;
    /** Called with no id when the host clears every output in the document. */
    disposeOutputItem(id?: string): void;
  };
}
