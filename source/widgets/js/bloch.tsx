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
  const onChange = () => {
    const initialGates = model.get("initial_gates") as string;
    prender(
      <BlochSphere
        initialGates={initialGates}
        onGatesChanged={(gates) => {
          model.set("gates", gates);
          model.save_changes();
        }}
      ></BlochSphere>,
      el,
    );
  };

  onChange();
  model.on("change:initial_gates", onChange);
}

export default {
  render,
};
