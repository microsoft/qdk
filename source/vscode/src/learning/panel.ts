// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Webview panel manager for the lesson panel.
 * This panel displays the lesson content and the action buttons to interact with
 * the learning feature.
 */

import * as vscode from "vscode";
import { qsharpExtensionId } from "../common.js";
import { LEARNING_FILE, LEARNING_TREE_VIEW_ID } from "./constants.js";
import type { LearningService } from "./service.js";
import type { TelemetrySource } from "./types.js";
import type {
  HostToWebviewMessage,
  ResultAction,
  ResultPayload,
  WebviewToHostMessage,
} from "./types.js";

/**
 * Register the WebviewPanelSerializer so the Lesson panel persists across
 * VS Code restarts.
 */
export function registerLessonPanelSerializer(
  context: vscode.ExtensionContext,
  manager: LessonPanelManager,
): void {
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer("qsharp-lesson", {
      async deserializeWebviewPanel(panel: vscode.WebviewPanel) {
        await manager.restore(panel);
      },
    }),
  );
}

export class LessonPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];
  /** Remembered editor column + layout from the last time a managed tab was open. */
  private savedEditorState:
    | { viewColumn: vscode.ViewColumn; layout: unknown }
    | undefined;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly service: LearningService,
  ) {}

  /**
   * Show or create the Lesson panel.
   */
  async show(): Promise<void> {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
      return;
    }

    const ok = await this.service.tryInitialize();
    if (!ok) {
      vscode.window.showWarningMessage(
        `No QDK Learning workspace detected. Open a folder containing ${LEARNING_FILE} first.`,
      );
      return;
    }

    this.panel = vscode.window.createWebviewPanel(
      "qsharp-lesson",
      "Lesson",
      { viewColumn: vscode.ViewColumn.One, preserveFocus: false },
      {
        enableScripts: true,
        enableFindWidget: true,
        retainContextWhenHidden: true,
        localResourceRoots: [
          vscode.Uri.joinPath(this.extensionUri, "out"),
          vscode.Uri.joinPath(this.extensionUri, "resources"),
          this.service.learningContentRoot,
        ],
      },
    );

    this.panel.iconPath = {
      light: vscode.Uri.joinPath(
        this.extensionUri,
        "resources",
        "mobius-light.svg",
      ),
      dark: vscode.Uri.joinPath(
        this.extensionUri,
        "resources",
        "mobius-dark.svg",
      ),
    };

    // Generate and set HTML
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    this.attachPanel();
  }

  /**
   * Restore a serialized Lesson panel after VS Code restarts.
   * Re-initializes the service from disk before reconnecting the webview.
   */
  async restore(panel: vscode.WebviewPanel): Promise<void> {
    const ok = await this.service.tryInitialize();
    if (!ok) {
      // Workspace no longer available — dispose the stale panel.
      panel.dispose();
      return;
    }

    this.panel = panel;

    // Re-set HTML — webview resource URIs change across sessions.
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    this.attachPanel();
  }

  /**
   * Wire up shared listeners on an already-created panel.
   * Called by both show() (new panel) and restore() (deserialized panel).
   */
  private attachPanel(): void {
    if (!this.panel) {
      return;
    }

    this.panel.onDidDispose(
      () => {
        this.panel = undefined;
        this.ready = false;
        this.queuedMessages = [];
      },
      undefined,
      this.disposables,
    );

    // Listen for webview messages
    this.panel.webview.onDidReceiveMessage(
      (msg) => this.handleMessage(msg),
      undefined,
      this.disposables,
    );

    // Listen for state changes from the service.
    this.disposables.push(
      this.service.onDidChangeState(() => {
        if (this.panel) {
          this.sendState();
          this.openCurrentCodeEditor().catch(() => {});
        }
      }),
    );
  }

  dispose(): void {
    this.panel?.dispose();
    // Close any lingering code editor tabs.
    if (this.service.initialized) {
      this.closeStaleEditorTabs(undefined).catch(() => {});
    }
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }

  private sendMessage(msg: HostToWebviewMessage): void {
    if (!this.panel) {
      return;
    }
    if (this.ready) {
      this.panel.webview.postMessage(msg);
    } else {
      this.queuedMessages.push(msg);
    }
  }

  private sendState(): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({ command: "state", state: this.service.getState() });
  }

  /**
   * Show the panel and execute the "check solution" action, sending the
   * result to the webview so it renders the same output as clicking the
   * panel's own Check button. Returns whether the solution passed.
   */
  async checkAndShowResult(): Promise<boolean> {
    await this.show();
    return this.checkSolutionAndSendResult();
  }

  /**
   * If the current position is an exercise or example, open the
   * corresponding .qs file in the secondary editor column.
   * Closes any previously-opened code editor tabs that are no longer current.
   */
  private async openCurrentCodeEditor(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const fileUri = this.service.getCurrentCodeFileUri();

    // Before closing stale tabs, snapshot the current layout and the view
    // column of any managed editor so we can restore it later — even if
    // intermediate lessons have no code file.
    const column = this.findManagedEditorColumn();
    if (column) {
      const layout = await vscode.commands.executeCommand(
        "vscode.getEditorLayout",
      );
      this.savedEditorState = { viewColumn: column, layout };
    }

    // Close stale editor tabs that don't match the current file.
    await this.closeStaleEditorTabs(fileUri);

    if (fileUri) {
      await this.showFileInEditor(fileUri);
    }
  }

  /**
   * Close any open editor tabs whose URI falls under the QDK Learning root
   * that don't match {@link keepUri}.
   * When {@link keepUri} is undefined, all code editor tabs are closed.
   */
  private async closeStaleEditorTabs(
    keepUri: vscode.Uri | undefined,
  ): Promise<void> {
    const learningRoot = this.service.learningContentRoot.toString();
    const keepStr = keepUri?.toString();

    const staleTabs: vscode.Tab[] = [];
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        if (tab.input instanceof vscode.TabInputText) {
          const tabUriStr = tab.input.uri.toString();
          if (tabUriStr.startsWith(learningRoot) && tabUriStr !== keepStr) {
            staleTabs.push(tab);
          }
        }
      }
    }
    if (staleTabs.length > 0) {
      await vscode.window.tabGroups.close(staleTabs);
    }
  }

  /**
   * Find the view column of any open editor tab whose URI falls under
   * the learning content root, or `undefined` if none is open.
   */
  private findManagedEditorColumn(): vscode.ViewColumn | undefined {
    const rootStr = this.service.learningContentRoot.toString();
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        if (
          tab.input instanceof vscode.TabInputText &&
          tab.input.uri.toString().startsWith(rootStr)
        ) {
          return group.viewColumn;
        }
      }
    }
    return undefined;
  }

  private async showFileInEditor(fileUri: vscode.Uri): Promise<void> {
    let viewColumn = this.findManagedEditorColumn();
    if (!viewColumn) {
      if (this.savedEditorState) {
        // Restore the layout the user had when a managed editor was last open.
        await vscode.commands.executeCommand(
          "vscode.setEditorLayout",
          this.savedEditorState.layout,
        );
        viewColumn = this.savedEditorState.viewColumn;
      } else {
        // First time — set a default two-column layout.
        await vscode.commands.executeCommand("vscode.setEditorLayout", {
          orientation: 0,
          groups: [{ size: 0.4 }, { size: 0.6 }],
        });
        viewColumn = vscode.ViewColumn.Two;
      }
    }

    await vscode.commands.executeCommand("vscode.open", fileUri, {
      viewColumn,
      preview: false,
    } satisfies vscode.TextDocumentShowOptions);
  }

  private sendResult<Action extends ResultAction>(
    action: Action,
    result: ResultPayload<Action>,
  ): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({
      command: "result",
      action,
      result,
      state: this.service.getState(),
    } as Extract<HostToWebviewMessage, { command: "result" }>);
  }

  private sendError(message: string): void {
    this.sendMessage({ command: "error", message });
  }

  private async handleMessage(msg: WebviewToHostMessage): Promise<void> {
    if (msg.command === "ready") {
      this.ready = true;
      for (const queued of this.queuedMessages) {
        this.panel?.webview.postMessage(queued);
      }
      this.queuedMessages = [];
      this.sendState();
      await this.openCurrentCodeEditor();
      return;
    }

    if (msg.command === "openFile") {
      const uri = vscode.Uri.parse(msg.uri);
      await this.showFileInEditor(uri);
      return;
    }

    if (msg.command === "openChat") {
      const text = msg.text || "Give me a hint";
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: `/qdk-learning ${text}`,
      });
      return;
    }

    if (msg.command === "focusProgress") {
      await vscode.commands.executeCommand(`${LEARNING_TREE_VIEW_ID}.focus`);
      return;
    }

    if (msg.command === "action") {
      await this.handleAction(msg.action);
    }
  }

  private async handleAction(action: string): Promise<void> {
    if (!this.service.initialized) {
      return;
    }

    try {
      switch (action) {
        case "next": {
          const result = await this.service.next("panel");
          this.sendResult("next", result);
          break;
        }
        case "back": {
          const result = await this.service.previous("panel");
          this.sendResult("back", result);
          break;
        }
        case "run": {
          await this.executeRun();
          this.sendResult("run", {});
          this.service.sendActivityActionTelemetry("run", "panel");
          break;
        }
        case "check": {
          await this.checkSolutionAndSendResult("panel");
          break;
        }
        case "reset": {
          const confirmed = await vscode.window.showWarningMessage(
            "Reset this exercise to the original placeholder code? Your current code will be lost.",
            { modal: true },
            "Reset",
          );
          if (confirmed === "Reset") {
            await this.service.resetExercise("panel");
          }
          this.sendState();
          break;
        }
        default:
          this.sendError(`Unknown action: ${action}`);
      }
    } catch (err: unknown) {
      this.sendError(err instanceof Error ? err.message : String(err));
    }
  }

  private async executeRun(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }

    const pos = this.service.getCurrentActivity();
    if (pos.content.type !== "lesson-example") {
      throw new Error("Current item cannot be run.");
    }

    const fileUri = this.service.getExampleFileUri();
    await this.service.markExampleRun();

    await this.openCurrentCodeEditor();
    await vscode.commands.executeCommand(
      `${qsharpExtensionId}.runProgram`,
      fileUri,
    );
  }

  private getWebviewContent(webview: vscode.Webview): string {
    const extensionUri = this.extensionUri;
    const cspSource = webview.cspSource;

    function getUri(...parts: string[]): vscode.Uri {
      return webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, ...parts));
    }

    const webviewClientJsUri = getUri(
      "out",
      "learning",
      "webview",
      "webview-client.js",
    );
    const cssUri = getUri("out", "learning", "webview", "webview-client.css");
    const katexCssUri = getUri("out", "katex", "katex.min.css");
    const codiconCssUri = getUri("out", "katex", "codicon.css");
    const mobiusUri = getUri("resources", "mobius.svg");

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; img-src ${cspSource}; style-src ${cspSource} 'unsafe-inline'; font-src ${cspSource}; script-src ${cspSource};" />
    <title>Lesson</title>
    <link rel="stylesheet" href="${katexCssUri}" />
    <link rel="stylesheet" href="${codiconCssUri}" />
    <link rel="stylesheet" href="${cssUri}" />
  </head>
  <body data-mobius-uri="${mobiusUri}">
    <script src="${webviewClientJsUri}"></script>
  </body>
</html>`;
  }

  private async checkSolutionAndSendResult(
    source?: TelemetrySource,
  ): Promise<boolean> {
    const { result, state } = await this.service.checkSolution(source);
    this.sendMessage({
      command: "result",
      action: "check",
      result,
      state,
    });
    return result.passed;
  }
}

/**
 * Find the tab whose document matches {@link uri} and return its view column,
 * or `undefined` if the file is not open in any tab.
 */
function findViewColumnForUri(uri: vscode.Uri): vscode.ViewColumn | undefined {
  const uriStr = uri.toString();
  for (const group of vscode.window.tabGroups.all) {
    for (const tab of group.tabs) {
      if (
        tab.input instanceof vscode.TabInputText &&
        tab.input.uri.toString() === uriStr
      ) {
        return group.viewColumn;
      }
    }
  }
  return undefined;
}
