/**
 * What the browser preview's backend plans and prints for one operation:
 * the argv each adapter builds (crates/banager-core/src/adapters), the
 * warnings its preview carries, the lines its tool writes while it runs,
 * and how it ends under `?outcome=`. Dev-only, like everything in src/dev.
 */
import type {
  InstalledArtifact,
  LogNote,
  ManagerInstance,
  OpRequest,
  Outcome,
  Plan,
  PlanAction,
  RemovedWhat,
  Stream,
  UninstallScope,
  UpdateCandidate,
  Warning,
} from "../lib/types";
import { IDS, inHome, mockHomeAsTilde, type World } from "./mockData";
import type { ScenarioOutcome } from "./scenario";
import { failureCause } from "../lib/failureCause";

/**
 * A refusal, as the string the real IPC rejects with: always a small
 * `{"kind": ...}` object (src-tauri/src/ipc.rs), which
 * `planErrorMessage` in src/lib/sources.ts words for the user.
 */
export function refusal(payload: Record<string, unknown>): string {
  return JSON.stringify(payload);
}

const BREW_ENV: [string, string][] = [
  ["HOMEBREW_NO_AUTO_UPDATE", "1"],
  ["HOMEBREW_NO_AUTOREMOVE", "1"],
  ["HOMEBREW_NO_ENV_HINTS", "1"],
  ["HOMEBREW_NO_INSTALL_CLEANUP", "1"],
  ["NO_COLOR", "1"],
];
const NPM_ENV: [string, string][] = [
  ["NO_COLOR", "1"],
  ["npm_config_update_notifier", "false"],
  ["npm_config_fund", "false"],
];
const CARGO_ENV: [string, string][] = [["RUSTUP_AUTO_INSTALL", "0"]];

/** `brew uses --installed <name>` on the preview's Homebrew. */
const BREW_DEPENDENTS: Record<string, string[]> = {
  "ca-certificates": ["openssl@3"],
  gettext: ["git", "wget"],
  "icu4c@78": ["node@22", "postgresql@17"],
  libnghttp2: ["node@22"],
  libunistring: ["gettext", "wget"],
  libuv: ["node@22"],
  mpdecimal: ["python@3.13"],
  "openssl@3": ["node@22", "postgresql@17", "python@3.13", "wget"],
  pcre2: ["git"],
  "python@3.13": ["pipx"],
  readline: ["postgresql@17", "python@3.13"],
  sqlite: ["python@3.13"],
  x264: ["ffmpeg"],
  xz: ["ffmpeg", "python@3.13"],
  zstd: ["ffmpeg"],
};

/** A formula whose `brew uses` did not finish: the preview says it could
 *  not check, rather than claiming nothing depends on it. */
const BREW_DEPENDENTS_UNKNOWN = new Set(["htop"]);

/**
 * What the preview's casks recorded at install beyond what Homebrew put
 * down (`uninstall_artifacts` in each one's INSTALL_RECEIPT.json, as the
 * catalogue for Homebrew 7.0.6 defines them): `visual-studio-code`'s
 * `launchctl` and `quit`. The others record only apps, links, fonts and a
 * `quit`, so their uninstall is plain (`cask_receipt::classify`). The app
 * it quits is the one it put in /Applications, so the preview names it
 * (`BrewAdapter::quit_app_names`).
 */
const CASK_STEPS: Record<string, Warning[]> = {
  "visual-studio-code": [
    { CaskUninstallStep: { step: "RemovesServices", items: ["com.microsoft.VSCode.ShipIt"] } },
    { CaskUninstallStep: { step: "QuitsNamedApps", items: ["Visual Studio Code"] } },
  ],
};

/**
 * A cask whose sentence is not the one its steps give, and the steps it
 * says: QuickJot as a cask from a tap Homebrew does not trust, saved as
 * Ruby, so its uninstall steps run only if Homebrew trusts the tap
 * (`HomebrewCaskStepsIfTrusted`).
 */
