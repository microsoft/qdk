// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { render as prender } from "preact";
import {
  ReTable,
  SpaceChart,
  Histogram,
  CreateSingleEstimateResult,
  EstimatesOverview,
  EstimatesPanel,
  ReData,
  Circuit,
  setRenderer,
  Atoms,
  type ZoneLayout,
  type TraceData,
  MoleculeViewer,
  ChordDiagram,
  OrbitalEntanglement,
} from "qsharp-lang/ux";
import markdownIt from "markdown-it";
import "./widgets.css";

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";

const md = markdownIt();
md.use(mk);
setRenderer((input: string) => md.render(input));

export function mdRenderer(input: string) {
  // Note: Need to ensure this 'fix' is still needed with the latest data JSON.
  // In early testing backslashes were being double-escaped in the results.
  return md.render(input.replace(/\\\\/g, "\\"));
}

// Param types for AnyWidget render functions
import type { AnyModel } from "@anywidget/types";

type RenderArgs = {
  model: AnyModel;
  el: HTMLElement;
};

function render({ model, el }: RenderArgs) {
  const componentType = model.get("comp");

  // There is an existing issue where in VS Code it always shows the widget background as white.
  // (See https://github.com/microsoft/vscode-jupyter/issues/7161)
  // We tried to fix this in CSS by overriding the style, but there is a race condition whereby
  // depending on which style gets injected first (ours or ipywidgets), it may or may not work.

  // The solution here is to force our own override to be last in the style list if not already.
  // It's a bit of a hack, but it works, and I couldn't find something better that wouldn't be fragile.

  if (
    !el.ownerDocument.head.lastChild?.textContent?.includes("widget-css-fix")
  ) {
    const forceStyle = el.ownerDocument.createElement("style");
    forceStyle.textContent = `/* widget-css-fix */ .cell-output-ipywidget-background {background-color: transparent !important;}`;
    el.ownerDocument.head.appendChild(forceStyle);
  }

  // Belt-and-suspenders: also set the background inline on the nearest
  // ipywidget container (if any) so it wins regardless of CSS load order.
  const bgContainer = el.closest(".cell-output-ipywidget-background");
  if (bgContainer instanceof HTMLElement) {
    bgContainer.style.backgroundColor = "transparent";
  }

  switch (componentType) {
    case "SpaceChart":
      renderChart({ model, el });
      break;
    case "EstimatesOverview":
      renderEstimatesOverview({ model, el });
      break;
    case "EstimateDetails":
      renderTable({ model, el });
      break;
    case "Histogram":
      renderHistogram({ model, el });
      break;
    case "EstimatesPanel":
      renderEstimatesPanel({ model, el });
      break;
    case "Circuit":
      renderCircuit({ model, el });
      break;
    case "Atoms":
      renderAtoms({ model, el });
      break;
    case "MoleculeViewer":
      renderMoleculeViewer({ model, el });
      break;
    case "ChordDiagram":
      renderChordDiagram({ model, el });
      break;
    case "OrbitalEntanglement":
      renderOrbitalEntanglement({ model, el });
      break;
    default:
      throw new Error(`Unknown component type ${componentType}`);
  }
}

export default {
  render,
};

