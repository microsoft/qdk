// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type {
  CallToolResult,
  ReadResourceResult,
} from "@modelcontextprotocol/sdk/types.js";
import fs from "node:fs/promises";
import path from "node:path";
import {
  registerAppTool,
  registerAppResource,
  RESOURCE_MIME_TYPE,
} from "@modelcontextprotocol/ext-apps/server";
import { getCompiler, QscEventTarget, QdkDiagnostics } from "qsharp-lang";
import {
  getAllKatas,
  getExerciseSources,
  type Kata,
  type Exercise,
} from "qsharp-lang/katas-md";
import { z } from "zod";

type Compiler = ReturnType<typeof getCompiler>;

let compiler: Compiler | undefined;

function ensureCompiler(): Compiler {
  if (!compiler) {
    compiler = getCompiler();
  }
  return compiler;
}

/**
 * Generate a circuit diagram from a Q# exercise solution.
 * Compiles the user code together with the exercise sources and a
 * circuit entry point wrapper, then returns the circuit data as a
 * plain object.  Returns null if for any reason the circuit cannot
 * be generated (missing entry point, compilation failure, etc.).
 */
async function generateCircuitFromQSharp(
  userCode: string,
  circuitEntryPoint: string | undefined,
): Promise<{ circuit: Record<string, unknown> | null; error?: string }> {
  if (!circuitEntryPoint) {
    const msg = "No circuitEntryPoint defined for this exercise";
    console.error(`[circuit] ${msg}`);
    return { circuit: null, error: msg };
  }

  try {
    const sources: [string, string][] = [
      ["solution.qs", userCode],
      ["circuit_entry.qs", circuitEntryPoint],
    ];

    console.error(
      `[circuit] Generating circuit with entry point: ${circuitEntryPoint}`,
    );

    const circuitData = await ensureCompiler().getCircuit(
      {
        sources,
        languageFeatures: [],
        profile: "adaptive_rif",
      },
      {
        generationMethod: "static",
        maxOperations: 10001,
        groupByScope: false,
        sourceLocations: false,
      },
      undefined,
    );
    return { circuit: circuitData as unknown as Record<string, unknown> };
  } catch (e) {
    let msg: string;
    if (e instanceof QdkDiagnostics) {
      const details = e.diagnostics
        .map((d) => {
          const loc = `${d.document}:${d.diagnostic.range.start.line + 1}:${d.diagnostic.range.start.character + 1}`;
          return `  [${d.diagnostic.severity}] ${loc}: ${d.diagnostic.message}`;
        })
        .join("\n");
      msg = `QdkDiagnostics (${e.diagnostics.length} errors):\n${details}`;
    } else {
      msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : String(e);
    }
    console.error(`[circuit] Circuit generation failed:\n${msg}`);
    return { circuit: null, error: msg };
  }
}

async function findExercise(
  kataId: string,
  exerciseId: string,
): Promise<{ kata: Kata; exercise: Exercise }> {
  const katas = await getAllKatas();
  const kata = katas.find((k) => k.id === kataId.trim());
  if (!kata) {
    throw new Error(`Kata not found: ${kataId}`);
  }
  // Exercise IDs in the content follow the convention kataId__name.
  // Accept both the short form (e.g. "flip_qubit") and the full form
  // (e.g. "getting_started__flip_qubit") so callers don't need to know
  // the internal convention.
  const trimmedId = exerciseId.trim();
  const fullId = trimmedId.includes("__")
    ? trimmedId
    : `${kata.id}__${trimmedId}`;
  const exercise = kata.sections.find(
    (s): s is Exercise => s.type === "exercise" && s.id.trim() === fullId,
  );
  if (!exercise) {
    throw new Error(`Exercise not found: ${exerciseId} in kata ${kataId}`);
  }
  return { kata, exercise };
}