const CASK_SCOPES: Record<string, { what: UninstallScope; steps: Warning[] }> = {
  quickjot: {
    what: "HomebrewCaskStepsIfTrusted",
    steps: [{ CaskUninstallStep: { step: "RunsOwnSteps", items: [] } }],
  },
};

/** The sentence an uninstall says under the tool (`Warning::UninstallScope`). */
function scope(what: UninstallScope): Warning {
  return { UninstallScope: { what } };
}

/** Ollama's own registries (`DEFAULT_REGISTRIES` in the ollama adapter). */
const OLLAMA_REGISTRIES = ["registry.ollama.ai", "hf.co"];

function command(program: string, args: string[], env: [string, string][] = []): PlanAction {
  return { Command: { program, args, env } };
}

function trash(paths: string[]): PlanAction {
  return { TrashPaths: { paths: paths.map(inHome) } };
}

/** `WillTrash` for each `~`-relative path, in the order it moves. */
function willTrash(...items: [path: string, what: RemovedWhat][]): Warning[] {
  return items.map(([path, what]) => ({ WillTrash: { path: `~/${path}`, what } }));
}

/** The argv every standalone tool's plan runs: its launcher. */
function standalonePlan(plan: Plan, inst: ManagerInstance, world: World): Plan {
  const { kind } = plan.request;
  switch (inst.adapter_id) {
    case "standalone-claude":
      if (kind === "Upgrade") {
        return { ...plan, action: command(inst.exe_path, ["update"]), timeout_secs: 1800 };
      }
      return {
        ...plan,
        action: trash([".local/share/claude", ".claude/downloads", ".local/bin/claude"]),
        warnings: [
          ...willTrash(
            [".local/share/claude", "Program"],
            [".claude/downloads", "Cache"],
            [".local/bin/claude", "Launcher"],
          ),
          { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
          { WillKeep: { path: "~/.claude.json", what: "Settings" } },
        ],
        timeout_secs: 120,
      };
    case "standalone-grok": {
      if (kind === "Upgrade") {
        return { ...plan, action: command(inst.exe_path, ["update"]), timeout_secs: 1800 };
      }
      const kept: Warning[] = [
        { WillKeep: { path: "~/.grok", what: "SettingsAndHistory" } },
        { WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } },
      ];
      if (inst.status.notes.includes("LauncherOnly")) {
        return {
          ...plan,
          action: trash([".grok/bin/grok"]),
          warnings: [
            { AlreadyGone: { path: "~/.grok/downloads" } },
            ...willTrash([".grok/bin/grok", "Launcher"]),
            ...kept,
          ],
          timeout_secs: 120,
        };
      }
      return {
        ...plan,
        action: trash([".grok/downloads", ".grok/completions", ".grok/bin/agent", ".grok/bin/grok"]),
        warnings: [
          ...willTrash(
            [".grok/downloads", "Program"],
            [".grok/completions", "Program"],
            [".grok/bin/agent", "Launcher"],
            [".grok/bin/grok", "Launcher"],
          ),
          ...kept,
        ],
        timeout_secs: 120,
      };
    }
    case "standalone-agy":
      // No update command Banager may run: the gate refuses first, from
      // the row's `blocked`; this is its late twin (`StandaloneAdapter::plan`).
      if (kind === "Upgrade") throw refusal({ kind: "update_blocked", reason: "SelfUpdatesOnly" });
      return {
        ...plan,
        action: trash([".local/bin/agy.1789563015.old", ".local/bin/agy"]),
        warnings: [
          ...willTrash([".local/bin/agy.1789563015.old", "Backups"], [".local/bin/agy", "Launcher"]),
          { WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } },
          { WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } },
          { WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } },
        ],
        timeout_secs: 120,
      };
    case "standalone-codex":
      // Nothing Banager may run for an update (`StandaloneAdapter::plan`'s
      // late twin of the gate); its uninstall is `recipes::CODEX`'s list.
      if (kind === "Upgrade") throw refusal({ kind: "update_blocked", reason: "SelfUpdatesOnly" });
      return {
        ...plan,
        action: trash([".local/bin/codex-code-mode-host", ".codex/packages/standalone", ".local/bin/codex"]),
        warnings: [
          ...willTrash(
            [".local/bin/codex-code-mode-host", "Program"],
            [".codex/packages/standalone", "Program"],
            [".local/bin/codex", "Launcher"],
          ),
          { WillKeep: { path: "~/.codex", what: "SettingsAndHistory" } },
          { WillKeep: { path: "~/.zprofile", what: "ShellConfigLines" } },
        ],
        timeout_secs: 120,
      };
    case "standalone-rustup": {
      // Both of rustup's own commands hold the cargo instance's lock too,
      // and neither can be stopped once it starts.
      const rustup: Plan = {
        ...plan,
        locks: [inst.id, IDS.cargo],
        cancel_policy: "NoCancel",
        timeout_secs: 600,
      };
      if (kind === "Upgrade") {
        return { ...rustup, action: command(inst.exe_path, ["self", "update"]) };
      }
      // By the name the Installed page gives each cargo row -- the crate's,
      // as `bin_programs_rustup_removes` names a recorded program.
      const cargoBins = world.artifacts
        .filter((a) => a.key.instance_id === IDS.cargo && a.path !== null)
        .map((a) => a.display_name);
      const warnings: Warning[] = [
        {
          RemovesToolchains: {
            path: "~/.rustup",
            names: ["nightly-aarch64-apple-darwin", "stable-aarch64-apple-darwin"],
          },
        },
        { DeletesCargoHome: { path: "~/.cargo" } },
        ...(cargoBins.length > 0 ? [{ RemovesCargoInstalled: { names: cargoBins } }] : []),
        "EditsShellConfig",
        { LeavesShellConfigLine: { path: "~/.zprofile", certain: false } },
        // A `~/.zshrc` kept in iCloud Drive (Mackup): not read, so said as
        // one whose Cargo line isn't known (z1's review).
        { ShellConfigUnread: { path: "~/.zshrc" } },
      ];
      return {
        ...rustup,
        action: command(inst.exe_path, ["self", "uninstall", "-y"]),
        warnings,
      };
    }
    default:
      throw refusal({ kind: "refused" });
  }
}