function renderTable({ model, el }: RenderArgs) {
  const onChange = () => {
    const estimates = model.get("estimates");
    const index = model.get("index");
    const singleEstimateResult = CreateSingleEstimateResult(estimates, index);
    prender(
      <ReTable
        estimatesData={singleEstimateResult}
        mdRenderer={mdRenderer}
      ></ReTable>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:index", onChange);
}

function renderChart({ model, el }: RenderArgs) {
  const onChange = () => {
    const estimates = model.get("estimates");
    const index = model.get("index");
    const singleEstimateResult = CreateSingleEstimateResult(estimates, index);
    prender(<SpaceChart estimatesData={singleEstimateResult}></SpaceChart>, el);
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:index", onChange);
}

function renderEstimatesOverview({ model, el }: RenderArgs) {
  const onChange = () => {
    const results = model.get("estimates");
    const colors = model.get("colors");
    const runNames = model.get("runNames");

    let estimates = [];
    if (results[0] == null) {
      estimates.push(results);
    } else {
      for (const estimate of Object.values(results)) {
        estimates.push(estimate);
      }
    }

    const onRowDeleted = createOnRowDeleted(estimates, (newEstimates) => {
      estimates = newEstimates;
      model.set("estimates", estimates);
    });

    prender(
      <EstimatesOverview
        estimatesData={estimates}
        runNames={runNames}
        colors={colors}
        isSimplifiedView={true}
        onRowDeleted={onRowDeleted}
        setEstimate={() => undefined}
        allowSaveImage={true}
      ></EstimatesOverview>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:colors", onChange);
  model.on("change:runNames", onChange);
}

function renderEstimatesPanel({ model, el }: RenderArgs) {
  const onChange = () => {
    const results = model.get("estimates");
    const colors = model.get("colors");
    const runNames = model.get("runNames");

    let estimates: ReData[] = [];
    if (results[0] == null) {
      estimates.push(results);
    } else {
      for (const estimate of Object.values(results)) {
        estimates.push(estimate as ReData);
      }
    }

    const onRowDeleted = createOnRowDeleted(estimates, (newEstimates) => {
      estimates = newEstimates;
      model.set("estimates", estimates);
    });

    prender(
      <EstimatesPanel
        estimatesData={estimates}
        runNames={runNames}
        colors={colors}
        renderer={mdRenderer}
        calculating={false}
        onRowDeleted={onRowDeleted}
        allowSaveImage={true}
      ></EstimatesPanel>,
      el,
    );
  };

  onChange();
  model.on("change:estimates", onChange);
  model.on("change:colors", onChange);
  model.on("change:runNames", onChange);
}

function createOnRowDeleted(
  estimates: ReData[],
  setEstimates: (estimates: ReData[]) => void,
) {
  return (rowId: string) => {
    // Clone estimates into a new object
    const newEstimates = JSON.parse(JSON.stringify(estimates)) as ReData[];

    // Splice out the estimate that was deleted
    const index = newEstimates.findIndex(
      (estimate) => estimate.jobParams.runName === rowId,
    );
    if (index >= 0) {
      newEstimates.splice(index, 1);
    }

    setEstimates(newEstimates);
  };
}

function renderHistogram({ model, el }: RenderArgs) {
  const onChange = () => {
    const buckets = model.get("buckets") as { [key: string]: number };
    const bucketMap = new Map(Object.entries(buckets));
    const shot_count = model.get("shot_count") as number;
    const shot_header = model.get("shot_header") as boolean;
    const labels = model.get("labels") as "raw" | "kets" | "none";
    const items = model.get("items") as "all" | "top-10" | "top-25";
    const sort = model.get("sort") as "a-to-z" | "high-to-low" | "low-to-high";

    prender(
      <Histogram
        data={bucketMap}
        shotCount={shot_count}
        filter={""}
        onFilter={() => undefined}
        shotsHeader={shot_header}
        labels={labels}
        items={items}
        sort={sort}
        onSettingsChange={(settings) => {
          model.set("labels", settings.labels);
          model.set("items", settings.items);
          model.set("sort", settings.sort);
          model.save_changes();
        }}
      ></Histogram>,
      el,
    );
  };

  onChange();
  model.on("change:buckets", onChange);
  model.on("change:shot_count", onChange);
  model.on("change:shot_header", onChange);
  model.on("change:labels", onChange);
  model.on("change:items", onChange);
  model.on("change:sort", onChange);

  // Cache the live SVG into a traitlet whenever the histogram DOM changes
  let svgCacheTimer: ReturnType<typeof setTimeout> | null = null;
  const cacheLiveSvg = () => {
    if (svgCacheTimer) clearTimeout(svgCacheTimer);
    svgCacheTimer = setTimeout(() => {
      const svg = serializeLiveSvg(el);
      if (svg) {
        model.set("_live_svg", svg);
        model.save_changes();
      }
    }, 200);
  };

  const observer = new MutationObserver(cacheLiveSvg);
  observer.observe(el, { childList: true, subtree: true });
}

function renderCircuit({ model, el }: RenderArgs) {
  const onChange = () => {
    const circuitJson = model.get("circuit_json") as string;
    prender(
      <Circuit
        circuit={JSON.parse(circuitJson)}
        renderLocations={(locations) => {
          return {
            title: locations
              .map((loc) => `${loc.file}:${loc.line}:${loc.column}`)
              .join("\n"),
            href: "#",
          };
        }}
      ></Circuit>,
      el,
    );
  };

  onChange();
  model.on("change:circuit_json", onChange);

  // Cache the live SVG into a traitlet whenever the circuit DOM changes
  // (initial render, expand/collapse, etc.).  Debounced to avoid
  // excessive updates during rapid DOM mutations.
  let svgCacheTimer: ReturnType<typeof setTimeout> | null = null;
  const cacheLiveSvg = () => {
    if (svgCacheTimer) clearTimeout(svgCacheTimer);
    svgCacheTimer = setTimeout(() => {
      const svg = serializeLiveSvg(el);
      if (svg) {
        model.set("_live_svg", svg);
        model.save_changes();
      }
    }, 200);
  };

  const observer = new MutationObserver(cacheLiveSvg);
  observer.observe(el, {
    childList: true,
    subtree: true,
    attributes: true,
    characterData: true,
  });
}

function renderAtoms({ model, el }: RenderArgs) {
  const onChange = () => {
    const machineLayout = model.get("machine_layout") as ZoneLayout;
    const traceData = model.get("trace_data") as TraceData;

    if (!machineLayout || !traceData) {
      return;
    }

    Atoms(el, machineLayout, traceData);
  };

  onChange();
  model.on("change:machine_layout", onChange);
  model.on("change:trace_data", onChange);
}

function renderChordDiagram({ model, el }: RenderArgs) {
  const onChange = () => {
    const nodeValues = model.get("node_values") as number[];
    const pairwiseWeights = model.get("pairwise_weights") as number[][];
    const labels = model.get("labels") as string[];
    const selectedIndices = model.get("selected_indices") as number[] | null;
    const options = (model.get("options") || {}) as Record<string, unknown>;

    prender(
      <ChordDiagram
        nodeValues={nodeValues}
        pairwiseWeights={pairwiseWeights}
        labels={labels}
        selectedIndices={selectedIndices ?? undefined}
        gapDeg={options.gap_deg as number | undefined}
        radius={options.radius as number | undefined}
        arcWidth={options.arc_width as number | undefined}
        lineScale={options.line_scale as number | null | undefined}
        edgeThreshold={options.edge_threshold as number | undefined}
        nodeVmax={options.node_vmax as number | null | undefined}
        edgeVmax={options.edge_vmax as number | null | undefined}
        nodeColormap={
          options.node_colormap as [string, string, string] | undefined
        }
        edgeColormap={
          options.edge_colormap as [string, string, string] | undefined
        }
        nodeColorbarLabel={
          options.node_colorbar_label as string | null | undefined
        }
        edgeColorbarLabel={
          options.edge_colorbar_label as string | null | undefined
        }
        nodeHoverPrefix={options.node_hover_prefix as string | undefined}
        edgeHoverPrefix={options.edge_hover_prefix as string | undefined}
        title={options.title as string | null | undefined}
        width={options.width as number | undefined}
        height={options.height as number | undefined}
        selectionColor={options.selection_color as string | undefined}
        selectionLinewidth={options.selection_linewidth as number | undefined}
        groupSelected={options.group_selected as boolean | undefined}
      />,
      el,
    );
  };

  onChange();
  model.on("change:node_values", onChange);
  model.on("change:pairwise_weights", onChange);
  model.on("change:labels", onChange);
  model.on("change:selected_indices", onChange);
  model.on("change:options", onChange);

  // Cache the live SVG into a traitlet whenever the diagram DOM changes
  let svgCacheTimer0: ReturnType<typeof setTimeout> | null = null;
  const cacheLiveSvg0 = () => {
    if (svgCacheTimer0) clearTimeout(svgCacheTimer0);
    svgCacheTimer0 = setTimeout(() => {
      const svg = serializeLiveSvg(el, ".oe-group-toggle");
      if (svg) {
        model.set("_live_svg", svg);
        model.save_changes();
      }
    }, 200);
  };

  const observer0 = new MutationObserver(cacheLiveSvg0);
  observer0.observe(el, {
    childList: true,
    subtree: true,
    attributes: true,
    characterData: true,
  });
}

function renderOrbitalEntanglement({ model, el }: RenderArgs) {
  const onChange = () => {
    const s1Entropies = model.get("s1_entropies") as number[];
    const mutualInformation = model.get("mutual_information") as number[][];
    const labels = model.get("labels") as string[];
    const selectedIndices = model.get("selected_indices") as number[] | null;
    const options = (model.get("options") || {}) as Record<string, unknown>;

    prender(
      <OrbitalEntanglement
        s1Entropies={s1Entropies}
        mutualInformation={mutualInformation}
        labels={labels}
        selectedIndices={selectedIndices ?? undefined}
        gapDeg={options.gap_deg as number | undefined}
        radius={options.radius as number | undefined}
        arcWidth={options.arc_width as number | undefined}
        lineScale={options.line_scale as number | null | undefined}
        miThreshold={options.mi_threshold as number | undefined}
        s1Vmax={options.s1_vmax as number | null | undefined}
        miVmax={options.mi_vmax as number | null | undefined}
        title={options.title as string | null | undefined}
        width={options.width as number | undefined}
        height={options.height as number | undefined}
        selectionColor={options.selection_color as string | undefined}
        selectionLinewidth={options.selection_linewidth as number | undefined}
        groupSelected={options.group_selected as boolean | undefined}
      />,
      el,
    );
  };

  onChange();
  model.on("change:s1_entropies", onChange);
  model.on("change:mutual_information", onChange);
  model.on("change:labels", onChange);
  model.on("change:selected_indices", onChange);
  model.on("change:options", onChange);

  // Cache the live SVG into a traitlet whenever the diagram DOM changes
  let svgCacheTimer: ReturnType<typeof setTimeout> | null = null;
  const cacheLiveSvg = () => {
    if (svgCacheTimer) clearTimeout(svgCacheTimer);
    svgCacheTimer = setTimeout(() => {
      const svg = serializeLiveSvg(el, ".oe-group-toggle");
      if (svg) {
        model.set("_live_svg", svg);
        model.save_changes();
      }
    }, 200);
  };

  const observer = new MutationObserver(cacheLiveSvg);
  observer.observe(el, {
    childList: true,
    subtree: true,
    attributes: true,
    characterData: true,
  });
}

/**
 * Serialize the first <svg> in the container to a string, optionally
 * removing elements matching `stripSelector` (e.g. interactive-only UI
 * that shouldn't appear in a static export).
 *
 * The resulting SVG is fully self-contained: it includes an embedded
 * `<defs><style>` block with the widget CSS so the SVG renders
 * correctly when opened outside the widget (e.g. saved to a file).
 */
function serializeLiveSvg(
  el: HTMLElement,
  stripSelector?: string,
): string | null {
  const svgEl = el.querySelector("svg");
  if (!svgEl) return null;

  // Clone so we don't mutate the live DOM
  const clone = svgEl.cloneNode(true) as SVGSVGElement;
  if (stripSelector) {
    clone.querySelectorAll(stripSelector).forEach((n) => n.remove());
  }
  // Remove interactive-only elements common across widgets
  clone
    .querySelectorAll(".menu-icon, #menu, [style*='display: none']")
    .forEach((n) => n.remove());
  // For circuit SVGs, qviz sets zoom-related inline styles
  // (max-width, width, height) that should not appear in the export.
  // Strip only those while preserving any other inline styles (e.g.
  // the OrbitalEntanglement's background setting).
  const inlineStyle = clone.getAttribute("style");
  if (inlineStyle) {
    const cleaned = inlineStyle
      .replace(/max-width:\s*[^;]+;?/gi, "")
      .replace(/\bwidth:\s*[^;]+;?/gi, "")
      .replace(/\bheight:\s*auto\s*;?/gi, "")
      .trim();
    if (cleaned) {
      clone.setAttribute("style", cleaned);
    } else {
      clone.removeAttribute("style");
    }
  }

  // Ensure xmlns is present for standalone SVG files
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");

  // Circuit SVGs have class "qviz" and rely on CSS scoped under
  // `.qs-circuit`.  Add that class to the SVG root so the selectors
  // match when the SVG is viewed standalone (without the wrapper div).
  if (clone.classList.contains("qviz")) {
    clone.classList.add("qs-circuit");
  }

  // Embed the widget CSS so the SVG is self-contained.
  // anywidget already injected our index.css into the page — we just
  // find that stylesheet and embed its full content.
  const css = getWidgetCssText();
  if (css) {
    const svgNS = "http://www.w3.org/2000/svg";
    let defs = clone.querySelector("defs");
    if (!defs) {
      defs = document.createElementNS(svgNS, "defs");
      clone.insertBefore(defs, clone.firstChild);
    }
    const styleEl = document.createElementNS(svgNS, "style");
    styleEl.textContent = css;
    defs.appendChild(styleEl);
  }

  const serializer = new XMLSerializer();
  return serializer.serializeToString(clone);
}

/**
 * Cached widget CSS text — computed once, reused for every SVG export.
 * `null` means "not yet computed", empty string means "not found".
 */
let _cachedWidgetCss: string | null = null;

/**
 * Return the full widget CSS text suitable for embedding in standalone
 * SVGs.  We find the stylesheet that anywidget injected (our index.css)
 * by checking for a known structural class (`.qs-circuit`),
 * then grab **all** its rules. This avoids maintaining a fragile
 * selector whitelist — the same CSS that styles the live widget is
 * embedded verbatim.
 *
 * CSS custom properties (--main-color etc.) are resolved to concrete
 * light-mode values so the SVG doesn't depend on the host page's
 * theme variables.
 */
function getWidgetCssText(): string {
  if (_cachedWidgetCss !== null) return _cachedWidgetCss;

  // Override CSS custom properties and background AFTER the widget CSS
  // so standalone SVGs don't pick up host-specific colours.
  const varOverrides = `:root { --main-color: #222222; --main-background: transparent; }`;

  const collectRules = (rules: CSSRuleList): string =>
    Array.from(rules)
      .map((r) => r.cssText)
      .join("\n");

  const isWidgetSheet = (rules: CSSRuleList): boolean =>
    Array.from(rules).some(
      (r) =>
        r instanceof CSSStyleRule && r.selectorText?.includes(".qs-circuit"),
    );

  try {
    // Check adoptedStyleSheets first (used by modern anywidget)
    for (const sheet of document.adoptedStyleSheets ?? []) {
      try {
        if (isWidgetSheet(sheet.cssRules)) {
          _cachedWidgetCss = `${varOverrides}\n${collectRules(sheet.cssRules)}`;
          return _cachedWidgetCss;
        }
      } catch {
        /* SecurityError */
      }
    }
    // Fall back to regular <style>/<link> stylesheets
    for (const sheet of document.styleSheets) {
      try {
        if (isWidgetSheet(sheet.cssRules)) {
          _cachedWidgetCss = `${varOverrides}\n${collectRules(sheet.cssRules)}`;
          return _cachedWidgetCss;
        }
      } catch {
        /* cross-origin SecurityError */
      }
    }
  } catch {
    /* unexpected error — degrade gracefully */
  }

  _cachedWidgetCss = varOverrides;
  return _cachedWidgetCss;
}

function renderMoleculeViewer({ model, el }: RenderArgs) {
  const onChange = () => {
    const moleculeData = model.get("molecule_data") as string;
    const cubeData = model.get("cube_data") as { [key: string]: string };
    const isoval = model.get("isoval") as number;
    prender(
      <MoleculeViewer
        moleculeData={moleculeData}
        cubeData={cubeData || {}}
        isoValue={isoval}
      ></MoleculeViewer>,
      el,
    );
  };
  onChange();
  model.on("change:molecule_data", onChange);
  model.on("change:cube_data", onChange);
  model.on("change:isoval", onChange);
}
