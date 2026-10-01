/**
 * The machine the browser preview pretends to be (docs/ui-preview.md):
 * every source Banager knows, with installed software and update rows in
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
  CommandFact,
  DetectOutcome,
  HomebrewFacts,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  SourceError,
  UnknownEntry,
  UnknownScan,
  UpdateCandidate,
  Warning,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";
import {
  MANY_CARGO,
  MANY_CASKS,
  MANY_DEPENDENCIES,
  MANY_FORMULAE,
  MANY_MODELS,
  MANY_NPM,
  MANY_PIPX,
  MANY_UV,
} from "./mockManyNames";
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
  /** An Intel Mac's Homebrew, carried over by Migration Assistant beside Apple silicon's. */
  brewIntel: "brew:/usr/local",
  ollama: "ollama:http://127.0.0.1:11434",
  pip: "pip:/opt/homebrew/bin/python3",
  pipx: "pipx",
  agy: "standalone-agy",
  claude: "standalone-claude",
  grok: "standalone-grok",
  rustup: "standalone-rustup",
  uv: "uv",
  /** Codex installed by its own script: listed only (`codexStandalone`). */
  codex: "standalone-codex",
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
    facts: NO_FACTS,
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

/** A row Banager could not check: `target` is the installed version
 *  (`uncheckable_candidate` in crates/banager-core/src/adapters/mod.rs). */
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
      facts: NO_FACTS,
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

/**
 * What `brew info --installed --json=v2` says beyond versions
 * (`ArtifactFacts.homebrew`), on a few of the rows above, and two rows of
 * its own: a cask Homebrew disabled for failing macOS's security check,
 * and a formula it deprecated in favour of another. Every cask also gets
 * its install time, as brew reports one for each.
 */
function withHomebrewState(artifacts: InstalledArtifact[]): InstalledArtifact[] {
  const empty: HomebrewFacts = { deprecated: null, disabled: null, caveats: null, other_versions: [] };
  const extra: Record<string, Partial<HomebrewFacts>> = {
    git: { other_versions: ["2.54.0"] },
    "openssl@3": {
      other_versions: ["3.6.3"],
      caveats:
        "A CA file has been bootstrapped using certificates from the system\nkeychain. To add additional certificates, place .pem files in\n  $HOMEBREW_PREFIX/etc/openssl@3/certs\n\nand run\n  $HOMEBREW_PREFIX/opt/openssl@3/bin/c_rehash",
    },
    readline: { other_versions: ["8.3.3"] },
    // The other kegs mockSizes.ts measures (OTHER_VERSIONS): the same kegs,
    // so the versions and their size agree.
    "node@22": { other_versions: ["22.22.0"] },
    gettext: { other_versions: ["0.26"] },
    libuv: { other_versions: ["1.51.0"] },
    "python@3.13": {
      other_versions: ["3.13.7"],
      caveats:
        "Python is installed as\n  $HOMEBREW_PREFIX/bin/python3.13\n\n`idle3.13` requires tkinter, which is available separately:\n  brew install python-tk@3.13",
    },
  };
  const caskDays: Record<string, number> = {
    "android-platform-tools": 75,
    "font-jetbrains-mono": 300,
    iterm2: 190,
    "visual-studio-code": 33,
  };
  const marked = artifacts.map((a) => {
    if (a.key.instance_id !== IDS.brew) return a;
    const more = extra[a.key.name];
    const facts = more === undefined ? a.facts : { ...a.facts, homebrew: { ...empty, ...more } };
    const days = caskDays[a.key.name];
    const installed_at = a.key.kind === "Cask" && days !== undefined ? daysAgo(days) : a.installed_at;
    return { ...a, facts, installed_at };
  });
  marked.push(
    artifact(IDS.brew, "Formula", "youtube-dl", "2021.12.17", {
      description: "Download YouTube videos from the command-line",
      homepage: "https://youtube-dl.org/",
      installed_at: daysAgo(500),
      facts: {
        ...NO_FACTS,
        // Every row Homebrew's facts can give a formula at once: its mark,
        // another version, caveats -- and, from the other helpers, a size
        // and a command.
        homebrew: {
          ...empty,
          deprecated: { date: "2025-11-01", reason: "unmaintained", replacement: "yt-dlp" },
          other_versions: ["2021.6.6"],
          caveats: "zsh completions have been installed to:\n  $HOMEBREW_PREFIX/share/zsh/site-functions",
        },
      },
    }),
    artifact(IDS.brew, "Cask", "quickjot", "2.3.1", {
      display_name: "QuickJot",
      description: "Menu bar notes",
      homepage: "https://quickjot.example/",
      installed_at: daysAgo(420),
      facts: {
        ...NO_FACTS,
        homebrew: {
          ...empty,
          disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: null },
        },
      },
    }),
  );
  return marked;
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
    // Hidden by Settings for 12 more days (snoozed_updates).
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
    // Two digests from different hash spaces: a "new version" marker.
    update(
      key(IDS.ollama, "Model", MODELS.coder),
      "52e05d4a30959ae2542932b2c473f476dca0ce371aaf9a2227badf4e3eeec4f4",
      "sha256:2a548b8405827e18697cc78e00b1c445de40756c6e6a1be1a4e37964a4e17342",
      "Digest",
    ),
    // pip is read-only: listed, never offered.
    update(key(IDS.pip, "Package", "requests"), "2.32.4", "2.32.5", "Native"),
    update(key(IDS.pipx, "Tool", "httpie"), "3.2.4", "3.3.0", "Native"),
    // agy installs its updates itself; Banager has nothing to run.
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

