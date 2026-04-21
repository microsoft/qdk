// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  IAIProvider,
  AIHintContext,
  AIErrorContext,
  AIReviewContext,
  AIQuestionContext,
} from "./types.js";

/** No-op provider — returns null for all methods. Used when no AI is configured. */
export class NoOpAIProvider implements IAIProvider {
  async getHint(_ctx: AIHintContext): Promise<string | null> {
    return null;
  }
  async explainError(_ctx: AIErrorContext): Promise<string | null> {
    return null;
  }
  async reviewSolution(_ctx: AIReviewContext): Promise<string | null> {
    return null;
  }
  async askQuestion(_ctx: AIQuestionContext): Promise<string | null> {
    return null;
  }
}

export interface LLMProviderConfig {
  /** OpenAI-compatible API endpoint (e.g. "https://api.openai.com/v1") */
  endpoint: string;
  apiKey: string;
  model: string;
}

interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

/**
 * AI provider that calls an OpenAI-compatible chat completions API.
 * Works with OpenAI, Azure OpenAI, Ollama, LM Studio, etc.
 */
export class LLMAIProvider implements IAIProvider {
  private endpoint: string;
  private apiKey: string;
  private model: string;

  constructor(config: LLMProviderConfig) {
    this.endpoint = config.endpoint.replace(/\/+$/, "");
    this.apiKey = config.apiKey;
    this.model = config.model;
  }

  async getHint(ctx: AIHintContext): Promise<string | null> {
    const levelDesc = [
      "Give a gentle conceptual nudge — point the student toward the right idea without being specific about code.",
      "Give more specific algorithmic guidance — describe the approach or gate sequence to consider, but don't write code.",
      "Give a targeted hint about which Q# operations or patterns to use — be specific but still don't write the full solution.",
    ];

    const system = `You are a quantum computing tutor helping a student learn Q# through interactive exercises.
The student is stuck on an exercise. Give a hint without revealing the full solution.
Hint level: ${ctx.hintLevel}/3
${levelDesc[Math.min(ctx.hintLevel - 1, 2)]}
Keep your response concise (2-4 sentences). Never show complete code solutions.`;

    let userMsg = `## Exercise\n${ctx.exerciseDescription}\n\n## Student's Current Code\n\`\`\`qsharp\n${ctx.userCode}\n\`\`\``;
    if (ctx.checkResult?.error) {
      userMsg += `\n\n## Last Check Result\nError: ${ctx.checkResult.error}`;
    }
    if (ctx.previousHints.length > 0) {
      userMsg += `\n\n## Previous Hints Given\n${ctx.previousHints.map((h, i) => `${i + 1}. ${h}`).join("\n")}`;
    }

    return this.chat([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async explainError(ctx: AIErrorContext): Promise<string | null> {
    const system = `You are a Q# compiler error explainer for quantum computing beginners.
Explain the error in plain English. Relate it to quantum computing concepts when relevant.
Keep your explanation concise (2-3 sentences). Be helpful and encouraging.`;

    let userMsg = `## Code\n\`\`\`qsharp\n${ctx.code}\n\`\`\`\n\n## Error\n${ctx.error}`;
    if (ctx.exerciseDescription) {
      userMsg += `\n\n## Exercise Context\n${ctx.exerciseDescription}`;
    }

    return this.chat([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async reviewSolution(ctx: AIReviewContext): Promise<string | null> {
    const system = `You are a quantum computing code reviewer. The student has successfully solved an exercise.
Both their solution and the reference solution are correct. In 3-5 sentences:
- Note what the student did well
- If their approach differs from the reference, explain the difference
- Mention any deeper quantum concepts illustrated by the solution
Be positive and educational.`;

    const userMsg = `## Exercise\n${ctx.exerciseDescription}\n\n## Student's Solution\n\`\`\`qsharp\n${ctx.userCode}\n\`\`\`\n\n## Reference Solution\n\`\`\`qsharp\n${ctx.referenceSolution}\n\`\`\``;

    return this.chat([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async askQuestion(ctx: AIQuestionContext): Promise<string | null> {
    const system = `You are a quantum computing educator helping a student who is working through Q# katas (interactive exercises).
They are currently studying: "${ctx.kataTitle}".
Answer their question using the provided lesson content as context.
If the question goes beyond the current lesson, give a brief answer and mention which topics might cover it in more depth.
Keep answers focused, educational, and concise.`;

    const messages: ChatMessage[] = [
      { role: "system", content: system },
    ];

    // Include recent conversation history (max 4 exchanges)
    if (ctx.history) {
      const recent = ctx.history.slice(-8);
      for (const h of recent) {
        messages.push({ role: h.role, content: h.content });
      }
    }

    // Truncate lesson content if too long (rough token budget)
    let lessonContent = ctx.lessonContent;
    if (lessonContent.length > 4000) {
      lessonContent = lessonContent.slice(0, 4000) + "\n...(truncated)";
    }

    messages.push({
      role: "user",
      content: `## Current Lesson Content\n${lessonContent}\n\n## My Question\n${ctx.question}`,
    });

    return this.chat(messages);
  }

  private async chat(messages: ChatMessage[]): Promise<string | null> {
    const url = `${this.endpoint}/chat/completions`;
    try {
      const response = await fetch(url, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify({
          model: this.model,
          messages,
          max_tokens: 500,
          temperature: 0.7,
        }),
      });

      if (!response.ok) {
        const text = await response.text();
        console.error(`AI API error (${response.status}): ${text}`);
        return "AI is currently unavailable. Try the built-in hints instead.";
      }

      const data = (await response.json()) as {
        choices?: Array<{ message?: { content?: string } }>;
      };
      return data.choices?.[0]?.message?.content ?? null;
    } catch (err) {
      console.error("AI API call failed:", err);
      return "AI is currently unavailable. Try the built-in hints instead.";
    }
  }
}
