// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

/**
 * URI scheme used for read-only Q# previews of circuit files.
 *
 * Documents under this scheme are served by `CircuitPreviewProvider` and
 * appear to VS Code as regular Q# documents, so they automatically pick up
 * Q# syntax highlighting, the language service, the editor theme, etc.
 *
 * The corresponding circuit document's URI is carried in the preview URI's
 * `query`, which keeps the preview URI stable across edits without baking
 * the (possibly long, possibly platform-specific) source path into the
 * preview's display path.
 */
export const qsharpCircuitPreviewScheme = "qsharp-circuit-preview";

/**
 * Build the deterministic preview URI for a given circuit document.
 *
 * The path is purely cosmetic (it controls the editor tab label and is what
 * the language service sees as the document name). The query carries the
 * full original circuit URI so the provider can map back when needed.
 */
export function circuitPreviewUriFor(circuitUri: vscode.Uri): vscode.Uri {
  // Use the basename for the display path, with a `.qs` suffix so VS Code
  // selects the Q# language for the editor.
  const basename = circuitUri.path.split(/[\\/]/).pop() ?? "circuit";
  return vscode.Uri.from({
    scheme: qsharpCircuitPreviewScheme,
    // Leading slash so the URI parses cleanly across platforms.
    path: `/${basename}.qs`,
    query: circuitUri.toString(),
  });
}

/**
 * Read-only content provider that serves Q# code generated from a circuit.
 *
 * The provider does not compute the Q# itself; callers (the circuit editor,
 * primarily) push the latest generated code in via `setContent`. This keeps
 * the provider free of any compiler / wasm dependency and lets the circuit
 * editor own debouncing, error handling, and lifecycle of the preview.
 *
 * Content is keyed by the preview URI (as a string), not the source circuit
 * URI, so two circuits with the same basename in different folders never
 * collide even though their tab labels are identical.
 */
export class CircuitPreviewProvider
  implements vscode.TextDocumentContentProvider, vscode.Disposable
{
  private readonly _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  private readonly _content = new Map<string, string>();

  readonly onDidChange = this._onDidChange.event;

  /**
   * Update the cached Q# content for a preview URI.
   *
   * Fires `onDidChange` so any open editor showing the preview re-fetches
   * via `provideTextDocumentContent`.
   */
  setContent(uri: vscode.Uri, content: string): void {
    this._content.set(uri.toString(), content);
    this._onDidChange.fire(uri);
  }

  /**
   * Drop cached content for a preview URI. Subsequent fetches will fall
   * back to the placeholder text.
   */
  clearContent(uri: vscode.Uri): void {
    if (this._content.delete(uri.toString())) {
      this._onDidChange.fire(uri);
    }
  }

  provideTextDocumentContent(uri: vscode.Uri): string {
    const cached = this._content.get(uri.toString());
    if (cached !== undefined) return cached;
    // Placeholder shown before the circuit editor has produced any content
    // (e.g. immediately after the preview is opened). Q# comments so syntax
    // highlighting still renders sensibly.
    return "// Q# preview will appear here as you edit the circuit.\n";
  }

  dispose(): void {
    this._content.clear();
    this._onDidChange.dispose();
  }
}

/**
 * Singleton instance of the preview provider.
 *
 * Created and registered with VS Code from `extension.ts` activation.
 * Held here so other modules (notably `CircuitEditorProvider`) can push
 * generated Q# into it without having to thread the provider through
 * many layers of constructors.
 */
let _provider: CircuitPreviewProvider | undefined;

/**
 * Register the circuit preview content provider with VS Code.
 *
 * Returns a Disposable suitable for `context.subscriptions.push(...)`.
 * Calling this more than once is a programming error.
 */
export function registerCircuitPreviewProvider(): vscode.Disposable {
  if (_provider !== undefined) {
    throw new Error("Circuit preview provider has already been registered.");
  }
  _provider = new CircuitPreviewProvider();
  const registration = vscode.workspace.registerTextDocumentContentProvider(
    qsharpCircuitPreviewScheme,
    _provider,
  );
  return vscode.Disposable.from(registration, _provider, {
    dispose: () => {
      _provider = undefined;
    },
  });
}

/**
 * Get the registered preview provider, if any.
 *
 * Returns `undefined` before activation has registered it (or after
 * deactivation), in which case callers should silently skip preview
 * updates rather than fail.
 */
export function getCircuitPreviewProvider():
  | CircuitPreviewProvider
  | undefined {
  return _provider;
}
