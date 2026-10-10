/**
 * How much the pretend Mac's tools take on disk (docs/ui-preview.md): what
 * the preview's `get_sizes` answers for a committed snapshot, measured the
 * way crates/banager-core/src/size.rs measures -- and not -- each kind.
 * Dev-only, like everything in src/dev.
 *
 * Measured: Homebrew formulae (their other versions apart), casks with an
 * app, casks with no app that keep their program in their Caskroom folder
 * (`COMMANDS_IN_CASKROOM`), npm packages, pipx and uv tools, crates, and
 * the tools with their own installer. Not: pip packages, any other cask
 * with no app (a font, a `pkg`), a tool whose program is gone, and an
 * Ollama model, which keeps the size Ollama gives it -- the models are
 * measured together, as their folder.
 */
import type { ArtifactSize, InstalledArtifact, ManagerInstance, Measured, ModelsSize, Sizes } from "../lib/types";

const MB = 1_000_000;
const GB = 1_000_000_000;

/** What each named tool takes, roughly as on a real Mac in late 2026. */
const KNOWN: Record<string, number> = {
  // Homebrew formulae.
  ffmpeg: 52.4 * MB,
  gh: 48.1 * MB,
  git: 71.3 * MB,
  htop: 0.4 * MB,
  jq: 1.2 * MB,
  "node@22": 312.6 * MB,
  ollama: 98.2 * MB,
  pipx: 2.1 * MB,
  "postgresql@17": 76.5 * MB,
  "python@3.13": 245.7 * MB,
  ripgrep: 6.3 * MB,
  wget: 4.2 * MB,
  "ca-certificates": 0.3 * MB,
  gettext: 9.4 * MB,
  "icu4c@78": 81.2 * MB,
  libnghttp2: 0.9 * MB,
  libunistring: 5.6 * MB,
  libuv: 1.2 * MB,
  mpdecimal: 0.6 * MB,
  "openssl@3": 36.8 * MB,
  pcre2: 4.7 * MB,
  readline: 1.9 * MB,
  sqlite: 7.4 * MB,
  x264: 4.4 * MB,
  xz: 2.3 * MB,
  zstd: 3.1 * MB,
  // Casks with an app.
  iterm2: 182.3 * MB,
  "visual-studio-code": 612.4 * MB,
  // A cask with no app, its program in its Caskroom folder.
  "android-platform-tools": 38.6 * MB,
  // npm, Cargo, pipx, uv and the tools with their own installer.
  corepack: 0.9 * MB,
  npm: 11.2 * MB,
  prettier: 8.4 * MB,
  typescript: 23.1 * MB,
  "@anthropic-ai/claude-code": 61.8 * MB,
  "jj-cli": 34.2 * MB,
  tokei: 9.8 * MB,
  httpie: 31.4 * MB,
  poetry: 64.9 * MB,
  agy: 176.3 * MB,
  claude: 203.5 * MB,
  grok: 131.6 * MB,
  rustup: 11.1 * MB,
  "pre-commit": 22.7 * MB,
  ruff: 28.3 * MB,
  "youtube-dl": 8.6 * MB,
};

/**
 * What the other kegs Homebrew keeps beside a formula take together: one
 * entry for each formula mockData.ts gives `other_versions`, as size.rs
 * measures the same kegs those versions are.
 */
const OTHER_VERSIONS: Record<string, number> = {
  "node@22": 298.4 * MB,
  "python@3.13": 231.9 * MB,
  gettext: 9.1 * MB,
  libuv: 1.2 * MB,
  git: 70.9 * MB,
  "openssl@3": 36.5 * MB,
  readline: 1.9 * MB,
  "youtube-dl": 8.4 * MB,
};

/**
 * The preview's casks with no app that size.rs measures as their folder in
 * `<prefix>/Caskroom` (`commands_staged_in`): every stanza a `binary` link
 * into that folder or one that leaves its files there, as Homebrew's cask
 * API has them in October 2026. What decides it (`CommandInputs`) is never
 * on the wire, so the preview names them. Not Flutter: its `suite` is
 * moved to `<prefix>/share/flutter`. Not Google Cloud CLI: its steps copy
 * the SDK to `<prefix>/share/google-cloud-sdk` and leave a link behind.
 */
const COMMANDS_IN_CASKROOM = new Set([
  "1password-cli",
  "android-platform-tools",
  "antigravity-cli",
  "aws-vault-binary",
  "claude-code",
  "clickhouse",
  "codeql",
  "coderabbit",
  "codex",
  "copilot-cli",
  "cursor-cli",
  "gitkraken-cli",
  "miniconda",
  "miniforge",
  "ngrok",
  "tuist",
]);

