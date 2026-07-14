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

// The webview's persisted state. VS Code retains this across window reloads
// but discards it when the panel is properly closed, so the sphere is only
// restored on reload and starts fresh from |0> when reopened. The applied-gate
// sequence is the canonical representation of the sphere's state; everything
// else the widget shows is derived from it.
type BlochWebviewState = { gates?: string };

function main() {
  const persisted = vscodeApi.getState() as BlochWebviewState | undefined;
  const initialGates = persisted?.gates ?? "";

  const onGatesChanged = (gates: string) => {
    // Persist the latest gate sequence so a window reload replays back to the
    // same sphere state.
    vscodeApi.setState({ gates } satisfies BlochWebviewState);
  };

  render(
    <BlochSphere initialGates={initialGates} onGatesChanged={onGatesChanged} />,
    document.body,
  );
  detectThemeChange(document.body, (isDark: boolean) => {
    updateStyleSheetTheme(
      isDark,
      "github-markdown",
      /(light\.css)|(dark\.css)/,
      "light.css",
      "dark.css",
    );
  });
}
