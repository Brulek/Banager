/**
 * The machine the browser preview pretends to be (docs/ui-preview.md):
 * every source Canager knows, with installed software and update rows in
 * every state the pages render. Dev-only; nothing outside src/dev imports
 * it. Typed against src/lib/types.ts, so `pnpm typecheck` holds it to the
 * wire format the real backend sends.
 *
 * Paths are under a generic home folder, `/Users/you`, never a real
 * account's. Versions and descriptions are plausible for late 2026, not a
 * recording of any one Mac. Timestamps are fixed, so every run shows the
 * same list; only `refreshed_at` and `issued_at` follow the clock.
 */
import type {
  ArtifactKey,
  ArtifactKind,
  DetectOutcome,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  SourceError,
  UnknownEntry,
  UnknownScan,
  UpdateCandidate,
  Warning,
} from "../lib/types";
import type { Scenario, ScenarioScan, ScenarioState } from "./scenario";

/** The home folder every path in the preview is under. */
export const HOME = "/Users/you";

/** `~/relative` as an absolute path. */
export function inHome(relative: string): string {
  return `${HOME}/${relative}`;
}

/** The instance ids the backend would build (`model::instance_id`). */
export const IDS = {
  brew: "brew:/opt/homebrew",
  cargo: `cargo:${HOME}/.cargo`,
  npm: "npm:/opt/homebrew",
  /** npm installed from nodejs.org, whose prefix the user cannot write. */
  npmSystem: "npm:/usr/local",
  ollama: "ollama:http://127.0.0.1:11434",
  pip: "pip:/opt/homebrew/bin/python3",
  pipx: "pipx",
  agy: "standalone-agy",
  claude: "standalone-claude",
  grok: "standalone-grok",
  rustup: "standalone-rustup",
  uv: "uv",
} as const;

/**
 * Each source's `verified_versions` (adapters/meta/*.toml): a version
 * outside it is flagged in a line at the top of the Installed page.
 */
const VERIFIED_VERSIONS: Record<string, string> = {
  brew: "7.0.3",
  cargo: "1.98.1",
  npm: "12.0.2",
  ollama: "0.34.1",
  pip: "26.2.1",
  pipx: "1.17.3",
  "standalone-agy": "1.2.11",
  "standalone-claude": "2.1.282",
  "standalone-grok": "1.0.41",
  "standalone-rustup": "1.29.1",
  uv: "0.12.17",
};

/** `ManagerInstance.unverified_version`, by `AdapterMeta::unverified_version`'s rule. */
export function unverifiedVersion(adapterId: string, version: string | null): string | null {
  const verified = VERIFIED_VERSIONS[adapterId];
  return version !== null && verified !== undefined && version !== verified ? version : null;
}

export function key(instanceId: string, kind: ArtifactKind, name: string): ArtifactKey {
  return { instance_id: instanceId, kind, name };
}

export function sameKey(a: ArtifactKey, b: ArtifactKey): boolean {
  return a.instance_id === b.instance_id && a.kind === b.kind && a.name === b.name;
}

function instance(
  adapterId: string,
  id: string,
  exePath: string,
  prefix: string,
  version: string | null,
  fields: Partial<ManagerInstance> = {},
): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: exePath,
    prefix,
    scope: "User",
    version,
    unverified_version: unverifiedVersion(adapterId, version),
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...fields,
  };
}

function artifact(
  instanceId: string,
  kind: ArtifactKind,
  name: string,
  version: string,
  fields: Partial<Omit<InstalledArtifact, "key" | "version">> = {},
): InstalledArtifact {
  return {
    key: key(instanceId, kind, name),
    display_name: name,
    version,
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    ...fields,
  };
}

function update(
  artifactKey: ArtifactKey,
  current: string,
  target: string,
  channel: UpdateCandidate["channel"],
  fields: Partial<UpdateCandidate> = {},
): UpdateCandidate {
  return {
    key: artifactKey,
    current,
    target,
    channel,
    checkable: true,
    warnings: [],
    blocked: null,
    ...fields,
  };
}

/** A row Canager could not check: `target` is the installed version
 *  (`uncheckable_candidate` in crates/canager-core/src/adapters/mod.rs). */
function uncheckable(
  artifactKey: ArtifactKey,
  current: string,
  channel: UpdateCandidate["channel"],
  warning: Warning,
): UpdateCandidate {
  return {
    key: artifactKey,
    current,
    target: current,
    channel,
    checkable: false,
    warnings: [warning],
    blocked: null,
  };
}

