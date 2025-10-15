// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useRef, useEffect } from "preact/hooks";
import { createRef } from "preact";
import * as utils from "../src/utils.js";
import { saveToImage } from "./saveImage.js";

export type ScatterSeries = {
  color: string;
  items: PlotItem[];
};

export type PlotItem = {
  x: number;
  y: number;
  label: string;
};

export type Axis = {
  isTime: boolean;
  label: string;
};

type Range = {
  min: number;
  max: number;
};

export function ScatterChart(props: {
  data: ScatterSeries[];
  xAxis: Axis;
  yAxis: Axis;
  onPointSelected(seriesIndex: number, pointIndex: number): void;
  selectedPoint?: [number, number];
  allowSaveImage: boolean;
}) {
  const selectedTooltipDiv = useRef<HTMLDivElement>(null);

  const { rangeX, rangeY } = utils.getRanges(props.data, 2 /* coefficient */);

  function createAxisTicks(range: Range, isTime: boolean): utils.Tick[] {
    return isTime
      ? utils.CreateTimeTicks(range.min, range.max)
      : utils.CreateIntegerTicks(range.min, range.max);
  }

  const xTicks = createAxisTicks(rangeX, props.xAxis.isTime);
  const yTicks = createAxisTicks(rangeY, props.yAxis.isTime);

  function coordinateToLogarithmic(value: number, range: Range): number {
    return (
      (Math.log(value) - Math.log(range.min)) /
      (Math.log(range.max) - Math.log(range.min))
    );
  }

  function toLogX(val: number): number {
    return coordinateToLogarithmic(val, rangeX) * plotAreaWidth;
  }

  function toLogY(val: number): number {
    return -coordinateToLogarithmic(val, rangeY) * plotAreaHeight;
  }

  const yAxisTitleWidth = 20;
  const yAxisTickCaptionMaxWidth = 100;
  const axisTickLength = 5;
  const axisLineWidth = 1;
  const xLeftMargin =
    yAxisTitleWidth + yAxisTickCaptionMaxWidth + axisTickLength + axisLineWidth;
  const xRightMargin = 160; // to show tooltips on the right hand side. If we can move tooltips dynamically, we can get rid of this.

  const axisTitleHeight = 20;
  const xAxisTickCaptionMaxHeight = 16;
  const yMargin =
    axisTitleHeight +
    xAxisTickCaptionMaxHeight +
    axisTickLength +
    axisLineWidth;

  const svgWidth = 960;
  const svgViewBoxWidthPadding = 50;
  const svgHeight = 480;
  const svgViewBoxHeightPadding = 10;
  const svgXMin = -xLeftMargin;

  const plotAreaWidth = svgWidth - xLeftMargin - xRightMargin;
  const plotAreaHeight = svgHeight - yMargin;

  const viewBox = `${svgXMin} ${-plotAreaHeight - svgViewBoxHeightPadding} ${
    svgWidth + svgViewBoxWidthPadding
  } ${svgHeight + svgViewBoxHeightPadding}`;

  const yAxisTextPaddingFromTicks = 5;
  const yAxisTextYPadding = 4;

  function renderTooltip(
    topDiv: HTMLDivElement,
    point: SVGCircleElement,
    tooltip: HTMLDivElement,
  ) {
    const label = point.getAttribute("data-label");
    tooltip.textContent = label;
    const halfWidth = tooltip.offsetWidth / 2;
    const pointRect = point.getBoundingClientRect();
    const centerY = (pointRect.top + pointRect.bottom) / 2;
    const centerX = (pointRect.left + pointRect.right) / 2;
    const divRect = topDiv.getBoundingClientRect();
    tooltip.style.left = `${centerX - divRect.left - halfWidth}px`;
    tooltip.style.top = `${centerY - divRect.top + 12}px`;
    tooltip.style.visibility = "visible";
  }

  function onPointMouseEvent(ev: MouseEvent, eventType: string) {
    // Ensure we have a point as the target
    if (!(ev.target instanceof SVGCircleElement)) return;
    const target = ev.target as SVGCircleElement;
    if (!target.classList.contains("qs-scatterChart-point")) return;

    // Get the div enclosing the chart, and the popup child of it.
    const topDiv = target.closest("div") as HTMLDivElement;
    const popup = topDiv.querySelector(
      ".qs-scatterChart-tooltip",
    ) as HTMLDivElement;

    switch (eventType) {
      case "over":
        {
          renderTooltip(topDiv, target, popup);
        }
        break;
      case "out":
        popup.style.visibility = "hidden";
        break;
      case "click":
        {
          if (target.classList.contains("qs-scatterChart-point-selected")) {
            // Clicked on the already selected point, so delete the point/row
            props.onPointSelected(-1, 0);
          } else {
            const index = JSON.parse(target.getAttribute("data-index")!);
            props.onPointSelected(index[0], index[1]);
          }
        }
        break;
      default:
        console.error("Unknown event type: ", eventType);
    }
  }

  function getSelectedPointData() {
    if (!props.selectedPoint) return null;
    const series = props.data[props.selectedPoint[0]];
    const item = series.items[props.selectedPoint[1]];
    return { ...item, color: series.color };
  }
  const selectedPoint = getSelectedPointData();

  const saveRef = createRef();

  const handleSaveImage = async () => {
    saveToImage(saveRef!.current);
  };

  // Need to render first to get the element layout to position the tooltip
  useEffect(() => {
    if (!selectedTooltipDiv.current) return;
    if (!props.selectedPoint) {
      selectedTooltipDiv.current.style.visibility = "hidden";
    } else {
      // Locate the selected point and put the tooltip under it
      const topDiv = selectedTooltipDiv.current.parentElement as HTMLDivElement;
      const selectedPoint = topDiv?.querySelector(
        ".qs-scatterChart-point-selected",
      ) as SVGCircleElement;
      if (!selectedPoint) return;
      renderTooltip(topDiv, selectedPoint, selectedTooltipDiv.current);
    }
  });

  // The mouse events (over, out, and click) bubble, so put the handlers on the
  // SVG element and check the target element in the handler.
  return (
    <div style="position: relative">
      <svg
        style="margin-top: 12px"
        viewBox={viewBox}
        width={svgWidth}
        height={svgHeight}
        onMouseOver={(ev) => onPointMouseEvent(ev, "over")}
        onMouseOut={(ev) => onPointMouseEvent(ev, "out")}
        onClick={(ev) => onPointMouseEvent(ev, "click")}
        ref={saveRef}
      >
        <line
          class="qs-scatterChart-axis"
          x1="0"
          y1="0"
          x2={plotAreaWidth}
          y2="0"
        />

        {xTicks.map((tick) => {
          return (
            <>
              <line
                y1="1"
                y2={axisTickLength}
                x1={toLogX(tick.value)}
                x2={toLogX(tick.value)}
                class="qs-scatterChart-tick-line"
              />
              <text
                y={axisTickLength + xAxisTickCaptionMaxHeight}
                x={toLogX(tick.value)}
                class="qs-scatterChart-x-tick-text"
              >
                {tick.label}
              </text>
            </>
          );
        })}

        <line
          class="qs-scatterChart-axis"
          x1="0"
          y1="0"
          x2="0"
          y2={-svgHeight}
        />

        {yTicks.map((tick) => {
          return (
            <>
              <line
                x1="0"
                x2={-axisTickLength}
                y1={toLogY(tick.value)}
                y2={toLogY(tick.value)}
                class="qs-scatterChart-tick-line"
              />
              <text
                x={-axisTickLength - yAxisTextPaddingFromTicks}
                y={toLogY(tick.value) + yAxisTextYPadding}
                class="qs-scatterChart-y-tick-text"
              >
                {tick.label}
              </text>
            </>
          );
        })}

        <text
          x={plotAreaWidth / 2}
          y={yMargin}
          class="qs-scatterChart-x-axisTitle"
        >
          {props.xAxis.label} (logarithmic)
        </text>

        <text
          x={xLeftMargin - axisTitleHeight}
          y={plotAreaHeight / 2}
          class="qs-scatterChart-y-axisTitle"
        >
          {props.yAxis.label} (logarithmic)
        </text>

        <text
          class="qs-scatterChart-watermark"
          x={xLeftMargin - axisTitleHeight}
          y={-svgHeight + yMargin}
        >
          Created with Azure Quantum Resource Estimator
        </text>
        <g>
          {props.data.map((series, seriesIdx) => {
            const centers = series.items.map((plot) => ({
              x: toLogX(plot.x),
              y: toLogY(plot.y),
            }));

            // Constructing the points string for the polyline
            const pointsString = centers
              .map((center) => `${center.x},${center.y}`)
              .join(" ");

            return (
              <>
                {/* Render circles */}
                {series.items.map((plot, plotIdx) => (
                  <circle
                    key={plotIdx}
                    data-index={JSON.stringify([seriesIdx, plotIdx])}
                    data-label={plot.label}
                    className="qs-scatterChart-point qs-scatterChart-hover"
                    cx={centers[plotIdx].x}
                    cy={centers[plotIdx].y}
                    stroke={series.color}
                  />
                ))}

                {/* Render polyline connecting circle centers */}
                <polyline
                  points={pointsString}
                  stroke={series.color}
                  fill="none"
                />
              </>
            );
          })}
        </g>
        {
          // Render the selected point last, so it's always on top of the others
          selectedPoint ? (
            <circle
              class="qs-scatterChart-point qs-scatterChart-point-selected"
              data-label={selectedPoint.label}
              cx={toLogX(selectedPoint.x)}
              cy={toLogY(selectedPoint.y)}
              stroke={selectedPoint.color}
            />
          ) : null
        }
      </svg>
      <div class="qs-scatterChart-selectedInfo" ref={selectedTooltipDiv}></div>
      <div class="qs-scatterChart-tooltip"></div>
      {props.allowSaveImage ? (
        <button
          role="button"
          onClick={handleSaveImage}
          className={"qs-estimatesOverview-saveIcon"}
        >
          <span>
            <svg
              width="75%"
              height="75%"
              viewBox="0 0 16 16"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                className={"qs-estimatesOverview-saveIconSvgPath"}
                d="M12.0147 2.8595L13.1397 3.9845L13.25 4.25V12.875L12.875 13.25H3.125L2.75 12.875V3.125L3.125 2.75H11.75L12.0147 2.8595ZM3.5 3.5V12.5H12.5V4.406L11.5947 3.5H10.25V6.5H5V3.5H3.5ZM8 3.5V5.75H9.5V3.5H8Z"
              />
            </svg>
          </span>
        </button>
      ) : null}
    </div>
  );
}
