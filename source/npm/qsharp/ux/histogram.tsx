// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useEffect, useRef, useState } from "preact/hooks";
import { h } from "preact";
import { renderToString } from "preact-render-to-string";

// Concrete color palettes for standalone SVG export (no host CSS vars).
const lightPalette = {
  hostBackground: "#eee",
  hostForeground: "#222",
  textHighContrast: "#000",
  widgetOutline: "#ccc",
  shapeFill: "#8ab8ff",
  shapeFillSelected: "#b5c5f2",
  shapeStrokeSelected: "#587ddd",
  shapeStrokeHover: "#6b6b6b",
  menuFill: "#c4dbeb",
  menuFillHover: "#9cf",
  menuFillSelected: "#7af",
  midGray: "#888",
};

const darkPalette = {
  hostBackground: "#222",
  hostForeground: "#eee",
  textHighContrast: "#fff",
  widgetOutline: "#444",
  shapeFill: "#4aa3ff",
  shapeFillSelected: "#ffd54f",
  shapeStrokeSelected: "#ffecb3",
  shapeStrokeHover: "#c5c5c5",
  menuFill: "#444",
  menuFillHover: "#468",
  menuFillSelected: "#47a",
  midGray: "#888",
};

/** Build a <style> block that resolves all --qdk-* vars to concrete values. */
function themeStyleBlock(dark: boolean): string {
  const p = dark ? darkPalette : lightPalette;
  return `:root {
  --qdk-host-background: ${p.hostBackground};
  --qdk-host-foreground: ${p.hostForeground};
  --qdk-text-high-contrast: ${p.textHighContrast};
  --qdk-widget-outline: ${p.widgetOutline};
  --qdk-shape-fill: ${p.shapeFill};
  --qdk-shape-fill-selected: ${p.shapeFillSelected};
  --qdk-shape-stroke-selected: ${p.shapeStrokeSelected};
  --qdk-shape-stroke-hover: ${p.shapeStrokeHover};
  --qdk-menu-fill: ${p.menuFill};
  --qdk-menu-fill-hover: ${p.menuFillHover};
  --qdk-menu-fill-selected: ${p.menuFillSelected};
  --qdk-mid-gray: ${p.midGray};
  --qdk-font-family: "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --qdk-font-family-monospace: Consolas, Menlo, monospace;
}`;
}

/** Embedded CSS for standalone SVG (mirrors the histogram rules in qsharp-ux.css). */
const histogramCss = `
.histogram {
  max-height: calc(100vh - 40px);
  max-width: 600px;
  border: 1px solid var(--qdk-widget-outline);
  background-color: var(--qdk-host-background);
}
.bar { fill: var(--qdk-shape-fill); }
.bar:hover { stroke: var(--qdk-shape-stroke-hover); stroke-width: 0.5; }
.bar-selected { stroke: var(--qdk-shape-stroke-selected); fill: var(--qdk-shape-fill-selected); }
.bar-label { font-size: 3pt; fill: var(--qdk-text-high-contrast); text-anchor: end; pointer-events: none; }
.bar-label-ket { font-family: var(--qdk-font-family-monospace); font-variant-ligatures: none; }
.histo-label { font-size: 3.5pt; fill: var(--qdk-host-foreground); }
.hover-text { font-size: 3.5pt; fill: var(--qdk-host-foreground); text-anchor: middle; }
.menu-icon * { fill: var(--qdk-host-background); stroke: var(--qdk-host-foreground); }
.menu-box { fill: var(--qdk-host-background); stroke: var(--qdk-host-foreground); stroke-width: 0.1; }
.menu-item { width: 32px; height: 10px; fill: var(--qdk-menu-fill); stroke: var(--qdk-mid-gray); stroke-width: 0.2; }
.menu-item:hover { stroke-width: 0.6; fill: var(--qdk-menu-fill-hover); }
.menu-selected { fill: var(--qdk-menu-fill-selected); }
.menu-text { font-size: 4.5px; pointer-events: none; fill: var(--qdk-host-foreground); }
.menu-separator { stroke: var(--qdk-mid-gray); stroke-width: 0.25; }
.help-info { fill: var(--qdk-host-background); stroke: var(--qdk-mid-gray); stroke-width: 0.5; }
.help-info-text { font-size: 4.5px; pointer-events: none; fill: var(--qdk-host-foreground); }
`;

