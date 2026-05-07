// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { ICompilerWorker } from "qsharp-lang";
import * as vscode from "vscode";
import {
  circuitPreviewUriFor,
  getCircuitPreviewProvider,
} from "./circuitPreview";
import { loadCompilerWorker } from "./common";
import { runProgramInTerminal } from "./run";

/**
 * Debounce window between circuit edits and recomputing the Q# preview.
 *
 * Short enough that the preview feels live during typical interactions
 * (drag, drop, parameter edits), long enough to coalesce the rapid bursts
 * of edits that happen during a drag.
 */
const PREVIEW_DEBOUNCE_MS = 200;

/**
 * Settings key for the auto-open preview behaviour. Read on editor open and
 * watched for changes so the user can flip it without restarting.
 */
const PREVIEW_SETTING_SECTION = "Q#";
const PREVIEW_SETTING_KEY = "circuits.showCodePreview";

function previewAutoOpenEnabled(): boolean {
  return vscode.workspace
    .getConfiguration(PREVIEW_SETTING_SECTION)
    .get<boolean>(PREVIEW_SETTING_KEY, true);
}

/**
 * Per-open-circuit hooks exposed to the showCircuitCodePreview command so it
 * can request the preview for whichever circuit is currently active without
 * the command needing to know about the editor's internal plumbing.
 */
interface CircuitPreviewController {
  show: () => Promise<void>;
}

const previewControllers = new Map<string, CircuitPreviewController>();

/**
 * Look up the controller for a circuit document URI, if one is currently open.
 * Used by the showCircuitCodePreview command.
 */
export function getCircuitPreviewController(
  circuitUri: vscode.Uri,
): CircuitPreviewController | undefined {
  return previewControllers.get(circuitUri.toString());
}

export class CircuitEditorProvider implements vscode.CustomTextEditorProvider {
  private static readonly viewType = "qsharp-webview.circuit";
  updatingDocument: boolean = false;

  public static register(context: vscode.ExtensionContext): vscode.Disposable {
    const provider = new CircuitEditorProvider(context);
    const providerRegistration = vscode.window.registerCustomEditorProvider(
      CircuitEditorProvider.viewType,
      provider,
      { webviewOptions: { retainContextWhenHidden: true } },
    );
    return providerRegistration;
  }

  constructor(private readonly context: vscode.ExtensionContext) {}