/** One that could not all be read, and one the round's budget cut short. */
const PARTIAL = new Set(["pre-commit"]);
const AT_LEAST = new Set(["visual-studio-code"]);

/** What the models of an Ollama take together: its folder, each shared layer once. */
const MODELS_FOLDER = 11.3 * GB;

/** A size for a tool the table has no line for (`?state=many`): 0.5 MB to 300 MB, the same on every run. */
function sizeFromName(name: string): number {
  let hash = 2166136261;
  for (const char of name) {
    hash = Math.imul(hash ^ char.charCodeAt(0), 16777619) >>> 0;
  }
  return Math.round((0.5 + (hash % 3000) / 10) * MB);
}

function measured(name: string, bytes: number): Measured {
  return { bytes: Math.round(bytes), partial: PARTIAL.has(name), at_least: AT_LEAST.has(name) };
}

/** Whether size.rs measures `artifact` (`roots_of`): its rules, on the preview's data. */
function isMeasured(artifact: InstalledArtifact, adapterId: string): boolean {
  switch (adapterId) {
    case "brew":
      return artifact.key.kind === "Formula"
        ? artifact.version !== ""
        : artifact.path !== null || COMMANDS_IN_CASKROOM.has(artifact.key.name);
    case "npm":
      return true;
    case "pipx":
    case "uv":
    case "cargo":
      return artifact.path !== null;
    default:
      return adapterId.startsWith("standalone-") && artifact.path !== null;
  }
}

/**
 * The sizes for `artifacts` and `instances` at `round`: each artifact
 * `isDone` says is measured with its numbers, the rest still "measuring";
 * the models and the total once nothing is.
 */
export function mockSizes(
  round: number,
  instances: ManagerInstance[],
  artifacts: InstalledArtifact[],
  isDone: (artifact: InstalledArtifact) => boolean,
): Sizes {
  const adapterOf = new Map(instances.map((instance) => [instance.id, instance.adapter_id]));
  const sizes: ArtifactSize[] = [];
  for (const artifact of artifacts) {
    const adapterId = adapterOf.get(artifact.key.instance_id);
    if (adapterId === undefined || !isMeasured(artifact, adapterId)) continue;
    const name = artifact.key.name;
    const done = isDone(artifact);
    // Only a formula Homebrew says has other versions has their size.
    const hasOthers = (artifact.facts.homebrew?.other_versions.length ?? 0) > 0;
    const old = hasOthers ? (OTHER_VERSIONS[name] ?? sizeFromName(`${name} other`)) : undefined;
    sizes.push({
      key: artifact.key,
      version: artifact.version,
      measured: done ? measured(name, KNOWN[name] ?? sizeFromName(name)) : null,
      old_versions: done && old !== undefined ? measured(`${name} old`, old) : null,
    });
  }
  const models: ModelsSize[] = instances
    .filter(
      (instance) =>
        instance.adapter_id === "ollama" &&
        artifacts.some((a) => a.key.instance_id === instance.id && a.key.kind === "Model"),
    )
    .map((instance) => ({
      instance_id: instance.id,
      measured: sizes.every((size) => size.measured !== null)
        ? { bytes: MODELS_FOLDER, partial: false, at_least: false }
        : null,
    }));
  const done = sizes.every((size) => size.measured !== null);
  // Each source's, then everything: the pretend Mac shares no file
  // between two tools, so a plain sum is what size.rs would count.
  const bySource = new Map<string, Measured[]>();
  const add = (instanceId: string, measured: Measured | null) => {
    if (measured === null) return;
    bySource.set(instanceId, [...(bySource.get(instanceId) ?? []), measured]);
  };
  for (const size of sizes) {
    add(size.key.instance_id, size.measured);
    add(size.key.instance_id, size.old_versions);
  }
  for (const m of models) add(m.instance_id, m.measured);
  return {
    round,
    done,
    artifacts: sizes,
    models,
    total: done ? together([...bySource.values()].flat()) : null,
    sources: done
      ? instances.flatMap((instance) => {
          const measured = together(bySource.get(instance.id) ?? []);
          return measured === null ? [] : [{ instance_id: instance.id, measured }];
        })
      : [],
  };
}

/** Several measurements as one: their bytes added up, partial or at least if any of them is. */
function together(all: Measured[]): Measured | null {
  if (all.length === 0) return null;
  return {
    bytes: all.reduce((sum, m) => sum + m.bytes, 0),
    partial: all.some((m) => m.partial),
    at_least: all.some((m) => m.at_least),
  };
}