const menuItems = [
  {
    category: "itemCount",
    options: ["Show all", "Top 10", "Top 25"],
  },
  {
    category: "sortOrder",
    options: ["Sort a-z", "High to low", "Low to high"],
  },
  {
    category: "labels",
    options: ["Raw labels", "Ket labels", "No labels"],
  },
];
const maxMenuOptions = 3;

function getDefaultMenuSelection(
  labels?: "raw" | "kets" | "none",
  items?: "all" | "top-10" | "top-25",
  sort?: "a-to-z" | "high-to-low" | "low-to-high",
): {
  [idx: string]: number;
} {
  const selection = {
    itemCount: 0,
    sortOrder: 0,
    labels: 0,
  };
  switch (items) {
    case "top-10":
      selection["itemCount"] = 1;
      break;
    case "top-25":
      selection["itemCount"] = 2;
      break;
    default:
      selection["itemCount"] = 0;
      break;
  }
  switch (sort) {
    case "high-to-low":
      selection["sortOrder"] = 1;
      break;
    case "low-to-high":
      selection["sortOrder"] = 2;
      break;
    default:
      selection["sortOrder"] = 0;
      break;
  }
  switch (labels) {
    case "kets":
      selection["labels"] = 1;
      break;
    case "none":
      selection["labels"] = 2;
      break;
    default:
      selection["labels"] = 0;
      break;
  }
  return selection;
}

const reKetResult =
  /^\[(?:(Zero|One|Loss|0|1|2|-), *)*(Zero|One|Loss|0|1|2|-)\]$/;
function resultToKet(result: string): string {
  if (typeof result !== "string") return "ERROR";

  if (reKetResult.test(result)) {
    // The result fits our expected pattern, so we can convert it to a ket. If not, just return the raw result.
    const matches = result.match(/(One|Zero|Loss|0|1|2|-)/g);
    let ket = "|";
    matches?.forEach(
      (digit) =>
        (ket +=
          digit == "One" || digit == "1"
            ? "1"
            : digit == "Zero" || digit == "0"
              ? "0"
              : "-"),
    );
    ket += "⟩";
    return ket;
  } else {
    return result;
  }
}

export type HistogramProps = {
  shotCount: number;
  data: Map<string, number>;
  filter: string;
  onFilter: (filter: string) => void;
  shotsHeader: boolean;
  labels?: "raw" | "kets" | "none";
  items?: "all" | "top-10" | "top-25";
  sort?: "a-to-z" | "high-to-low" | "low-to-high";
  onSettingsChange?: (settings: {
    labels: "raw" | "kets" | "none";
    items: "all" | "top-10" | "top-25";
    sort: "a-to-z" | "high-to-low" | "low-to-high";
  }) => void;
  /** When true, suppress interactive elements (menu, info, zoom). */
  static?: boolean;
  /** undefined → inherit from host CSS, true → dark, false → light. */
  darkMode?: boolean;
};

