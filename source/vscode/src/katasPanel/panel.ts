// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Singleton webview panel manager for the Quantum Katas full-view experience.
 *
 * Creates a WebviewPanel, bridges postMessage ↔ KatasEngine, delegates run /
 * circuit to VS Code commands, uses loadCompilerWorker for exercise checking,
 * listens to ProgressWatcher.onDidChange for progress sync, and watches
 * .navigate.json for tree-view navigation signals, and .panel-navigate.json
 * for MCP server navigation signals.
 */

import * as vscode from "vscode";
import { QscEventTarget } from "qsharp-lang";
import { getExerciseSources } from "qsharp-lang/katas";
import type { Exercise } from "qsharp-lang/katas";
import { loadCompilerWorker, qsharpExtensionId } from "../common.js";
import { KatasEngine } from "./engine.js";
import type { ProgressWatcher } from "../katasProgress/progressReader.js";
import {
  detectKatasWorkspace,
  NAVIGATE_FILE,
  PANEL_NAVIGATE_FILE,
} from "../katasProgress/detector.js";
import type { SolutionCheckResult } from "./types.js";

let instance: KatasPanelManager | undefined;

export class KatasPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private engine: KatasEngine | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];
  private navigateWatcher: vscode.FileSystemWatcher | undefined;
  private panelNavigateWatcher: vscode.FileSystemWatcher | undefined;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly progressWatcher: ProgressWatcher,
  ) {}

  /**
   * Show (or create) the Katas panel.
   * If the panel already exists, it's revealed; otherwise a new one is created.
   */
  async show(): Promise<void> {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
      return;
    }

    // Detect the katas workspace
    const info = await detectKatasWorkspace();
    if (!info) {
      vscode.window.showWarningMessage(
        "No Quantum Katas workspace detected. Open a folder containing qdk-learning.json first.",
      );
      return;
    }

    // Initialize engine
    this.engine = new KatasEngine();
    await this.engine.initialize(info.workspaceRoot, info.katasRoot);

    // Create webview panel
    this.panel = vscode.window.createWebviewPanel(
      "qsharp-katas",
      "Quantum Katas",
      { viewColumn: vscode.ViewColumn.One, preserveFocus: false },
      {
        enableScripts: true,
        enableFindWidget: true,
        retainContextWhenHidden: true,
        localResourceRoots: [
          vscode.Uri.joinPath(this.extensionUri, "out"),
          info.katasRoot,
        ],
      },
    );

    this.panel.iconPath = vscode.Uri.joinPath(
      this.extensionUri,
      "resources",
      "qdk.svg",
    );

    this.panel.onDidDispose(
      () => {
        this.panel = undefined;
        this.ready = false;
        this.queuedMessages = [];
        this.engine?.dispose();
        this.engine = undefined;
        this.disposeWatchers();
      },
      undefined,
      this.disposables,
    );

    // Generate and set HTML
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    // Listen for webview messages
    this.panel.webview.onDidReceiveMessage(
      (msg) => this.handleMessage(msg),
      undefined,
      this.disposables,
    );

    // Listen for progress changes (external edits to qdk-learning.json)
    this.disposables.push(
      this.progressWatcher.onDidChange(async () => {
        if (this.engine && this.panel) {
          await this.engine.reloadProgress();
          this.sendState();
        }
      }),
    );

    // Watch .navigate.json for tree-view navigation signals
    this.setupNavigateWatcher(info);
  }

  dispose(): void {
    this.panel?.dispose();
    this.engine?.dispose();
    this.disposeWatchers();
    for (const d of this.disposables) d.dispose();
    this.disposables = [];
    instance = undefined;
  }

  // ─── Singleton accessor ───

  static getInstance(
    extensionUri: vscode.Uri,
    progressWatcher: ProgressWatcher,
  ): KatasPanelManager {
    if (!instance) {
      instance = new KatasPanelManager(extensionUri, progressWatcher);
    }
    return instance;
  }

  // ─── Message bridge ───

  private sendMessage(msg: unknown): void {
    if (!this.panel) return;
    if (this.ready) {
      this.panel.webview.postMessage(msg);
    } else {
      this.queuedMessages.push(msg);
    }
  }

  private sendState(): void {
    if (!this.engine) return;
    this.sendMessage({ command: "state", state: this.engine.getState() });
  }

  /**
   * If the current position is an exercise or example, open the
   * corresponding .qs file in the secondary editor column.
   */
  private async openCurrentFile(): Promise<void> {
    if (!this.engine) return;
    const pos = this.engine.getPosition();
    let fileUri: vscode.Uri | undefined;
    if (pos.item.type === "exercise") {
      fileUri = this.engine.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      fileUri = this.engine.getExampleFileUri();
    }
    if (fileUri) {
      await vscode.commands.executeCommand(
        "vscode.open",
        fileUri,
        vscode.ViewColumn.Two,
      );
    }
  }

  private sendResult(action: string, result: unknown): void {
    if (!this.engine) return;
    this.sendMessage({
      command: "result",
      action,
      result,
      state: this.engine.getState(),
    });
  }

  private sendError(message: string): void {
    this.sendMessage({ command: "error", message });
  }

  private async handleMessage(msg: any): Promise<void> {
    if (msg.command === "ready") {
      this.ready = true;
      for (const queued of this.queuedMessages) {
        this.panel?.webview.postMessage(queued);
      }
      this.queuedMessages = [];
      this.sendState();
      await this.openCurrentFile();
      return;
    }

    if (msg.command === "openFile") {
      const uri = vscode.Uri.parse(msg.uri);
      await vscode.commands.executeCommand(
        "vscode.open",
        uri,
        vscode.ViewColumn.Two,
      );
      return;
    }

    if (msg.command === "action") {
      await this.handleAction(msg.action);
    }
  }

  private async handleAction(action: string): Promise<void> {
    if (!this.engine) return;

    try {
      switch (action) {
        case "next": {
          const result = this.engine.next();
          this.sendResult("next", result);
          break;
        }
        case "back": {
          const result = this.engine.previous();
          this.sendResult("back", result);
          break;
        }
        case "run": {
          await this.executeRun();
          this.sendResult("run", {});
          break;
        }
        case "circuit": {
          await this.executeCircuit();
          this.sendResult("circuit", {});
          break;
        }
        case "check": {
          const result = await this.executeCheck();
          this.sendResult("check", result);
          break;
        }
        case "hint": {
          const { result } = this.engine.getNextHint();
          this.sendResult("hint", result);
          break;
        }
        case "solution": {
          const code = this.engine.getFullSolution();
          this.sendResult("solution", code);
          break;
        }
        case "reveal-answer": {
          const { result } = this.engine.revealAnswer();
          this.sendResult("reveal-answer", result);
          break;
        }
        case "progress": {
          const progress = this.engine.getProgress();
          this.sendResult("progress", progress);
          break;
        }
        default:
          this.sendError(`Unknown action: ${action}`);
      }
    } catch (err: unknown) {
      this.sendError(err instanceof Error ? err.message : String(err));
    }
  }

  // ─── Command delegation ───

  private async executeRun(): Promise<void> {
    if (!this.engine) return;
    const pos = this.engine.getPosition();

    let fileUri: vscode.Uri;
    if (pos.item.type === "exercise") {
      fileUri = this.engine.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      fileUri = this.engine.getExampleFileUri();
      this.engine.markExampleRun(pos.item.id);
    } else {
      throw new Error("Current item cannot be run.");
    }

    // Open the file first, then run
    await vscode.commands.executeCommand(
      "vscode.open",
      fileUri,
      vscode.ViewColumn.Two,
    );
    await vscode.commands.executeCommand(
      `${qsharpExtensionId}.runProgram`,
      fileUri,
    );
  }

  private async executeCircuit(): Promise<void> {
    if (!this.engine) return;
    const pos = this.engine.getPosition();

    let fileUri: vscode.Uri;
    if (pos.item.type === "exercise") {
      fileUri = this.engine.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      fileUri = this.engine.getExampleFileUri();
    } else {
      throw new Error("Current item cannot show a circuit.");
    }

    await vscode.commands.executeCommand(
      "vscode.open",
      fileUri,
      vscode.ViewColumn.Two,
    );
    await vscode.commands.executeCommand(
      `${qsharpExtensionId}.showCircuit`,
      fileUri,
    );
  }

  private async executeCheck(): Promise<SolutionCheckResult> {
    if (!this.engine) return { passed: false, events: [], error: "No engine" };

    const pos = this.engine.getPosition();
    if (pos.item.type !== "exercise") {
      throw new Error("Current item is not an exercise.");
    }

    // Get the exercise object for exercise sources
    const exercise = this.engine["resolveExercise"]() as Exercise;
    const userCode = await this.engine.readUserCode();
    const sources = await getExerciseSources(exercise);

    // Use the extension's compiler worker
    const worker = loadCompilerWorker(this.extensionUri);
    const eventTarget = new QscEventTarget(true);

    try {
      const passed = await worker.checkExerciseSolution(
        userCode,
        sources,
        eventTarget,
      );

      const events: { type: string; message?: string }[] = [];
      const resultCount = eventTarget.resultCount();
      const results = eventTarget.getResults();
      for (let i = 0; i < resultCount; i++) {
        const r = results[i];
        for (const evt of r.events) {
          if (evt.type === "Message") {
            events.push({
              type: "Message",
              message: (evt as { message: string }).message,
            });
          } else {
            events.push({ type: evt.type });
          }
        }
      }

      if (passed) {
        await this.engine.markExerciseComplete(pos.kataId, pos.sectionId);
      }

      return {
        passed,
        events,
        error: passed
          ? undefined
          : events.length > 0
            ? events
                .filter((e) => e.message)
                .map((e) => e.message)
                .join("\n") || undefined
            : "Solution check failed.",
      };
    } catch (err: unknown) {
      return {
        passed: false,
        events: [],
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      worker.terminate();
    }
  }

  // ─── .navigate.json / .panel-navigate.json watchers ───

  private setupNavigateWatcher(info: {
    workspaceRoot: vscode.Uri;
    katasRoot: vscode.Uri;
  }): void {
    this.disposeWatchers();

    // Shared handler for both signal files: read JSON, navigate engine,
    // update webview, open the .qs file beside the panel, and delete the
    // signal file to indicate consumption.
    const makeHandler = (signalFile: string) => async () => {
      if (!this.engine) return;
      const navUri = vscode.Uri.joinPath(info.katasRoot, signalFile);
      try {
        const bytes = await vscode.workspace.fs.readFile(navUri);
        const data = JSON.parse(new TextDecoder("utf-8").decode(bytes));
        if (data.kataId) {
          this.engine.goTo(data.kataId, data.sectionId, data.itemIndex ?? 0);
          this.sendState();
          await this.openCurrentFile();
          this.panel?.reveal(vscode.ViewColumn.One);
        }
        // Delete the file to signal we consumed it
        await vscode.workspace.fs.delete(navUri);
      } catch {
        // File already consumed or missing
      }
    };

    // Tree-view → panel (written by katasProgress tree provider)
    const treePattern = new vscode.RelativePattern(
      info.katasRoot,
      NAVIGATE_FILE,
    );
    this.navigateWatcher =
      vscode.workspace.createFileSystemWatcher(treePattern);
    const handleTreeNavigate = makeHandler(NAVIGATE_FILE);
    this.navigateWatcher.onDidCreate(handleTreeNavigate);
    this.navigateWatcher.onDidChange(handleTreeNavigate);

    // MCP server → panel (written by the out-of-process MCP katas server).
    // This is a temporary signal-file IPC workaround. When the katas tool
    // implementations are moved in-proc, replace with a direct call to
    // this.engine.goTo() from the tool handler.
    const mcpPattern = new vscode.RelativePattern(
      info.katasRoot,
      PANEL_NAVIGATE_FILE,
    );
    this.panelNavigateWatcher =
      vscode.workspace.createFileSystemWatcher(mcpPattern);
    const handleMcpNavigate = makeHandler(PANEL_NAVIGATE_FILE);
    this.panelNavigateWatcher.onDidCreate(handleMcpNavigate);
    this.panelNavigateWatcher.onDidChange(handleMcpNavigate);
  }

  private disposeWatchers(): void {
    this.navigateWatcher?.dispose();
    this.navigateWatcher = undefined;
    this.panelNavigateWatcher?.dispose();
    this.panelNavigateWatcher = undefined;
  }

  // ─── HTML generation ───

  private getWebviewContent(webview: vscode.Webview): string {
    const extensionUri = this.extensionUri;

    function getUri(...parts: string[]): vscode.Uri {
      return webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, ...parts));
    }

    // Read the HTML template, CSS, render.js, and KaTeX scripts
    // These are all inlined into the HTML for CSP compliance
    const nonce = getNonce();

    // We use webview URIs for external resources, but inline scripts/styles
    // with a nonce for CSP.
    const katexJsUri = getUri(
      "out",
      "learning",
      "web",
      "public",
      "shared",
      "katex.min.js",
    );
    const katexAutoRenderUri = getUri(
      "out",
      "learning",
      "web",
      "public",
      "shared",
      "auto-render.min.js",
    );
    const renderJsUri = getUri(
      "out",
      "learning",
      "web",
      "public",
      "shared",
      "render.js",
    );
    const cssUri = getUri("out", "katasPanel", "katas-webview.css");

    const cspSource = webview.cspSource;

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta
      http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src ${cspSource} 'nonce-${nonce}'; script-src 'nonce-${nonce}'; font-src ${cspSource};"
    />
    <title>Quantum Katas</title>
    <link rel="stylesheet" href="${cssUri}" nonce="${nonce}" />
  </head>
  <body>
    <header id="header" class="header">
      <span class="crumb"></span>
      <span class="badge"></span>
    </header>
    <section id="content" class="content"></section>
    <div id="output" class="output" hidden></div>
    <nav id="actions" class="action-bar"></nav>
    <footer id="progress-bar" class="progress-bar"></footer>

    <script nonce="${nonce}" src="${katexJsUri}"></script>
    <script nonce="${nonce}" src="${katexAutoRenderUri}"></script>
    <script nonce="${nonce}" src="${renderJsUri}"></script>
    <script nonce="${nonce}">
      (() => {
        "use strict";
        // Use MathML output so KaTeX works without external CSS / inline styles.
        globalThis.__KATAS_KATEX_CONFIG = { output: "mathml" };
        const vscodeApi = acquireVsCodeApi();
        const R = globalThis.KatasRender;

        // ─── DOM slots ───
        const headerEl = document.getElementById("header");
        const crumbEl = headerEl.querySelector(".crumb");
        const badgeEl = headerEl.querySelector(".badge");
        const contentEl = document.getElementById("content");
        const outputEl = document.getElementById("output");
        const actionsEl = document.getElementById("actions");
        const progressEl = document.getElementById("progress-bar");

        let busy = false;
        let lastPositionKey = null;

        // ─── Rendering ───

        function renderHeader(position) {
          const item = position.item;
          const sectionTitle =
            item.type === "exercise"
              ? item.title
              : item.sectionTitle || "";
          crumbEl.textContent = position.kataId + " · " + position.sectionId + " · #" + (position.itemIndex + 1) + (sectionTitle ? " — " + sectionTitle : "");

          badgeEl.className = "badge";
          if (item.type === "exercise") {
            badgeEl.textContent = item.isComplete ? "✔ done" : "exercise";
            badgeEl.classList.add(item.isComplete ? "complete" : "exercise");
          } else if (item.type === "lesson-text") {
            badgeEl.textContent = "lesson";
          } else if (item.type === "lesson-example") {
            badgeEl.textContent = "example";
          } else if (item.type === "lesson-question") {
            badgeEl.textContent = "question";
          } else {
            badgeEl.textContent = item.type;
          }
        }

        function renderContent(position) {
          contentEl.innerHTML = R.renderContentBody(position.item);
          R.renderMath(contentEl);
          contentEl.scrollTop = 0;
        }

        function showOutput(html, variant) {
          outputEl.className = "output" + (variant ? " " + variant : "");
          outputEl.innerHTML =
            '<button class="out-dismiss" aria-label="Dismiss" title="Dismiss">×</button>' +
            '<div class="out-label">' + labelFor(variant) + "</div>" +
            '<div class="out-body">' + html + "</div>";
          outputEl.hidden = false;
          outputEl
            .querySelector(".out-dismiss")
            .addEventListener("click", clearOutput);
          R.renderMath(outputEl);
        }

        function labelFor(variant) {
          if (variant === "pass") return "Result";
          if (variant === "fail") return "Result";
          return "Output";
        }

        function clearOutput() {
          outputEl.hidden = true;
          outputEl.innerHTML = "";
        }

        function renderActions(groups) {
          actionsEl.innerHTML = "";
          for (const group of groups) {
            const div = document.createElement("div");
            div.className = "action-group";
            for (const binding of group) {
              if (binding.action === "quit" || binding.action === "menu")
                continue;
              const btn = document.createElement("button");
              btn.textContent = binding.label;
              if (binding.primary) btn.classList.add("primary");
              btn.dataset.action = binding.action;
              btn.disabled = busy;
              btn.addEventListener("click", () =>
                executeAction(binding.action),
              );
              div.appendChild(btn);
            }
            if (div.children.length > 0) actionsEl.appendChild(div);
          }
        }

        function setBusy(b) {
          busy = b;
          for (const btn of actionsEl.querySelectorAll("button"))
            btn.disabled = b;
        }

        function renderProgressBar(progress) {
          try {
            progressEl.innerHTML = R.renderProgressBar(progress);
          } catch {
            progressEl.innerHTML = "";
          }
        }

        function applyState(state) {
          if (!state) return;
          const pos = state.position;
          const key = pos.kataId + ":" + pos.sectionId + ":" + pos.itemIndex;
          if (key !== lastPositionKey) {
            renderHeader(pos);
            renderContent(pos);
            clearOutput();
            lastPositionKey = key;
          } else {
            renderHeader(pos);
          }
          renderActions(state.actions);
          renderProgressBar(state.progress);
        }

        function invalidateContent() {
          lastPositionKey = null;
        }

        // ─── Action dispatch ───

        function executeAction(action) {
          if (busy) return;
          setBusy(true);

          var slow = ["run", "circuit", "check"].indexOf(action) >= 0;
          if (slow) showOutput('<div class="loading">Working…</div>');

          vscodeApi.postMessage({ command: "action", action: action });
        }

        // ─── Messages from extension host ───

        window.addEventListener("message", function(event) {
          var msg = event.data;
          switch (msg.command) {
            case "state":
              applyState(msg.state);
              setBusy(false);
              break;
            case "result": {
              var action = msg.action;
              var result = msg.result;
              switch (action) {
                case "next":
                case "back":
                  if (result && !result.moved) {
                    showOutput(
                      action === "next"
                        ? '<div class="success">🎉 You have completed all content!</div>'
                        : '<div class="message">Already at the beginning.</div>',
                    );
                  }
                  break;
                case "check":
                  showOutput(
                    R.renderSolutionCheck(result),
                    result.passed ? "pass" : "fail",
                  );
                  if (result.passed) invalidateContent();
                  break;
                case "hint":
                  showOutput(
                    result
                      ? R.renderHint(result)
                      : '<div class="message">No more hints available.</div>',
                  );
                  break;
                case "reveal-answer":
                  showOutput("<div>" + result + "</div>");
                  break;
                case "solution":
                  showOutput(
                    '<div style="margin-bottom:0.3rem"><strong>Reference Solution</strong></div><pre>' +
                      R.escapeHtml(result) +
                      "</pre>",
                  );
                  break;
                case "progress":
                  showOutput(R.renderProgress(result));
                  break;
                case "run":
                case "circuit":
                  clearOutput();
                  break;
              }
              if (msg.state) applyState(msg.state);
              setBusy(false);
              break;
            }
            case "error":
              showOutput(
                '<div class="fail">Error: ' +
                  R.escapeHtml(msg.message) +
                  "</div>",
                "fail",
              );
              setBusy(false);
              break;
          }
        });

        // Copy-to-clipboard for file paths.
        contentEl.addEventListener("click", function(e) {
          var btn = e.target.closest(".copy-btn");
          if (btn && navigator.clipboard) {
            navigator.clipboard.writeText(btn.dataset.copy);
          }
        });

        // File-path link clicks → open via extension host.
        contentEl.addEventListener("click", function(e) {
          var a = e.target.closest("a.file-path-link");
          if (!a) return;
          e.preventDefault();
          var url = a.getAttribute("href");
          if (url) {
            vscodeApi.postMessage({ command: "openFile", uri: url });
          }
        });

        // Signal ready
        vscodeApi.postMessage({ command: "ready" });
      })();
    </script>
  </body>
</html>`;
  }
}

function getNonce(): string {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let result = "";
  for (let i = 0; i < 32; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}
