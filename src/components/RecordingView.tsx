interface Props {
  recordingId: string;
  onRecordingChanged: () => void;
  onRecordingDeleted: () => void;
}

export default function RecordingView(_: Props) {
  return <div className="p-8 text-white/40">Recording view (stub)</div>;
}
