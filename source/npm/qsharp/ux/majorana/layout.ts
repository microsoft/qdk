// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  MZM_IDS,
  type MzmId,
  type Orientation,
  type ValidatedMajoranaDevice,
} from "./types.js";

export type Point = {
  x: number;
  y: number;
};

export type Rectangle = Point & {
  width: number;
  height: number;
};

export type TetronGeometry = {
  qubitId: number;
  center: Point;
  qubitBox: Rectangle;
  mzmAnchors: Readonly<Record<MzmId, Point>>;
  upperRail: LineGeometry;
  lowerRail: LineGeometry;
  bridge: LineGeometry;
};

export type LinearIsland = LineGeometry & {
  upperQubitId: number;
  lowerQubitId: number;
};

export type LineGeometry = {
  id: string;
  start: Point;
  end: Point;
};

export type AdjacencyEdge = LineGeometry & {
  orientation: Orientation;
  firstQubitId: number;
  secondQubitId: number;
};

export type RoutingObstacleGeometry = {
  id: string;
  kind: "block" | "mzm-connector" | "vertical-connector";
  bounds: Rectangle;
};

export type MajoranaLayout = {
  device: ValidatedMajoranaDevice;
  viewBox: Rectangle;
  tetrons: readonly TetronGeometry[];
  linearIslands: ReadonlyMap<string, LinearIsland>;
  adjacencyEdges: ReadonlyMap<string, AdjacencyEdge>;
  routingObstacles: readonly RoutingObstacleGeometry[];
};

export const MAJORANA_GEOMETRY = {
  tetronWidth: 200,
  tetronHeight: 72,
  tetronColumnPitch: 380,
  tetronRowPitch: 212,
  outerMargin: 24,
  qubitBoxWidth: 176,
  qubitBoxHeight: 112,
  islandWidth: 200,
  routingBlockSize: 42,
  routingBlockGap: 34,
  mzmConnectorWidth: 14,
  mzmConnectorHeight: 40,
  verticalConnectorWidth: 42,
  verticalConnectorHeight: 14,
  routingObstacleCornerRadius: 5,
} as const;

export function createMajoranaLayout(
  device: ValidatedMajoranaDevice,
): MajoranaLayout {
  const tetrons = Array.from({ length: device.qubitCount }, (_, id) => {
    const { row, column } = getTetronGridPosition(id, device);
    return createTetronGeometry(id, row, column);
  });
  const linearIslands = new Map<string, LinearIsland>();
  const adjacencyEdges = new Map<string, AdjacencyEdge>();
  const routingObstacles: RoutingObstacleGeometry[] = [];

  for (const tetron of tetrons) {
    routingObstacles.push(...createTetronRoutingObstacles(tetron));
  }

  for (let id = 0; id < device.qubitCount - device.tetronColumns; id++) {
    const upper = tetrons[id];
    const lower = tetrons[id + device.tetronColumns];
    const { row, column } = getTetronGridPosition(id, device);
    const islandId = getLinearIslandId(row, column);
    const center = {
      x: upper.center.x,
      y: (upper.center.y + lower.center.y) / 2,
    };
    const island = {
      id: islandId,
      upperQubitId: id,
      lowerQubitId: lower.qubitId,
      start: { x: center.x - MAJORANA_GEOMETRY.islandWidth / 2, y: center.y },
      end: { x: center.x + MAJORANA_GEOMETRY.islandWidth / 2, y: center.y },
    };
    linearIslands.set(islandId, island);
    routingObstacles.push(
      ...createEndpointRoutingObstacles(islandId, island.start, "left"),
      ...createEndpointRoutingObstacles(islandId, island.end, "right"),
      ...createIslandStackConnectors(islandId, island.start, "left"),
      ...createIslandStackConnectors(islandId, island.end, "right"),
    );
  }

  function addEdge(
    first: TetronGeometry,
    second: TetronGeometry,
    orientation: Orientation,
  ): void {
    const id = getAdjacencyEdgeId(orientation, first.qubitId, second.qubitId);
    adjacencyEdges.set(id, {
      id,
      orientation,
      firstQubitId: first.qubitId,
      secondQubitId: second.qubitId,
      start: adjacencyEndpoint(first, second.center, orientation),
      end: adjacencyEndpoint(second, first.center, orientation),
    });
    if (orientation === "horizontal") {
      routingObstacles.push(
        ...createHorizontalBridgeConnectors(id, first, second),
      );
    }
  }

  for (const tetron of tetrons) {
    const { row, column } = getTetronGridPosition(tetron.qubitId, device);
    if (column + 1 < device.tetronColumns) {
      addEdge(tetron, tetrons[tetron.qubitId + 1], "horizontal");
    }
    if (row + 1 < device.tetronRows) {
      addEdge(
        tetron,
        tetrons[tetron.qubitId + device.tetronColumns],
        "vertical",
      );
    }
  }

  const width =
    MAJORANA_GEOMETRY.outerMargin * 2 +
    routingObstacleExtent() * 2 +
    MAJORANA_GEOMETRY.tetronWidth +
    (device.tetronColumns - 1) * MAJORANA_GEOMETRY.tetronColumnPitch;
  const height =
    MAJORANA_GEOMETRY.outerMargin * 2 +
    MAJORANA_GEOMETRY.tetronHeight +
    (device.tetronRows - 1) * MAJORANA_GEOMETRY.tetronRowPitch;

  return {
    device,
    viewBox: { x: 0, y: 0, width, height },
    tetrons,
    linearIslands,
    adjacencyEdges,
    routingObstacles,
  };
}

