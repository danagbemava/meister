import type { ReactNode } from "react";

type Block =
  | { kind: "heading"; level: 1 | 2 | 3; text: string }
  | { kind: "list"; ordered: boolean; items: { depth: number; text: string }[] }
  | { kind: "paragraph"; lines: string[] }
  | { kind: "rule" };

const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

export function parseNotes(markdown: string): Block[] {
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let inItem = false;
  const flush = () => {
    if (paragraph.length) blocks.push({ kind: "paragraph", lines: paragraph });
    paragraph = [];
  };

  for (const raw of markdown.split("\n")) {
    const line = raw.trimEnd();
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    const item = LIST_ITEM.exec(line);
    const continuesItem = inItem;
    inItem = false;
    if (!line.trim()) {
      flush();
    } else if (heading) {
      flush();
      blocks.push({ kind: "heading", level: heading[1].length as 1 | 2 | 3, text: heading[2] });
    } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
    } else if (item) {
      flush();
      const ordered = /\d/.test(item[2]);
      const depth = Math.floor(item[1].replace(/\t/g, "    ").length / 2);
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list" && last.ordered === ordered) last.items.push({ depth, text: item[3] });
      else blocks.push({ kind: "list", ordered, items: [{ depth, text: item[3] }] });
      inItem = true;
    } else {
      const last = blocks[blocks.length - 1];
      if (continuesItem && last?.kind === "list") {
        last.items[last.items.length - 1].text += ` ${line.trim()}`;
        inItem = true;
      } else {
        paragraph.push(line.trim());
      }
    }
  }
  flush();
  return blocks;
}

function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|__[^_]+__|`[^`]+`|\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g).map((part, i) => {
    if (/^(\*\*|__).+\1$/.test(part)) return <strong key={i} className="font-semibold text-label-primary">{part.slice(2, -2)}</strong>;
    if (/^`.+`$/.test(part)) return <code key={i} className="font-mono text-[0.9em] bg-fill-tertiary rounded px-1">{part.slice(1, -1)}</code>;
    if (/^([*_]).+\1$/.test(part)) return <em key={i}>{part.slice(1, -1)}</em>;
    return part;
  });
}

const HEADING_CLASS = {
  1: "text-[22px] mt-2",
  2: "text-[18px] mt-6",
  3: "text-[15px] mt-4 uppercase tracking-wider font-sans text-label-secondary",
};

export default function Notes({ markdown }: { markdown: string }) {
  return (
    <article className="max-w-[68ch] font-display text-[15px] leading-[1.65] text-label-primary flex flex-col gap-3">
      {parseNotes(markdown).map((b, i) => {
        if (b.kind === "heading") {
          const Tag = (["h2", "h3", "h4"] as const)[b.level - 1];
          return (
            <Tag key={i} className={`font-display font-semibold leading-snug text-label-primary first:mt-0 ${HEADING_CLASS[b.level]}`}>
              {inline(b.text)}
            </Tag>
          );
        }
        if (b.kind === "rule") return <hr key={i} className="border-separator my-2" />;
        if (b.kind === "paragraph") {
          return (
            <p key={i}>
              {b.lines.map((line, j) => (
                <span key={j}>{j > 0 && <br />}{inline(line)}</span>
              ))}
            </p>
          );
        }
        const List = b.ordered ? "ol" : "ul";
        return (
          <List key={i} className={`${b.ordered ? "list-decimal" : "list-disc"} pl-5 flex flex-col gap-1.5 marker:text-label-tertiary`}>
            {b.items.map((it, j) => (
              <li key={j} style={{ marginLeft: `${it.depth * 1.25}rem` }}>{inline(it.text)}</li>
            ))}
          </List>
        );
      })}
    </article>
  );
}