/**
 * The plan the real adapter would build for `request` on `inst`, or a
 * thrown refusal. Called after the actionability gate, as
 * `Session::issue_plan` calls `Adapter::plan`.
 */
export function buildPlan(world: World, inst: ManagerInstance, request: OpRequest): Plan {
  // Installing is phase 5: no page offers it, and `plan_operation` refuses
  // it before the gate (mockBackend.ts), as the real IPC does.
  if (request.kind === "Install") throw refusal({ kind: "refused" });
  const { kind, name } = request;
  const upgrade = kind === "Upgrade";
  const plan: Plan = {
    request,
    action: command(inst.exe_path, []),
    needs_password: false,
    locks: [inst.id],
    cancel_policy: "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 600,
  };
  switch (inst.adapter_id) {
    case "brew": {
      const cask = request.artifact_kind === "Cask";
      const flag = cask ? "--cask" : "--formula";
      if (upgrade) {
        return {
          ...plan,
          action: command(inst.exe_path, ["upgrade", flag, name], BREW_ENV),
          // Every cask may ask for the Mac's password (`needs_password`).
          needs_password: cask,
          timeout_secs: 1800,
        };
      }
      // `brew uses` reads the catalogue `brew update` is rewriting.
      if (inst.status.notes.includes("IndexUpdating")) throw refusal({ kind: "index_updating" });
      const installed = (formula: string) =>
        world.artifacts.some((a) => a.key.instance_id === inst.id && a.key.name === formula);
      const unknown = BREW_DEPENDENTS_UNKNOWN.has(name);
      const affected = unknown ? [] : (BREW_DEPENDENTS[name] ?? []).filter(installed);
      const special = cask ? CASK_SCOPES[name] : undefined;
      const steps = cask ? (special?.steps ?? CASK_STEPS[name] ?? []) : [];
      const what: UninstallScope = !cask
        ? "HomebrewFormulaOnly"
        : special !== undefined
          ? special.what
          : steps.length > 0
            ? "HomebrewCaskSteps"
            : "HomebrewCaskPlain";
      return {
        ...plan,
        action: command(inst.exe_path, ["uninstall", flag, name], BREW_ENV),
        needs_password: cask,
        warnings: [
          scope(what),
          ...(unknown
            ? (["DependentsUnknown"] as Warning[])
            : affected.length > 0
              ? [{ WouldBreak: { names: affected } }]
              : []),
          ...steps,
        ],
        affected,
        timeout_secs: 1800,
      };
    }
    case "npm": {
      // Said only by an npm of 7 or later (`uninstall_scope` in the npm adapter).
      const major = Number.parseInt((inst.version ?? "").split(".")[0] ?? "", 10);
      return {
        ...plan,
        action: command(
          inst.exe_path,
          upgrade ? ["install", "-g", `${name}@latest`] : ["uninstall", "-g", name],
          NPM_ENV,
        ),
        warnings: !upgrade && major >= 7 ? [scope("Npm")] : [],
      };
    }
    case "pipx":
      return {
        ...plan,
        action: command(inst.exe_path, [upgrade ? "upgrade" : "uninstall", name]),
        warnings: upgrade ? [] : [scope("Pipx")],
      };
    case "uv":
      return {
        ...plan,
        action: command(inst.exe_path, ["tool", upgrade ? "upgrade" : "uninstall", name]),
        warnings: upgrade ? [] : [scope("Uv")],
      };
    case "cargo":
      // No cargo-binstall on this Mac: `cargo install` builds from source.
      return upgrade
        ? {
            ...plan,
            action: command(inst.exe_path, ["install", "--force", name], CARGO_ENV),
            warnings: ["CompilesLocally"],
            timeout_secs: 1800,
          }
        : {
            ...plan,
            action: command(inst.exe_path, ["uninstall", name], CARGO_ENV),
            warnings: [scope("Cargo")],
            timeout_secs: 300,
          };
    case "ollama": {
      const host = name.includes("/") ? name.slice(0, name.indexOf("/")) : "";
      const thirdParty = host.includes(".") && !OLLAMA_REGISTRIES.includes(host);
      return {
        ...plan,
        action: command(inst.exe_path, [upgrade ? "pull" : "rm", name]),
        // The registry is said on the download only, never on an uninstall;
        // then that the upgrade downloads what changed.
        warnings: upgrade
          ? [...(thirdParty ? [{ ThirdPartyRegistry: { host } }] : []), "DownloadsModelChanges"]
          : [scope("Ollama")],
        timeout_secs: 3600,
      };
    }
    default:
      return standalonePlan(plan, inst, world);
  }
}

