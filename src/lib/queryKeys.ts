import type { ArtifactKey } from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
  unknown: ["unknown"] as const,
  /** One cask's app icon (`useArtifactIcon`), by the whole key. */
  artifactIcon: (key: ArtifactKey) =>
    ["artifactIcon", key.instance_id, key.kind, key.name] as const,
};
