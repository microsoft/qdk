// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  IOperationInfo,
  QscEventTarget,
  VSDiagnostic,
  getCompilerWorker,
  log,
} from "qsharp-lang";
import {
  ExtensionContext,
  Uri,
  ViewColumn,
  Webview,
  WebviewPanel,
  WebviewPanelSerializer,
  commands,
  window,
} from "vscode";
import { showCircuitCommand } from "./circuit";
import { clearCommandDiagnostics } from "./diagnostics";
import { showDocumentationCommand } from "./documentation";
import { getActiveProgram } from "./programConfig";
import {
  EventType,
  getActiveDocumentType,
  sendTelemetryEvent,
  UserTaskInvocationType,
} from "./telemetry";
import { getRandomGuid } from "./utils";
import { getPauliNoiseModel, getQubitLossSetting } from "./config";
import { qsharpExtensionId } from "./common";
import { resourceEstimateCommand } from "./estimate";

const QSharpWebViewType = "qsharp-webview";
const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes

export function registerWebViewCommands(context: ExtensionContext) {
  QSharpWebViewPanel.extensionUri = context.extensionUri;

  window.registerWebviewPanelSerializer(
    QSharpWebViewType,
    new QSharpViewViewPanelSerializer(),
  );

  const compilerWorkerScriptPath = Uri.joinPath(
    context.extensionUri,
    "./out/compilerWorker.js",
  ).toString();

  context.subscriptions.push(
    commands.registerCommand(
      `${qsharpExtensionId}.showRe`,
      async (resource?: vscode.Uri, expr?: string) =>
        resourceEstimateCommand(context.extensionUri, resource, expr),
    ),
  );

  context.subscriptions.push(
    commands.registerCommand(`${qsharpExtensionId}.showHelp`, async () => {
      const message = {};
      sendMessageToPanel({ panelType: "help" }, true, message);
    }),
  );

  const handleShowHistogram = async (resource?: vscode.Uri, expr?: string) => {
    clearCommandDiagnostics();

    const associationId = getRandomGuid();
    sendTelemetryEvent(
      EventType.TriggerHistogram,
      {
        associationId,
        documentType: getActiveDocumentType(),
        invocationType: UserTaskInvocationType.Command,
      },
      {},
    );
    function resultToLabel(result: string | VSDiagnostic): string {
      if (typeof result !== "string") return "ERROR";
      return result;
    }

    const program = await getActiveProgram({ showModalError: true });
    if (!program.success) {
      throw new Error(program.errorMsg);
    }

    const panelId = program.programConfig.projectName;

    // Start the worker, run the code, and send the results to the webview
    const worker = getCompilerWorker(compilerWorkerScriptPath);
    const compilerTimeout = setTimeout(() => {
      worker.terminate();
    }, compilerRunTimeoutMs);

    try {
      const validateShotsInput = (input: string) => {
        const result = parseFloat(input);
        if (isNaN(result) || Math.floor(result) !== result || result <= 0) {
          return "Number of shots must be a positive integer";
        }
      };

      const numberOfShotsInput = await window.showInputBox({
        value: "100",
        prompt: "Number of shots",
        validateInput: validateShotsInput,
      });

      // abort if the user hits <Esc> during shots entry
      if (numberOfShotsInput === undefined) {
        return;
      }

      const numberOfShots = numberOfShotsInput;

      sendMessageToPanel(
        { panelType: "histogram", id: panelId },
        true,
        undefined,
      );

      const evtTarget = new QscEventTarget(true);
      evtTarget.addEventListener("uiResultsRefresh", () => {
        const results = evtTarget.getResults();
        const resultCount = evtTarget.resultCount();
        const buckets = new Map();
        for (let i = 0; i < resultCount; ++i) {
          const key = results[i].result;
          const strKey = resultToLabel(key);
          const newValue = (buckets.get(strKey) || 0) + 1;
          buckets.set(strKey, newValue);
        }
        const message = {
          buckets: Array.from(buckets.entries()),
          shotCount: resultCount,
        };
        sendMessageToPanel(
          { panelType: "histogram", id: panelId },
          false,
          message,
        );
      });
      const start = performance.now();
      sendTelemetryEvent(EventType.HistogramStart, { associationId }, {});

      const noise = getPauliNoiseModel();
      const qubitLoss = getQubitLossSetting();
      if (noise[0] != 0 || noise[1] != 0 || noise[2] != 0 || qubitLoss != 0) {
        sendTelemetryEvent(EventType.NoisySimulation, { associationId }, {});
      }
      await worker.runWithNoise(
        program.programConfig,
        expr ?? "",
        parseInt(numberOfShots),
        noise,
        qubitLoss,
        evtTarget,
      );
      sendTelemetryEvent(
        EventType.HistogramEnd,
        { associationId },
        { timeToCompleteMs: performance.now() - start },
      );
      clearTimeout(compilerTimeout);
    } catch (e: any) {
      log.error("Histogram error. ", e.toString());
      throw new Error("Run failed. " + e.toString());
    } finally {
      worker.terminate();
    }
  };

  context.subscriptions.push(
    commands.registerCommand(
      `${qsharpExtensionId}.showHistogram`,
      handleShowHistogram,
    ),
  );

  context.subscriptions.push(
    commands.registerCommand(
      `${qsharpExtensionId}.showCircuit`,
      async (resource?: vscode.Uri, operation?: IOperationInfo) => {
        await showCircuitCommand(
          context.extensionUri,
          operation,
          UserTaskInvocationType.Command,
        );
      },
    ),
  );

  context.subscriptions.push(
    commands.registerCommand(
      `${qsharpExtensionId}.showDocumentation`,
      async () => {
        await showDocumentationCommand(context.extensionUri);
      },
    ),
  );
}

