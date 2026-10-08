// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { h, render } from "preact";
import { draw } from "../dist/ux/circuit-vis/index.js";
import { Circuit } from "../dist/ux/circuit.js";
import { renderCircuitSvg } from "../dist/ux/circuitSvgExport.js";

test("renderCircuitSvg creates deterministic standalone SVG without a DOM", () => {
  assert.equal(globalThis.document, undefined);
  const circuit = representativeCircuit();
  const before = JSON.stringify(circuit);

  const first = renderCircuitSvg(circuit, {
    title: "Parameterized Bell circuit",
    description: "A Bell circuit with an Rx gate and measurement.",
  });
  const second = renderCircuitSvg(circuit, {
    title: "Parameterized Bell circuit",
    description: "A Bell circuit with an Rx gate and measurement.",
  });

  assert.equal(first, second);
  assert.equal(JSON.stringify(circuit), before);
  assert.match(first, /^<\?xml version="1\.0" encoding="UTF-8"\?>\n/);
  assert.match(first, /data:font\/woff2;base64,/);
  assert.doesNotMatch(first, /Segoe WPC/);
  assert.doesNotMatch(first, /gate-control/);
  assert.doesNotMatch(first, /\sdata-[A-Za-z0-9_-]+=/);
  assert.doesNotMatch(first, /<script|<foreignObject|<a(?:\s|>)/);

  const parsed = parseSvg(first);
  assert.equal(parsed.localName, "svg");
  assert.equal(parsed.getAttribute("xmlns"), "http://www.w3.org/2000/svg");
  assert.equal(parsed.getAttribute("version"), "1.1");
  assert.equal(parsed.getAttribute("role"), "img");
  assert.equal(
    parsed.getAttribute("aria-labelledby"),
    "qdk-circuit-title qdk-circuit-description",
  );
  assert.equal(
    parsed.querySelector("title")?.textContent,
    "Parameterized Bell circuit",
  );
  assert.equal(
    parsed.querySelector("desc")?.textContent,
    "A Bell circuit with an Rx gate and measurement.",
  );

  const width = parsed.getAttribute("width");
  const height = parsed.getAttribute("height");
  assert.ok(width);
  assert.ok(height);
  assert.equal(parsed.getAttribute("viewBox"), `0 0 ${width} ${height}`);
  assert.equal(parsed.getAttribute("preserveAspectRatio"), "xMinYMin meet");
  assert.equal(parsed.querySelector(".qdk-circuit-export-background"), null);
  assert.ok(parsed.querySelector(".oplus"));
  assert.ok(parsed.querySelector(".gate-measure"));

  const argument = parsed.querySelector(".arg-button");
  assert.ok(argument);
  assert.ok(argument.classList.contains("qs-maintext"));
  assert.match(argument.textContent ?? "", /π\/2/);
  assert.equal(argument.getAttribute("font-family"), null);
});

test("renderCircuitSvg supports reference fonts and a solid background", () => {
  const exported = renderCircuitSvg(representativeCircuit(), {
    background: "solid",
    fontMode: "reference",
  });
  const parsed = parseSvg(exported);
  const style = parsed.querySelector("style")?.textContent ?? "";

  assert.doesNotMatch(style, /data:font\/woff2;base64,/);
  assert.match(style, /"KaTeX_Main"/);
  assert.match(style, /"KaTeX_Math"/);
  assert.equal(
    parsed
      .querySelector(".qdk-circuit-export-background")
      ?.getAttribute("fill"),
    null,
  );
});

test("renderCircuitSvg applies explicit expansion policies", () => {
  const circuit = groupedCircuit();
  const collapsed = parseSvg(
    renderCircuitSvg(circuit, {
      expansion: "collapsed",
      fontMode: "reference",
    }),
  );
  const expanded = parseSvg(
    renderCircuitSvg(circuit, {
      expansion: "expanded",
      fontMode: "reference",
    }),
  );

  assert.equal(collapsed.textContent?.includes("ChildH"), false);
  assert.equal(expanded.textContent?.includes("ChildH"), true);
  assert.equal(collapsed.querySelector(".gate-control"), null);
  assert.equal(expanded.querySelector(".gate-control"), null);
});

test("renderCircuitSvg escapes labels and rejects invalid XML", () => {
  const circuit = singleQubitCircuit("<A&B>");
  const exported = renderCircuitSvg(circuit, { fontMode: "reference" });
  assert.match(exported, /&lt;/);
  assert.match(exported, /&amp;/);
  assert.equal(parseSvg(exported).textContent?.includes("<A&B>"), true);

  assert.throws(
    () =>
      renderCircuitSvg(singleQubitCircuit(`bad\u0000label`), {
        fontMode: "reference",
      }),
    /invalid XML character U\+0000/,
  );
});

test("renderCircuitSvg validates model rendering options", () => {
  assert.throws(
    () =>
      renderCircuitSvg(
        {
          version: 2,
          circuits: [singleQubitCircuit("H")],
        },
        { circuitIndex: 1 },
      ),
    /outside the available range/,
  );
  assert.throws(
    () => renderCircuitSvg(singleQubitCircuit("H"), { renderDepth: -1 }),
    /non-negative integer/,
  );
  assert.throws(
    () =>
      renderCircuitSvg(singleQubitCircuit("H"), {
        idPrefix: "invalid prefix",
      }),
    /Invalid SVG id prefix/,
  );
  assert.throws(
    () =>
      renderCircuitSvg(singleQubitCircuit("H"), {
        // @ts-expect-error JavaScript callers can pass values outside the TypeScript union.
        expansion: "invalid",
      }),
    /Circuit SVG expansion/,
  );
  assert.throws(
    () =>
      renderCircuitSvg(singleQubitCircuit("H"), {
        // @ts-expect-error JavaScript callers can pass values outside the TypeScript union.
        background: "invalid",
      }),
    /Circuit SVG background/,
  );
  assert.throws(
    () =>
      renderCircuitSvg(singleQubitCircuit("H"), {
        // @ts-expect-error JavaScript callers can pass values outside the TypeScript union.
        fontMode: "invalid",
      }),
    /Circuit SVG font mode/,
  );
});