export function Histogram(props: HistogramProps) {
  const [hoverLabel, setHoverLabel] = useState("");
  const [scale, setScale] = useState({ zoom: 1.0, offset: 1.0 });
  const [menuSelection, setMenuSelection] = useState(() => {
    return getDefaultMenuSelection(props.labels, props.items, props.sort);
  });

  useEffect(() => {
    setMenuSelection(
      getDefaultMenuSelection(props.labels, props.items, props.sort),
    );
  }, [props.labels, props.items, props.sort]);

  const gMenu = useRef<SVGGElement>(null);
  const gInfo = useRef<SVGGElement>(null);

  let maxItemsToShow = 0; // All
  switch (menuSelection["itemCount"]) {
    case 1:
      maxItemsToShow = 10;
      break;
    case 2:
      maxItemsToShow = 25;
      break;
  }
  const showKetLabels = menuSelection["labels"] === 1;

  const bucketArray = [...props.data];

  // Calculate bucket percentages before truncating for display
  let totalAllBuckets = 0;
  let sizeBiggestBucket = 0;
  bucketArray.forEach((x) => {
    totalAllBuckets += x[1];
    sizeBiggestBucket = Math.max(x[1], sizeBiggestBucket);
  });

  let histogramLabel = `${bucketArray.length} unique results`;
  if (maxItemsToShow > 0) {
    // Sort from high to low then take the first n
    bucketArray.sort((a, b) => (a[1] < b[1] ? 1 : -1));
    if (bucketArray.length > maxItemsToShow) {
      histogramLabel = `Top ${maxItemsToShow} of ${histogramLabel}`;
      bucketArray.length = maxItemsToShow;
    }
  }
  if (props.filter) {
    histogramLabel += `. Shot filter: ${
      showKetLabels ? resultToKet(props.filter) : props.filter
    }`;
  }

  bucketArray.sort((a, b) => {
    const a_label = showKetLabels ? resultToKet(a[0]) : a[0];
    const b_label = showKetLabels ? resultToKet(b[0]) : b[0];

    // If they can be converted to numbers, then sort as numbers, else lexically
    const ax = Number(a_label);
    const bx = Number(b_label);
    switch (menuSelection["sortOrder"]) {
      case 1: // high-to-low
        return a[1] < b[1] ? 1 : -1;
        break;
      case 2: // low-to-high
        return a[1] > b[1] ? 1 : -1;
        break;
      default: // a-z
        if (!isNaN(ax) && !isNaN(bx)) return ax < bx ? -1 : 1;
        return a_label < b_label ? -1 : 1;
        break;
    }
  });

  function onMouseOverRect(evt: MouseEvent) {
    const target = evt.target as SVGRectElement;
    const title = target.querySelector("title")?.textContent;
    setHoverLabel(title || "");
  }

  function onMouseOutRect() {
    setHoverLabel("");
  }

  function onClickRect(evt: MouseEvent) {
    const targetElem = evt.target as SVGRectElement;
    const rawLabel = targetElem.getAttribute("data-raw-label");

    if (rawLabel === props.filter) {
      // Clicked the already selected bar. Clear the filter
      props.onFilter("");
    } else {
      props.onFilter(rawLabel || "");
    }
  }

  function toggleMenu() {
    if (!gMenu.current) return;
    if (gMenu.current.style.display === "inline") {
      gMenu.current.style.display = "none";
    } else {
      gMenu.current.style.display = "inline";
      if (gInfo.current) gInfo.current.style.display = "none";
    }
  }

  function menuClicked(category: string, idx: number) {
    if (!gMenu.current) return;
    const newMenuSelection = { ...menuSelection };
    newMenuSelection[category] = idx;
    setMenuSelection(newMenuSelection);
    if (category === "itemCount") {
      setScale({ zoom: 1, offset: 1 });
    }
    gMenu.current.style.display = "none";

    // Notify parent of settings change
    if (props.onSettingsChange) {
      const sortValues: ("a-to-z" | "high-to-low" | "low-to-high")[] = [
        "a-to-z",
        "high-to-low",
        "low-to-high",
      ];
      const labelsValues: ("raw" | "kets" | "none")[] = ["raw", "kets", "none"];
      const itemsValues: ("all" | "top-10" | "top-25")[] = [
        "all",
        "top-10",
        "top-25",
      ];
      props.onSettingsChange({
        sort: sortValues[newMenuSelection["sortOrder"] ?? 0],
        labels: labelsValues[newMenuSelection["labels"] ?? 0],
        items: itemsValues[newMenuSelection["itemCount"] ?? 0],
      });
    }
  }

  function toggleInfo() {
    if (!gInfo.current) return;

    gInfo.current.style.display =
      gInfo.current.style.display === "inline" ? "none" : "inline";
  }

  // Each menu item has a width of 32px and a height of 10px
  // Menu items are 38px apart on the x-axis, and 11px on the y-axis.
  const menuItemWidth = 38;
  const menuItemHeight = 11;
  const menuBoxWidth = menuItems.length * menuItemWidth - 2;
  const menuBoxHeight = maxMenuOptions * menuItemHeight + 3;

  const barAreaWidth = 163;
  const barAreaHeight = 72;
  const fontOffset = 1.2;

  // Scale the below for when zoomed
  const barBoxWidth = (barAreaWidth * scale.zoom) / bucketArray.length;
  const barPaddingPercent = 0.1; // 10%
  const barPaddingSize = barBoxWidth * barPaddingPercent;
  const barFillWidth = barBoxWidth - 2 * barPaddingSize;
  const showLabels = barBoxWidth > 5 && menuSelection["labels"] !== 2;

  function onWheel(e: WheelEvent): void {
    // Ctrl+scroll is the event sent by pinch-to-zoom on a trackpad. Shift+scroll is common for
    // panning horizontally.
    if (!e.ctrlKey && !e.shiftKey) return;

    // When using a mouse wheel, the deltaY is the scroll amount, but if the shift key is pressed
    // this swaps and deltaX is the scroll amount. The swap doesn't happen for trackpad scrolling.
    // To complicate matters more, on the trackpad sometimes both deltaX and deltaY have a value.
    // So, if the shift key is pressed and deltaY is 0, then assume mouse wheel and use deltaX.
    let delta = e.shiftKey && !e.deltaY ? e.deltaX : e.deltaY;

    // Scrolling with the wheel can result in really large deltas, so we need to cap them.
    if (Math.abs(delta) > 20) {
      delta = Math.sign(delta) * 20;
    }

    e.preventDefault();

    // currentTarget is the element the listener is attached to, the main svg
    // element in this case.
    const svgElem = e.currentTarget as SVGSVGElement;

    // Below gets the mouse location in the svg element coordinates. This stays
    // consistent while the scroll is occurring (i.e. it is the point the mouse
    // was at when scrolling started).
    const mousePoint = new DOMPoint(e.clientX, e.clientY).matrixTransform(
      svgElem.getScreenCTM()?.inverse(),
    );

    /*
    While zooming, we want is to track the point the mouse is at when scrolling, and pin
    that location on the screen. That means adjusting the scroll offset.

    SVG translation is used to pan left and right, but zooming is done manually (making the
    bars wider or thinner) to keep the fonts from getting stretched, which occurs with scaling.

    deltaX and deltaY do not accumulate across events, they are a new delta each time.
    */

    let newScrollOffset = scale.offset;
    let newZoom = scale.zoom;

    if (!e.shiftKey) {
      // *** Zooming ***
      newZoom = scale.zoom - delta * 0.05;
      newZoom = Math.min(Math.max(1, newZoom), 50);

      // On zooming in, need to shift left to maintain mouse point, and vice verca.
      const oldChartWidth = barAreaWidth * scale.zoom;
      const mousePointOnChart = 0 - scale.offset + mousePoint.x;
      const percentRightOnChart = mousePointOnChart / oldChartWidth;
      const chartWidthGrowth =
        newZoom * barAreaWidth - scale.zoom * barAreaWidth;
      const shiftLeftAdjust = percentRightOnChart * chartWidthGrowth;
      newScrollOffset = scale.offset - shiftLeftAdjust;
    } else {
      // *** Panning ***
      newScrollOffset -= delta;
    }

    // Don't allow offset > 1 (scrolls the first bar right of the left edge of the area)
    // Don't allow for less than 0 - barwidths + screen width (scrolls last bar left of the right edge)
    const maxScrollRight = 1 - (barAreaWidth * newZoom - barAreaWidth);
    const boundScrollOffset = Math.min(
      Math.max(newScrollOffset, maxScrollRight),
      1,
    );

    setScale({ zoom: newZoom, offset: boundScrollOffset });
  }

  const label_class = showKetLabels ? "bar-label bar-label-ket" : "bar-label";
  const isStatic = props.static === true;
  const embedCss = props.darkMode !== undefined;

  return (
    <>
      {props.shotsHeader && !isStatic ? (
        <h4 style="margin: 8px 0px">Total shots: {props.shotCount}</h4>
      ) : null}
      <svg
        class="histogram"
        viewBox="0 0 165 100"
        onWheel={isStatic ? undefined : onWheel}
        {...(embedCss
          ? {
              xmlns: "http://www.w3.org/2000/svg",
              "xmlns:xlink": "http://www.w3.org/1999/xlink",
            }
          : {})}
      >
        {embedCss ? (
          <defs>
            <style
              dangerouslySetInnerHTML={{
                __html: themeStyleBlock(props.darkMode === true) + histogramCss,
              }}
            />
          </defs>
        ) : null}
        <g transform={`translate(${scale.offset},4)`}>
          {bucketArray.map((entry, idx) => {
            const label = showKetLabels ? resultToKet(entry[0]) : entry[0];

            const height = barAreaHeight * (entry[1] / sizeBiggestBucket);
            const x = barBoxWidth * idx + barPaddingSize;
            const labelX = barBoxWidth * idx + barBoxWidth / 2 - fontOffset;
            const y = barAreaHeight + 15 - height;
            const barLabel =
              props.shotCount == 0
                ? `${entry[1]}`
                : `${label} at ${((entry[1] / totalAllBuckets) * 100).toFixed(
                    2,
                  )}%`;
            let barClass = "bar";

            if (entry[0] === props.filter) {
              barClass += " bar-selected";
            }

            return (
              <>
                <rect
                  class={barClass}
                  x={x}
                  y={y}
                  width={barFillWidth}
                  height={height}
                  onMouseOver={isStatic ? undefined : onMouseOverRect}
                  onMouseOut={isStatic ? undefined : onMouseOutRect}
                  onClick={isStatic ? undefined : onClickRect}
                  data-raw-label={entry[0]}
                >
                  <title>{barLabel}</title>
                </rect>
                {
                  <text
                    class={label_class}
                    x={labelX}
                    y="85"
                    visibility={showLabels ? "visible" : "hidden"}
                    transform={`rotate(90, ${labelX}, 85)`}
                  >
                    {label}
                  </text>
                }
              </>
            );
          })}
        </g>

        <text class="histo-label" x="2" y="97">
          {histogramLabel}
        </text>
        {!isStatic && (
          <text class="hover-text" x="85" y="6">
            {hoverLabel}
          </text>
        )}

        {/* The settings icon */}
        {!isStatic && (
          <g
            class="menu-icon"
            transform="translate(2, 2) scale(0.3 0.3)"
            onClick={toggleMenu}
          >
            <rect
              width="24"
              height="24"
              fill="white"
              stroke-widths="0.5"
            ></rect>
            <path
              d="M3 5 H21 M3 12 H21 M3 19 H21"
              stroke-width="1.75"
              stroke-linecap="round"
            />
            <rect x="6" y="3" width="4" height="4" rx="1" stroke-width="1.5" />
            <rect
              x="15"
              y="10"
              width="4"
              height="4"
              rx="1"
              stroke-width="1.5"
            />
            <rect x="9" y="17" width="4" height="4" rx="1" stroke-width="1.5" />
          </g>
        )}

        {/* The info icon */}
        {!isStatic && (
          <g
            class="menu-icon"
            transform="translate(156, 2) scale(0.3 0.3)"
            onClick={toggleInfo}
          >
            <rect width="24" height="24" stroke-width="0"></rect>
            <circle cx="12" cy="13" r="10" stroke-width="1.5" />
            <path
              stroke-width="2.5"
              stroke-linecap="round"
              d="M12 8 V8 M12 12.5 V18"
            />
          </g>
        )}

        {/* The menu box */}
        {!isStatic && (
          <g
            id="menu"
            ref={gMenu}
            transform="translate(8, 2)"
            style="display: none;"
          >
            <rect
              x="0"
              y="0"
              rx="2"
              width={menuBoxWidth}
              height={menuBoxHeight}
              class="menu-box"
            ></rect>

            {
              // Menu items
              menuItems.map((item, col) => {
                return item.options.map((option, row) => {
                  let classList = "menu-item";
                  if (menuSelection[item.category] === row)
                    classList += " menu-selected";
                  return (
                    <>
                      <rect
                        x={2 + col * menuItemWidth}
                        y={2 + row * menuItemHeight}
                        width="32"
                        height="10"
                        rx="1"
                        class={classList}
                        onClick={() => menuClicked(item.category, row)}
                      ></rect>
                      <text
                        x={18 + col * menuItemWidth}
                        y={7 + row * menuItemHeight}
                        dominant-baseline="middle"
                        text-anchor="middle"
                        class="menu-text"
                      >
                        {option}
                      </text>
                    </>
                  );
                });
              })
            }
            {
              // Column separators
              menuItems.map((item, idx) => {
                return idx >= menuItems.length - 1 ? null : (
                  <line
                    class="menu-separator"
                    x1={37 + idx * menuItemWidth}
                    y1="2"
                    x2={37 + idx * menuItemWidth}
                    y2={maxMenuOptions * menuItemHeight + 1}
                  ></line>
                );
              })
            }
          </g>
        )}

        {/* The info box */}
        {!isStatic && (
          <g ref={gInfo} style="display: none;">
            <rect
              width="155"
              height="76"
              rx="5"
              x="5"
              y="6"
              class="help-info"
              onClick={toggleInfo}
            />
            <text y="6" class="help-info-text">
              <tspan x="10" dy="10">
                This histogram shows the frequency of unique 'shot' results.
              </tspan>
              <tspan x="10" dy="10">
                Click the top-left 'settings' icon for display options.
              </tspan>
              <tspan x="10" dy="10">
                You can zoom the chart using the pinch-to-zoom gesture,
              </tspan>
              <tspan x="10" dy="10">
                or use Ctrl+scroll wheel to zoom in/out.
              </tspan>
              <tspan x="10" dy="10">
                To pan left &amp; right, press Shift while zooming.
              </tspan>
              <tspan x="10" dy="10">
                Click on a bar to filter the shot details to that result.
              </tspan>
              <tspan x="10" dy="10">
                Click anywhere in this box to dismiss it.
              </tspan>
            </text>
          </g>
        )}
      </svg>
    </>
  );
}

/**
 * Render a standalone Histogram SVG string.
 * Uses `renderToString` from preact-render-to-string.
 *
 * @param props - Props for the Histogram component.
 *   `darkMode` should be `true` or `false` (not `undefined`) so
 *   CSS custom properties are resolved to concrete values.
 *   `static` defaults to `true` when not specified.
 */
export function histogramToSvg(
  props: Omit<HistogramProps, "onFilter" | "onSettingsChange" | "shotsHeader">,
): string {
  const fullProps: HistogramProps = {
    ...props,
    static: props.static ?? true,
    onFilter: () => {},
    onSettingsChange: undefined,
    shotsHeader: false,
  };
  let svg = renderToString(h(Histogram, fullProps));
  // renderToString wraps in a fragment — extract the <svg …>…</svg>
  const svgStart = svg.indexOf("<svg");
  if (svgStart > 0) svg = svg.slice(svgStart);
  const svgEnd = svg.lastIndexOf("</svg>");
  if (svgEnd >= 0) svg = svg.slice(0, svgEnd + 6);
  return svg;
}
