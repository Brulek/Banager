import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { BatchUninstallSheet, useBatchUninstall, type BatchTool } from "./BatchUninstallSheet";
import { artifactKeyId, useUiStore } from "../store/ui";
import type {
  CommandFact,
  InstalledArtifact,
  IssuedPlan,
  ManagerInstance,
  Measured,
  OpRequest,
  Plan,
  Settings,
  Sizes,
  Snapshot,
  Warning,
} from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";

// The batch uninstall's sheet, with its hook, against a backend that plans
// what the real adapters would (src/dev/mockPlans.ts is the long version).

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};
const pipx: ManagerInstance = { ...brew, id: "pipx", adapter_id: "pipx", exe_path: "/opt/homebrew/bin/pipx" };
const ollama: ManagerInstance = {
  ...brew,
  id: "ollama:http://127.0.0.1:11434",
  adapter_id: "ollama",
  exe_path: "/opt/homebrew/bin/ollama",
};
const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" };
const claude: ManagerInstance = {
  ...brew,
  id: "standalone-claude",
  adapter_id: "standalone-claude",
  exe_path: "/Users/you/.local/bin/claude",
};
const rustup: ManagerInstance = {
  ...brew,
  id: "standalone-rustup",
  adapter_id: "standalone-rustup",
  exe_path: "/Users/you/.cargo/bin/rustup",
};
const instances = [brew, pipx, ollama, npm, claude, rustup];

const runs = (...names: string[]): CommandFact[] => names.map((name) => ({ name, state: "Runs" }));