test("draw and direct rendering share the same SVG path", async () => {
  await withDom(async (document) => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const circuit = representativeCircuit();
    const renderer = draw(
      {
        version: 2,
        circuits: [circuit],
      },
      container,
    );
    const options = {
      title: "Shared renderer",
      fontMode: /** @type {const} */ ("reference"),
    };

    assert.equal(
      renderer.exportSvg(options),
      renderCircuitSvg(circuit, options),
    );
    assert.throws(
      () =>
        renderer.exportSvg(
          // @ts-expect-error Renderer-owned exports cannot select another circuit.
          { circuitIndex: 1 },
        ),
      /only supported by renderCircuitSvg/,
    );
    assert.ok(container.querySelector("svg.qviz"));
    assert.ok(container.querySelector("[data-location]"));
  });
});

test("renderer export preserves the current expansion state", async () => {
  await withDom(async (document) => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const renderer = draw(
      {
        version: 2,
        circuits: [groupedCircuit()],
      },
      container,
    );

    assert.equal(
      parseSvg(
        renderer.exportSvg({ fontMode: "reference" }),
      ).textContent?.includes("ChildH"),
      false,
    );
    const expand = container.querySelector(".gate-expand");
    assert.ok(expand);
    expand.dispatchEvent(new document.defaultView.Event("click"));

    assert.equal(
      parseSvg(
        renderer.exportSvg({ fontMode: "reference" }),
      ).textContent?.includes("ChildH"),
      true,
    );
  });
});

test("Circuit exports through its host callback", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    /** @type {{ contents: string; suggestedName: string } | undefined} */
    let saved;
    render(
      h(Circuit, {
        title: "Bell pair",
        circuit: representativeCircuit(),
        onExportSvg: (contents, suggestedName) => {
          saved = { contents, suggestedName };
        },
      }),
      host,
    );

    await waitFor(() => host.querySelector("button")?.disabled === false);
    const button = host.querySelector("button");
    assert.equal(button?.textContent, "Export as SVG");
    button?.click();
    await waitFor(() => saved !== undefined);

    assert.equal(saved?.suggestedName, "bell-pair.svg");
    assert.equal(
      parseSvg(saved?.contents ?? "").querySelector("title")?.textContent,
      "Bell pair",
    );
    render(null, host);
  });
});

test("Circuit bounds host export filenames for long titles", async () => {
  await withDom(async (document) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    let suggestedName;
    render(
      h(Circuit, {
        title: "a".repeat(200),
        circuit: singleQubitCircuit("H"),
        onExportSvg: (_contents, name) => {
          suggestedName = name;
        },
      }),
      host,
    );

    await waitFor(() => host.querySelector("button")?.disabled === false);
    host.querySelector("button")?.click();
    await waitFor(() => suggestedName !== undefined);

    assert.equal(suggestedName?.length, 128);
    assert.equal(suggestedName?.endsWith(".svg"), true);
    render(null, host);
  });
});

test("Circuit downloads in browser hosts without a save callback", async () => {
  await withDom(async (document, window) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    let downloadedName;
    let revokedUrl;
    window.URL.createObjectURL = () => "blob:qdk-circuit";
    window.URL.revokeObjectURL = (url) => {
      revokedUrl = url;
    };
    const originalClick = window.HTMLAnchorElement.prototype.click;
    window.HTMLAnchorElement.prototype.click = function () {
      downloadedName = this.download;
    };

    try {
      render(
        h(Circuit, {
          title: "Browser circuit",
          circuit: singleQubitCircuit("H"),
        }),
        host,
      );
      await waitFor(() => host.querySelector("button")?.disabled === false);
      host.querySelector("button")?.click();
      await waitFor(() => downloadedName !== undefined);
      await waitFor(() => revokedUrl !== undefined);

      assert.equal(downloadedName, "browser-circuit.svg");
      assert.equal(revokedUrl, "blob:qdk-circuit");
    } finally {
      window.HTMLAnchorElement.prototype.click = originalClick;
      render(null, host);
    }
  });
});

function representativeCircuit() {
  return {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            controls: [{ qubit: 0 }],
            targets: [{ qubit: 1 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Rx",
            args: ["π/2"],
            targets: [{ qubit: 1 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  };
}

function groupedCircuit() {
  return {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "ChildH",
                    targets: [{ qubit: 0 }],
                  },
                ],
              },
            ],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 0 }],
          },
        ],
      },
    ],
  };
}

function singleQubitCircuit(gate) {
  return {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate,
            targets: [{ qubit: 0 }],
          },
        ],
      },
    ],
  };
}

function parseSvg(contents) {
  return new JSDOM(contents, {
    contentType: "image/svg+xml",
  }).window.document.documentElement;
}

async function withDom(callback) {
  const jsdom = new JSDOM("<!doctype html><html><body></body></html>", {
    pretendToBeVisual: true,
  });
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    Node: globalThis.Node,
    HTMLElement: globalThis.HTMLElement,
    SVGElement: globalThis.SVGElement,
  };
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  try {
    await callback(jsdom.window.document, jsdom.window);
  } finally {
    for (const [name, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete globalThis[name];
      } else {
        globalThis[name] = value;
      }
    }
    jsdom.window.close();
  }
}

async function waitFor(predicate) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("Timed out waiting for the circuit UI.");
}
