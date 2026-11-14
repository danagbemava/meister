/** Return `name`, or `name 2`, `name 3`, … so it differs from every name in `taken`. */
export function uniqueName(name: string, taken: string[]): string {
  const lower = new Set(taken.map((t) => t.toLowerCase()));
  let candidate = name;
  for (let n = 2; lower.has(candidate.toLowerCase()); n++) candidate = `${name} ${n}`;
  return candidate;
}
