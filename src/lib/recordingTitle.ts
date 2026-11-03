export function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function captureStamp(date: Date): string {
  return date.toLocaleString(undefined, {
    weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit",
  });
}

const CAPTURE_NAME = /^capture-(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})-[0-9a-f]+$/;

/** Human title for a recording. Captures saved before auto-naming kept the
 *  raw file name (UTC timestamp); show those as a dated title instead. */
export function recordingTitle(name: string): string {
  const m = CAPTURE_NAME.exec(name);
  if (!m) return name;
  const [y, mo, d, h, mi, s] = m.slice(1).map(Number);
  return `Recording · ${captureStamp(new Date(Date.UTC(y, mo - 1, d, h, mi, s)))}`;
}
