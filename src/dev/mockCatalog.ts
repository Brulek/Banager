/**
 * The browser preview's `get_catalog` (docs/ui-preview.md): the catalog of
 * crates/banager-core/src/catalog.rs, with each entry's state worked out
 * from the pretend Mac's snapshot by the same rules -- a copy of them, for
 * a preview with no Rust. The pretend Mac is Apple silicon on macOS 15.
 */
import type { CatalogEntry, CatalogInstall, CatalogState, InstalledArtifact, ManagerInstance } from "../lib/types";

type Recognizer = { family: string } | { source: string } | { formula: string } | "anyModel";

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
    installsHere: true,
  },
  {
    id: "model",
    category: "LocalModels",
    // `starter_model` for the pretend Mac's 16 GB.
    install: { OllamaModel: { name: "gemma3:4b", download_bytes: 3_350_272_403 } },
    recognizedBy: ["anyModel"],
    installsHere: true,
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
const HOST = { macosMajor: 15, appleSilicon: true, freeBytes: 182_400_000_000 };

/** `catalog::this_macs_ollama`: an Ollama source at this Mac's own address. */
function thisMacsOllama(instanceId: string): boolean {
  if (!instanceId.startsWith("ollama:")) return false;
  try {
    const host = new URL(instanceId.slice("ollama:".length)).hostname.toLowerCase();
    // As `host_is_this_mac`: localhost, any loopback address, and the
    // "every interface" ones, which land on the loopback.
    return ["localhost", "[::1]", "0.0.0.0", "[::]"].includes(host) || /^127(\.\d{1,3}){3}$/.test(host);
  } catch {
    return false;
  }
}

function has(way: Recognizer, instances: ManagerInstance[], artifacts: InstalledArtifact[]): boolean {
  if (way === "anyModel") return artifacts.some((a) => a.key.kind === "Model" && thisMacsOllama(a.key.instance_id));
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

/** `catalog::id_installing`: the entry that installs this Homebrew formula or cask, if one does. */
export function catalogIdInstalling(kind: "Formula" | "Cask", name: string): string | null {
  const item = CATALOG.find(
    (candidate) =>
      typeof candidate.install === "object" &&
      "Homebrew" in candidate.install &&
      candidate.install.Homebrew.kind === kind &&
      candidate.install.Homebrew.name === name,
  );
  return item?.id ?? null;
}

/** `catalog::entries`. */
export function mockCatalog(instances: ManagerInstance[], artifacts: InstalledArtifact[]): CatalogEntry[] {
  const canInstall = instances.some(
    (i) => i.adapter_id === "brew" && i.status.unavailable === null && i.read_only_reason === null,
  );
  const canPull = instances.some(
    (i) => thisMacsOllama(i.id) && i.status.unavailable === null && i.read_only_reason === null,
  );
  return CATALOG.map((item) => {
    let state: CatalogState;
    if (item.recognizedBy.some((way) => has(way, instances, artifacts))) state = "Installed";
    else if (item.install === "HomebrewItself") state = HOST.appleSilicon ? "NotInstalled" : "HomebrewUnsupported";
    else if ("OllamaModel" in item.install)
      state = canPull ? "NotInstalled" : instances.some((i) => i.adapter_id === "ollama") ? "OllamaNotReady" : "NeedsOllama";
    else if (item.needsMacOS !== undefined && HOST.macosMajor < item.needsMacOS) state = { NeedsMacOS: { major: item.needsMacOS } };
    else
      state = canInstall
        ? "NotInstalled"
        : instances.some((i) => i.adapter_id === "brew")
          ? "HomebrewNotReady"
          : "NeedsHomebrew";
    return {
      id: item.id,
      category: item.category,
      install: item.install,
      state,
      installs_here: item.installsHere,
      // The pretend Mac's free room, for the model's sheet alone.
      free_bytes: "OllamaModel" in (typeof item.install === "object" ? item.install : {}) ? HOST.freeBytes : null,
    };
  });
}
