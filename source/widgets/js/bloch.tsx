// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { AnyModel } from "@anywidget/types";
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import { render as prender } from "preact";

import { BlochSphere } from "../../npm/qsharp/ux/bloch/bloch.js";
import { setRenderer } from "../../npm/qsharp/ux/renderers.js";
import "./bloch.css";

const md = markdownIt();
md.use(mk);
setRenderer((input: string) => md.render(input));

type RenderArgs = {
  model: AnyModel;
  el: HTMLElement;
};

function render({ model, el }: RenderArgs) {
  // VS Code may inject an opaque widget background after the widget CSS.
  if (
    !el.ownerDocument.head.lastChild?.textContent?.includes("widget-css-fix")
  ) {
    const forceStyle = el.ownerDocument.createElement("style");
    forceStyle.textContent = `/* widget-css-fix */ .cell-output-ipywidget-background {background-color: transparent !important;}`;
    el.ownerDocument.head.appendChild(forceStyle);
  }

  const initialGates = model.get("_initial_gates") as string;
  prender(<BlochSphere initialGates={initialGates}></BlochSphere>, el);
}

export default {
  render,
};
