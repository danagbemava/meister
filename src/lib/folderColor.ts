const PALETTE = ["blue", "green", "purple", "orange"] as const;
export type FolderColor = (typeof PALETTE)[number];

/** Map a project ID deterministically to one of four folder colors via
 * FNV-1a 32-bit hash. */
export function folderColorFor(projectId: string): FolderColor {
  let h = 0x811c9dc5;
  for (let i = 0; i < projectId.length; i++) {
    h ^= projectId.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return PALETTE[Math.abs(h) % PALETTE.length];
}