const DAY = 86_400;
/** 2026-09-01T00:00:00Z: install dates count back from here. */
const SEPTEMBER_2026 = 1_788_220_800;
const daysAgo = (days: number) => SEPTEMBER_2026 - days * DAY;

// ---------------------------------------------------------------- Homebrew

type FormulaRow = [
  name: string,
  version: string,
  description: string,
  homepage: string | null,
  installedDaysAgo: number,
];

/** What the user asked Homebrew for. */
const REQUESTED_FORMULAE: FormulaRow[] = [
  ["ffmpeg", "9.0.1_1", "Play, record, convert, and stream select audio and video codecs", "https://ffmpeg.org/", 40],
  ["gh", "2.101.0", "GitHub command-line tool", "https://cli.github.com/", 12],
  ["git", "2.55.0", "Distributed revision control system", "https://git-scm.com", 20],
  ["htop", "3.4.1", "Improved top (interactive process viewer)", "https://htop.dev/", 150],
  ["jq", "1.8.2", "Lightweight and flexible command-line JSON processor", "https://jqlang.org/", 60],
  ["node@22", "22.23.2_2", "Open-source, cross-platform JavaScript runtime environment", "https://nodejs.org/", 9],
  ["ollama", "0.34.1", "Create, run, and share large language models (LLMs)", "https://ollama.com/", 15],
  ["pipx", "1.17.3", "Execute binaries from Python packages in isolated environments", "https://pipx.pypa.io", 80],
  ["postgresql@17", "17.9", "Object-relational database system", "https://www.postgresql.org/", 120],
  ["python@3.13", "3.13.8", "Interpreted, interactive, object-oriented programming language", "https://www.python.org/", 30],
  ["ripgrep", "15.1.0", "Search tool like grep and The Silver Searcher", "https://github.com/BurntSushi/ripgrep", 200],
  ["wget", "1.25.0", "Internet file retriever", "https://www.gnu.org/software/wget/", 210],
];

/** What Homebrew installed for them (folded behind "N components"). */
const DEPENDENCY_FORMULAE: FormulaRow[] = [
  ["ca-certificates", "2026-08-13", "Mozilla CA certificate store", "https://curl.se/docs/caextract.html", 30],
  ["gettext", "1.0", "GNU internationalization (i18n) and localization (l10n) library", "https://www.gnu.org/software/gettext/", 90],
  ["icu4c@78", "78.3", "C/C++ and Java libraries for Unicode and globalization", "https://icu.unicode.org/home", 60],
  ["libnghttp2", "1.70.0", "HTTP/2 C Library", "https://nghttp2.org/", 45],
  ["libunistring", "1.4.2", "C string library for manipulating Unicode strings", "https://www.gnu.org/software/libunistring/", 100],
  ["libuv", "1.52.1", "Multi-platform support library with a focus on asynchronous I/O", "https://libuv.org/", 70],
  ["mpdecimal", "4.0.1", "Library for decimal floating point arithmetic", "https://www.bytereef.org/mpdecimal/", 110],
  ["openssl@3", "3.6.4", "Cryptography and SSL/TLS Toolkit", "https://openssl-library.org", 25],
  ["pcre2", "10.48", "Perl compatible regular expressions library with a new API", "https://pcre2project.github.io/pcre2/", 55],
  ["readline", "8.3.6", "Library for command-line editing", "https://tiswww.case.edu/php/chet/readline/rltop.html", 35],
  ["sqlite", "3.53.4", "Command-line interface for SQLite", "https://sqlite.org/index.html", 28],
  ["x264", "r3222", "H.264/AVC encoder", "https://www.videolan.org/developers/x264.html", 160],
  ["xz", "5.8.4", "General-purpose data compression with high compression ratio", "https://tukaani.org/xz/", 50],
  ["zstd", "1.5.7_1", "Zstandard is a real-time compression algorithm", "https://facebook.github.io/zstd/", 140],
];

/** The one pinned formula: no Update, no Uninstall. */
const PINNED_FORMULA = "postgresql@17";