/**
 * AI coding tools from four sources besides the three standalone ones
 * and Homebrew's `ollama` above, so the 「AI工具」 filter shows a mix: two
 * npm packages, a Homebrew formula and a pipx tool, three with an update.
 * Their `facts.family` is not set here: the mock backend sets it from the
 * real table where it puts a snapshot together (`withFamilies`), as Rust
 * does. None of these names is in ./mockManyNames.ts, so `?state=many`
 * lists each once.
 */
function aiTools(): { artifacts: InstalledArtifact[]; updates: UpdateCandidate[] } {
  const codex = artifact(IDS.npm, "Package", "@openai/codex", "0.155.1", { installed_at: daysAgo(6) });
  const opencode = artifact(IDS.npm, "Package", "opencode-ai", "1.18.34", { installed_at: daysAgo(3) });
  const gemini = artifact(IDS.brew, "Formula", "gemini-cli", "0.60.0", {
    description: "Interact with Google Gemini AI models from the command-line",
    homepage: "https://github.com/google-gemini/gemini-cli",
    installed_at: daysAgo(18),
  });
  const aider = artifact(IDS.pipx, "Tool", "aider-chat", "0.86.1", {
    path: inHome(".local/pipx/venvs/aider-chat"),
    installed_at: daysAgo(44),
  });
  return {
    artifacts: [codex, opencode, gemini, aider],
    updates: [
      update(codex.key, "0.155.1", "0.159.3", "Native"),
      update(gemini.key, "0.60.0", "0.62.0", "Native"),
      update(aider.key, "0.86.1", "0.86.2", "Native"),
    ],
  };
}

/**
 * Codex installed by its own script beside npm's `@openai/codex`
 * (`aiTools`): listed only. Its version is the release folder
 * `~/.codex/packages/standalone/current` points at; it follows the latest
 * release, so it updates itself (`auto_updates`); Banager checks nothing
 * and removes nothing (`NoSafeMethod`), so it is on no update list. Its
 * metadata lists no verified version, so none is flagged.
 */
function codexStandalone(): { instance: ManagerInstance; artifact: InstalledArtifact } {
  return {
    instance: instance(
      "standalone-codex",
      IDS.codex,
      inHome(".local/bin/codex"),
      inHome(".codex/packages/standalone"),
      "0.159.3",
    ),
    artifact: artifact(IDS.codex, "Binary", "codex", "0.159.3", {
      display_name: "Codex",
      homepage: "https://github.com/openai/codex",
      path: inHome(".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex"),
      auto_updates: true,
      uninstall_blocked: "NoSafeMethod",
      installed_at: daysAgo(2),
    }),
  };
}

