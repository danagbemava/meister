import type { Project, Recording } from "../types/storage";
import type { Selection } from "../App";

interface Props {
  projects: Project[];
  recordingsByProject: Record<string, Recording[]>;
  selection: Selection;
  onSelect: (s: Selection) => void;
  onProjectsChanged: () => void;
}

export default function Sidebar({ projects }: Props) {
  return (
    <aside className="w-60 shrink-0 border-r border-white/5 bg-surface-secondary p-3 text-sm text-white/40">
      <div>Projects ({projects.length})</div>
    </aside>
  );
}
