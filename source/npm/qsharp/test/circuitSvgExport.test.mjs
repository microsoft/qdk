// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { h, render } from "preact";
import { draw } from "../dist/ux/circuit-vis/index.js";
import { Circuit } from "../dist/ux/circuit.js";
test("renderer exports the full mounted SVG without changing its geometry or zoom", async () => {
  await withDom(async (document) => {
    const circuit = representativeCircuit();
    const before = JSON.stringify(circuit);
    const { renderer, container } = drawCircuit(document, circuit);
    renderer.userSetZoomLevel(25);
    const mounted = container.querySelector("svg.qviz");
    const markup = container.innerHTML;
    const options = {
      title: "Parameterized Bell circuit",
      description: "A Bell circuit with an Rx gate and measurement.",
    };
    const first = renderer.exportSvg(options);
    const parsed = parseSvg(first);

    assert.equal(first, renderer.exportSvg(options));
    assert.equal(JSON.stringify(circuit), before);
    assert.equal(container.innerHTML, markup);
    assert.deepEqual(geometry(parsed), geometry(mounted));
    assert.equal(parsed.getAttribute("width"), mounted.getAttribute("width"));
    assert.equal(parsed.getAttribute("height"), mounted.getAttribute("height"));
    assert.equal(
      parsed.getAttribute("viewBox"),
      mounted.getAttribute("viewBox"),
    );
    assert.equal(parsed.getAttribute("style"), null);
    assert.match(first, /^<\?xml version="1\.0" encoding="UTF-8"\?>\n/);
    assert.match(first, /@font-face\{font-family:"KaTeX_Main";/);
    assert.match(first, /@font-face\{font-family:"KaTeX_Math";/);
    assert.match(first, /data:font\/woff2;base64,/);
    assert.doesNotMatch(first, /QDKCircuitMain|QDKCircuitMath|var\(--/);
    assert.doesNotMatch(first, /gate-control|\sdata-[A-Za-z0-9_-]+=/);
    assert.equal(parsed.getAttribute("xmlns"), "http://www.w3.org/2000/svg");
    assert.equal(parsed.getAttribute("role"), "img");
    assert.equal(
      parsed.getAttribute("aria-labelledby"),
      "qdk-circuit-title qdk-circuit-description",
    );
    assert.equal(parsed.querySelector("title")?.textContent, options.title);
    assert.equal(
      parsed.querySelector("desc")?.textContent,
      options.description,
    );
    assert.equal(parsed.querySelector(".qdk-circuit-export-background"), null);
    assert.ok(parsed.querySelector(".oplus"));
    assert.ok(parsed.querySelector(".gate-measure"));
    assert.match(
      parsed.querySelector(".arg-button")?.textContent ?? "",
      /π\/2/,
    );
    assert.ok(container.querySelector("[data-location]"));
  });
});

test("renderer export removes editor controls and source links only from the copy", async () => {
  await withDom(async (document) => {
    const { renderer, container } = drawCircuit(document, groupedCircuit());
    const svg = container.querySelector("svg.qviz");
    const namespace = "http://www.w3.org/2000/svg";
    const overlay = document.createElementNS(namespace, "g");
    overlay.classList.add("editor-overlay");
    overlay.appendChild(document.createElementNS(namespace, "rect"));
    svg.appendChild(overlay);
    const link = document.createElementNS(namespace, "a");
    link.setAttribute("href", "command:qsharp-vscode.open-source");
    const input = svg.firstElementChild;
    svg.prepend(link);
    link.appendChild(input);
    const before = container.innerHTML;

    const parsed = parseSvg(renderer.exportSvg());
    assert.equal(parsed.querySelector("a,.editor-overlay,.gate-control"), null);
    assert.deepEqual(geometry(parsed), geometry(svg));
    assert.equal(container.innerHTML, before);
  });
});

test("standalone SWAP hitboxes do not become black rectangles in SVG viewers", async () => {
  await withDom(async (document) => {
    const circuit = representativeCircuit();
    circuit.componentGrid.push({
      components: [
        {
          kind: "unitary",
          gate: "SWAP",
          targets: [{ qubit: 0 }, { qubit: 1 }],
        },
      ],
    });
    const { renderer } = drawCircuit(document, circuit);
    const parsed = parseSvg(renderer.exportSvg());
    assert.ok(parsed.querySelector("rect.gate-swap"));
    assert.match(
      parsed.querySelector("style")?.textContent ?? "",
      /rect\.gate-swap\{fill:none;stroke:none\}/,
    );
  });
});

test("renderer export escapes labels and accessible titles", async () => {
  await withDom(async (document) => {
    const { renderer } = drawCircuit(document, singleQubitCircuit("<A&B>"));
    const exported = renderer.exportSvg({ title: "A & B" });
    assert.match(exported, /&lt;/);
    assert.match(exported, /&amp;/);
    const parsed = parseSvg(exported);
    assert.equal(parsed.textContent?.includes("<A&B>"), true);
    assert.equal(parsed.querySelector("title")?.textContent, "A & B");
  });
});

test("renderer export rejects invalid XML characters instead of saving a broken SVG", async () => {
  await withDom(async (document) => {
    for (const label of ["bad\u0000label", "bad\uD800label"]) {
      const { renderer } = drawCircuit(document, singleQubitCircuit(label));
      assert.throws(() => renderer.exportSvg(), /invalid XML character/);
    }

    const { renderer } = drawCircuit(document, singleQubitCircuit("H"));
    assert.throws(
      () => renderer.exportSvg({ title: "bad\u0000title" }),
      /invalid XML character/,
    );
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
      parseSvg(renderer.exportSvg()).textContent?.includes("ChildH"),
      false,
    );
    const expand = container.querySelector(".gate-expand");
    assert.ok(expand);
    expand.dispatchEvent(new document.defaultView.Event("click"));

    assert.equal(
      parseSvg(renderer.exportSvg()).textContent?.includes("ChildH"),
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

function drawCircuit(document, circuit) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const renderer = draw({ version: 2, circuits: [circuit] }, container);
  return { renderer, container };
}

function geometry(svg) {
  return Array.from(svg.querySelectorAll("*"))
    .filter(
      (element) =>
        ["line", "rect", "circle", "path", "text", "tspan"].includes(
          element.localName,
        ) && !element.closest(".gate-control,.editor-overlay"),
    )
    .map((element) => ({
      tag: element.localName,
      attributes: Array.from(element.attributes)
        .filter((attribute) => !attribute.name.startsWith("data-"))
        .map((attribute) => [attribute.name, attribute.value]),
      text: ["text", "tspan"].includes(element.localName)
        ? element.textContent
        : undefined,
    }));
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
    XMLSerializer: globalThis.XMLSerializer,
  };
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.XMLSerializer = jsdom.window.XMLSerializer;
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
