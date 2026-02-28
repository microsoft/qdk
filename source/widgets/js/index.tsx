// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { render as prender } from "preact";
import {
  ReTable,
  SpaceChart,
  Histogram,
  histogramToSvg,
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
  chordDiagramToSvg,
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
    case "OrbitalEntanglement":
      renderChordDiagram({ model, el });
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

function histogramPropsFromModel(model: AnyModel) {
  const buckets = model.get("buckets") as { [key: string]: number };
  const bucketMap = new Map(Object.entries(buckets));
  const shotCount = model.get("shot_count") as number;
  const shotHeader = model.get("shot_header") as boolean;
  const labels = model.get("labels") as "raw" | "kets" | "none";
  const items = model.get("items") as "all" | "top-10" | "top-25";
  const sort = model.get("sort") as "a-to-z" | "high-to-low" | "low-to-high";
  return { bucketMap, shotCount, shotHeader, labels, items, sort };
}

function renderHistogram({ model, el }: RenderArgs) {
  const onChange = () => {
    const { bucketMap, shotCount, shotHeader, labels, items, sort } =
      histogramPropsFromModel(model);

    prender(
      <Histogram
        data={bucketMap}
        shotCount={shotCount}
        filter={""}
        onFilter={() => undefined}
        shotsHeader={shotHeader}
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

  // Handle SVG export requests from Python
  model.on("msg:custom", (msg: Record<string, unknown>) => {
    if (msg.type === "export_svg") {
      const { bucketMap, shotCount, labels, items, sort } =
        histogramPropsFromModel(model);
      const svg = histogramToSvg({
        data: bucketMap,
        shotCount,
        filter: "",
        labels,
        items,
        sort,
        darkMode: msg.dark_mode as boolean | undefined,
      });
      model.send({ type: "svg_result", svg });
    }
  });
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

function chordPropsFromModel(model: AnyModel) {
  const nodeValues = model.get("node_values") as number[];
  const pairwiseWeights = model.get("pairwise_weights") as number[][];
  const labels = model.get("labels") as string[];
  const selectedIndices = model.get("selected_indices") as number[] | null;
  const options = (model.get("options") || {}) as Record<string, unknown>;
  return { nodeValues, pairwiseWeights, labels, selectedIndices, options };
}

function renderChordDiagram({ model, el }: RenderArgs) {
  const onChange = () => {
    const { nodeValues, pairwiseWeights, labels, selectedIndices, options } =
      chordPropsFromModel(model);

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
        onGroupChange={(grouped) => {
          const newOpts = { ...options, group_selected: grouped };
          model.set("options", newOpts);
          model.save_changes();
        }}
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

  // Handle SVG export requests from Python — same rendering function
  // used by the Node SSR script, executed here in-browser so no
  // subprocess is needed when the widget is live.
  model.on("msg:custom", (msg: Record<string, unknown>) => {
    if (msg.type === "export_svg") {
      const { nodeValues, pairwiseWeights, labels, selectedIndices, options } =
        chordPropsFromModel(model);
      const svg = chordDiagramToSvg({
        nodeValues,
        pairwiseWeights,
        labels,
        selectedIndices: selectedIndices ?? undefined,
        darkMode: msg.dark_mode as boolean | undefined,
        ...snakeToCamelOptions(options),
      });
      model.send({ type: "svg_result", svg });
    }
  });
}

/** Convert a snake_case options dict to camelCase props. */
function snakeToCamelOptions(
  options: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, val] of Object.entries(options)) {
    const camel = key.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
    result[camel] = val;
  }
  return result;
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
