import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import type { OutputFormat } from "../types/storage";

const FILTER_LABELS: Record<OutputFormat, string> = {
  md: "Markdown",
  txt: "Plain text",
  docx: "Word document",
  pdf: "PDF",
};

const ALL_FORMATS: OutputFormat[] = ["md", "txt", "docx", "pdf"];

function filtersFor(defaultFmt: OutputFormat) {
  const ordered = [defaultFmt, ...ALL_FORMATS.filter((f) => f !== defaultFmt)];
  return ordered.map((f) => ({ name: FILTER_LABELS[f], extensions: [f] }));
}

function inferFormat(path: string, fallback: OutputFormat): OutputFormat {
  const ext = path.split(".").pop()?.toLowerCase();
  if (ext && (ALL_FORMATS as string[]).includes(ext)) return ext as OutputFormat;
  return fallback;
}

function normalizeExt(path: string, fmt: OutputFormat): string {
  const lower = path.toLowerCase();
  if (lower.endsWith(`.${fmt}`)) return path;
  const dot = path.lastIndexOf(".");
  if (dot > 0 && (ALL_FORMATS as string[]).includes(lower.slice(dot + 1))) {
    return path.slice(0, dot) + `.${fmt}`;
  }
  return path + `.${fmt}`;
}

export interface ExportInput {
  formatted: string;
  recordingName: string;
  defaultFormat: OutputFormat;
}

export interface ExportResult {
  path: string | null;
}

export async function exportRecording(input: ExportInput): Promise<ExportResult> {
  const dest = await saveDialog({
    defaultPath: `${input.recordingName}.${input.defaultFormat}`,
    filters: filtersFor(input.defaultFormat),
  });
  if (typeof dest !== "string") return { path: null };

  const fmt = inferFormat(dest, input.defaultFormat);
  const final = normalizeExt(dest, fmt);

  if (fmt === "md" || fmt === "txt") {
    await invoke("save_text_file", { content: input.formatted, path: final });
  } else if (fmt === "docx") {
    const bytes = await generateDocx(input.formatted);
    await invoke("save_binary_file", { bytes: Array.from(bytes), path: final });
  } else if (fmt === "pdf") {
    const bytes = await generatePdf(input.formatted);
    await invoke("save_binary_file", { bytes: Array.from(bytes), path: final });
  }

  console.log(`exported to ${final}`);
  return { path: final };
}

async function generateDocx(markdown: string): Promise<Uint8Array> {
  const { Document, Packer, Paragraph, TextRun, HeadingLevel } = await import("docx");
  const lines = markdown.split("\n");
  const children = lines.map((line) => {
    if (line.startsWith("### ")) {
      return new Paragraph({ text: line.slice(4), heading: HeadingLevel.HEADING_3 });
    } else if (line.startsWith("## ")) {
      return new Paragraph({ text: line.slice(3), heading: HeadingLevel.HEADING_2 });
    } else if (line.startsWith("# ")) {
      return new Paragraph({ text: line.slice(2), heading: HeadingLevel.HEADING_1 });
    } else if (line.startsWith("**") && line.endsWith("**") && line.length > 4) {
      return new Paragraph({ children: [new TextRun({ text: line.slice(2, -2), bold: true })] });
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      return new Paragraph({ text: line.slice(2), bullet: { level: 0 } });
    } else if (line.trim() === "") {
      return new Paragraph({ text: "" });
    } else {
      return new Paragraph({ text: line });
    }
  });
  const doc = new Document({ sections: [{ properties: {}, children }] });
  const blob = await Packer.toBlob(doc);
  const ab = await blob.arrayBuffer();
  return new Uint8Array(ab);
}

async function generatePdf(markdown: string): Promise<Uint8Array> {
  const { jsPDF } = await import("jspdf");
  const doc = new jsPDF({ unit: "pt", format: "a4" });
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = 60;
  const maxWidth = pageWidth - margin * 2;
  let y = 80;
  const lines = markdown.split("\n");
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (y > doc.internal.pageSize.getHeight() - 60) {
      doc.addPage();
      y = 60;
    }
    if (line.startsWith("# ")) {
      doc.setFontSize(20).setFont("helvetica", "bold");
      doc.text(line.slice(2), margin, y); y += 28;
    } else if (line.startsWith("## ")) {
      doc.setFontSize(16).setFont("helvetica", "bold");
      doc.text(line.slice(3), margin, y); y += 22;
    } else if (line.startsWith("### ")) {
      doc.setFontSize(13).setFont("helvetica", "bold");
      doc.text(line.slice(4), margin, y); y += 18;
    } else if (line.startsWith("- ") || line.startsWith("* ")) {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(`• ${line.slice(2)}`, maxWidth - 12);
      doc.text(wrapped, margin + 8, y); y += wrapped.length * 14 + 2;
    } else if (line.trim() === "") {
      y += 8;
    } else {
      doc.setFontSize(11).setFont("helvetica", "normal");
      const wrapped = doc.splitTextToSize(line, maxWidth);
      doc.text(wrapped, margin, y); y += wrapped.length * 14 + 2;
    }
  }
  return new Uint8Array(doc.output("arraybuffer") as ArrayBuffer);
}
