import table from "../../crates/banager-core/data/ai-tools.json";
import type { ArtifactKey, InstalledArtifact, ManagerInstance } from "../lib/types";

/**
 * The preview's stand-in for `families::assign`
 * (crates/banager-core/src/families.rs): the same bundled table, read from
 * the same file, and the same matching -- an npm name on npm, a formula or
 * a cask name on Homebrew, a PyPI name (as PEP 503 normalises it) on pipx,
 * uv and pip, a recipe id on its own standalone source -- so the 「AI工具」
 * filter shows in the preview what the app would show. Applied once, where
 * the mock backend puts a snapshot together.
 */

type Source = "npm" | "formula" | "cask" | "pypi" | "standalone";

function normalise(source: Source, name: string): string {
  return source === "pypi" ? name.toLowerCase().replace(/[-_.]+/g, "-") : name;
}

const INDEX = new Map<string, string>();
for (const family of table.families) {
  for (const member of family.members) {
    const source = member.source as Source;
    INDEX.set(`${source}|${normalise(source, member.name)}`, family.id);
  }
}

function sourceOf(adapterId: string, key: ArtifactKey): Source | null {
  if (adapterId === "npm" && key.kind === "Package") return "npm";
  if (adapterId === "brew" && key.kind === "Formula") return "formula";
  if (adapterId === "brew" && key.kind === "Cask") return "cask";
  if ((adapterId === "pipx" || adapterId === "uv") && key.kind === "Tool") return "pypi";
  if (adapterId === "pip" && key.kind === "Package") return "pypi";
  if (key.kind === "Binary" && adapterId === `standalone-${key.name}`) return "standalone";
  return null;
}

/** The family of an artifact of a source with `adapterId`, or null. */
export function mockFamilyOf(adapterId: string, key: ArtifactKey): string | null {
  const source = sourceOf(adapterId, key);
  return source === null ? null : (INDEX.get(`${source}|${normalise(source, key.name)}`) ?? null);
}

/** Every artifact with `facts.family` set from the table, by its instance's adapter. */
export function withFamilies(instances: ManagerInstance[], artifacts: InstalledArtifact[]): InstalledArtifact[] {
  const adapters = new Map(instances.map((instance) => [instance.id, instance.adapter_id]));
  return artifacts.map((artifact) => {
    const adapterId = adapters.get(artifact.key.instance_id);
    const family = adapterId === undefined ? null : mockFamilyOf(adapterId, artifact.key);
    return artifact.facts.family === family ? artifact : { ...artifact, facts: { ...artifact.facts, family } };
  });
}