type PanelDesc = {
  title: string;
  panel: QSharpWebViewPanel;
  state: any;
};

type PanelType =
  | "histogram"
  | "estimates"
  | "help"
  | "circuit"
  | "documentation";

const panels: Record<PanelType, { [id: string]: PanelDesc }> = {
  histogram: {},
  estimates: {},
  circuit: {},
  help: {},
  documentation: {},
};

const panelTypeToTitle: Record<PanelType, string> = {
  histogram: "QDK Histogram",
  estimates: "QDK Estimates",
  circuit: "QDK Circuit",
  help: "Q# Help",
  documentation: "Q# Documentation",
};

function getPanel(type: PanelType, id?: string): PanelDesc | undefined {
  if (id) {
    return panels[type][id];
  } else {
    return panels[type][""];
  }
}

export function isPanelOpen(panelType: PanelType, id?: string): boolean {
  return getPanel(panelType, id)?.panel !== undefined;
}

function createPanel(
  type: PanelType,
  id?: string,
  webViewPanel?: WebviewPanel,
): PanelDesc {
  if (id == undefined) {
    id = "";
  }
  if (webViewPanel) {
    const title = webViewPanel.title;
    const panel = new QSharpWebViewPanel(type, webViewPanel, id);
    panels[type][id] = { title, panel, state: {} };
    return panels[type][id];
  } else {
    let title = `${panelTypeToTitle[type]}`;
    if (type == "circuit" || type == "histogram") {
      title = title + ` ${id}`;
    }
    const newPanel = window.createWebviewPanel(
      QSharpWebViewType,
      title,
      {
        viewColumn: ViewColumn.Two,
        preserveFocus: true,
      },
      {
        enableCommandUris: true,
        enableScripts: true,
        enableFindWidget: true,
        retainContextWhenHidden: true,
        // Note: If retainContextWhenHidden is false, the webview gets reloaded
        // every time you hide it by switching to another tab and then switch
        // back. While we've done the work to persist the underlying state, the
        // dynamic state of the DOM on the page - such as which detail sections are
        // expanded, whether in summary or detail view, etc. - is lost if this occurs.
      },
    );

    const panel = new QSharpWebViewPanel(type, newPanel, id);
    panels[type][id] = { title, panel, state: {} };
    return panels[type][id];
  }
}

export function getOrCreatePanel(type: PanelType, id?: string): PanelDesc {
  const panel = getPanel(type, id);
  if (panel) {
    return panel;
  } else {
    return createPanel(type, id);
  }
}

