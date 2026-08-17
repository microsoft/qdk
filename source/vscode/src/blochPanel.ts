// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
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
import { qsharpExtensionId } from "./common";

// The Bloch sphere is a standalone interactive view with its own dedicated
// bundle (bloch.js), so it gets its own webview type and serializer rather
// than sharing the generic qsharp-webview plumbing. This keeps three.js out of
// the shared webview.js and lets the panel be restored on reload without a
// panel-type discriminator.
const BlochWebViewType = "qsharp-webview.bloch";
const BlochTitle = "Bloch Sphere";

let extensionUri: Uri;

// Only a single Bloch sphere panel can be open at a time.
let blochPanel: WebviewPanel | undefined;

export function registerBlochCommand(context: ExtensionContext) {
  extensionUri = context.extensionUri;

  context.subscriptions.push(
    window.registerWebviewPanelSerializer(
      BlochWebViewType,
      new BlochViewPanelSerializer(),
    ),
  );

  context.subscriptions.push(
    commands.registerCommand(`${qsharpExtensionId}.showBloch`, async () => {
      showBlochPanel();
    }),
  );
}

function showBlochPanel() {
  if (blochPanel) {
    // If it's already visible, don't do anything, else it messes up the
    // existing layout.
    if (!blochPanel.visible) {
      blochPanel.reveal(ViewColumn.Active, true);
    }
    return;
  }

  const panel = window.createWebviewPanel(
    BlochWebViewType,
    BlochTitle,
    {
      // The Bloch sphere is a standalone interactive view, so open it in the
      // active (main) editor column rather than beside the source.
      viewColumn: ViewColumn.Active,
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
      // dynamic state of the DOM on the page is lost if this occurs.
    },
  );

  initBlochPanel(panel);
}

function initBlochPanel(panel: WebviewPanel) {
  log.info("Creating Bloch sphere webview panel");
  blochPanel = panel;
  panel.webview.html = getWebviewContent(panel.webview);
  panel.onDidDispose(() => {
    log.info("Disposing Bloch sphere webview panel");
    blochPanel = undefined;
  });
}

function getWebviewContent(webview: Webview) {
  function getUri(pathList: string[]) {
    return webview.asWebviewUri(Uri.joinPath(extensionUri, ...pathList));
  }

  const katexCss = getUri(["out", "katex", "katex.min.css"]);
  const githubCss = getUri(["out", "katex", "github-markdown-dark.css"]);
  const blochCss = getUri(["out", "webview", "bloch.css"]);
  const blochJs = getUri(["out", "webview", "bloch.js"]);
  const resourcesUri = getUri(["resources"]);

  return /*html*/ `
  <!DOCTYPE html>
  <html lang="en">
    <head>
      <meta charset="UTF-8">
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <title>Bloch Sphere</title>
      <link rel="stylesheet" href="${githubCss}" />
      <link rel="stylesheet" href="${katexCss}" />
      <link rel="stylesheet" href="${blochCss}" />
      <script src="${blochJs}"></script>
      <script>
        window.resourcesUri = "${resourcesUri.toString()}";
      </script>
    </head>
    <body>
    </body>
  </html>
`;
}

export class BlochViewPanelSerializer implements WebviewPanelSerializer {
  async deserializeWebviewPanel(panel: WebviewPanel) {
    log.info("Deserializing Bloch sphere webview panel");

    if (blochPanel !== undefined) {
      log.error("Bloch sphere panel already exists");
      panel.dispose();
      return;
    }

    initBlochPanel(panel);
  }
}
