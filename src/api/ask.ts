import { invoke } from "@tauri-apps/api/core";
import type { AskCitation, AskTurn } from "../types/ask";

export interface AskAnswer {
  text: string;
  citations: AskCitation[];
}

export const askApi = {
  query: (recordingId: string, question: string, history: AskTurn[]) =>
    invoke<AskAnswer>("ask_query", {
      recordingId,
      question,
      history: history.map((t) => ({ role: t.role, content: t.content })),
    }),

  cancel: (recordingId: string) =>
    invoke<void>("cancel_ask", { recordingId }),
};
