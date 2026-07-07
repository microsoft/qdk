// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="@types/vscode-webview"/>

// Dedicated entry point for the Bloch sphere panel. Keeping this separate from
// the shared webview.tsx bundle means three.js only lands in this chunk instead
// of bloating the webview.js used by every other panel.

const vscodeApi = acquireVsCodeApi();

import { render } from "preact";
import {
  BlochSphere,
  setRenderer,
  detectThemeChange,
  updateStyleSheetTheme,
} from "qsharp-lang/ux";
import "./webview.css";

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
const md = markdownIt("commonmark");
md.use(mk, {
  enableMathBlockInHtml: true,
  enableMathInlineInHtml: true,
});
setRenderer((input: string) => md.render(input));

window.addEventListener("load", main);

function main() {
  render(<BlochSphere />, document.body);
  detectThemeChange(document.body, (isDark: boolean) => {
    updateStyleSheetTheme(
      isDark,
      "github-markdown",
      /(light\.css)|(dark\.css)/,
      "light.css",
      "dark.css",
    );
  });
  vscodeApi.postMessage({ command: "ready" });
}