function formulae(): InstalledArtifact[] {
  const rows = [
    ...REQUESTED_FORMULAE.map((row) => ({ row, reason: "Requested" as const })),
    ...DEPENDENCY_FORMULAE.map((row) => ({ row, reason: "Dependency" as const })),
  ];
  // `brew info --installed` lists formulae by name, byte by byte.
  rows.sort((a, b) => (a.row[0] < b.row[0] ? -1 : 1));
  return rows.map(({ row: [name, version, description, homepage, days], reason }) =>
    artifact(IDS.brew, "Formula", name, version, {
      reason,
      description,
      homepage,
      installed_at: daysAgo(days),
      uninstall_blocked: name === PINNED_FORMULA ? "Pinned" : null,
    }),
  );
}

function casks(): InstalledArtifact[] {
  return [
    artifact(IDS.brew, "Cask", "android-platform-tools", "36.0.0", {
      display_name: "Android SDK Platform-Tools",
      description: "Android SDK component",
      homepage: "https://developer.android.com/tools/releases/platform-tools",
    }),
    // A font: no app, and no description in its cask.
    artifact(IDS.brew, "Cask", "font-jetbrains-mono", "2.304", {
      display_name: "JetBrains Mono",
      homepage: "https://www.jetbrains.com/lp/mono/",
    }),
    artifact(IDS.brew, "Cask", "iterm2", "3.6.4", {
      display_name: "iTerm2",
      description: "Terminal emulator as alternative to Apple's Terminal app",
      homepage: "https://iterm2.com/",
      path: "/Applications/iTerm.app",
    }),
    // The app that updates itself: listed as updatable only while Settings'
    // Show self-updating apps is on (brew outdated --greedy).
    artifact(IDS.brew, "Cask", "visual-studio-code", "1.116.1", {
      display_name: "Microsoft Visual Studio Code",
      description: "Open-source code editor",
      homepage: "https://code.visualstudio.com/",
      path: "/Applications/Visual Studio Code.app",
      auto_updates: true,
    }),
  ];
}

function brewUpdates(): UpdateCandidate[] {
  const formula = (name: string) => key(IDS.brew, "Formula", name);
  return [
    // Hidden by Settings: "Never remind me" (ignored_updates).
    update(formula("ffmpeg"), "9.0.1_1", "9.0.2", "Native"),
    // Hidden by Settings: this version skipped (skipped_versions).
    update(formula("gh"), "2.101.0", "2.102.0", "Native"),
    update(formula("git"), "2.55.0", "2.55.1", "Native"),
    update(formula(PINNED_FORMULA), "17.9", "17.10", "Native", { blocked: "Pinned" }),
    update(formula("wget"), "1.25.0", "1.26.0", "Native"),
    update(key(IDS.brew, "Cask", "android-platform-tools"), "36.0.0", "36.0.2", "Native"),
  ];
}

/** `brew outdated --greedy`'s extra rows: the casks that update themselves. */
function brewGreedyUpdates(): UpdateCandidate[] {
  return [update(key(IDS.brew, "Cask", "visual-studio-code"), "1.116.1", "1.117.0", "Native")];
}

// ------------------------------------------------------------ the others

/** Ollama models: the version is the local manifest digest, never shown. */
export const MODELS = {
  coder: "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M",
  llama: "llama3.2:3b",
} as const;