/** One thing an operation's log shows: a tool's line, or a note of Banager's. */
export type LogLine = { stream: Stream; line: string } | { note: LogNote };

const out = (line: string): LogLine => ({ stream: "Stdout", line });
const err = (line: string): LogLine => ({ stream: "Stderr", line });

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** "4.7 GB", for Ollama's progress lines. */
function gigabytes(bytes: number | null): string {
  return bytes === null ? "" : `${(bytes / 1e9).toFixed(1)} GB`;
}

/** Where macOS puts each moved item: `~/.Trash/<name>`, with the time
 *  appended to a name already used, as Finder does. */
function trashNotes(paths: string[]): LogNote[] {
  const used = new Set<string>();
  return paths.map((path) => {
    const base = fileName(path);
    const name = used.has(base) ? `${base} 10.24.31` : base;
    used.add(base);
    return { MovedToTrash: { path: mockHomeAsTilde(path), trashed_to: `~/.Trash/${name}` } };
  });
}

/** What the operation's package is, for its log lines. */
export interface Subject {
  inst: ManagerInstance;
  artifact: InstalledArtifact | undefined;
  candidate: UpdateCandidate | undefined;
}

/** The lines a successful run of `plan` writes, in order. */
function successLog(plan: Plan, { inst, artifact, candidate }: Subject): LogLine[] {
  const { kind, name } = plan.request;
  const from = candidate?.current ?? artifact?.version ?? "";
  const to = candidate?.target ?? from;
  const upgrade = kind === "Upgrade";
  if ("TrashPaths" in plan.action) {
    return trashNotes(plan.action.TrashPaths.paths).map((note) => ({ note }));
  }
  switch (inst.adapter_id) {
    case "brew":
      if (plan.request.artifact_kind === "Cask") {
        return upgrade
          ? [
              out("==> Upgrading 1 outdated package:"),
              out(`${name} ${from} -> ${to}`),
              out(`==> Upgrading ${name}`),
              out(`==> Downloading ${name} ${to}`),
              out(`==> Uninstalling Cask ${name}`),
              out(`==> Installing Cask ${name}`),
              out(`🍺  ${name} was successfully upgraded!`),
            ]
          : [
              out(`==> Uninstalling Cask ${name}`),
              ...(artifact?.path ? [out(`==> Removing App '${artifact.path}'`)] : []),
              out(`==> Purging files for version ${from} of Cask ${name}`),
            ];
      }
      return upgrade
        ? [
            out("==> Upgrading 1 outdated package:"),
            out(`${name} ${from} -> ${to}`),
            out(`==> Fetching downloads for: ${name}`),
            out(`==> Upgrading ${name}`),
            out(`  ${from} -> ${to}`),
            out(`==> Pouring ${name}--${to}.arm64_tahoe.bottle.tar.gz`),
            out(`🍺  /opt/homebrew/Cellar/${name}/${to}: 1,742 files, 57.3MB`),
          ]
        : [out(`Uninstalling /opt/homebrew/Cellar/${name}/${from}... (117 files, 4.2MB)`)];
    case "npm":
      return upgrade
        ? [out(""), out("changed 1 package in 3s")]
        : [out(""), out("removed 1 package in 412ms")];
    case "pipx":
      return upgrade
        ? [out(`upgraded package ${name} from ${from} to ${to} (location: ${artifact?.path ?? ""})`)]
        : [out(`uninstalled ${name}! ✨ 🌟 ✨`)];
    case "uv":
      return upgrade
        ? [err(`Updated ${name} v${from} -> v${to}`), err(` - ${name}==${from}`), err(` + ${name}==${to}`)]
        : [err(`Uninstalled 1 executable: ${name}`)];
    case "cargo": {
      // cargo reports its progress on stderr.
      const bin = fileName(artifact?.path ?? name);
      return upgrade
        ? [
            err("    Updating crates.io index"),
            err(`  Downloaded ${name} v${to}`),
            err(`  Installing ${name} v${to}`),
            err(`   Compiling ${name} v${to}`),
            err("    Finished `release` profile [optimized] target(s) in 1m 04s"),
            err(`   Replacing ${inHome(`.cargo/bin/${bin}`)}`),
            err(`    Replaced package \`${name} v${from}\` with \`${name} v${to}\` (executable \`${bin}\`)`),
          ]
        : [err(`    Removing ${inHome(`.cargo/bin/${bin}`)}`)];
    }
    case "ollama":
      return upgrade
        ? [
            err("pulling manifest"),
            err(`pulling 6a0746a1ec1a: 100% ▕████████████████████▏ ${gigabytes(artifact?.size_bytes ?? null)}`),
            err("pulling 4fa551d4f938: 100% ▕████████████████████▏  12 KB"),
            err("verifying sha256 digest"),
            err("writing manifest"),
            err("success"),
          ]
        : [out(`deleted '${name}'`)];
    case "standalone-claude":
      return [
        out(`Current version: ${from}`),
        out("Checking for updates to latest version..."),
        out(`New version available: ${to} (current: ${from})`),
        out("Installing update..."),
        out(`Successfully updated from ${from} to version ${to}`),
      ];
    case "standalone-grok":
      return [
        out("Checking for updates..."),
        out(`Downloading grok ${to} (macos-aarch64)...`),
        out(`Installed grok ${to}`),
      ];
    case "standalone-rustup":
      return upgrade
        ? [
            err("info: checking for self-update"),
            err("info: downloading self-update (11.4 MiB)"),
            out(`  rustup updated - ${to} (from ${from})`),
          ]
        : [
            out("Thanks for hacking in Rust!"),
            err("info: removing rustup home"),
            err("info: removing cargo home"),
            err("info: removing rustup binaries"),
            err("info: rustup is uninstalled"),
          ];
    default:
      return [];
  }
}