export function createServer(): McpServer {
  const server = new McpServer({
    name: "QDK MCP Server",
    version: "0.0.1",
  });

  const circuitUri = "ui://qdk/circuit-app.html";

  // --- renderCircuit tool ---

  registerAppTool(
    server,
    "renderCircuit",
    {
      title: "Render Circuit",
      description:
        "Render a quantum circuit diagram from JSON circuit data. " +
        "Accepts either a CircuitGroup object ({ circuits: [...] }) or " +
        "a bare Circuit object ({ qubits: [...], componentGrid: [...] }). " +
        "Use this to visualize circuit data obtained from the QDK Python library " +
        "(e.g. qsharp.circuit(...).json()). This tool displays a circuit diagram directly in the conversation. " +
        "Do **NOT** draw the circuit diagram as ASCII art, generate an image, or try to render it yourself. " +
        "The user interface will handle rendering the circuit from the structured JSON data that this tool returns. ",
      inputSchema: z.object({
        circuitJson: z
          .string()
          .describe(
            "JSON string representing a Circuit or CircuitGroup object, " +
              "as returned by the Python qsharp.circuit().json() method.",
          ),
      }),
      _meta: { ui: { resourceUri: circuitUri } },
    },
    async (args: { circuitJson: string }): Promise<CallToolResult> => {
      let parsed: Record<string, unknown>;
      try {
        parsed = JSON.parse(args.circuitJson) as Record<string, unknown>;
      } catch {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: "Invalid JSON: the circuitJson input could not be parsed.",
            },
          ],
        };
      }

      // Normalize: accept both a bare Circuit ({ qubits, componentGrid })
      // and a full CircuitGroup ({ version, circuits: [...] }).
      // The circuit-app's toCircuitGroup() handles both, but we normalize
      // here so the summary stats are correct.
      let circuitData: Record<string, unknown>;
      if (
        typeof parsed.version === "number" &&
        Array.isArray(parsed.circuits)
      ) {
        // Already a CircuitGroup
        circuitData = parsed;
      } else if (
        Array.isArray(parsed.qubits) &&
        Array.isArray(parsed.componentGrid)
      ) {
        // Bare Circuit — wrap into a CircuitGroup with version
        circuitData = { version: 1, circuits: [parsed] };
      } else {
        circuitData = parsed;
      }

      // Extract stats from the first circuit for the summary text
      const circuits = circuitData.circuits;
      let numQubits = 0;
      let numOps = 0;
      if (Array.isArray(circuits) && circuits.length > 0) {
        const first = circuits[0] as Record<string, unknown>;
        numQubits = Array.isArray(first.qubits) ? first.qubits.length : 0;
        numOps = Array.isArray(first.componentGrid)
          ? first.componentGrid.length
          : 0;
      }

      return {
        structuredContent: circuitData,
        content: [
          {
            type: "text",
            text: `Circuit rendered: ${numQubits} qubit(s), ${numOps} gate column(s).`,
          },
        ],
      };
    },
  );

  registerAppResource(
    server,
    circuitUri,
    circuitUri,
    { mimeType: RESOURCE_MIME_TYPE },
    async (): Promise<ReadResourceResult> => {
      const html = await fs.readFile(
        path.join(__dirname, "circuit-app.html"),
        "utf-8",
      );
      return {
        contents: [
          { uri: circuitUri, mimeType: RESOURCE_MIME_TYPE, text: html },
        ],
      };
    },
  );

  // --- listKatas tool ---

  server.registerTool(
    "listKatas",
    {
      title: "List Quantum Katas",
      description:
        "Browse available quantum computing tutorials (katas). " +
        "Each kata covers a topic (gates, measurements, algorithms) " +
        "with lessons and exercises. Set includeSections to true to " +
        "expand the full section hierarchy (lessons and exercises with " +
        "IDs, titles, and available languages) inline under each kata.",
      inputSchema: z.object({
        includeSections: z
          .boolean()
          .optional()
          .default(false)
          .describe(
            "When true, include the full list of sections (lessons and " +
              "exercises) under each kata. Defaults to false for a compact summary.",
          ),
      }),
    },
    async (args: { includeSections: boolean }): Promise<CallToolResult> => {
      const katas = await getAllKatas();
      const result = katas.map((k) => {
        const base: Record<string, unknown> = {
          id: k.id,
          title: k.title,
          sectionCount: k.sections.length,
          exerciseCount: k.sections.filter((s) => s.type === "exercise").length,
        };

        if (args.includeSections) {
          const prefix = `${k.id}__`;
          base.sections = k.sections.map((s) => {
            const shortId = s.id.startsWith(prefix)
              ? s.id.slice(prefix.length)
              : s.id;
            if (s.type === "exercise") {
              const availableLanguages: string[] = ["qsharp"];
              if (s.openQasm) {
                availableLanguages.push("openqasm");
              }
              return {
                type: s.type,
                exerciseId: shortId,
                title: s.title,
                availableLanguages,
              };
            }
            return { type: s.type, id: shortId, title: s.title };
          });
        }

        return base;
      });

      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  // --- getExerciseBriefing tool ---

  server.registerTool(
    "getExerciseBriefing",
    {
      title: "Get Exercise Briefing",
      description:
        "Get the prerequisite lesson content and exercise details for a " +
        "single exercise. Returns only the lessons that appear between " +
        "the previous exercise (or the start of the kata) and this exercise, " +
        "pre-sliced and ready to teach. Use this in the teaching phase " +
        "to present lesson material before asking the user to solve an exercise.",
      inputSchema: z.object({
        kataId: z.string().describe("The kata ID, e.g. 'single_qubit_gates'."),
        exerciseId: z.string().describe("The exercise ID, e.g. 'flip_qubit'."),
        language: z
          .enum(["qsharp", "openqasm"])
          .optional()
          .default("qsharp")
          .describe(
            "Programming language for the exercise. Defaults to 'qsharp'.",
          ),
      }),
    },
    async (args: {
      kataId: string;
      exerciseId: string;
      language: "qsharp" | "openqasm";
    }): Promise<CallToolResult> => {
      let kata: Kata;
      let exercise: Exercise;
      try {
        ({ kata, exercise } = await findExercise(args.kataId, args.exerciseId));
      } catch (e) {
        return {
          isError: true,
          content: [{ type: "text", text: (e as Error).message }],
        };
      }

      if (args.language === "openqasm" && !exercise.openQasm) {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Exercise ${args.exerciseId} does not have an OpenQASM variant.`,
            },
          ],
        };
      }

      const sections = kata.sections;
      const targetIdx = sections.indexOf(exercise);

      // Walk backward from the exercise to collect prerequisite lessons.
      // Stop when we hit another exercise or the start of the array.
      const prerequisiteLessons: {
        id: string;
        title: string;
        items: unknown[];
      }[] = [];
      for (let i = targetIdx - 1; i >= 0; i--) {
        const section = sections[i];
        if (section.type === "lesson") {
          const shortId = section.id.startsWith(`${kata.id}__`)
            ? section.id.slice(kata.id.length + 2)
            : section.id;
          prerequisiteLessons.push({
            id: shortId,
            title: section.title,
            items: section.items,
          });
        } else {
          // Hit another exercise — stop collecting
          break;
        }
      }
      // Reverse so lessons appear in their original order
      prerequisiteLessons.reverse();

      // Compute exercise position
      const exercises = sections.filter((s) => s.type === "exercise");
      const exerciseIndex = exercises.indexOf(exercise) + 1; // 1-based
      const totalExercises = exercises.length;

      const shortExId = exercise.id.startsWith(`${kata.id}__`)
        ? exercise.id.slice(kata.id.length + 2)
        : exercise.id;

      const placeholderCode =
        args.language === "openqasm" && exercise.openQasm
          ? exercise.openQasm.placeholderCode
          : exercise.placeholderCode;

      const result = {
        exercise: {
          id: shortExId,
          title: exercise.title,
          description: exercise.description,
          placeholderCode,
          language: args.language,
        },
        prerequisiteLessons,
        exerciseIndex,
        totalExercises,
      };

      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(result, null, 2),
          },
        ],
      };
    },
  );

  // --- checkExerciseSolution tool ---

  server.registerTool(
    "checkExerciseSolution",
    {
      title: "Check Exercise Solution",
      description:
        "Verify a user's solution against the exercise test harness. " +
        "Reads the solution from the exercise folder on disk and updates " +
        "progress.json automatically on success. " +
        "Supports both Q# (.qs) and OpenQASM (.qasm) solutions. " +
        "Returns pass/fail with diagnostic messages and the user's code. " +
        "On success for Q# solutions, also returns a circuit field containing " +
        "the circuit diagram JSON for the solution, which can be rendered " +
        "using the renderCircuit tool. ",
      inputSchema: z.object({
        kataId: z.string().describe("The kata ID, e.g. 'single_qubit_gates'."),
        exerciseId: z.string().describe("The exercise ID, e.g. 'flip_qubit'."),
        workspaceRoot: z
          .string()
          .describe(
            "Absolute path to the workspace root containing the quantum-katas folder. " +
              "The tool reads the solution from the exercise folder and updates " +
              "progress.json automatically on success.",
          ),
        language: z
          .enum(["qsharp", "openqasm"])
          .optional()
          .default("qsharp")
          .describe(
            "Programming language of the solution. Defaults to 'qsharp'.",
          ),
      }),
    },
    async (args: {
      kataId: string;
      exerciseId: string;
      workspaceRoot: string;
      language: "qsharp" | "openqasm";
    }): Promise<CallToolResult> => {
      let exercise: Exercise;
      try {
        ({ exercise } = await findExercise(args.kataId, args.exerciseId));
      } catch (e) {
        return {
          isError: true,
          content: [{ type: "text", text: (e as Error).message }],
        };
      }

      // Read progress.json to find the exercise folder
      const baseDir = path.join(
        path.resolve(args.workspaceRoot),
        "quantum-katas",
      );
      const progressPath = path.join(baseDir, "progress.json");

      let progress: {
        level: string;
        startedAt: string;
        currentExercise: number;
        exercises: {
          sequence: number;
          kataId: string;
          exerciseId: string;
          title: string;
          folder: string;
          status: string;
          completedAt: string | null;
        }[];
      };
      try {
        const raw = await fs.readFile(progressPath, "utf-8");
        progress = JSON.parse(raw);
      } catch {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Could not read progress.json at ${progressPath}. Has createExerciseWorkspace been called?`,
            },
          ],
        };
      }

      const trimmedExId = args.exerciseId.trim();
      const progressEntry = progress.exercises.find(
        (e) => e.kataId === args.kataId.trim() && e.exerciseId === trimmedExId,
      );
      if (!progressEntry) {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Exercise ${args.exerciseId} not found in progress.json for kata ${args.kataId}.`,
            },
          ],
        };
      }

      // Read the user's solution from the exercise folder
      const solutionFile =
        args.language === "openqasm" ? "solution.qasm" : "solution.qs";
      const solutionPath = path.join(
        baseDir,
        "exercises",
        progressEntry.folder,
        solutionFile,
      );
      let userCode: string;
      try {
        userCode = await fs.readFile(solutionPath, "utf-8");
      } catch {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Could not read solution file at ${solutionPath}.`,
            },
          ],
        };
      }

      // Run the exercise check
      const sources = await getExerciseSources(exercise);
      const eventTarget = new QscEventTarget(true);

      let passed: boolean;
      if (args.language === "openqasm" && exercise.openQasm) {
        passed = await ensureCompiler().checkOpenQasmExerciseSolution(
          userCode,
          exercise.openQasm.operationName,
          sources,
          eventTarget,
        );
      } else {
        passed = await ensureCompiler().checkExerciseSolution(
          userCode,
          sources,
          eventTarget,
        );
      }

      const results = eventTarget.getResults();
      const messages: string[] = [];
      for (const shot of results) {
        for (const event of shot.events) {
          if (event.type === "Message") {
            messages.push(event.message);
          }
        }
      }

      // On success, generate a circuit from the user's Q# code (best-effort)
      let circuit: Record<string, unknown> | null = null;
      let circuitError: string | undefined;
      if (passed && args.language === "qsharp") {
        const circuitResult = await generateCircuitFromQSharp(
          userCode,
          exercise.circuitEntryPoint,
        );
        circuit = circuitResult.circuit;
        circuitError = circuitResult.error;
      }

      // On success, update progress.json
      if (passed) {
        progressEntry.status = "completed";
        progressEntry.completedAt = new Date().toISOString();

        // Advance currentExercise to the next incomplete exercise
        const nextIdx = progress.exercises.findIndex(
          (e) => e.status === "not-started" || e.status === "in-progress",
        );
        progress.currentExercise =
          nextIdx === -1 ? progress.exercises.length : nextIdx;

        try {
          await fs.writeFile(
            progressPath,
            JSON.stringify(progress, null, 2),
            "utf-8",
          );
        } catch {
          // Non-fatal: report the pass but note that progress wasn't saved
          const result: Record<string, unknown> = {
            passed,
            messages,
            userCode,
            progressUpdated: false,
            warning: "Solution passed but progress.json could not be updated.",
          };
          if (circuit) result.circuit = circuit;
          if (circuitError) result.circuitError = circuitError;
          return {
            content: [
              {
                type: "text",
                text: JSON.stringify(result, null, 2),
              },
            ],
          };
        }
      }

      const result: Record<string, unknown> = {
        passed,
        messages,
        userCode,
        progressUpdated: passed,
      };
      if (circuit) result.circuit = circuit;
      if (circuitError) result.circuitError = circuitError;
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(result, null, 2),
          },
        ],
      };
    },
  );

  // --- getExerciseHint tool ---

  server.registerTool(
    "getExerciseHint",
    {
      title: "Get Exercise Hint",
      description:
        "Get the explained solution for an exercise to use as progressive hints. " +
        "DO NOT reveal the entire solution at once. Break the content into " +
        "incremental hints, sharing one step at a time. Only show the complete " +
        "solution if the user explicitly asks for it.",
      inputSchema: z.object({
        kataId: z.string().describe("The kata ID, e.g. 'single_qubit_gates'."),
        exerciseId: z.string().describe("The exercise ID, e.g. 'flip_qubit'."),
        language: z
          .enum(["qsharp", "openqasm"])
          .optional()
          .default("qsharp")
          .describe("Programming language for the hint. Defaults to 'qsharp'."),
      }),
    },
    async (args: {
      kataId: string;
      exerciseId: string;
      language: "qsharp" | "openqasm";
    }): Promise<CallToolResult> => {
      let exercise: Exercise;
      try {
        ({ exercise } = await findExercise(args.kataId, args.exerciseId));
      } catch (e) {
        return {
          isError: true,
          content: [{ type: "text", text: (e as Error).message }],
        };
      }

      const explainedSolution =
        args.language === "openqasm" && exercise.openQasm
          ? exercise.openQasm.explainedSolution
          : exercise.explainedSolution;

      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(explainedSolution, null, 2),
          },
        ],
      };
    },
  );

  // --- getExerciseCircuit tool ---

  server.registerTool(
    "getExerciseCircuit",
    {
      title: "Get Exercise Circuit",
      description:
        "Generate a circuit diagram from the reference solution of an exercise. " +
        "Returns circuit JSON that can be rendered using the renderCircuit tool. " +
        "Use this during lesson demonstrations to show the circuit for a concept " +
        "before the user attempts the exercise. Only supports Q# exercises.",
      inputSchema: z.object({
        kataId: z.string().describe("The kata ID, e.g. 'single_qubit_gates'."),
        exerciseId: z.string().describe("The exercise ID, e.g. 'flip_qubit'."),
      }),
    },
    async (args: {
      kataId: string;
      exerciseId: string;
    }): Promise<CallToolResult> => {
      let exercise: Exercise;
      try {
        ({ exercise } = await findExercise(args.kataId, args.exerciseId));
      } catch (e) {
        return {
          isError: true,
          content: [{ type: "text", text: (e as Error).message }],
        };
      }

      // Find the solution code from the explained solution items
      const items = exercise.explainedSolution.items as Array<{
        type: string;
        code?: string;
      }>;
      const solutionItem = items.find(
        (item) => item.type === "solution" && item.code,
      );
      if (!solutionItem?.code) {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: "No reference solution found for this exercise.",
            },
          ],
        };
      }

      const circuitResult = await generateCircuitFromQSharp(
        solutionItem.code,
        exercise.circuitEntryPoint,
      );
      if (!circuitResult.circuit) {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Could not generate circuit for this exercise.${circuitResult.error ? ` Error: ${circuitResult.error}` : ""}`,
            },
          ],
        };
      }

      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(circuitResult.circuit, null, 2),
          },
        ],
      };
    },
  );

  // --- createExerciseWorkspace tool ---

  server.registerTool(
    "createExerciseWorkspace",
    {
      title: "Create Exercise Workspace",
      description:
        "Creates the quantum-katas workspace folder structure on disk. " +
        "Accepts a curated list of exercises and creates the directory layout, " +
        "progress.json, and solution files initialized from placeholder code. " +
        "Supports both Q# (.qs) and OpenQASM (.qasm) exercises.",
      inputSchema: z.object({
        workspaceRoot: z
          .string()
          .describe(
            "Absolute path to the workspace root where the quantum-katas folder will be created.",
          ),
        level: z
          .string()
          .describe(
            "The learning path level: beginner, intermediate, advanced, or custom.",
          ),
        language: z
          .enum(["qsharp", "openqasm"])
          .optional()
          .default("qsharp")
          .describe(
            "Programming language for exercises. Defaults to 'qsharp'.",
          ),
        exercises: z
          .array(
            z.object({
              sequence: z
                .number()
                .describe(
                  "One-based sequence number for the exercise. Used as the two-digit folder prefix.",
                ),
              kataId: z
                .string()
                .describe("The kata ID, e.g. 'single_qubit_gates'."),
              exerciseId: z
                .string()
                .describe("The exercise ID, e.g. 'flip_qubit'."),
              title: z.string().describe("Human-readable exercise title."),
            }),
          )
          .describe("Ordered list of exercises to include in the workspace."),
      }),
    },
    async (args: {
      workspaceRoot: string;
      level: string;
      language: "qsharp" | "openqasm";
      exercises: {
        sequence: number;
        kataId: string;
        exerciseId: string;
        title: string;
      }[];
    }): Promise<CallToolResult> => {
      const baseDir = path.join(
        path.resolve(args.workspaceRoot),
        "quantum-katas",
      );
      const exercisesDir = path.join(baseDir, "exercises");

      try {
        await fs.mkdir(exercisesDir, { recursive: true });

        const exerciseEntries = [];

        for (const ex of args.exercises) {
          let exercise: Exercise;
          try {
            ({ exercise } = await findExercise(ex.kataId, ex.exerciseId));
          } catch (e) {
            return {
              isError: true,
              content: [
                {
                  type: "text",
                  text: `Failed to find exercise ${ex.kataId}/${ex.exerciseId}: ${(e as Error).message}`,
                },
              ],
            };
          }

          const nn = String(ex.sequence).padStart(2, "0");
          const safeId = ex.exerciseId.trim().replace(/[^a-zA-Z0-9_-]/g, "_");
          const folderName = `${nn}_${safeId}`;
          const folderPath = path.join(exercisesDir, folderName);

          await fs.mkdir(folderPath, { recursive: true });

          if (args.language === "openqasm" && exercise.openQasm) {
            await fs.writeFile(
              path.join(folderPath, "solution.qasm"),
              exercise.openQasm.placeholderCode,
              "utf-8",
            );
          } else {
            await fs.writeFile(
              path.join(folderPath, "solution.qs"),
              exercise.placeholderCode,
              "utf-8",
            );
          }

          exerciseEntries.push({
            sequence: ex.sequence,
            kataId: ex.kataId,
            exerciseId: ex.exerciseId,
            title: ex.title,
            folder: folderName,
            status: "not-started",
            completedAt: null,
          });
        }

        const progress = {
          level: args.level,
          startedAt: new Date().toISOString(),
          currentExercise: 0,
          exercises: exerciseEntries,
        };

        await fs.writeFile(
          path.join(baseDir, "progress.json"),
          JSON.stringify(progress, null, 2),
          "utf-8",
        );

        return {
          content: [
            {
              type: "text",
              text: JSON.stringify(
                {
                  workspacePath: baseDir,
                  exerciseCount: exerciseEntries.length,
                  exercises: exerciseEntries,
                },
                null,
                2,
              ),
            },
          ],
        };
      } catch (e) {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `Failed to create workspace: ${(e as Error).message}`,
            },
          ],
        };
      }
    },
  );

  // --- promptExerciseAction tool ---

  server.registerTool(
    "promptExerciseAction",
    {
      title: "Prompt Exercise Action",
      description:
        "Present an interactive prompt to the user while they work on a " +
        "quantum kata exercise. Shows a form with predefined actions " +
        "(check solution, get a hint, explain the problem again). " +
        "Call this after presenting an exercise to let the user choose " +
        "their next action instead of waiting for a free-form chat message.",
      inputSchema: z.object({
        exerciseTitle: z
          .string()
          .describe("The title of the current exercise, for display context."),
      }),
    },
    async (args: { exerciseTitle: string }): Promise<CallToolResult> => {
      const result = await server.server.elicitInput(
        {
          message: `You're working on: **${args.exerciseTitle}**\n\nEdit the solution file, then choose an action:`,
          requestedSchema: {
            type: "object" as const,
            properties: {
              action: {
                type: "string",
                title: "What would you like to do?",
                enum: [
                  "Check my solution",
                  "Give me a hint",
                  "Explain the problem again",
                ],
              },
            },
            required: ["action"],
          },
        },
        {
          timeout: 1200000, // 20 minutes - give the user time to complete the exercise
        },
      );

      if (result.action === "accept" && result.content) {
        return {
          content: [
            {
              type: "text",
              text: JSON.stringify({
                action: "accept",
                choice: result.content.action,
              }),
            },
          ],
        };
      }

      return {
        content: [
          {
            type: "text",
            text: JSON.stringify({
              action: result.action,
              choice: null,
            }),
          },
        ],
      };
    },
  );

  return server;
}
