import type { ArtifactKey } from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
  unknown: ["unknown"] as const,
  /**
   * The first refresh's list before its update checks are done
   * (`InventoryPreview`): written by `writeInventoryPreview`
   * (src/lib/events.ts), never fetched, read by `useInventoryPreview`.
   */
  inventoryPreview: ["inventoryPreview"] as const,
  /** How much disk each installed thing takes (`useSizes`). */
  sizes: ["sizes"] as const,
  /** The operations kept across launches (`useHistory`). */
  history: ["history"] as const,
  /** One cask's app icon (`useArtifactIcon`), by the whole key. */
  artifactIcon: (key: ArtifactKey) =>
    ["artifactIcon", key.instance_id, key.kind, key.name] as const,
};