/** The last line a failed run of `plan` writes on stderr: its summary. */
function failureLine(plan: Plan, { inst, candidate }: Subject): string {
  const { kind, name } = plan.request;
  const upgrade = kind === "Upgrade";
  switch (inst.adapter_id) {
    case "brew":
      return upgrade
        ? `Error: Failed to download resource "${name} (${candidate?.target ?? ""})"`
        : `Error: Permission denied @ dir_s_rmdir - /opt/homebrew/Cellar/${name}`;
    case "npm":
      return upgrade ? "npm error code ETIMEDOUT" : "npm error code EACCES";
    case "pipx":
      return `Error: could not ${upgrade ? "upgrade" : "uninstall"} ${name}`;
    case "cargo":
      return upgrade
        ? `error: failed to compile \`${name} v${candidate?.target ?? ""}\``
        : `error: could not remove the \`${name}\` binaries`;
    case "ollama":
      return upgrade ? "Error: pull model manifest: file does not exist" : `Error: model '${name}' not found`;
    case "standalone-rustup":
      return upgrade
        ? "error: could not download the rustup self-update"
        : "error: could not remove the rustup home directory";
    default:
      return "error: the command did not finish";
  }
}

/** Why macOS would not move a path to the Trash, in its own words. */
function trashRefusal(path: string): string {
  return `“${fileName(path)}” couldn’t be moved to the Trash because you don’t have permission to access it.`;
}