function createTetronGeometry(
  qubitId: number,
  row: number,
  column: number,
): TetronGeometry {
  const center = {
    x:
      MAJORANA_GEOMETRY.outerMargin +
      routingObstacleExtent() +
      MAJORANA_GEOMETRY.tetronWidth / 2 +
      column * MAJORANA_GEOMETRY.tetronColumnPitch,
    y:
      MAJORANA_GEOMETRY.outerMargin +
      MAJORANA_GEOMETRY.tetronHeight / 2 +
      row * MAJORANA_GEOMETRY.tetronRowPitch,
  };
  const halfHeight = MAJORANA_GEOMETRY.tetronHeight / 2;
  const bounds = centeredRectangle(
    center,
    MAJORANA_GEOMETRY.tetronWidth,
    MAJORANA_GEOMETRY.tetronHeight,
  );
  const upperLeft = { x: bounds.x, y: bounds.y };
  const upperRight = { x: bounds.x + bounds.width, y: bounds.y };
  const lowerLeft = { x: bounds.x, y: bounds.y + bounds.height };
  const lowerRight = {
    x: bounds.x + bounds.width,
    y: bounds.y + bounds.height,
  };
  return {
    qubitId,
    center,
    qubitBox: centeredRectangle(
      center,
      MAJORANA_GEOMETRY.qubitBoxWidth,
      MAJORANA_GEOMETRY.qubitBoxHeight,
    ),
    mzmAnchors: {
      1: upperLeft,
      2: upperRight,
      3: lowerLeft,
      4: lowerRight,
    },
    upperRail: {
      id: `q${qubitId}-upper-rail`,
      start: upperLeft,
      end: upperRight,
    },
    lowerRail: {
      id: `q${qubitId}-lower-rail`,
      start: lowerLeft,
      end: lowerRight,
    },
    bridge: {
      id: `q${qubitId}-bridge`,
      start: { x: center.x, y: center.y - halfHeight },
      end: { x: center.x, y: center.y + halfHeight },
    },
  };
}

function createTetronRoutingObstacles(
  tetron: TetronGeometry,
): RoutingObstacleGeometry[] {
  const obstacles: RoutingObstacleGeometry[] = [];
  for (const mzmId of MZM_IDS) {
    const anchor = tetron.mzmAnchors[mzmId];
    obstacles.push(
      ...createEndpointRoutingObstacles(
        `q${tetron.qubitId}-m${mzmId}`,
        anchor,
        mzmId === 1 || mzmId === 3 ? "left" : "right",
      ),
    );
  }

  for (const side of ["left", "right"] as const) {
    const upperAnchor =
      side === "left" ? tetron.mzmAnchors[1] : tetron.mzmAnchors[2];
    const direction = side === "left" ? -1 : 1;
    const center = {
      x:
        upperAnchor.x +
        direction *
          (MAJORANA_GEOMETRY.routingBlockGap +
            MAJORANA_GEOMETRY.routingBlockSize / 2),
      y: tetron.center.y,
    };
    obstacles.push({
      id: `q${tetron.qubitId}-${side}-vertical-connector`,
      kind: "vertical-connector",
      bounds: centeredRectangle(
        center,
        MAJORANA_GEOMETRY.verticalConnectorWidth,
        MAJORANA_GEOMETRY.verticalConnectorHeight,
      ),
    });
  }
  return obstacles;
}