function everythingElse(): { artifacts: InstalledArtifact[]; updates: UpdateCandidate[] } {
  const artifacts: InstalledArtifact[] = [
    // Cargo (~/.cargo): one crate from crates.io, one from a git repository.
    artifact(IDS.cargo, "Binary", "jj-cli", "0.35.0", { path: inHome(".cargo/bin/jj") }),
    artifact(IDS.cargo, "Binary", "tokei", "12.1.2", { path: inHome(".cargo/bin/tokei") }),
    // npm (Homebrew's Node): global packages, which carry no description.
    artifact(IDS.npm, "Package", "corepack", "0.36.0"),
    artifact(IDS.npm, "Package", "npm", "12.0.2"),
    artifact(IDS.npm, "Package", "prettier", "3.8.1"),
    artifact(IDS.npm, "Package", "typescript", "6.0.2"),
    // Ollama: one model from a third-party registry (its update warns about
    // the host), one from Ollama's own library.
    artifact(IDS.ollama, "Model", MODELS.coder, "52e05d4a30959ae2542932b2c473f476dca0ce371aaf9a2227badf4e3eeec4f4", {
      size_bytes: 4_683_087_520,
    }),
    artifact(IDS.ollama, "Model", MODELS.llama, "8e4cdead7463ce276b20d4e33341950d7bb40847f70a9882567a188e24ec1f66", {
      size_bytes: 2_019_393_189,
    }),
    // pip (Homebrew's Python): read-only by design. pip reports only
    // Unknown or Dependency, never Requested.
    artifact(IDS.pip, "Package", "certifi", "2026.8.3", { reason: "Dependency" }),
    artifact(IDS.pip, "Package", "charset-normalizer", "3.4.3", { reason: "Dependency" }),
    artifact(IDS.pip, "Package", "pip", "26.2.1", { reason: "Unknown" }),
    artifact(IDS.pip, "Package", "requests", "2.32.4", { reason: "Unknown" }),
    artifact(IDS.pip, "Package", "urllib3", "2.5.0", { reason: "Dependency" }),
    // pipx.
    artifact(IDS.pipx, "Tool", "httpie", "3.2.4", { path: inHome(".local/pipx/venvs/httpie") }),
    artifact(IDS.pipx, "Tool", "poetry", "2.2.1", { path: inHome(".local/pipx/venvs/poetry") }),
    // The tools with their own installer: no description on the wire (the
    // page has a sentence per tool), the real binary as the path.
    artifact(IDS.agy, "Binary", "agy", "1.2.11", {
      display_name: "Antigravity CLI",
      homepage: "https://antigravity.google/docs/cli/install/",
      path: inHome(".local/bin/agy"),
      auto_updates: true,
    }),
    artifact(IDS.claude, "Binary", "claude", "2.1.282", {
      display_name: "Claude Code",
      homepage: "https://code.claude.com/docs/en/setup",
      path: inHome(".local/share/claude/versions/2.1.282"),
      auto_updates: true,
    }),
    artifact(IDS.grok, "Binary", "grok", "1.0.41", {
      display_name: "Grok Build",
      homepage: "https://x.ai/build",
      path: inHome(".grok/downloads/grok-1.0.41-macos-aarch64"),
    }),
    artifact(IDS.rustup, "Binary", "rustup", "1.29.1", {
      display_name: "rustup",
      homepage: "https://rust-lang.github.io/rustup/",
      path: inHome(".cargo/bin/rustup"),
    }),
    // uv: carried forward from the last refresh it answered.
    artifact(IDS.uv, "Tool", "pre-commit", "4.3.0", { path: inHome(".local/share/uv/tools/pre-commit") }),
    artifact(IDS.uv, "Tool", "ruff", "0.14.3", { path: inHome(".local/share/uv/tools/ruff") }),
  ];
  const updates: UpdateCandidate[] = [
    // Installed from a git repository: never checkable.
    uncheckable(key(IDS.cargo, "Binary", "jj-cli"), "0.35.0", "Registry", "NonRegistrySource"),
    update(key(IDS.cargo, "Binary", "tokei"), "12.1.2", "13.0.1", "Registry"),
    update(key(IDS.npm, "Package", "typescript"), "6.0.2", "6.0.3", "Native"),
    // Two digests from different hash spaces: a "newer build" marker.
    update(
      key(IDS.ollama, "Model", MODELS.coder),
      "52e05d4a30959ae2542932b2c473f476dca0ce371aaf9a2227badf4e3eeec4f4",
      "sha256:2a548b8405827e18697cc78e00b1c445de40756c6e6a1be1a4e37964a4e17342",
      "Digest",
    ),
    // pip is read-only: listed, never offered.
    update(key(IDS.pip, "Package", "requests"), "2.32.4", "2.32.5", "Native"),
    update(key(IDS.pipx, "Tool", "httpie"), "3.2.4", "3.3.0", "Native"),
    // agy installs its updates itself; Canager has nothing to run.
    update(key(IDS.agy, "Binary", "agy"), "1.2.11", "1.2.12", "Registry", { blocked: "SelfUpdatesOnly" }),
    // Updates itself too, but has `claude update`: a button and a hint.
    update(key(IDS.claude, "Binary", "claude"), "2.1.282", "2.1.290", "Registry"),
    update(key(IDS.grok, "Binary", "grok"), "1.0.41", "1.0.43", "Native"),
    // `rustup self update` cannot be cancelled once it starts.
    update(key(IDS.rustup, "Binary", "rustup"), "1.29.1", "1.30.0", "Registry"),
    // uv did not answer: the newer version is from last time, no button.
    update(key(IDS.uv, "Tool", "ruff"), "0.14.3", "0.14.5", "Native"),
  ];
  return { artifacts, updates };
}