export function sendMessageToPanel(
  panel: { panelType: PanelType; id?: string },
  reveal: boolean,
  message: any,
) {
  const panelRecord = getOrCreatePanel(panel.panelType, panel.id);
  if (reveal) panelRecord.panel.reveal(ViewColumn.Two);
  if (message) panelRecord.panel.sendMessage(message);
}

export class QSharpWebViewPanel {
  public static extensionUri: Uri;
  private _ready = false;
  private _queuedMessages: any[] = [];

  constructor(
    private type: PanelType,
    private panel: WebviewPanel,
    private id: string,
  ) {
    log.info(`Creating webview panel of type ${type} and id ${id}`);
    this.panel.onDidDispose(() => this.dispose());

    this.panel.webview.html = this._getWebviewContent(this.panel.webview);
    this._setWebviewMessageListener(this.panel.webview);
  }

  reveal(column: ViewColumn) {
    // If it's already visible, don't do anything else it messes up the existing layout
    // This isn't perfect, as if the tab is just in the background it will still call
    // reveal to bring to the foreground and change the layout, but this is the best we
    // can do with the current API.
    if (!this.panel.visible) {
      this.panel.reveal(column, true);
    }
  }

  private _getWebviewContent(webview: Webview) {
    const extensionUri = QSharpWebViewPanel.extensionUri;

    function getUri(pathList: string[]) {
      return webview.asWebviewUri(Uri.joinPath(extensionUri, ...pathList));
    }

    const katexCss = getUri(["out", "katex", "katex.min.css"]);
    const githubCss = getUri(["out", "katex", "github-markdown-dark.css"]);
    const webviewCss = getUri(["out", "webview", "webview.css"]);
    const webviewJs = getUri(["out", "webview", "webview.js"]);
    const resourcesUri = getUri(["resources"]);

    return /*html*/ `
  <!DOCTYPE html>
  <html lang="en">
    <head>
      <meta charset="UTF-8">
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <title>Q#</title>
      <link rel="stylesheet" href="${githubCss}" />
      <link rel="stylesheet" href="${katexCss}" />
      <link rel="stylesheet" href="${webviewCss}" />
      <script src="${webviewJs}"></script>
      <script>
        window.resourcesUri = "${resourcesUri.toString()}";
      </script>
    </head>
    <body>
    </body>
  </html>
`;
  }

  sendMessage(message: any) {
    message.command = message.command || this.type;
    message.panelId = message.panelId || this.id;
    if (this._ready) {
      log.debug("Sending message to webview", message);
      this.panel.webview.postMessage(message);
    } else {
      log.debug("Queuing message to webview", message);
      this._queuedMessages.push(message);
    }
  }

  private _setWebviewMessageListener(webview: Webview) {
    console.log("Setting up webview message listener");
    webview.onDidReceiveMessage((message: any) => {
      if (message.command === "ready") {
        this._ready = true;
        this._queuedMessages.forEach((message) =>
          this.panel.webview.postMessage(message),
        );
        this._queuedMessages = [];
      }

      // No messages are currently sent from the webview
      console.log("Message for webview received", message);
    });
  }

  public dispose() {
    log.info("Disposing webview panel", this.type);
    const panel = getPanel(this.type, this.id);
    if (panel) {
      panel.state = {};
      delete panels[this.type][this.id];
    }
    this.panel.dispose();
  }
}

export class QSharpViewViewPanelSerializer implements WebviewPanelSerializer {
  async deserializeWebviewPanel(panel: WebviewPanel, state: any) {
    log.info("Deserializing webview panel", state);

    const panelType: PanelType = state?.viewType;
    const id = state?.panelId;

    if (
      panelType !== "estimates" &&
      panelType !== "histogram" &&
      panelType !== "circuit" &&
      panelType !== "help" &&
      panelType != "documentation"
    ) {
      // If it was loading when closed, that's fine
      if (panelType === "loading") {
        return;
      }
      log.error("Unknown panel type", panelType);
      return;
    }

    if (getPanel(panelType, id) !== undefined) {
      log.error(`Panel of type ${panelType} and id ${id} already exists`);
      return;
    }

    createPanel(panelType, id, panel);
  }
}