/** `list` with `added` placed by adapter id, as `refresh_round` orders instances. */
function withInstance(list: ManagerInstance[], added: ManagerInstance): ManagerInstance[] {
  const at = list.findIndex((i) => i.adapter_id > added.adapter_id);
  return at === -1 ? [...list, added] : [...list.slice(0, at), added, ...list.slice(at)];
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
  const ai = aiTools();
  const codex = codexStandalone();
  return {
    detect: "Found",
    instances: withInstance(instances(), codex.instance),
    artifacts: withHomebrewState([...formulae(), ...casks(), ...rest.artifacts, ...ai.artifacts, codex.artifact]),
    updates: [...brewUpdates(), ...rest.updates, ...ai.updates],
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
  // this npm is newer than the one Banager was verified against. It also
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
  // A second Homebrew, in /usr/local, left by Migration Assistant from an
  // Intel Mac, which did not answer and has nothing carried over from an
  // earlier check: two sources of one kind, which the sidebar tells apart
  // by where each is, and one whose page says why it lists nothing (spec
  // R8). Sorted by adapter id, after the first.
  world.instances.splice(
    world.instances.findIndex((i) => i.id === IDS.brew) + 1,
    0,
    instance("brew", IDS.brewIntel, "/usr/local/bin/brew", "/usr/local", null, {
      status: { unavailable: "NotResponding", notes: [] },
    }),
  );
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

// ------------------------------------------------------------ a long list

/**
 * A stream of numbers in [0, 1) of `name`'s own (mulberry32, seeded with
 * the name's FNV-1a hash): the same on every run, and a tool's version
 * does not move when the list around it does. Nothing in the preview is
 * random; `?state=many` only looks it.
 */
function seededStream(name: string): () => number {
  let seed = 0x811c9dc5;
  for (let i = 0; i < name.length; i += 1) {
    seed = Math.imul(seed ^ name.charCodeAt(i), 0x01000193);
  }
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/** A whole number from 0 to `max`, from `next`. */
function upTo(next: () => number, max: number): number {
  return Math.floor(next() * (max + 1));
}

/**
 * A version as its source would write it: mostly `M.m.p`, one in seven
 * `M.m`, and -- where `revisions` -- now and then Homebrew's `_1` for a
 * formula rebuilt at the same version. Low majors are the most common, as
 * they are on a real Mac; `major` fixes it (`temurin@21`).
 */
function plausibleVersion(next: () => number, revisions: boolean, major?: number): string {
  const roll = next();
  const M = major ?? (roll < 0.6 ? upTo(next, 3) : roll < 0.9 ? 4 + upTo(next, 5) : 10 + upTo(next, 20));
  // Never 0.0: a version is at least 0.1.
  const m = upTo(next, 24) || (M === 0 ? 1 : 0);
  const p = upTo(next, 12);
  const shape = next();
  if (shape < 0.14) return `${M}.${m}`;
  if (revisions && shape < 0.23) return `${M}.${m}.${p}_1`;
  return `${M}.${m}.${p}`;
}

/**
 * The version a source offers after `version`: most often a later patch,
 * a quarter of the time the next minor, now and then the next major, and
 * without the Homebrew revision (`_1`) the installed one may carry.
 */
function laterVersion(version: string, next: () => number): string {
  const parts = version.split("_")[0].split(".").map(Number);
  const roll = next();
  const at = roll < 0.05 ? 0 : roll < 0.3 || parts.length < 3 ? 1 : 2;
  const step = at === 2 ? 1 + upTo(next, 2) : 1;
  return parts.map((part, i) => (i < at ? part : i === at ? part + step : 0)).join(".");
}

/** A 64-hex digest, as an Ollama manifest's. */
function digestFrom(next: () => number): string {
  let hex = "";
  while (hex.length < 64) hex += upTo(next, 0xffff_ffff).toString(16).padStart(8, "0");
  return hex;
}

/** About one tool in seven has an update to offer (`?state=many`). */
const MANY_UPDATE_SHARE = 0.15;

/**
 * `?state=many`: a Mac with about 800 things installed, as real ones with
 * Homebrew have -- the pretend Mac, and another 741 tools from
 * ./mockManyNames.ts over its sources, mostly Homebrew's. Each has a
 * version of its own and was installed some day in the last two and a
 * half years, about one in seven has an update, and 40 of the formulae
 * are libraries Homebrew installed for the others; which versions, which
 * days and which tools have an update come from each tool's own
 * `seededStream`. No description: npm, pipx, uv and Cargo give none, and
 * the preview has no Homebrew catalogue to take one from, so a row reads
 * its line from the app's tables where they have one -- every row in
 * Chinese, the npm, PyPI and Cargo ones in English -- and otherwise what
 * its source says it is ("Homebrew package").
 */
function addMany(world: World): void {
  const installedDay = (next: () => number) => daysAgo(1 + upTo(next, 900));
  // One in seven gets an update, in `updates` -- Homebrew's check -- or,
  // for an app that updates itself, in `greedyUpdates`, which only
  // Settings' Show self-updating apps lists.
  const offerUpdate = (
    row: InstalledArtifact,
    next: () => number,
    channel: UpdateCandidate["channel"],
    into: UpdateCandidate[] = world.updates,
  ): InstalledArtifact => {
    if (next() < MANY_UPDATE_SHARE) {
      const target = channel === "Digest" ? `sha256:${digestFrom(next)}` : laterVersion(row.version, next);
      into.push(update(row.key, row.version, target, channel));
    }
    return row;
  };
  const formula = (name: string, reason: InstalledArtifact["reason"]) => {
    const next = seededStream(`brew:${name}`);
    const version = plausibleVersion(next, true);
    const row = artifact(IDS.brew, "Formula", name, version, { reason, installed_at: installedDay(next) });
    return offerUpdate(row, next, "Native");
  };
  const cask = ({ token, name, app, autoUpdates }: (typeof MANY_CASKS)[number]) => {
    const next = seededStream(`cask:${token}`);
    const major = /@(\d+)$/.exec(token)?.[1];
    const version = plausibleVersion(next, false, major === undefined ? undefined : Number(major));
    const row = artifact(IDS.brew, "Cask", token, version, {
      display_name: name,
      installed_at: installedDay(next),
      path: app === undefined ? null : `/Applications/${app}`,
      auto_updates: autoUpdates === true,
    });
    return offerUpdate(row, next, "Native", autoUpdates === true ? world.greedyUpdates : world.updates);
  };
  const tool = (instanceId: string, kind: ArtifactKind, name: string, path: string | null) => {
    const next = seededStream(`${instanceId}|${name}`);
    const version = plausibleVersion(next, false);
    const row = artifact(instanceId, kind, name, version, { installed_at: installedDay(next), path });
    return offerUpdate(row, next, instanceId === IDS.cargo ? "Registry" : "Native");
  };
  const model = ({ name, sizeBytes }: (typeof MANY_MODELS)[number]) => {
    const next = seededStream(`ollama|${name}`);
    const row = artifact(IDS.ollama, "Model", name, digestFrom(next), { size_bytes: sizeBytes });
    return offerUpdate(row, next, "Digest");
  };

  world.artifacts.push(
    ...MANY_FORMULAE.map((name) => formula(name, "Requested")),
    ...MANY_DEPENDENCIES.map((name) => formula(name, "Dependency")),
    ...MANY_CASKS.map(cask),
    ...MANY_NPM.map((name) => tool(IDS.npm, "Package", name, null)),
    ...MANY_PIPX.map((name) => tool(IDS.pipx, "Tool", name, inHome(`.local/pipx/venvs/${name}`))),
    ...MANY_UV.map((name) => tool(IDS.uv, "Tool", name, inHome(`.local/share/uv/tools/${name}`))),
    ...MANY_CARGO.map((name) => tool(IDS.cargo, "Binary", name, inHome(`.cargo/bin/${name}`))),
    ...MANY_MODELS.map(model),
  );
  // Claude Code from npm as well, beside the native install, as many Macs
  // have it: its uninstall preview names the `~/.claude` and
  // `~/.claude.json` it leaves behind (./mockKeptData.ts).
  world.artifacts.push(tool(IDS.npm, "Package", NPM_CLAUDE, null));
}

/**
 * The world `?state=` describes, fresh, with what its tools' commands run
 * (`addCommands`), worked out from that state's sources as the backend's
 * `commands::judge` works it out from the disk.
 */
export function buildWorld(state: ScenarioState): World {
  const world = scenarioWorld(state);
  onlyHomebrewDates(world);
  addCommands(world);
  addCodexCommands(world);
  return world;
}

/**
 * Only Homebrew says when a tool was installed (`brew/parse.rs`); npm,
 * pipx, uv, Cargo, pip, Ollama and the standalone installers say nothing,
 * so their rows have no `installed_at`, whatever day a helper above gave
 * them -- 「按安装日期」 then lists Homebrew's first and the rest by name
 * with 「—」, as the app does.
 */
function onlyHomebrewDates(world: World): void {
  const homebrew = new Set(world.instances.filter((i) => i.adapter_id === "brew").map((i) => i.id));
  world.artifacts = world.artifacts.map((a) =>
    homebrew.has(a.key.instance_id) || a.installed_at === null ? a : { ...a, installed_at: null },
  );
}

/** The world `?state=` describes, before its commands. */
function scenarioWorld(state: ScenarioState): World {
  const world = fullWorld();
  switch (state) {
    case "full":
    case "loading":
    case "error":
    case "refresh-error":
    case "preview":
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
      // Without Codex's own install, whose updates Banager never checks:
      // with it, no update listed is no news, and the pages say so
      // (`everySourceChecked`) instead of "Everything is up to date".
      world.instances = world.instances.filter((i) => i.id !== IDS.codex);
      world.artifacts = world.artifacts.filter((a) => a.key.instance_id !== IDS.codex);
      return world;
    case "hidden":
      allAnswering(world);
      world.updates = world.updates.filter(
        (u) =>
          u.key.instance_id === IDS.brew && (u.key.name === "ffmpeg" || u.key.name === "gh" || u.key.name === "wget"),
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
    case "many":
      allAnswering(world);
      addMany(world);
      return world;
  }
}

/**
 * Settings at startup: the scenario's language and technical-details
 * switch, one package the user asked never to be reminded about, one
 * version they skipped and one package they put off for 30 days (all
 * Homebrew rows above, so all are hidden from the Updates page and marked
 * with a chip on the Installed page).
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
    auto_check_every: "Day",
    notify_operations: false,
    // Put off with 「30天内不提醒」 18 days ago: hidden for 12 more days
    // (another Homebrew row above), listed in Settings with its date.
    snoozed_updates: [
      { key: key(IDS.brew, "Formula", "wget"), until: Math.floor(Date.now() / 1000) + 12 * 24 * 60 * 60 },
    ],
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

// ------------------------------------------- which copy a command runs

/** Commands that run the copy they belong to. */
function runs(names: string[]): CommandFact[] {
  return names.map((name) => ({ name, state: "Runs" }));
}

/** Commands Banager names and says nothing about: a dependency's. */
function unjudged(names: string[]): CommandFact[] {
  return names.map((name) => ({ name, state: null }));
}

/**
 * The commands each of the pretend Mac's tools puts on it, by its source's
 * adapter id, kind and name -- so npm's rows keep theirs when `?state=notices`
 * moves them to the npm in /usr/local -- and what typing each runs where
 * nothing else is on the same name.
 */
const COMMANDS: Record<string, CommandFact[]> = {
  "brew|Formula|ffmpeg": runs(["ffmpeg", "ffplay", "ffprobe"]),
  "brew|Formula|gh": runs(["gh"]),
  // More than three on one line: 「git、git-cvsserver、git-receive-pack等7个」.
  "brew|Formula|git": runs([
    "git",
    "git-cvsserver",
    "git-receive-pack",
    "git-shell",
    "git-upload-archive",
    "git-upload-pack",
    "scalar",
  ]),
  "brew|Formula|htop": runs(["htop"]),
  "brew|Formula|jq": runs(["jq"]),
  // Keg-only, linked by hand (`brew link --force`): judged as any
  // formula's, never said to be "not found". Its `npm` and `npx` links are
  // npm's own now (`npm install -g npm`).
  "brew|Formula|node@22": runs(["node"]),
  "brew|Formula|ollama": runs(["ollama"]),
  "brew|Formula|pipx": runs(["pipx"]),
  "brew|Formula|postgresql@17": runs(["pg_dump", "pg_restore", "postgres", "psql"]),
  "brew|Formula|python@3.13": runs(["idle3.13", "pip3.13", "pydoc3.13", "python3.13"]),
  "brew|Formula|ripgrep": runs(["rg"]),
  "brew|Formula|wget": runs(["wget"]),
  "brew|Formula|youtube-dl": runs(["youtube-dl"]),
  // Dependencies: nobody asked for them by name.
  "brew|Formula|openssl@3": unjudged(["openssl"]),
  "brew|Formula|sqlite": unjudged(["sqlite3"]),
  "brew|Formula|xz": unjudged(["unxz", "xz", "xzcat"]),
  "brew|Cask|android-platform-tools": runs(["adb", "fastboot"]),
  "brew|Cask|visual-studio-code": runs(["code"]),
  "cargo|Binary|jj-cli": runs(["jj"]),
  "cargo|Binary|tokei": runs(["tokei"]),
  "npm|Package|corepack": runs(["corepack"]),
  "npm|Package|npm": runs(["npm", "npx"]),
  "npm|Package|prettier": runs(["prettier"]),
  "npm|Package|typescript": runs(["tsc", "tsserver"]),
  "pipx|Tool|httpie": runs(["http", "httpie", "https"]),
  "pipx|Tool|poetry": runs(["poetry"]),
  "uv|Tool|pre-commit": runs(["pre-commit"]),
  "uv|Tool|ruff": runs(["ruff"]),
  "standalone-agy|Binary|agy": runs(["agy"]),
  "standalone-claude|Binary|claude": runs(["claude"]),
  // Fourteen with one verdict: one line, 「cargo、cargo-clippy、cargo-fmt等14个」.
  "standalone-rustup|Binary|rustup": runs([
    "cargo",
    "cargo-clippy",
    "cargo-fmt",
    "cargo-miri",
    "clippy-driver",
    "rls",
    "rust-analyzer",
    "rust-gdb",
    "rust-gdbgui",
    "rust-lldb",
    "rustc",
    "rustdoc",
    "rustfmt",
    "rustup",
  ]),
};

/** npm's copy of Claude Code, which `?state=notices` adds. */
const NPM_CLAUDE = "@anthropic-ai/claude-code";

/**
 * `ArtifactFacts.commands` for the state's rows, as the backend would judge
 * them on this Mac, consistent with what each source's notice says:
 *
 * - Grok Build's `grok` and `agent` are not found while `~/.grok/bin` is off
 *   the search path (its `NotOnPath` note, `?state=full`), and named not at
 *   all while its program is gone (`LauncherOnly`, `?state=notices`).
 * - With npm's copy of Claude Code beside the native one (`?state=notices`),
 *   both are the `claude-code` tool -- 「装了两份」 on both rows -- and
 *   typing `claude` runs npm's, as the native one's `ShadowedByNpm` note says.
 *
 * Each row gets facts of its own: `NO_FACTS` is shared, and never changed.
 */
function addCommands(world: World): void {
  const adapterOf = new Map(world.instances.map((instance) => [instance.id, instance.adapter_id]));
  const grok = world.instances.find((instance) => instance.id === IDS.grok);
  const npmClaude = world.artifacts.find((a) => a.key.kind === "Package" && a.key.name === NPM_CLAUDE);
  for (const artifact of world.artifacts) {
    const adapterId = adapterOf.get(artifact.key.instance_id) ?? "";
    let commands = COMMANDS[`${adapterId}|${artifact.key.kind}|${artifact.key.name}`];
    let family = artifact.facts.family;
    if (artifact.key.instance_id === IDS.grok) {
      const notes = grok?.status.notes ?? [];
      const names = ["agent", "grok"];
      commands = notes.includes("LauncherOnly")
        ? []
        : notes.includes("NotOnPath")
          ? names.map((name) => ({ name, state: { NotOnPath: { dir: "~/.grok/bin" } } }))
          : runs(names);
    }
    if (npmClaude !== undefined && artifact === npmClaude) {
      family = "claude-code";
      commands = runs(["claude"]);
    }
    if (npmClaude !== undefined && artifact.key.instance_id === IDS.claude) {
      family = "claude-code";
      commands = [{ name: "claude", state: { ShadowedBy: { by: npmClaude.key } } }];
    }
    if (commands !== undefined) artifact.facts = { ...artifact.facts, family, commands };
  }
}

/**
 * `codex` from the two copies of Codex: Codex's own install puts it in
 * `~/.local/bin`, which comes first on this Mac's search path, so typing
 * `codex` runs that one and npm's `@openai/codex` waits behind it -- 「装了两
 * 份」 on both rows, as `commands::judge` would say. Either copy alone:
 * nothing to add.
 */
function addCodexCommands(world: World): void {
  const own = world.artifacts.find((a) => a.key.instance_id === IDS.codex);
  const npm = world.artifacts.find((a) => a.key.kind === "Package" && a.key.name === "@openai/codex");
  if (own === undefined || npm === undefined) return;
  own.facts = { ...own.facts, commands: runs(["codex"]) };
  npm.facts = { ...npm.facts, commands: [{ name: "codex", state: { ShadowedBy: { by: own.key } } }] };
}