function createEndpointRoutingObstacles(
  id: string,
  anchor: Point,
  side: "left" | "right",
): RoutingObstacleGeometry[] {
  const direction = side === "left" ? -1 : 1;
  const connectorCenter = {
    x: anchor.x + direction * (MAJORANA_GEOMETRY.routingBlockGap / 2),
    y: anchor.y,
  };
  const blockCenter = {
    x:
      anchor.x +
      direction *
        (MAJORANA_GEOMETRY.routingBlockGap +
          MAJORANA_GEOMETRY.routingBlockSize / 2),
    y: anchor.y,
  };
  return [
    {
      id: `${id}-connector`,
      kind: "mzm-connector",
      bounds: centeredRectangle(
        connectorCenter,
        MAJORANA_GEOMETRY.mzmConnectorWidth,
        MAJORANA_GEOMETRY.mzmConnectorHeight,
      ),
    },
    {
      id: `${id}-block`,
      kind: "block",
      bounds: centeredRectangle(
        blockCenter,
        MAJORANA_GEOMETRY.routingBlockSize,
        MAJORANA_GEOMETRY.routingBlockSize,
      ),
    },
  ];
}

function createIslandStackConnectors(
  islandId: string,
  anchor: Point,
  side: "left" | "right",
): RoutingObstacleGeometry[] {
  const direction = side === "left" ? -1 : 1;
  const centerX =
    anchor.x +
    direction *
      (MAJORANA_GEOMETRY.routingBlockGap +
        MAJORANA_GEOMETRY.routingBlockSize / 2);
  return (["upper", "lower"] as const).map((position) => ({
    id: `${islandId}-${side}-${position}-vertical-connector`,
    kind: "vertical-connector",
    bounds: centeredRectangle(
      {
        x: centerX,
        y:
          anchor.y +
          (position === "upper" ? -1 : 1) *
            (MAJORANA_GEOMETRY.tetronHeight / 2),
      },
      MAJORANA_GEOMETRY.verticalConnectorWidth,
      MAJORANA_GEOMETRY.verticalConnectorHeight,
    ),
  }));
}

function createHorizontalBridgeConnectors(
  edgeId: string,
  leftTetron: TetronGeometry,
  rightTetron: TetronGeometry,
): RoutingObstacleGeometry[] {
  return (["upper", "lower"] as const).map((position) => {
    const leftAnchor =
      position === "upper"
        ? leftTetron.mzmAnchors[2]
        : leftTetron.mzmAnchors[4];
    const rightAnchor =
      position === "upper"
        ? rightTetron.mzmAnchors[1]
        : rightTetron.mzmAnchors[3];
    return {
      id: `${edgeId}-${position}-connector`,
      kind: "mzm-connector",
      bounds: centeredRectangle(
        {
          x: (leftAnchor.x + rightAnchor.x) / 2,
          y: leftAnchor.y,
        },
        MAJORANA_GEOMETRY.mzmConnectorWidth,
        MAJORANA_GEOMETRY.mzmConnectorHeight,
      ),
    };
  });
}

function routingObstacleExtent(): number {
  return MAJORANA_GEOMETRY.routingBlockGap + MAJORANA_GEOMETRY.routingBlockSize;
}

function adjacencyEndpoint(
  tetron: TetronGeometry,
  otherCenter: Point,
  orientation: Orientation,
): Point {
  if (orientation === "horizontal") {
    return {
      x:
        tetron.center.x +
        Math.sign(otherCenter.x - tetron.center.x) *
          (tetron.qubitBox.width / 2),
      y: tetron.center.y,
    };
  }
  return {
    x: tetron.center.x,
    y:
      tetron.center.y +
      Math.sign(otherCenter.y - tetron.center.y) * (tetron.qubitBox.height / 2),
  };
}

function centeredRectangle(
  center: Point,
  width: number,
  height: number,
): Rectangle {
  return {
    x: center.x - width / 2,
    y: center.y - height / 2,
    width,
    height,
  };
}

export function requiredTetron(
  layout: MajoranaLayout,
  qubitId: number,
): TetronGeometry {
  const geometry = layout.tetrons[qubitId];
  if (geometry === undefined) {
    throw new Error(`Missing geometry for qubit ${qubitId}`);
  }
  return geometry;
}

export function getTetronGridPosition(
  qubitId: number,
  device: ValidatedMajoranaDevice,
): { row: number; column: number } {
  if (
    !Number.isInteger(qubitId) ||
    qubitId < 0 ||
    qubitId >= device.qubitCount
  ) {
    throw new RangeError(
      `Qubit ID ${qubitId} is outside 0 through ${device.qubitCount - 1}`,
    );
  }
  return {
    row: Math.floor(qubitId / device.tetronColumns),
    column: qubitId % device.tetronColumns,
  };
}

export function getLinearIslandId(row: number, column: number): string {
  return `island-r${row}-c${column}`;
}

export function getAdjacencyEdgeId(
  orientation: Orientation,
  firstQubitId: number,
  secondQubitId: number,
): string {
  return `${orientation}-q${firstQubitId}-q${secondQubitId}`;
}
