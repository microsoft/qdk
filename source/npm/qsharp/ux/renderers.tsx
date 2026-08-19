// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createElement } from "preact";
import { useMemo } from "preact/hooks";

// Default renderer to be replaced before components are first used.
// Expectation is that this will convert markdown and/or LaTeX to HTML.
let theRenderer = function (input: string): string {
  const err = "ERROR: Rendered has not been set";
  console.error(err);
  return err + ". " + input;
};

export function setRenderer(renderer: (input: string) => string) {
  theRenderer = renderer;
}

export function Markdown(props: {
  markdown: string;
  className?: string;
  tagName?: string;
}) {
  const tag = props.tagName || "div";
  // Cache the rendered HTML keyed on the markdown input. The renderer
  // runs markdown + KaTeX conversion, which is expensive; without this
  // memo every parent re-render (e.g. dragging an unrelated slider/dial)
  // would re-run the conversion for every Markdown instance on screen.
  // Returning a stable __html string also lets preact skip re-setting
  // innerHTML when the markdown hasn't changed.
  const html = useMemo(() => theRenderer(props.markdown), [props.markdown]);
  const nodeProps = {
    className: props.className,
    dangerouslySetInnerHTML: {
      __html: html,
    },
  };

  return createElement(tag, nodeProps);
}