/**
 * The run as it plays out under `?outcome=`: the lines the log shows and
 * how the operation ends. Only `succeeded` changes anything on the
 * machine (the backend applies it); `banager` means nothing started, so
 * nothing was written.
 */
export function playOutcome(
  plan: Plan,
  subject: Subject,
  outcome: Exclude<ScenarioOutcome, "mixed">,
): { lines: LogLine[]; outcome: Outcome } {
  const lines = successLog(plan, subject);
  const firstHalf = lines.slice(0, Math.ceil(lines.length / 2));
  const trashPaths = "TrashPaths" in plan.action ? plan.action.TrashPaths.paths : null;
  switch (outcome) {
    case "succeeded":
      return { lines, outcome: "Succeeded" };
    case "failed":
      if (trashPaths !== null) {
        const failedAt = trashPaths[Math.min(1, trashPaths.length - 1)];
        return {
          lines: [
            ...lines.slice(0, Math.min(1, trashPaths.length - 1)),
            { note: { TrashFailed: { path: mockHomeAsTilde(failedAt), error: trashRefusal(failedAt) } } },
          ],
          outcome: { Failed: { exit_code: null, summary: trashRefusal(failedAt), cause: failureCause(trashRefusal(failedAt)) } },
        };
      }
      return {
        lines: [...firstHalf, err(failureLine(plan, subject))],
        outcome: { Failed: { exit_code: 1, summary: failureLine(plan, subject), cause: failureCause(failureLine(plan, subject)) } },
      };
    case "cancelled":
      return { lines: firstHalf, outcome: "Cancelled" };
    case "unconfirmed":
      return { lines: firstHalf, outcome: "Unconfirmed" };
    case "attention": {
      const kind = plan.request.kind;
      if (kind === "Upgrade") return { lines, outcome: { NeedsAttention: "UnchangedAfterUpgrade" } };
      if (kind === "Install") return { lines, outcome: { NeedsAttention: "NotInstalledAfterInstall" } };
      if (trashPaths === null) {
        return { lines, outcome: { NeedsAttention: "StillInstalledAfterUninstall" } };
      }
      // A copy of the tool that was still running put its launcher back.
      const launcher = trashPaths[trashPaths.length - 1];
      return {
        lines: [...lines, { note: { BackAfterUninstall: { path: mockHomeAsTilde(launcher) } } }],
        outcome: { NeedsAttention: "BackAfterUninstall" },
      };
    }
    case "banager":
      return {
        lines: [],
        outcome: { BanagerFailed: { SpawnFailed: { detail: "Operation not permitted (os error 1)" } } },
      };
    case "password": {
      // A plan that runs no command never meets sudo: it ends as `failed`.
      if (trashPaths !== null) return playOutcome(plan, subject, "failed");
      const said = sudoNeedsPassword(plan.request.name);
      return {
        lines: [...firstHalf, ...said.map(err)],
        outcome: { Failed: { exit_code: 1, summary: said.join("\n"), cause: failureCause(said.join("\n")) } },
      };
    }
  }
}