  public async resolveCustomTextEditor(
    document: vscode.TextDocument,
    webviewPanel: vscode.WebviewPanel,
  ): Promise<void> {
    // Setup initial content for the webview
    webviewPanel.webview.options = {
      enableScripts: true,
    };
    webviewPanel.webview.html = this.getHtmlForWebview(webviewPanel.webview);

    // Per-editor preview state. Held in closure variables (rather than on
    // `this`) because a single CircuitEditorProvider services all open
    // circuit editors and would otherwise need a Map keyed by document URI.
    const previewUri = circuitPreviewUriFor(document.uri);
    let previewWorker: ICompilerWorker | undefined;
    let previewTimer: ReturnType<typeof setTimeout> | undefined;
    // Monotonically increasing request ID so out-of-order async results from
    // the compiler worker can be discarded (latest edit wins).
    let previewRequestId = 0;
    let lastAppliedRequestId = -1;
    // Cache of the last content we pushed to the preview provider. Used to
    // suppress redundant updates (notably during error storms when every
    // keystroke produces the same "invalid JSON" comment).
    let lastAppliedContent: string | undefined;

    const getPreviewWorker = (): ICompilerWorker => {
      if (!previewWorker) {
        previewWorker = loadCompilerWorker(this.context.extensionUri);
      }
      return previewWorker;
    };

    const applyPreviewContent = (content: string, requestId: number) => {
      if (requestId <= lastAppliedRequestId) return;
      lastAppliedRequestId = requestId;
      // Avoid no-op updates: TextDocumentContentProvider.onDidChange would
      // otherwise force VS Code to re-tokenize and refresh the editor for
      // identical content (common when the same JSON-parse error repeats
      // on every keystroke).
      if (content === lastAppliedContent) return;
      lastAppliedContent = content;
      getCircuitPreviewProvider()?.setContent(previewUri, content);
    };

    /**
     * Recompute the Q# preview for the current document content and push it
     * to the preview provider. Errors are surfaced inline as Q# comments so
     * the preview surface remains a valid Q# document at all times.
     */
    const refreshPreviewNow = async () => {
      const provider = getCircuitPreviewProvider();
      if (!provider) return;

      const requestId = ++previewRequestId;
      const text = document.getText();
      const operationName = previewOperationNameFor(document.uri);

      if (text.trim().length === 0) {
        applyPreviewContent(
          `// Q# preview — empty circuit\n// Add gates to ${operationName} to generate Q#.\n`,
          requestId,
        );
        return;
      }

      try {
        // Validate the JSON shape on the host first so an unparseable
        // file produces a friendly comment rather than a wasm panic.
        JSON.parse(text);
      } catch (err: any) {
        applyPreviewContent(
          previewErrorComment(
            "invalid JSON",
            `Circuit file is not valid JSON: ${err?.message ?? err}`,
          ),
          requestId,
        );
        return;
      }

      let qsharp: string;
      try {
        const worker = getPreviewWorker();
        const circuits = JSON.parse(text);
        qsharp = await worker.circuitsToQsharp(operationName, circuits);
      } catch (err: any) {
        applyPreviewContent(
          previewErrorComment(
            "generation failed",
            `Could not generate Q#: ${err?.message ?? err}`,
          ),
          requestId,
        );
        return;
      }

      applyPreviewContent(qsharp, requestId);
    };

    const schedulePreviewRefresh = () => {
      if (previewTimer) clearTimeout(previewTimer);
      previewTimer = setTimeout(() => {
        previewTimer = undefined;
        // Fire-and-forget; errors are already surfaced into the preview text.
        void refreshPreviewNow();
      }, PREVIEW_DEBOUNCE_MS);
    };

    /**
     * Force the preview to be visible and current. Used by the
     * showCircuitCodePreview command and by the config-change listener when
     * the user flips the auto-open setting on while a circuit is open.
     */
    const showPreview = async () => {
      await refreshPreviewNow();
      await openPreviewBeside(previewUri);
    };

    // Make this editor's preview reachable from the showCircuitCodePreview
    // command. Keyed by the original circuit URI (not the preview URI).
    previewControllers.set(document.uri.toString(), { show: showPreview });

    webviewPanel.webview.onDidReceiveMessage(async (e) => {
      switch (e.command) {
        case "update":
          this.updateTextDocument(document, e.text);
          return;
        case "read":
          updateWebview();
          return;
        case "run": {
          const entry = await generateQubitCircuitExpression(document.uri);
          runProgramInTerminal(
            this.context.extensionUri,
            document.uri,
            "QDK: Run Circuit File",
            entry,
          );
          return;
        }
      }
    });

    const updateWebview = () => {
      const result = this.getDocumentAsJson(document);
      const filename = document.fileName.split(/\\|\//).pop()!.split(".")[0];

      if (result.error) {
        const message = {
          command: "error",
          props: {
            title: `${filename}`,
            message: result.error,
          },
        };
        webviewPanel.webview.postMessage(message);
        return;
      }

      const circuit = result.data;

      const props = {
        title: `${filename}`,
        targetProfile: "",
        simulated: false,
        calculating: false,
        circuit,
      };

      const message = {
        command: "circuit",
        props,
      };
      webviewPanel.webview.postMessage(message);
    };

    // Update the webview when the text document changes
    const changeDocumentSubscription = vscode.workspace.onDidChangeTextDocument(
      (event) => {
        if (event.document.uri.toString() === document.uri.toString()) {
          if (!this.updatingDocument && event.contentChanges.length > 0) {
            // Update the webview with the new document content
            updateWebview();
          }
          // Refresh the preview for any change (including ones initiated by
          // the webview itself), so external edits and webview edits stay
          // in sync with the side-by-side Q# preview.
          if (event.contentChanges.length > 0) {
            schedulePreviewRefresh();
          }
        }
      },
    );

    // React to the user toggling the auto-open setting on. Toggling off
    // intentionally leaves any already-open preview tab in place — the user
    // can close it manually — but stops further auto-opens for new circuits.
    let lastAutoOpenEnabled = previewAutoOpenEnabled();
    const configSubscription = vscode.workspace.onDidChangeConfiguration(
      (e) => {
        if (
          !e.affectsConfiguration(
            `${PREVIEW_SETTING_SECTION}.${PREVIEW_SETTING_KEY}`,
          )
        ) {
          return;
        }
        const enabled = previewAutoOpenEnabled();
        if (enabled && !lastAutoOpenEnabled) {
          void showPreview();
        }
        lastAutoOpenEnabled = enabled;
      },
    );

    // Dispose of the event listener when the webview is closed
    webviewPanel.onDidDispose(() => {
      changeDocumentSubscription.dispose();
      configSubscription.dispose();
      previewControllers.delete(document.uri.toString());
      if (previewTimer) {
        clearTimeout(previewTimer);
        previewTimer = undefined;
      }
      if (previewWorker) {
        previewWorker.terminate();
        previewWorker = undefined;
      }
      // Drop cached preview content for this document so a subsequent open
      // doesn't briefly show stale Q# before the first refresh completes.
      getCircuitPreviewProvider()?.clearContent(previewUri);
    });

    // Generate the initial preview content unconditionally so the command
    // (or a later setting flip) gets an instant response. Only auto-open
    // the side panel if the setting allows it.
    void refreshPreviewNow();
    if (lastAutoOpenEnabled) {
      void openPreviewBeside(previewUri);
    }
  }

  private getHtmlForWebview(webview: vscode.Webview): string {
    const extensionUri = this.context.extensionUri;

    function getUri(pathList: string[]) {
      return webview.asWebviewUri(
        vscode.Uri.joinPath(extensionUri, ...pathList),
      );
    }

    const katexCss = getUri(["out", "katex", "katex.min.css"]);
    const githubCss = getUri(["out", "katex", "github-markdown-dark.css"]);
    const webviewCss = getUri(["out", "webview", "webview.css"]);
    const scriptUri = getUri(["out", "webview", "editor.js"]);
    const resourcesUri = getUri(["resources"]);
    return `
      <!DOCTYPE html>
      <html lang="en">
        <head>
          <meta charset="UTF-8">
          <meta name="viewport" content="width=device-width, initial-scale=1.0">
          <title>Q#</title>
          <link rel="stylesheet" href="${githubCss}" />
          <link rel="stylesheet" href="${katexCss}" />
          <link rel="stylesheet" href="${webviewCss}" />
          <script src="${scriptUri}"></script>
          <script>
            window.resourcesUri = "${resourcesUri.toString()}";
          </script>
        </head>
        <body>
        </body>
      </html>`;
  }

  private getDocumentAsJson(document: vscode.TextDocument): {
    error?: string;
    data?: any;
  } {
    const text = document.getText();
    if (text.trim().length === 0) {
      return { data: {} };
    }

    try {
      return { data: JSON.parse(text) };
    } catch {
      return { error: "Content is not valid JSON" };
    }
  }

  private async updateTextDocument(
    document: vscode.TextDocument,
    circuit: string,
  ) {
    // Short-circuit if there are no changes to be made.
    if (
      circuit.trim().replace(/\r\n/g, "\n") ===
      document.getText().trim().replace(/\r\n/g, "\n")
    ) {
      return;
    }

    const edit = new vscode.WorkspaceEdit();

    edit.replace(
      document.uri,
      new vscode.Range(0, 0, document.lineCount, 0),
      circuit.trim(),
    );
    this.updatingDocument = true;
    await vscode.workspace.applyEdit(edit);
    this.updatingDocument = false;
  }
}

/**
 * Generates a Q# entry expression for simulating a circuit operation defined in a JSON circuit file.
 * The entry expression will use the number of qubits specified in the JSON file and
 * call the operation with these qubits. It will then dump the machine state, reset the qubits,
 * and return the results (if any) of running the circuit.
 *
 * If any error occurs (invalid structure, missing fields, etc.), this function throws an error.
 *
 * @param resource The URI of the circuit JSON file.
 * @returns A Q# code block as a string.
 * @throws Error if the circuit file is invalid or required fields are missing.
 */
export async function generateQubitCircuitExpression(
  resource: vscode.Uri,
): Promise<string> {
  let numQubits: number | undefined;

  try {
    const document = await vscode.workspace.openTextDocument(resource);
    const text = document.getText();
    const json = JSON.parse(text);

    if (
      !Array.isArray(json.circuits) ||
      json.circuits.length === 0 ||
      !Array.isArray(json.circuits[0].qubits)
    ) {
      throw new Error("Circuit file does not have expected structure.");
    }
    numQubits = json.circuits[0].qubits.length;
    if (typeof numQubits !== "number" || numQubits < 0) {
      throw new Error("Could not determine number of qubits.");
    } else if (numQubits === 0) {
      return `Message("Circuit is empty. Please add operations to the circuit.")`;
    }

    // Get operation name (file name without extension)
    const fileName = resource.path.substring(
      resource.path.lastIndexOf("/") + 1,
    );
    const operationName = fileName.replace(/\.[^/.]+$/, "");
    if (!operationName) {
      throw new Error("Could not determine operation name from file name.");
    }

    const namespaceName = operationName;

    const expr = `{
    import Std.Diagnostics.DumpMachine;
    import ${namespaceName}.${operationName};
    use qs = Qubit[${numQubits}];
    let results = ${operationName}(qs);
    DumpMachine();
    ResetAll(qs);
    results
}`;
    return expr;
  } catch (err: any) {
    throw new Error(
      `Failed to generate Q# circuit expression: ${err?.message ?? err}`,
      { cause: err },
    );
  }
}

/**
 * Derive the Q# operation name shown in the preview from the circuit URI.
 *
 * Mirrors the convention used by `CircuitEditorProvider.updateWebview`, which
 * derives the title from the basename minus extension. Falls back to a safe
 * default for URIs without a recognizable basename.
 */
function previewOperationNameFor(circuitUri: vscode.Uri): string {
  const basename = circuitUri.path.split(/[\\/]/).pop() ?? "";
  const name = basename.replace(/\.[^/.]+$/, "");
  return name.length > 0 ? name : "Circuit";
}

/**
 * Format an error message as a Q# comment block so the preview tab keeps
 * rendering as valid Q# even when generation fails. The `kind` shows up in
 * the header so users can tell at a glance whether the issue is a malformed
 * file vs. a compiler problem.
 */
function previewErrorComment(kind: string, message: string): string {
  const lines = String(message).split(/\r?\n/);
  return [
    `// Q# preview unavailable — ${kind}`,
    ...lines.map((line) => `// ${line}`),
    "",
  ].join("\n");
}

/**
 * Reveal (or open) the Q# preview document in the editor group beside the
 * circuit. Best-effort: if VS Code rejects the open (e.g. no editor group is
 * available), the preview simply isn't shown and the circuit editor is
 * unaffected.
 */
async function openPreviewBeside(previewUri: vscode.Uri): Promise<void> {
  try {
    // If the preview is already showing in some tab, reveal that one instead
    // of opening a duplicate. This handles the common case of toggling back
    // to a circuit whose preview was opened earlier in this session.
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        const input = tab.input as { uri?: vscode.Uri } | undefined;
        if (input?.uri?.toString() === previewUri.toString()) {
          await vscode.window.showTextDocument(previewUri, {
            viewColumn: group.viewColumn,
            preserveFocus: true,
            preview: false,
          });
          return;
        }
      }
    }

    await vscode.window.showTextDocument(previewUri, {
      viewColumn: vscode.ViewColumn.Beside,
      preserveFocus: true,
      preview: false,
    });
  } catch {
    // Best-effort; the circuit editor itself works without the preview.
  }
}