function artifact(
  instance: ManagerInstance,
  kind: InstalledArtifact["key"]["kind"],
  name: string,
  more: Partial<InstalledArtifact> = {},
): InstalledArtifact {
  return {
    key: { instance_id: instance.id, kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...more,
  };
}

const formula = (name: string, more: Partial<InstalledArtifact> = {}) => artifact(brew, "Formula", name, more);
const pipxFormula = formula("pipx", { facts: { ...NO_FACTS, commands: runs("pipx") } });
const python = formula("python@3.13");
const openssl = formula("openssl@3", { reason: "Dependency" });
const node = formula("node@22");
const wget = formula("wget");
const git = formula("git", { facts: { ...NO_FACTS, commands: runs("git", "scalar") } });
const jq = formula("jq");
const htop = formula("htop");
const postgres = formula("postgresql@17");
const ollamaFormula = formula("ollama", { facts: { ...NO_FACTS, commands: runs("ollama") } });
const zoom = artifact(brew, "Cask", "zoom");
const adobe = artifact(brew, "Cask", "adobe-creative-cloud", { display_name: "Adobe Creative Cloud" });
const vscode = artifact(brew, "Cask", "visual-studio-code", { display_name: "Microsoft Visual Studio Code" });
const llama = artifact(ollama, "Model", "llama3.2:3b", { size_bytes: 2_000_000_000 });
const httpie = artifact(pipx, "Tool", "httpie");
const npmClaude = artifact(npm, "Package", "@anthropic-ai/claude-code", {
  facts: { ...NO_FACTS, family: "claude-code", commands: runs("claude") },
});
const codex = artifact(npm, "Package", "@openai/codex", { facts: { ...NO_FACTS, family: "codex" } });
const claudeCode = artifact(claude, "Binary", "claude", {
  display_name: "Claude Code",
  facts: {
    ...NO_FACTS,
    family: "claude-code",
    commands: [{ name: "claude", state: { ShadowedBy: { by: npmClaude.key } } }],
  },
});
const rustupTool = artifact(rustup, "Binary", "rustup");

const everything = [
  pipxFormula,
  python,
  openssl,
  node,
  wget,
  git,
  jq,
  htop,
  postgres,
  ollamaFormula,
  zoom,
  adobe,
  vscode,
  llama,
  httpie,
  npmClaude,
  codex,
  claudeCode,
  rustupTool,
];

/** What `brew uses --installed` says of each formula. */
const DEPENDENTS: Record<string, string[]> = {
  "python@3.13": ["pipx"],
  "openssl@3": ["node@22", "postgresql@17", "wget"],
};

const scope = (what: string): Warning => ({ UninstallScope: { what } }) as Warning;
const about = (bytes: number): Measured => ({ bytes, partial: false, at_least: false });

/** The plan the real adapter would build, or its refusal as the IPC words it. */
function planOf(request: OpRequest): Plan {
  const base: Plan = {
    request,
    action: { Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--formula", request.name], env: [["HOMEBREW_NO_AUTOREMOVE", "1"]] } },
    needs_password: false,
    locks: [request.instance_id],
    cancel_policy: "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 1800,
  };
  const refuse = (payload: Record<string, unknown>) => {
    throw new Error(JSON.stringify(payload));
  };
  switch (request.name) {
    case "postgresql@17":
      return refuse({ kind: "uninstall_blocked", reason: "Pinned" });
    case "jq":
      return refuse({ kind: "index_updating" });
    case "rustup":
      return {
        ...base,
        action: { Command: { program: rustup.exe_path, args: ["self", "uninstall", "-y"], env: [] } },
        cancel_policy: "NoCancel",
        warnings: [{ DeletesCargoHome: { path: "~/.cargo" } }],
      };
    case "zoom":
      return {
        ...base,
        warnings: [scope("HomebrewCaskSteps"), { CaskUninstallStep: { step: "Deletes", items: ["/Library/Zoom"] } }],
      };
    case "adobe-creative-cloud":
      return { ...base, warnings: [scope("HomebrewCaskStepsUnseen")] };
    case "visual-studio-code":
      return {
        ...base,
        needs_password: true,
        warnings: [scope("HomebrewCaskSteps"), { CaskUninstallStep: { step: "QuitsNamedApps", items: ["Visual Studio Code"] } }],
      };
    case "htop":
      return { ...base, warnings: [scope("HomebrewFormulaOnly"), "DependentsUnknown"] };
    case "llama3.2:3b":
      return { ...base, action: { Command: { program: ollama.exe_path, args: ["rm", request.name], env: [] } }, warnings: [scope("Ollama")] };
    case "httpie":
      return { ...base, action: { Command: { program: pipx.exe_path, args: ["uninstall", "httpie"], env: [] } }, warnings: [scope("Pipx")] };
    case "@anthropic-ai/claude-code":
      return {
        ...base,
        action: { Command: { program: npm.exe_path, args: ["uninstall", "-g", request.name], env: [] } },
        warnings: [scope("Npm"), { KeepsData: { path: "~/.claude", what: "ToolData", size: about(412_300_000), left_out: [] } }],
      };
    case "@openai/codex":
      return {
        ...base,
        action: { Command: { program: npm.exe_path, args: ["uninstall", "-g", request.name], env: [] } },
        warnings: [scope("Npm"), { KeepsData: { path: "~/.codex", what: "ToolData", size: about(38_400_000), left_out: [] } }],
      };
    case "claude":
      return {
        ...base,
        action: { TrashPaths: { paths: ["/Users/you/.local/share/claude", "/Users/you/.claude/downloads", "/Users/you/.local/bin/claude"] } },
        warnings: [
          { WillTrash: { path: "~/.local/share/claude", what: "Program" } },
          { WillTrash: { path: "~/.claude/downloads", what: "Cache" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
          { AlreadyGone: { path: "~/.claude/local" } },
          { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
        ],
        timeout_secs: 120,
      };
    default:
      return {
        ...base,
        warnings: [scope("HomebrewFormulaOnly"), ...(neededByOf[request.name] ?? [])],
        affected: DEPENDENTS[request.name] ?? [],
      };
  }
}

let served: Snapshot;
let settings: Settings;
let sizes: Sizes;
/** Each plan request in the order it was asked for. */
let planned: string[];
/** The sources each Homebrew formula's preview says run on it (`Warning::NeededBySource`). */
let neededByOf: Record<string, Warning[]>;
/** When set, previews wait until a test lets them go. */
let holdPlans: boolean;
let held: Array<() => void>;
/** Each submitted preview's tool, in order. */
let submitted: string[];
/** The tools whose submit is refused, with the IPC's words. */
let refuseSubmit: Record<string, string>;
let holdSubmits: boolean;
let heldSubmits: Array<() => void>;
const planNames = new Map<string, string>();

beforeEach(() => {
  mockInvoke.mockReset();
  served = {
    generation: 1,
    round: 1,
    detect: "Found",
    instances,
    artifacts: everything,
    updates: [],
    refreshed_at: 1789700000,
    stale: false,
    errors: [],
  };
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  sizes = {
    ...NO_SIZES,
    round: 1,
    done: true,
    artifacts: [
      { key: wget.key, version: "1.0", measured: about(5_000_000), old_versions: null },
      { key: git.key, version: "1.0", measured: about(20_000_000), old_versions: null },
    ],
  };
  planned = [];
  neededByOf = {};
  holdPlans = false;
  held = [];
  submitted = [];
  refuseSubmit = {};
  holdSubmits = false;
  heldSubmits = [];
  planNames.clear();
  let plans = 0;
  let ops = 40;
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return served;
    if (cmd === "get_settings") return settings;
    if (cmd === "get_sizes") return sizes;
    if (cmd === "list_operations") return [];
    if (cmd === "plan_operation") {
      const request = (args as { request: OpRequest }).request;
      planned.push(request.name);
      if (holdPlans) await new Promise<void>((resolve) => held.push(resolve));
      const plan = planOf(request);
      plans += 1;
      const id = plans.toString(16).padStart(32, "0");
      planNames.set(id, request.name);
      return { id, plan, issued_at: 1789700000 } satisfies IssuedPlan;
    }
    if (cmd === "submit_operation") {
      const name = planNames.get((args as { planId: string }).planId) ?? "?";
      if (holdSubmits) await new Promise<void>((resolve) => heldSubmits.push(resolve));
      if (name in refuseSubmit) throw refuseSubmit[name];
      submitted.push(name);
      ops += 1;
      return ops;
    }
    return undefined;
  });
});

afterEach(async () => {
  vi.restoreAllMocks();
  await i18n.changeLanguage("en");
});

const instanceOf = (target: InstalledArtifact) => instances.find((i) => i.id === target.key.instance_id)!;
const tool = (target: InstalledArtifact): BatchTool => ({ artifact: target, instance: instanceOf(target), name: target.display_name });

let onStarted: ReturnType<typeof vi.fn<() => void>>;

function Harness({ tools }: { tools: InstalledArtifact[] }) {
  const uninstall = useBatchUninstall();
  return (
    <>
      <button type="button" onClick={(event) => uninstall.open(tools.map(tool), event.currentTarget, onStarted)}>
        open
      </button>
      <BatchUninstallSheet uninstall={uninstall} />
    </>
  );
}

/** Opens the sheet on `tools` and waits until it has every preview, unless they are held. */
async function openSheet(tools: InstalledArtifact[]): Promise<HTMLElement> {
  onStarted = vi.fn<() => void>();
  renderWithProviders(<Harness tools={tools} />);
  // The snapshot and the settings first, as on the page that opens it.
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_snapshot"));
  await act(async () => {});
  fireEvent.click(screen.getByRole("button", { name: "open" }));
  const dialog = await screen.findByRole("alertdialog");
  if (!holdPlans) await untilChecked(dialog);
  return dialog;
}

/** Waits until every preview is back: the line that says it checks is gone. */
async function untilChecked(dialog: HTMLElement): Promise<void> {
  await waitFor(() => expect(within(dialog).queryByText(/Checking what this affects|正在检查影响/)).toBeNull());
}

const toolItem = (dialog: HTMLElement, name: string): HTMLElement => {
  const items = within(dialog)
    .getAllByText(name, { selector: "[data-sheet-name]" })
    .map((element) => element.closest("[data-sheet-tool]") as HTMLElement);
  if (items.length !== 1) throw new Error(`expected one tool named ${name}, found ${items.length}`);
  return items[0];
};
const listOf = (dialog: HTMLElement, label: string): string[] =>
  [...within(dialog).getByRole("list", { name: label }).querySelectorAll("[data-sheet-name]")].map(
    (element) => element.textContent ?? "",
  );
/** The sheet's statuses that say something: one is there empty, for the re-check's note. */
const saying = (dialog: HTMLElement): HTMLElement[] =>
  within(dialog)
    .queryAllByRole("status")
    .filter((status) => status.textContent !== "");

describe("the batch uninstall's sheet", () => {
  it("opens at once with every name, checks at most three at a time, and keeps Uninstall off until all are back", async () => {
    holdPlans = true;
    const dialog = await openSheet([wget, git, jq, htop, node]);
    expect(within(dialog).getByRole("heading", { name: "Uninstall these 5 tools?" })).toBeInTheDocument();
    expect(listOf(dialog, "Will be uninstalled")).toEqual(["wget", "git", "jq", "htop", "node@22"]);
    expect(saying(dialog)).toHaveLength(1);
    expect(saying(dialog)[0]).toHaveTextContent("Checking what this affects: 0 of 5 checked…");
    // The re-check's note has its status there already, empty and out of
    // sight, so that its words are heard when they come.
    expect(within(dialog).getAllByRole("status").filter((status) => status.textContent === "")).toHaveLength(1);
    expect(within(dialog).getByRole("button", { name: "Uninstall 5 Tools" })).toBeDisabled();
    // Cancel has the focus: nothing here is one keypress from removing.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await waitFor(() => expect(planned).toEqual(["wget", "git", "jq"]));

    await act(async () => held.shift()!());
    await waitFor(() => expect(planned).toEqual(["wget", "git", "jq", "htop"]));
    expect(saying(dialog)).toHaveLength(1);
    expect(saying(dialog)[0]).toHaveTextContent("Checking what this affects: 1 of 5 checked…");
    while (held.length > 0 || planned.length < 5) {
      await act(async () => held.shift()?.());
    }
    await untilChecked(dialog);
    // jq's preview was refused (Homebrew updating its list): four go.
    expect(within(dialog).getByRole("button", { name: "Uninstall 4 Tools" })).toBeEnabled();
  });

  it("is an alert dialog, as NSAlert is, described by what it says from the moment it opens", async () => {
    // Decision I21c. Checking, it has only the list's name to say; then
    // what goes and what it takes.
    holdPlans = true;
    const dialog = await openSheet([wget, git]);
    expect(dialog).toHaveAttribute("role", "alertdialog");
    expect(screen.queryByRole("dialog")).toBeNull();
    // The list, which a screen reader names by its label, as WebKit's
    // description does (jsdom's would read its rows instead).
    const list = within(dialog).getByRole("list", { name: "Will be uninstalled" });
    expect(list.id).not.toBe("");
    expect(dialog).toHaveAttribute("aria-describedby", list.id);
    while (held.length > 0 || planned.length < 2) {
      await act(async () => held.shift()?.());
    }
    await untilChecked(dialog);
    expect(dialog).toHaveAccessibleDescription(/^These 2 tools will be uninstalled/);
  });

  it("asks about one tool as its own Uninstall does", async () => {
    const dialog = await openSheet([wget]);
    expect(within(dialog).getByRole("heading", { name: "Uninstall “wget”?" })).toBeInTheDocument();
    expect(dialog.querySelector("[data-dialog-subtitle]")?.textContent).toBe("Homebrew · 1.0");
    expect(within(dialog).getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("says why each tool it leaves out is left out", async () => {
    const dialog = await openSheet([postgres, jq, rustupTool, zoom, adobe, ollamaFormula, llama, openssl, node, wget]);
    expect(within(dialog).getByRole("heading", { name: "Uninstall these 3 tools?" })).toBeInTheDocument();
    expect(listOf(dialog, "Will be uninstalled")).toEqual(["llama3.2:3b", "node@22", "wget"]);
    expect(listOf(dialog, "Won't be uninstalled")).toEqual([
      "postgresql@17",
      "jq",
      "rustup",
      "zoom",
      "Adobe Creative Cloud",
      "ollama",
      "openssl@3",
    ]);
    expect(within(dialog).getByText("7 more won't be uninstalled; see why below.")).toBeInTheDocument();
    // A refusal (X1) is an alert, in red; why a tool is left out otherwise
    // is an explanation, in the secondary colour.
    const reason = (name: string): HTMLElement => {
      const item = toolItem(dialog, name);
      const found = item.querySelector<HTMLElement>("[role=alert], [data-sheet-reason]");
      if (found === null) throw new Error(`no reason under ${name}`);
      return found;
    };
    for (const name of ["rustup", "zoom", "Adobe Creative Cloud", "ollama", "openssl@3"]) {
      expect(reason(name)).toHaveAttribute("data-sheet-reason");
      expect(reason(name).className).not.toMatch(/danger/);
    }
    expect(reason("jq")).toHaveAttribute("role", "alert");
    // Pinned: the single dialog's sentence, with the unpin command as code.
    expect(reason("postgresql@17")).toHaveTextContent(
      "Couldn't uninstall it because it's pinned in Homebrew. Run /opt/homebrew/bin/brew unpin postgresql@17 in Terminal to unpin it first.",
    );
    expect(within(reason("postgresql@17")).getByText("/opt/homebrew/bin/brew unpin postgresql@17").tagName).toBe("CODE");
    expect(reason("jq")).toHaveTextContent(
      "Couldn't check what this affects: Homebrew is checking online for new versions, so dependencies can't be checked reliably now. Try again in a minute or two.",
    );
    expect(reason("rustup")).toHaveTextContent("This uninstall can't be cancelled once it starts. Uninstall it on its own, from its row.");
    expect(reason("zoom")).toHaveTextContent("This uninstall permanently deletes files. Uninstall it on its own, from its row.");
    expect(reason("Adobe Creative Cloud")).toHaveTextContent(
      "Its uninstall runs steps of its own, and what they delete can't be known in advance. Uninstall it on its own, from its row.",
    );
    expect(reason("ollama")).toHaveTextContent(
      "The Ollama tools you selected need it. Uninstalls from different sources run at the same time, so it can't be put after them. Uninstall those first, then this one on its own.",
    );
    // postgresql@17 is ticked, and left out itself: it keeps openssl@3.
    expect(reason("openssl@3")).toHaveTextContent(
      "postgresql@17 still uses it and won't be uninstalled, so it won't be either.",
    );
  });

  it("says of one ticked tool it cannot take that this one can't be uninstalled here, not these", async () => {
    const dialog = await openSheet([rustupTool]);
    expect(within(dialog).getByRole("heading", { name: "“rustup” can't be uninstalled here" })).toBeInTheDocument();
  });

  it("lists the tools in the body's own scroll, with no box scrolling inside it", async () => {
    const dialog = await openSheet([python, pipxFormula, rustupTool]);
    for (const list of dialog.querySelectorAll("[data-sheet-tools]")) {
      expect(list.className).not.toMatch(/max-h|overflow/);
      expect(list).not.toHaveAttribute("tabindex");
    }
  });

  it("says why the other sources' refusals are, in the single dialog's words", async () => {
    served = {
      ...served,
      instances: [...instances.filter((i) => i !== pipx), { ...pipx, status: { unavailable: "NotResponding", notes: [] } }],
    };
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return served;
      if (cmd === "get_settings") return settings;
      if (cmd === "get_sizes") return sizes;
      if (cmd === "list_operations") return [];
      if (cmd === "plan_operation") {
        const request = (args as { request: OpRequest }).request;
        if (request.name === "httpie") throw JSON.stringify({ kind: "not_actionable", read_only: null, unavailable: "NotResponding" });
        throw JSON.stringify({ kind: "uninstall_unsafe", path: "~/.local/bin/claude", reason: "outside_home" });
      }
      return undefined;
    });
    const dialog = await openSheet([httpie, claudeCode]);
    expect(within(dialog).getByRole("heading", { name: "These tools can't be uninstalled together here" })).toBeInTheDocument();
    expect(within(toolItem(dialog, "httpie")).getByRole("alert")).toHaveTextContent(
      "Couldn't check what this affects: pipx didn't respond, so what it has installed can't be shown. Check again later.",
    );
    expect(within(toolItem(dialog, "Claude Code")).getByRole("alert")).toHaveTextContent(
      "Couldn't move ~/.local/bin/claude because the folder holding it leads outside your home folder.",
    );
    // Nothing to include: OK alone, with the focus.
    const ok = within(dialog).getByRole("button", { name: "OK" });
    await waitFor(() => expect(ok).toHaveFocus());
    expect(within(dialog).queryByRole("button", { name: "Cancel" })).toBeNull();
    fireEvent.click(ok);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("puts a ticked dependent first, and says under its dependency why it waits", async () => {
    const dialog = await openSheet([python, pipxFormula]);
    expect(listOf(dialog, "Will be uninstalled")).toEqual(["pipx", "python@3.13"]);
    const item = toolItem(dialog, "python@3.13");
    expect(item).toHaveTextContent("Will be uninstalled after pipx.");
    fireEvent.click(within(item).getByRole("button", { name: "Details: Will be uninstalled after pipx." }));
    expect(
      await screen.findByText("If pipx isn't uninstalled, Homebrew won't uninstall this one, because it's still needed."),
    ).toBeInTheDocument();
    // Said once: the plan's own "still needed by" is left out.
    expect(within(item).queryByText(/pipx uses it/)).toBeNull();
    // Neither preview names a source that runs on it, so neither says
    // anything of pipx's tools: no guess from command names.
    expect(within(dialog).queryByText(/installed with pipx/)).toBeNull();
  });

  it("leaves out a Homebrew package another source runs on, in the single dialog's words, and what only it needed", async () => {
    // The real preview of Homebrew's pipx, while the pipx source runs it
    // and lists httpie (`Warning::NeededBySource`).
    neededByOf.pipx = [{ NeededBySource: { instance_id: pipx.id, program: true, tools: 1 } }];
    neededByOf.ollama = [{ NeededBySource: { instance_id: ollama.id, program: true, tools: 2 } }];
    const dialog = await openSheet([python, pipxFormula, ollamaFormula, wget]);
    expect(listOf(dialog, "Will be uninstalled")).toEqual(["wget"]);
    expect(listOf(dialog, "Won't be uninstalled")).toEqual(["python@3.13", "pipx", "ollama"]);
    const reason = (name: string) => toolItem(dialog, name).querySelector<HTMLElement>("[data-sheet-reason]");
    expect(reason("pipx")).toHaveTextContent(
      "Still used by pipx with its 1 tool. To uninstall it, first uninstall the tool installed with pipx.",
    );
    expect(reason("ollama")).toHaveTextContent(
      "Still used by Ollama with its 2 models. To uninstall it, first uninstall Ollama's 2 models.",
    );
    expect(reason("python@3.13")).toHaveTextContent("pipx still uses it and won't be uninstalled, so it won't be either.");
    // An explanation, not a refusal: the secondary colour.
    expect(reason("pipx")?.className).not.toMatch(/danger/);
    // Nothing of theirs was started.
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall" }));
    await waitFor(() => expect(submitted).toEqual(["wget"]));
  });

  it("says what each one deletes, where its files go, what it takes, and what they take together", async () => {
    const dialog = await openSheet([wget, claudeCode, llama, git]);
    expect(within(toolItem(dialog, "wget")).getByText(
      "Removes only the Homebrew version of wget and the links to it. Settings and data stored elsewhere are kept. Removed files don't go to the Trash.",
    )).toBeInTheDocument();
    expect(within(toolItem(dialog, "wget")).getByText("About 5 MB")).toBeInTheDocument();
    const claudeItem = toolItem(dialog, "Claude Code");
    // Its own files, not 「这3项」, which read as three of the tools ticked.
    expect(within(claudeItem).getByText("Removed files go to the Trash, where you can drag them back out.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/These \d+ items/)).toBeNull();
    // Only under the one that moves its files to the Trash.
    for (const name of ["wget", "llama3.2:3b", "git"]) {
      expect(within(toolItem(dialog, name)).queryByText(/go to the Trash, where/)).toBeNull();
    }
    expect(within(claudeItem).getByText(/~\/\.claude\/local/)).toBeInTheDocument();
    expect(within(toolItem(dialog, "llama3.2:3b")).getByText("Frees about 2 GB, less any part other models share.")).toBeInTheDocument();
    expect(within(toolItem(dialog, "llama3.2:3b")).getByText("About 2 GB")).toBeInTheDocument();
    // Claude Code's size is not known: what they take is more.
    const text = within(dialog).getByText(/^These 4 tools will be uninstalled\./);
    expect(text).toHaveTextContent("These 4 tools will be uninstalled. Together they take up 2 GB or more.");
    // What they take, never what removing them frees up.
    expect(text.textContent).not.toMatch(/free/i);
    fireEvent.click(within(dialog).getByRole("button", { name: "About this size" }));
    expect(await screen.findByText("1 of them has no size yet and isn't counted.")).toBeInTheDocument();
    expect(screen.getByText("What goes to the Trash frees space only once the Trash is emptied.")).toBeInTheDocument();
    expect(screen.getByText("Settings and data that stay after uninstalling aren't counted.")).toBeInTheDocument();
    expect(screen.getByText("Some of it is shared with other software and stays.")).toBeInTheDocument();
    expect(screen.getByText("Counts only its program files, not what it downloads or caches.")).toBeInTheDocument();
    expect(dialog.textContent).not.toMatch(/free up/i);
  });

  it("says which commands Terminal will no longer find, and which another copy runs instead", async () => {
    const dialog = await openSheet([git, npmClaude]);
    const lost = within(dialog).getByText("After this, Terminal won't find these commands: git and scalar.");
    expect(lost.closest("[data-caution]")).not.toBeNull();
    expect(
      within(toolItem(dialog, "@anthropic-ai/claude-code")).getByText(
        "After this, claude in Terminal runs the copy from Claude Code's own installer.",
      ),
    ).toBeInTheDocument();
  });

  it("names the commands it could not judge as going with their copies, not as lost", async () => {
    const unjudgedWget = { ...wget, facts: { ...NO_FACTS, commands: [{ name: "wget", state: null }] } };
    const dialog = await openSheet([git, unjudgedWget]);
    expect(within(dialog).getByText("After this, Terminal won't find these commands: git and scalar.")).toBeInTheDocument();
    const goes = within(dialog).getByText("This command is removed along with its copy: wget.");
    expect(goes.closest("[data-caution]")).toBeNull();
  });

  it("names nothing Terminal loses where both copies go", async () => {
    const dialog = await openSheet([claudeCode, npmClaude]);
    expect(within(dialog).getByText("After this, Terminal won't find this command: claude.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/runs the copy from/)).toBeNull();
  });

  it("lists what stays once per path, whose it is, with Copy Path and no way to delete it", async () => {
    const dialog = await openSheet([claudeCode, npmClaude, codex, vscode]);
    // Every tool the group names after "From", whether its own data or
    // what its installer's uninstall keeps (Claude Code's ~/.claude): 3.
    expect(within(dialog).getByText("3 of them leave some files behind, listed below.")).toBeInTheDocument();
    // The app that may ask, by name: not 「some of these」.
    expect(
      within(dialog).getByText(
        "Microsoft Visual Studio Code may ask for your Mac password, which can't be entered here. If it does, you'll see how to finish in Terminal.",
      ),
    ).toBeInTheDocument();
    const kept = within(dialog).getByRole("region", { name: "Stays after uninstalling" });
    expect([...kept.querySelectorAll("[data-kept-path]")].map((path) => path.textContent)).toEqual(["~/.claude", "~/.codex"]);
    expect(within(kept).getByText("From Claude Code and @anthropic-ai/claude-code")).toBeInTheDocument();
    expect(within(kept).getByText("From @openai/codex")).toBeInTheDocument();
    expect(within(kept).getByRole("button", { name: "Copy path: ~/.claude" })).toBeInTheDocument();
    for (const button of within(dialog).getAllByRole("button")) {
      expect(button.textContent ?? "").not.toMatch(/delete|remove|trash/i);
    }
  });

  it("says by name what every other confirmation says: the password can't be entered here, and Terminal finishes it (r24 W6)", async () => {
    // 「可能会要求输入Mac的密码」 alone had a person wait for a prompt that
    // cannot come; the single uninstall, an update and Update All say the
    // whole of it (`commandPreview.needsPassword`).
    await openSheet([wget, vscode]);
    await i18n.changeLanguage("zh-CN");
    expect(
      await screen.findByText("“Microsoft Visual Studio Code”可能会要求输入Mac密码，这里无法输入；需要时会告诉你在终端里怎么完成。"),
    ).toBeInTheDocument();
    await i18n.changeLanguage("zh-Hant");
    expect(
      await screen.findByText("「Microsoft Visual Studio Code」可能會要求輸入Mac密碼，這裡無法輸入；需要時會告訴你在終端機裡怎麼完成。"),
    ).toBeInTheDocument();
    // The same tail as the sentence of a confirmation of one, word for word.
    for (const language of ["zh-CN", "zh-Hant"]) {
      const t = i18n.getFixedT(language);
      const tail = t("commandPreview.needsPassword").split("Mac")[1];
      expect(t("reviewFixes.passwordNamed", { names: "x", count: 2 }).endsWith(`Mac${tail}`)).toBe(true);
    }
  });

  describe("says that what stays can go to the Trash in Finder only where no tool left installed may use it (U15 e)", () => {
    const trashLine = (dialog: HTMLElement) => dialog.querySelector("[data-kept-trash]");

    it("says it where nothing else of the family stays: npm's Codex alone", async () => {
      expect(trashLine(await openSheet([codex]))).toHaveTextContent(
        /^If you don't need these settings and data, you can move them to the Trash in Finder\.$/,
      );
    });

    it("says it where every copy goes together", async () => {
      expect(trashLine(await openSheet([claudeCode, npmClaude]))).not.toBeNull();
    });

    it("not while a copy that isn't ticked stays: npm's Claude Code without the native one", async () => {
      const dialog = await openSheet([npmClaude]);
      expect(within(dialog).getByRole("region", { name: "Stays after uninstalling" })).toBeInTheDocument();
      expect(trashLine(dialog)).toBeNull();
    });

    it("not for any of the list where one tool's copy stays", async () => {
      const dialog = await openSheet([codex, npmClaude]);
      expect(trashLine(dialog)).toBeNull();
    });
  });

  describe("says how many of them leave files behind", () => {
    const line = (dialog: HTMLElement) =>
      within(dialog).queryByText(/leaves? some files behind/)?.textContent ?? null;

    it("not when none does", async () => {
      expect(line(await openSheet([wget, git]))).toBeNull();
    });

    it("by number, one of several", async () => {
      expect(line(await openSheet([wget, codex, git]))).toBe("One of them leaves some files behind, listed below.");
    });

    it("as all of them, where it is", async () => {
      expect(line(await openSheet([codex, npmClaude]))).toBe("Each of them leaves some files behind, listed below.");
      await i18n.changeLanguage("zh-CN");
      expect(await screen.findByText("这些工具都会留下一些文件，列在下方。")).toBeInTheDocument();
    });

    it("not for one tool alone, which the group below says", async () => {
      const dialog = await openSheet([claudeCode]);
      expect(line(dialog)).toBeNull();
      expect(within(dialog).getByRole("region", { name: "Stays after uninstalling" })).toBeInTheDocument();
    });

    it("counting no tool left out, nor listing what it keeps", async () => {
      // python@3.13 would keep a folder, but pipx still uses it and is not ticked: it stays.
      neededByOf["python@3.13"] = [
        { KeepsData: { path: "~/.python_history", what: "ToolData", size: null, left_out: [] } },
      ];
      const dialog = await openSheet([python, wget, codex]);
      expect(listOf(dialog, "Won't be uninstalled")).toEqual(["python@3.13"]);
      expect(line(dialog)).toBe("One of them leaves some files behind, listed below.");
      const kept = within(dialog).getByRole("region", { name: "Stays after uninstalling" });
      expect([...kept.querySelectorAll("[data-kept-path]")].map((path) => path.textContent)).toEqual(["~/.codex"]);
    });

    it("naming each copy where two of the same name both keep files", async () => {
      // npm's copy, called Claude Code too: two tools keep ~/.claude, and the line says which.
      const npmNamedLikeIt = { ...npmClaude, display_name: "Claude Code" };
      const dialog = await openSheet([claudeCode, npmNamedLikeIt]);
      expect(line(dialog)).toBe("Each of them leaves some files behind, listed below.");
      const kept = within(dialog).getByRole("region", { name: "Stays after uninstalling" });
      expect(within(kept).getByText(/^From /).textContent).toBe(
        "From Claude Code installed with Claude Code's own installer and Claude Code installed with npm",
      );
      await i18n.changeLanguage("zh-CN");
      expect(await within(kept).findByText(/^来自/)).toHaveTextContent(
        "来自Claude Code自带的安装程序装的Claude Code和npm装的Claude Code",
      );
    });
  });

  it("shows the exact commands and paths in the order they run, open with technical details on", async () => {
    settings = { ...settings, show_technical_details: true };
    const dialog = await openSheet([python, claudeCode, pipxFormula]);
    expect(within(dialog).getByRole("button", { name: "Show Commands and Paths" })).toHaveAttribute("aria-expanded", "true");
    const shown = [...dialog.querySelectorAll("[data-batch-plans] code")].map((code) => code.textContent);
    expect(shown).toEqual([
      "/Users/you/.local/share/claude\n/Users/you/.claude/downloads\n/Users/you/.local/bin/claude",
      "HOMEBREW_NO_AUTOREMOVE=1 /opt/homebrew/bin/brew uninstall --formula pipx",
      "HOMEBREW_NO_AUTOREMOVE=1 /opt/homebrew/bin/brew uninstall --formula python@3.13",
    ]);
  });

  it("calls it Show Commands, closed, where nothing moves to the Trash and technical details are off", async () => {
    const dialog = await openSheet([wget, git]);
    const disclosure = within(dialog).getByRole("button", { name: "Show Commands" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(disclosure);
    expect(dialog.querySelectorAll("[data-batch-plans] code")).toHaveLength(2);
  });

  it("starts one uninstall per tool in the order they run, unticks each, and closes once all have started", async () => {
    useUiStore.getState().selectUninstalls([python.key, pipxFormula.key, wget.key, git.key]);
    const dialog = await openSheet([python, pipxFormula, wget]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall 3 Tools" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(submitted).toEqual(["pipx", "python@3.13", "wget"]);
    // git was ticked and not part of this batch: still ticked.
    expect(useUiStore.getState().selectedUninstalls).toEqual([artifactKeyId(git.key)]);
    expect(useUiStore.getState().uninstallBatch?.items).toEqual([
      { key: pipxFormula.key, name: "pipx", opId: 41, after: [] },
      { key: python.key, name: "python@3.13", opId: 42, after: [artifactKeyId(pipxFormula.key)] },
      { key: wget.key, name: "wget", opId: 43, after: [] },
    ]);
    expect(useUiStore.getState().opNames).toEqual({ 41: "pipx", 42: "python@3.13", 43: "wget" });
    await waitFor(() => expect(onStarted).toHaveBeenCalledTimes(1));
    // The operation bar carries the run: no log opens by itself.
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });

  it("does not start a dependency whose dependent did not start, and stays open saying why", async () => {
    refuseSubmit = { pipx: JSON.stringify({ kind: "expired" }) };
    const dialog = await openSheet([python, pipxFormula, wget]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall 3 Tools" }));
    const close = await within(dialog).findByRole("button", { name: "Close" });
    expect(submitted).toEqual(["wget"]);
    expect(within(toolItem(dialog, "pipx")).getByRole("alert")).toHaveTextContent(
      "Couldn't start the uninstall: This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    );
    const held = toolItem(dialog, "python@3.13").querySelector("[data-sheet-reason]");
    expect(held).toHaveTextContent("Didn't start, because pipx didn't start uninstalling.");
    expect(held?.querySelector("svg")).not.toBeNull();
    expect(within(toolItem(dialog, "wget")).getByText("Started")).toBeInTheDocument();
    await waitFor(() => expect(close).toHaveFocus());
    expect(useUiStore.getState().uninstallBatch?.items.map((item) => item.opId)).toEqual([null, null, 41]);
    fireEvent.click(close);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(onStarted).not.toHaveBeenCalled();
  });

  it("checks again instead of starting previews older than nine and a half minutes, and asks once more", async () => {
    let now = 1_000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    const dialog = await openSheet([wget, git]);
    expect(planned).toEqual(["wget", "git"]);
    // The note's status, there empty before there is anything to say.
    const emptyStatuses = within(dialog)
      .getAllByRole("status")
      .filter((status) => status.textContent === "");
    expect(emptyStatuses).toHaveLength(1);
    const noteStatus = emptyStatuses[0];
    now += 9.6 * 60 * 1000;
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall 2 Tools" }));
    expect(await within(dialog).findByText("That didn't start, so it was checked again. Confirm once more.")).toBeInTheDocument();
    // The same node as before, its words changed rather than a new status
    // put in with them: what a screen reader reads out.
    expect(saying(dialog)).toEqual([noteStatus]);
    expect(planned).toEqual(["wget", "git", "wget", "git"]);
    expect(submitted).toEqual([]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall 2 Tools" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(submitted).toEqual(["wget", "git"]);
  });

  it("drops what was still being checked once it is closed, and checks nothing more", async () => {
    holdPlans = true;
    const dialog = await openSheet([wget, git, jq, htop, node]);
    await waitFor(() => expect(planned).toHaveLength(3));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await act(async () => {
      while (held.length > 0) held.shift()!();
    });
    expect(planned).toEqual(["wget", "git", "jq"]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("closes with Escape, but not while it starts the uninstalls", async () => {
    holdSubmits = true;
    const dialog = await openSheet([wget, git]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall 2 Tools" }));
    await waitFor(() => expect(heldSubmits).toHaveLength(1));
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    await act(async () => {
      while (heldSubmits.length > 0 || submitted.length < 2) {
        heldSubmits.shift()?.();
        await Promise.resolve();
      }
    });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());

    holdSubmits = false;
    fireEvent.click(screen.getByRole("button", { name: "open" }));
    await untilChecked(await screen.findByRole("alertdialog"));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    const dialog = await openSheet([python, pipxFormula, wget]);
    expect(within(dialog).getByRole("heading", { name: "要卸载这3个工具吗？" })).toBeInTheDocument();
    expect(toolItem(dialog, "python@3.13")).toHaveTextContent("在“pipx”卸载之后再卸载。");
    expect(within(dialog).getByRole("button", { name: "卸载这3个" })).toBeEnabled();
    const said = within(dialog).getByText(/^会卸载下面3个工具，它们共占/);
    expect(said.textContent).not.toContain("腾出");
  });
});
