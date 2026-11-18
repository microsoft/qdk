// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import { getCircuitSvgSaveMessageError } from "../../../src/circuitSvgProtocol.js";

type SvgParts = {
  root?: string;
  label?: string;
  css?: string;
  markup?: string;
};

const UNSAFE = "The generated file is not a safe standalone SVG.";
const EMBEDDED_FONT =
  '@font-face{font-family:"KaTeX_Main";src:url(data:font/woff2;base64,d09GMgABAAAAAA==) format("woff2")}';

// The shape the exporter writes: a style sheet, glyph outlines in defs, and labels that reuse them.
function saveMessage({
  root = "",
  label = "",
  css = "",
  markup = "",
}: SvgParts = {}) {
  return {
    type: "qdk.circuit/save-svg",
    suggestedName: "circuit.svg",
    contents:
      '<?xml version="1.0" encoding="UTF-8"?>\n' +
      `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"${root}>` +
      `<title>Quantum circuit</title><style>${css}</style>` +
      '<defs><path id="qdk-glyph-main-48" d="M0 0L1 1Z"/></defs>' +
      `<g class="gate"${label}><use xlink:href="#qdk-glyph-main-48" transform="matrix(0.01 0 0 0.01 1 2)"/></g>` +
      `${markup}</svg>\n`,
  };
}

suite("Circuit SVG save validation", () => {
  test("accepts outlined labels, and labels kept as text with embedded fonts", () => {
    assert.isUndefined(getCircuitSvgSaveMessageError(saveMessage()));
    assert.isUndefined(
      getCircuitSvgSaveMessageError(
        saveMessage({
          root: ' data-qdk-text-fallback="U+00E9"',
          css: `${EMBEDDED_FONT}.qdk-circuit-export text{fill:currentColor;dominant-baseline:middle}`,
          markup: '<text class="qs-maintext" x="10" y="20">é</text>',
        }),
      ),
    );
  });

  const remote = "https://example.invalid/remote.svg";
  const unsafe: [string, SvgParts][] = [
    ["xml:base on the root", { root: ` xml:base="${remote}"` }],
    ["xml:base on a label", { label: ` xml:base="${remote}"` }],
    ["an event handler", { root: ' onload="alert(1)"' }],
    [
      "an external glyph reference",
      { markup: `<use xlink:href="${remote}#qdk-glyph-main-48"/>` },
    ],
    [
      "an element the exporter never writes",
      { markup: "<script>alert(1)</script>" },
    ],
    ["markup other than elements", { markup: "<!-- note -->" }],
    [
      "a style sheet split by an element",
      { markup: "<style>.a{}<g/></style>" },
    ],
    ["a style sheet import", { css: `@import "${remote}";` }],
    ["a remote url in a style sheet", { css: `text{fill:url(${remote}#p)}` }],
    [
      "a remote font beside an embedded one",
      { css: EMBEDDED_FONT.replace("}", `,url(${remote})}`) },
    ],
    ["a CSS escape", { css: `text{fill:u\\72l(${remote}#p)}` }],
    ["a CSS comment", { css: `text{fill:url/**/(${remote}#p)}` }],
    ["a character reference", { css: `text{fill:url&#40;${remote}#p)}` }],
    [
      "a remote url in a style attribute",
      { label: ` style="fill:url(${remote}#p)"` },
    ],
  ];
  for (const [name, parts] of unsafe) {
    test(`rejects ${name}`, () => {
      assert.strictEqual(
        getCircuitSvgSaveMessageError(saveMessage(parts)),
        UNSAFE,
      );
    });
  }
});
