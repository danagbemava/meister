import type { Recording } from "../types/storage";

interface Props {
  projectId: string;
  onProjectChanged: () => void;
  onRecordingCreated: (r: Recording) => void;
}

export default function ProjectView(_: Props) {
  return <div className="p-8 text-white/40">Project view (stub)</div>;
}
