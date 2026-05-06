// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

const viewId = "qsharp-vscode.learningWelcome";

/**
 * Registers a WebviewView that shows the QDK Learning welcome screen
 * (with the colorful Möbius logo) when no learning workspace is detected.
 */
export function registerLearningWelcomeView(
  context: vscode.ExtensionContext,
): void {
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(viewId, {
      resolveWebviewView(webviewView: vscode.WebviewView) {
        webviewView.webview.options = {
          enableScripts: false,
          enableCommandUris: true,
          localResourceRoots: [
            vscode.Uri.joinPath(context.extensionUri, "resources"),
            vscode.Uri.joinPath(context.extensionUri, "out"),
          ],
        };

        const logoUri = webviewView.webview.asWebviewUri(
          vscode.Uri.joinPath(context.extensionUri, "resources", "mobius.png"),
        );

        const codiconCssUri = webviewView.webview.asWebviewUri(
          vscode.Uri.joinPath(context.extensionUri, "out", "katex", "codicon.css"),
        );

        webviewView.webview.html = getHtml(
          logoUri,
          codiconCssUri,
          webviewView.webview.cspSource,
        );
      },
    }),
  );
}

function getHtml(logoUri: vscode.Uri, codiconCssUri: vscode.Uri, cspSource: string): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy"
    content="default-src 'none'; img-src ${cspSource}; style-src ${cspSource} 'unsafe-inline'; font-src ${cspSource};" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <link rel="stylesheet" href="${codiconCssUri}" />
  <style>
    body {
      padding: 16px;
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      display: flex;
      flex-direction: column;
      align-items: center;
      text-align: center;
    }
    .logo {
      width: 64px;
      height: 64px;
      margin-bottom: 16px;
    }
    h2 {
      margin: 0 0 8px;
      font-size: 1.1em;
      font-weight: 600;
    }
    p {
      margin: 0 0 16px;
      line-height: 1.4;
      color: var(--vscode-descriptionForeground);
    }
    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 14px;
      border-radius: 2px;
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      text-decoration: none;
      font-size: var(--vscode-font-size);
      cursor: pointer;
    }
    .btn:hover {
      background: var(--vscode-button-hoverBackground);
    }
  </style>
</head>
<body>
  <img class="logo" src="${logoUri}" alt="Microsoft Quantum" />
  <h2>QDK Learning</h2>
  <p>
    Interactive courses for quantum computing — from introductory
    tutorials to advanced research topics.
  </p>
  <p>
    Follow guided lessons, solve hands-on exercises, and build
    practical skills at your own pace, right in your editor.
  </p>
  <a class="btn" href="command:qsharp-vscode.katasContinue"><span class="codicon codicon-sparkle"></span> Get started</a>
</body>
</html>`;
}
