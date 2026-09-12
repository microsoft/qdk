// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaTopology, MzmId } from "./topology.js";

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
  bounds: Rectangle;
  qubitBox: Rectangle;
  mzmAnchors: Readonly<Record<MzmId, Point>>;
  upperRail: AdjacencyGeometry;
  lowerRail: AdjacencyGeometry;
  bridge: AdjacencyGeometry;
};

export type LinearIslandGeometry = {
  islandId: string;
  center: Point;
  bounds: Rectangle;
  start: Point;
  end: Point;
};

export type AdjacencyGeometry = {
  edgeId: string;
  start: Point;
  end: Point;
};

export type RoutingObstacleGeometry = {
  id: string;
  kind: "block" | "mzm-connector" | "vertical-connector";
  bounds: Rectangle;
};

export type MajoranaGeometry = {
  viewBox: Rectangle;
  tetrons: ReadonlyMap<number, TetronGeometry>;
  linearIslands: ReadonlyMap<string, LinearIslandGeometry>;
  adjacencyEdges: ReadonlyMap<string, AdjacencyGeometry>;
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
  islandHeight: 12,
  routingBlockSize: 42,
  routingBlockGap: 34,
  mzmConnectorWidth: 14,
  mzmConnectorHeight: 40,
  verticalConnectorWidth: 42,
  verticalConnectorHeight: 14,
  routingObstacleCornerRadius: 5,
  topologyStrokeWidth: 12,
} as const;

export function createMajoranaGeometry(
  topology: MajoranaTopology,
): MajoranaGeometry {
  const tetrons = new Map<number, TetronGeometry>();
  const linearIslands = new Map<string, LinearIslandGeometry>();
  const adjacencyEdges = new Map<string, AdjacencyGeometry>();
  const routingObstacles: RoutingObstacleGeometry[] = [];

  for (const tetron of topology.tetrons) {
    const geometry = createTetronGeometry(tetron.id, tetron.row, tetron.column);
    tetrons.set(tetron.id, geometry);
    routingObstacles.push(...createTetronRoutingObstacles(geometry));
  }

  for (const island of topology.linearIslands) {
    const upper = requiredTetronGeometry(tetrons, island.upperQubitId);
    const lower = requiredTetronGeometry(tetrons, island.lowerQubitId);
    const center = {
      x: upper.center.x,
      y: (upper.center.y + lower.center.y) / 2,
    };
    const bounds = centeredRectangle(
      center,
      MAJORANA_GEOMETRY.islandWidth,
      MAJORANA_GEOMETRY.islandHeight,
    );
    const islandGeometry = {
      islandId: island.id,
      center,
      bounds,
      start: { x: bounds.x, y: center.y },
      end: { x: bounds.x + bounds.width, y: center.y },
    };
    linearIslands.set(island.id, islandGeometry);
    routingObstacles.push(
      ...createEndpointRoutingObstacles(
        island.id,
        islandGeometry.start,
        "left",
      ),
      ...createEndpointRoutingObstacles(island.id, islandGeometry.end, "right"),
      ...createIslandStackConnectors(island.id, islandGeometry.start, "left"),
      ...createIslandStackConnectors(island.id, islandGeometry.end, "right"),
    );
  }

  for (const edge of topology.adjacencyEdges) {
    const first = requiredTetronGeometry(tetrons, edge.firstQubitId);
    const second = requiredTetronGeometry(tetrons, edge.secondQubitId);
    adjacencyEdges.set(edge.id, {
      edgeId: edge.id,
      start: adjacencyEndpoint(first, second.center, edge.orientation),
      end: adjacencyEndpoint(second, first.center, edge.orientation),
    });
    if (edge.orientation === "horizontal") {
      routingObstacles.push(
        ...createHorizontalBridgeConnectors(edge.id, first, second),
      );
    }
  }

  const width =
    MAJORANA_GEOMETRY.outerMargin * 2 +
    routingObstacleExtent() * 2 +
    MAJORANA_GEOMETRY.tetronWidth +
    (topology.device.tetronColumns - 1) * MAJORANA_GEOMETRY.tetronColumnPitch;
  const height =
    MAJORANA_GEOMETRY.outerMargin * 2 +
    MAJORANA_GEOMETRY.tetronHeight +
    (topology.device.tetronRows - 1) * MAJORANA_GEOMETRY.tetronRowPitch;

  return {
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
    bounds,
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
      edgeId: `q${qubitId}-upper-rail`,
      start: upperLeft,
      end: upperRight,
    },
    lowerRail: {
      edgeId: `q${qubitId}-lower-rail`,
      start: lowerLeft,
      end: lowerRight,
    },
    bridge: {
      edgeId: `q${qubitId}-bridge`,
      start: { x: center.x, y: center.y - halfHeight },
      end: { x: center.x, y: center.y + halfHeight },
    },
  };
}

function createTetronRoutingObstacles(
  tetron: TetronGeometry,
): RoutingObstacleGeometry[] {
  const obstacles: RoutingObstacleGeometry[] = [];
  for (const mzmId of [1, 2, 3, 4] as const) {
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
  orientation: "horizontal" | "vertical",
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

function requiredTetronGeometry(
  tetrons: ReadonlyMap<number, TetronGeometry>,
  qubitId: number,
): TetronGeometry {
  const geometry = tetrons.get(qubitId);
  if (geometry === undefined) {
    throw new Error(`Missing geometry for qubit ${qubitId}`);
  }
  return geometry;
}