function instances(): ManagerInstance[] {
  // Sorted by adapter id, as `refresh_round` fans them out.
  return [
    instance("brew", IDS.brew, "/opt/homebrew/bin/brew", "/opt/homebrew", "7.0.3"),
    instance("cargo", IDS.cargo, inHome(".cargo/bin/cargo"), inHome(".cargo"), "1.98.1"),
    instance("npm", IDS.npm, "/opt/homebrew/bin/npm", "/opt/homebrew", "12.0.2"),
    instance("ollama", IDS.ollama, "/opt/homebrew/bin/ollama", inHome(".ollama"), "0.34.1"),
    instance("pip", IDS.pip, "/opt/homebrew/bin/python3", "/opt/homebrew/bin", "26.2.1", {
      read_only_reason: "ByDesign",
    }),
    instance("pipx", IDS.pipx, "/opt/homebrew/bin/pipx", "/opt/homebrew/bin", "1.17.3"),
    instance("standalone-agy", IDS.agy, inHome(".local/bin/agy"), inHome(".gemini/antigravity-cli"), "1.2.11"),
    instance("standalone-claude", IDS.claude, inHome(".local/bin/claude"), inHome(".local/share/claude"), "2.1.282"),
    // ~/.grok/bin is not on this PATH: an info notice, nothing blocked.
    instance("standalone-grok", IDS.grok, inHome(".grok/bin/grok"), inHome(".grok"), "1.0.41", {
      status: { unavailable: null, notes: ["NotOnPath"] },
    }),
    instance("standalone-rustup", IDS.rustup, inHome(".cargo/bin/rustup"), inHome(".cargo"), "1.29.1"),
    // `uv --version` did not answer this time: its rows are last time's.
    instance("uv", IDS.uv, inHome(".local/bin/uv"), inHome(".local/bin"), null, {
      status: { unavailable: "NotResponding", notes: [] },
    }),
  ];
}

// ------------------------------------------------------------ the world

/**
 * Everything the next refresh would report, mutable: operations change it
 * (an upgrade bumps a version and drops its row, an uninstall drops the
 * package), and `refresh` turns it into a `Snapshot`.
 */
export interface World {
  detect: DetectOutcome;
  instances: ManagerInstance[];
  artifacts: InstalledArtifact[];
  /** What each source's update check finds. */
  updates: UpdateCandidate[];
  /** Listed only while Settings' Show self-updating apps is on. */
  greedyUpdates: UpdateCandidate[];
  errors: SourceError[];
}

function fullWorld(): World {
  const rest = everythingElse();
  return {
    detect: "Found",
    instances: instances(),
    artifacts: [...formulae(), ...casks(), ...rest.artifacts],
    updates: [...brewUpdates(), ...rest.updates],
    greedyUpdates: brewGreedyUpdates(),
    errors: [],
  };
}

function findInstance(world: World, id: string): ManagerInstance {
  const found = world.instances.find((i) => i.id === id);
  if (found === undefined) throw new Error(`mock data has no instance ${id}`);
  return found;
}

/** Every source answered and nothing about it needs saying. */
function allAnswering(world: World): void {
  const uv = findInstance(world, IDS.uv);
  uv.version = "0.12.17";
  uv.status = { unavailable: null, notes: [] };
  findInstance(world, IDS.grok).status.notes = [];
}

