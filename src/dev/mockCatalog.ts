/**
 * The browser preview's `get_catalog` (docs/ui-preview.md): the catalog of
 * crates/banager-core/src/catalog.rs, with each entry's state worked out
 * from the pretend Mac's snapshot by the same rules -- a copy of them, for
 * a preview with no Rust. The pretend Mac is Apple silicon on macOS 15.
 */
import type { CatalogEntry, CatalogInstall, CatalogState, InstalledArtifact, ManagerInstance } from "../lib/types";

type Recognizer = { family: string } | { source: string } | { formula: string };

interface Item {
  id: string;
  category: CatalogEntry["category"];
  install: CatalogInstall;
  recognizedBy: Recognizer[];
  needsMacOS?: number;
  /** `installs_here`: Discover installs it from here. */
  installsHere: boolean;
}

/** `CATALOG`, in its order. */
const CATALOG: Item[] = [
  { id: "homebrew", category: "Basics", install: "HomebrewItself", recognizedBy: [{ source: "brew" }], installsHere: false },
  {
    id: "claude-code",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Cask", name: "claude-code" } },
    recognizedBy: [{ family: "claude-code" }],
    installsHere: true,
  },
  {
    id: "codex",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Cask", name: "codex" } },
    recognizedBy: [{ family: "codex" }],
    installsHere: true,
  },
  {
    id: "gemini-cli",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Formula", name: "gemini-cli" } },
    recognizedBy: [{ family: "gemini-cli" }],
    installsHere: true,
  },
  {
    id: "ollama",
    category: "LocalModels",
    install: { Homebrew: { kind: "Cask", name: "ollama-app" } },
    recognizedBy: [{ family: "ollama" }, { source: "ollama" }],
    needsMacOS: 14,
    installsHere: false,
  },
  {
    id: "node",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "node" } },
    recognizedBy: [{ formula: "node" }, { source: "npm" }],
    installsHere: true,
  },
  {
    id: "uv",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "uv" } },
    recognizedBy: [{ formula: "uv" }, { source: "uv" }],
    installsHere: true,
  },
  {
    id: "ffmpeg",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "ffmpeg" } },
    recognizedBy: [{ formula: "ffmpeg" }],
    installsHere: true,
  },
];

/** The pretend Mac. */
const HOST = { macosMajor: 15, appleSilicon: true };

function has(way: Recognizer, instances: ManagerInstance[], artifacts: InstalledArtifact[]): boolean {
  if ("family" in way) return artifacts.some((a) => a.facts.family === way.family);
  if ("source" in way) return instances.some((i) => i.adapter_id === way.source);
  return artifacts.some(
    (a) =>
      a.key.kind === "Formula" &&
      a.key.instance_id.startsWith("brew:") &&
      (a.key.name === way.formula ||
        (a.key.name.startsWith(way.formula) && /^@[0-9.]+$/.test(a.key.name.slice(way.formula.length)))),
  );
}

/** `catalog::entries`. */
export function mockCatalog(instances: ManagerInstance[], artifacts: InstalledArtifact[]): CatalogEntry[] {
  const canInstall = instances.some(
    (i) => i.adapter_id === "brew" && i.status.unavailable === null && i.read_only_reason === null,
  );
  return CATALOG.map((item) => {
    let state: CatalogState;
    if (item.recognizedBy.some((way) => has(way, instances, artifacts))) state = "Installed";
    else if (item.install === "HomebrewItself") state = HOST.appleSilicon ? "NotInstalled" : "HomebrewUnsupported";
    else if (item.needsMacOS !== undefined && HOST.macosMajor < item.needsMacOS) state = { NeedsMacOS: { major: item.needsMacOS } };
    else state = canInstall ? "NotInstalled" : "NeedsHomebrew";
    return { id: item.id, category: item.category, install: item.install, state, installs_here: item.installsHere };
  });
}