/**
 * What Homebrew prints, on stderr, when it refuses to uninstall a formula
 * that something still installed needs -- which it does whatever else
 * happens, Banager never passing `--ignore-dependencies` -- or null when
 * nothing installed needs it: the dependents `brew uses --installed`
 * names (`BREW_DEPENDENTS`), as the pretend Mac has them when the
 * uninstall runs. A batch that uninstalls a dependent first, as it does,
 * meets this only when that one did not go. Homebrew 7.0.7-9's own words
 * (`DependentsMessage#output`, Library/Homebrew/dependents_message.rb:25-45,
 * through `ofail`'s `Error:` label; `Utils::Text.to_sentence`,
 * utils/text.rb:14-25, joins the names; a keg is named by its path).
 */
export function homebrewRefusal(world: World, inst: ManagerInstance, plan: Plan): string[] | null {
  const { kind, name, artifact_kind: artifactKind } = plan.request;
  if (inst.adapter_id !== "brew" || kind !== "Uninstall" || artifactKind !== "Formula") return null;
  const installed = (formula: string) => world.artifacts.find((a) => a.key.instance_id === inst.id && a.key.name === formula);
  const dependents = (BREW_DEPENDENTS[name] ?? []).filter((formula) => installed(formula) !== undefined);
  if (dependents.length === 0) return null;
  const sentence =
    dependents.length === 1
      ? dependents[0]
      : `${dependents.slice(0, -1).join(", ")} and ${dependents[dependents.length - 1]}`;
  return [
    `Error: Refusing to uninstall ${inst.prefix}/Cellar/${name}/${installed(name)?.version ?? ""}`,
    `because it is required by ${sentence}, which ${dependents.length === 1 ? "is" : "are"} currently installed.`,
    "You can override this and force removal with:",
    `  brew uninstall --ignore-dependencies ${name}`,
  ];
}

/**
 * What Homebrew prints when a cask's own step runs `sudo` and sudo, with
 * no terminal and no askpass helper, cannot ask for the password: the
 * failed command, then sudo's two lines (sudo 1.9).
 */
function sudoNeedsPassword(name: string): string[] {
  return [
    `Error: Failure while executing; \`/usr/bin/sudo -u root -E LOGNAME=me USER=me USERNAME=me -- /bin/launchctl bootout system/com.${name}.helper\` exited with 1. Here's the output:`,
    "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper",
    "sudo: a password is required",
  ];
}