/** Every source notice with a look of its own (`?state=notices`). */
function withNotices(world: World): void {
  // Homebrew is still downloading its catalogue: an info notice, and every
  // Homebrew operation first waits for it (the backend's
  // `WaitingForBrewUpdate` note).
  findInstance(world, IDS.brew).status.notes = ["IndexUpdating"];
  // Node from nodejs.org: npm's prefix is not the user's to write, and
  // this npm is newer than the one Canager was verified against. It also
  // has an older npm copy of Claude Code, which is what runs when the
  // user types `claude`.
  const npm = findInstance(world, IDS.npm);
  npm.id = IDS.npmSystem;
  npm.exe_path = "/usr/local/bin/npm";
  npm.prefix = "/usr/local";
  npm.version = "12.1.0";
  npm.unverified_version = unverifiedVersion("npm", npm.version);
  npm.read_only_reason = "PrefixNotWritable";
  const rekey = (k: ArtifactKey) => (k.instance_id === IDS.npm ? { ...k, instance_id: IDS.npmSystem } : k);
  for (const a of world.artifacts) a.key = rekey(a.key);
  for (const u of world.updates) u.key = rekey(u.key);
  world.artifacts.push(artifact(IDS.npmSystem, "Package", "@anthropic-ai/claude-code", "2.1.269"));
  world.updates.push(
    update(key(IDS.npmSystem, "Package", "@anthropic-ai/claude-code"), "2.1.269", "2.1.290", "Native"),
  );
  findInstance(world, IDS.claude).status.notes = ["ShadowedByNpm"];
  // Ollama is installed but not running: the notice offers Open Ollama,
  // and the next refresh after pressing it finds it running.
  findInstance(world, IDS.ollama).status.unavailable = "NotRunning";
  // Grok Build's launcher is left without its program: no version to
  // read, nothing to update, and Uninstall finishes the job.
  const grok = findInstance(world, IDS.grok);
  grok.version = null;
  grok.unverified_version = null;
  grok.status.notes = ["LauncherOnly"];
  const grokRow = world.artifacts.find((a) => a.key.instance_id === IDS.grok);
  if (grokRow !== undefined) {
    grokRow.version = "";
    grokRow.path = null;
  }
  world.updates = world.updates.filter((u) => u.key.instance_id !== IDS.grok);
}

/** What each lookup said when nothing could be reached (`?state=offline`). */
const OFFLINE_REASONS: Record<string, string> = {
  cargo: "crates.io request failed: network error: dns error: failed to lookup address information",
  npm: "npm outdated -g: npm error code ENOTFOUND",
  ollama: "registry request failed: network error: dns error: failed to lookup address information",
  pip: "pip list --outdated: WARNING: Retrying (Retry(total=4, connect=None, read=None, redirect=None, status=None)) after connection broken by 'NewConnectionError'",
  pipx: "pipx list --outdated: error: could not reach https://pypi.org/simple/ (network is unreachable)",
  "standalone-agy":
    "request to https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json failed: network error: dns error: failed to lookup address information",
  "standalone-claude":
    "downloads.claude.ai request failed: network error: dns error: failed to lookup address information",
  "standalone-grok": "the update check reported: could not reach the update server",
  "standalone-rustup":
    "request to https://static.rust-lang.org/rustup/release-stable.toml failed: network error: dns error: failed to lookup address information",
};

/** The channel each of those sources' rows carry; the rest are `Registry`. */
const OFFLINE_CHANNELS: Record<string, UpdateCandidate["channel"]> = {
  npm: "Native",
  ollama: "Digest",
  pip: "Native",
  pipx: "Native",
};

/**
 * No registry answered (`?state=offline`): Homebrew could not download its
 * catalogue and checked against the copy it has, and every other lookup
 * became a "could not check" row -- one per installed package for the
 * sources that check everything with one command.
 */
function offline(world: World): void {
  findInstance(world, IDS.brew).status.notes = ["IndexMayBeStale"];
  const adapterOf = new Map(world.instances.map((i) => [i.id, i.adapter_id]));
  const kept = world.updates.filter((u) => {
    const adapterId = adapterOf.get(u.key.instance_id) ?? "";
    // brew checked its own list; uv's rows are last time's anyway; a git
    // crate says why it is never checkable.
    return adapterId === "brew" || adapterId === "uv" || u.warnings.includes("NonRegistrySource");
  });
  const failed = world.artifacts.flatMap((a): UpdateCandidate[] => {
    const adapterId = adapterOf.get(a.key.instance_id) ?? "";
    const reason = OFFLINE_REASONS[adapterId];
    if (reason === undefined || kept.some((u) => sameKey(u.key, a.key))) return [];
    return [uncheckable(a.key, a.version, OFFLINE_CHANNELS[adapterId] ?? "Registry", { Message: reason })];
  });
  world.updates = [...kept, ...failed];
}

