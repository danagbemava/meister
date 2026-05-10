export interface AskCitation {
  quote: string;
  start: number | null;
  end: number | null;
}

export interface AskTurn {
  role: "user" | "assistant";
  content: string;
  citations?: AskCitation[];
}

export interface AskThread {
  id: string;
  turns: AskTurn[];
  pending: boolean;
  streamingText?: string;
}

export type AskProgressPayload =
  | { phase: "search"; chunk: { index: number; total: number } }
  | { phase: "answer"; chars: number }
  | { phase: "done" };