/** The world `?state=` describes, fresh. */
export function buildWorld(state: ScenarioState): World {
  const world = fullWorld();
  switch (state) {
    case "full":
    case "loading":
    case "error":
    case "refresh-error":
      return world;
    case "empty":
      return {
        detect: "Missing",
        instances: [],
        artifacts: [],
        updates: [],
        greedyUpdates: [],
        errors: [],
      };
    case "nothing":
      return {
        detect: "Found",
        instances: [findInstance(world, IDS.brew)],
        artifacts: [],
        updates: [],
        greedyUpdates: [],
        errors: [],
      };
    case "uptodate":
      allAnswering(world);
      world.updates = [];
      return world;
    case "hidden":
      allAnswering(world);
      world.updates = world.updates.filter(
        (u) => u.key.instance_id === IDS.brew && (u.key.name === "ffmpeg" || u.key.name === "gh"),
      );
      return world;
    case "stale":
      world.errors = [
        { instance_id: IDS.pipx, message: "pipx list --json timed out after 60 s" },
        {
          instance_id: IDS.cargo,
          message: `could not read ${inHome(".cargo/.crates2.json")}: Permission denied (os error 13)`,
        },
      ];
      return world;
    case "notices":
      withNotices(world);
      return world;
    case "offline":
      offline(world);
      return world;
  }
}

/**
 * Settings at startup: the scenario's language and technical-details
 * switch, one package the user asked never to be reminded about and one
 * version they skipped (both Homebrew rows above, so both are hidden from
 * the Updates page and marked with a chip on the Installed page).
 */
export function initialSettings(scenario: Scenario): Settings {
  return {
    language: scenario.language,
    show_technical_details: scenario.technicalDetails,
    ignored_updates: [key(IDS.brew, "Formula", "ffmpeg")],
    skipped_versions: [{ key: key(IDS.brew, "Formula", "gh"), version: "2.102.0" }],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
}

// ---------------------------------------------------------- Unknown page

const UNKNOWN_ENTRIES: UnknownEntry[] = [
  // A script of the user's own.
  {
    path: "~/bin/sync-photos",
    kind: "File",
    resolved: inHome("bin/sync-photos"),
    link_target: null,
    size_bytes: 2_184,
    modified_at: daysAgo(64),
    owned_by_me: true,
    app_bundle: null,
  },
  // An installer run with administrator rights put this link here.
  {
    path: "/usr/local/bin/aws",
    kind: "Symlink",
    resolved: "/usr/local/aws-cli/aws",
    link_target: "/usr/local/aws-cli/aws",
    size_bytes: 11_532_288,
    modified_at: daysAgo(33),
    owned_by_me: false,
    app_bundle: null,
  },
  // An app's command-line link, made by the app with administrator rights.
  {
    path: "/usr/local/bin/docker",
    kind: "Symlink",
    resolved: "/Applications/Docker.app/Contents/Resources/bin/docker",
    link_target: "/Applications/Docker.app/Contents/Resources/bin/docker",
    size_bytes: 58_901_232,
    modified_at: daysAgo(18),
    owned_by_me: false,
    app_bundle: "Docker",
  },
  // The app was deleted; its link was not.
  {
    path: "/usr/local/bin/subl",
    kind: "BrokenSymlink",
    resolved: null,
    link_target: "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
    size_bytes: null,
    modified_at: null,
    owned_by_me: true,
    app_bundle: "Sublime Text",
  },
  // `go install` put this here.
  {
    path: "~/go/bin/golangci-lint",
    kind: "File",
    resolved: inHome("go/bin/golangci-lint"),
    link_target: null,
    size_bytes: 51_234_816,
    modified_at: daysAgo(7),
    owned_by_me: true,
    app_bundle: null,
  },
];

/** The Unknown page's scan (`?scan=`); `error` is the backend's to reject. */
export function unknownScan(scan: Exclude<ScenarioScan, "error">): UnknownScan {
  // Claude Code, agy, uv and uvx in ~/.local/bin, rustup with its 13
  // proxies, tokei and jj in ~/.cargo/bin: 20 programs a known source
  // accounts for, and not listed.
  const attributed = 20;
  if (scan === "empty") {
    return {
      scanned: [
        { path: "~/.local/bin", entries: 4 },
        { path: "~/.cargo/bin", entries: 16 },
      ],
      entries: [],
      attributed,
      stopped: null,
    };
  }
  return {
    scanned: [
      { path: "~/.local/bin", entries: 4 },
      { path: "~/bin", entries: 1 },
      { path: "/usr/local/bin", entries: 3 },
      { path: "~/.cargo/bin", entries: 16 },
      { path: "~/go/bin", entries: 1 },
    ],
    entries: UNKNOWN_ENTRIES.map((entry) => ({ ...entry })),
    attributed,
    stopped: scan === "stopped" ? { TimeLimit: { max_secs: 10 } } : null,
  };
}
