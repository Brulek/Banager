import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import i18n from "../i18n";
import zhCN from "../i18n/zh-CN.json";
import { UninstallDialog } from "./UninstallDialog";
import { BUTTON } from "./ui/controls";
import type { InstalledArtifact, IssuedPlan, ManagerInstance, OpRequest, Plan, Snapshot, Warning } from "../lib/types";
import { NO_FACTS } from "../lib/types";

const request: OpRequest = {
  kind: "Uninstall",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
};

function issuedPlanFor(overrides: Partial<Plan> = {}): IssuedPlan {
  return {
    id: "1",
    plan: {
      request,
      action: {
        Command: {
          program: "/opt/homebrew/bin/brew",
          args: ["uninstall", "--formula", "jq"],
          env: [],
        },
      },
      needs_password: false,
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
      ...overrides,
    },
    issued_at: 1758000000,
  };
}

function brewInstance(over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id: "brew:/opt/homebrew",
    adapter_id: "brew",
    exe_path: "/opt/homebrew/bin/brew",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "7.0.3",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    ...over,
  };
}

function snapshotWith(instances: ManagerInstance[], artifacts: InstalledArtifact[] = []): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances,
    artifacts,
    updates: [],
    refreshed_at: 1,
    stale: false,
    errors: [],
  };
}

const JQ_COMMAND = "/opt/homebrew/bin/brew uninstall --formula jq";

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

function submitCalls() {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "submit_operation");
}

/**
 * Waits for the plan to land -- its command's disclosure is there once it
 * has -- and opens it: the command is one press away, not on the sheet.
 */
async function showCommand() {
  const disclosure = await screen.findByRole("button", { name: "Show Command" });
  if (disclosure.getAttribute("aria-expanded") !== "true") fireEvent.click(disclosure);
}

/** The group headed `title` (the copy table's C4). */
function group(title: string): HTMLElement {
  return screen.getByRole("region", { name: title });
}

/** The lines of the group headed `title`, top to bottom (an ⓘ adds no words). */
function linesOf(title: string): string[] {
  return within(group(title))
    .queryAllByRole("listitem")
    .map((item) => (item.textContent ?? "").trim());
}

// A sheet's note by its whole sentence: its last word is held on one line
// with its ⓘ, in a span of their own (`TextWithInfo`), so no one text node
// holds the sentence.
function noteLine(text: string) {
  return (_content: string, element: Element | null) =>
    element?.tagName === "SPAN" && element.parentElement?.tagName === "LI" && element.textContent?.trim() === text;
}

/** The 「卸载后会保留」 group's lines, as "what: path". */
function keptLines(): string[] {
  return [...document.querySelectorAll("[data-kept-data]")].map(
    (line) =>
      `${line.querySelector("[data-kept-what]")?.textContent?.trim()}: ${line.querySelector("[data-kept-path]")?.textContent}`,
  );
}

/** The sheet's statuses that say something: one is there empty, for the re-check's note. */
function saying(): HTMLElement[] {
  return screen.queryAllByRole("status").filter((status) => status.textContent !== "");
}

describe("UninstallDialog", () => {
  it("shows a checking message and a disabled confirm button while the plan is loading", async () => {
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // `planMutation.mutate()` runs in an effect and TanStack Query v5 pushes
    // the `isPending` transition to React through a setTimeout(0) scheduler,
    // so right after render the component is still idle: wait for it.
    expect(await screen.findByText("Checking what this affects…")).toBeInTheDocument();
    // A status, said as the dialog opens on it.
    expect(saying()).toHaveLength(1);
    expect(saying()[0]).toHaveTextContent("Checking what this affects…");
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
    // The re-check's note has its status there already, empty and out of
    // sight, so that its words are heard when they come.
    expect(screen.getAllByRole("status").filter((status) => status.textContent === "")).toHaveLength(1);
  });

  it("asks the question as its title, under the tool's 48 icon, with its source and the version it has under it", async () => {
    const jq: InstalledArtifact = {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.1",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
      facts: NO_FACTS,
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshotWith([brewInstance()], [jq]);
      if (cmd === "plan_operation") return issuedPlanFor({ warnings: [{ UninstallScope: { what: "HomebrewFormulaOnly" } }] });
      return undefined;
    });

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    // An alert dialog to a screen reader, as NSAlert is (decision I21c).
    const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" });
    expect(screen.queryByRole("dialog")).toBeNull();
    // An alert's layout (spec §3.6): one tool, 360 wide; its icon over the
    // question, and where it comes from and the version it has under it.
    expect(dialog).toHaveAttribute("data-dialog-width", "360");
    const subtitle = (await within(dialog).findByText("1.8.1")).closest("[data-dialog-subtitle]");
    expect(subtitle).toHaveTextContent("Homebrew · 1.8.1");
    // Described to a screen reader as it opens by that line, then its text:
    // what goes and what stays.
    await within(dialog).findByText(/^Removes only /, { selector: "[data-sheet-text]" });
    expect(dialog).toHaveAccessibleDescription(
      "Homebrew · 1.8.1 Removes only the Homebrew version of jq and the links to it. Settings and data stored elsewhere are kept. Removed files don't go to the Trash.",
    );
    const icon = dialog.querySelector("[data-dialog-icon]") as HTMLElement;
    expect(icon.compareDocumentPosition(within(dialog).getByRole("heading", { name: "Uninstall “jq”?" }))).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    // The avatar a row has, at 48: jq has no logo in this test's pack, so
    // the program tile (I8), with the source's initial on its corner.
    const tile = icon.querySelector("[data-program-tile]") as HTMLElement;
    expect(tile).toHaveAttribute("aria-hidden", "true");
    expect(tile.className).toMatch(/\bh-12 w-12\b/);
    expect(within(icon).getByText("H").closest("[data-source-badge]")).not.toBeNull();
  });

  it("says which of two Homebrews it uninstalls from, by the Mac each is for", async () => {
    // A Mac migrated from Intel keeps a Homebrew in /usr/local beside the
    // one in /opt/homebrew: 「Homebrew」 alone would not say which one's jq goes.
    const intelRequest: OpRequest = { ...request, instance_id: "brew:/usr/local" };
    const jq: InstalledArtifact = {
      key: { instance_id: "brew:/usr/local", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.7.1",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
      facts: NO_FACTS,
    };
    const intel = brewInstance({ id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshotWith([brewInstance(), intel], [jq]);
      if (cmd === "plan_operation") return issuedPlanFor({ request: intelRequest });
      return undefined;
    });

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={intelRequest} displayName="jq" />);

    const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" });
    const subtitle = (await within(dialog).findByText("1.7.1")).closest("[data-dialog-subtitle]");
    expect(subtitle).toHaveTextContent("Homebrew (Intel) · 1.7.1");
  });

  it("shows an app's own icon beside its name, as its row does, once the icon arrives", async () => {
    const icon = "data:image/png;base64,iVBORw0KGgo=";
    const itermRequest: OpRequest = { ...request, artifact_kind: "Cask", name: "iterm2" };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshotWith([brewInstance()]);
      if (cmd === "plan_operation") return issuedPlanFor({ request: itermRequest });
      if (cmd === "artifact_icon") return icon;
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={itermRequest} displayName="iTerm2" />,
    );

    const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “iTerm2”?" });
    const item = dialog.querySelector("[data-dialog-icon]") as HTMLElement;
    await waitFor(() => expect(item.querySelector("img[data-app-icon]")).toHaveAttribute("src", icon));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("artifact_icon", {
      key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" },
    });
    // In place of the source's initial, which moves to the icon's corner.
    expect(within(item).getByText("H").closest("[data-source-badge]")).not.toBeNull();
  });

  it("puts the focus on Cancel as it opens, and makes Uninstall the default button, not a red one", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    const cancel = screen.getByRole("button", { name: "Cancel" });
    await waitFor(() => expect(document.activeElement).toBe(cancel));
    const uninstall = screen.getByRole("button", { name: "Uninstall" });
    // Large, both: Cancel grey, Uninstall the accent -- the user chose it,
    // so it is not tinted as a warning (HIG).
    expect(cancel.className).toBe(BUTTON.large.grey);
    expect(uninstall.className).toBe(BUTTON.large.default);
    for (const button of [cancel, uninstall]) expect(button.className).not.toMatch(/danger/);
  });

  it("says some apps ask for the Mac's password when the plan may need it, and stays quiet when it does not", async () => {
    // Every Cask uninstall sets `needs_password` (crates/banager-core/src/
    // adapters/brew/mod.rs), though not every app then asks for it: T3 of
    // the copy table. Spec §6: an operation that needs a password is
    // marked in the preview -- a password is never a surprise.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: true }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("You can't enter your Mac password here. If an app asks for it at this step, you'll see how to finish in Terminal.")).toBeInTheDocument();
    expect(linesOf("Notes")).toEqual(["You can't enter your Mac password here. If an app asks for it at this step, you'll see how to finish in Terminal."]);
    // A line of its own under the text, with no 「请注意」 heading over it,
    // and no ⚠︎: a password is not a caution.
    expect(within(group("Notes")).queryByRole("heading")).toBeNull();
    expect(within(group("Notes")).getByRole("listitem")).not.toHaveAttribute("data-caution");

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: false }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await showCommand();
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    expect(screen.queryByText(/password/)).not.toBeInTheDocument();
  });

  describe("the sources that run on a Homebrew package (Warning.NeededBySource)", () => {
    const base = {
      version: "1.0",
      reason: "Requested" as const,
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    };
    const formula = (name: string): InstalledArtifact => ({
      ...base,
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
      display_name: name,
      // What the command check says of it: the copy Terminal runs. The old
      // caution read this; the preview's own look decides now.
      facts: { ...NO_FACTS, commands: [{ name, state: "Runs" }] },
    });
    const npm = brewInstance({ id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" });
    const pipx = brewInstance({ id: "pipx", adapter_id: "pipx", exe_path: "/opt/homebrew/bin/pipx", prefix: "/opt/homebrew/bin" });
    const ollama = brewInstance({
      id: "ollama:http://127.0.0.1:11434",
      adapter_id: "ollama",
      exe_path: "/opt/homebrew/bin/ollama",
      prefix: "/Users/you/.ollama",
    });
    const tool = (instanceId: string, name: string): InstalledArtifact => ({
      ...base,
      key: { instance_id: instanceId, kind: "Tool", name },
      display_name: name,
      facts: NO_FACTS,
    });

    function open(name: string, plan: Partial<Plan>, artifacts: InstalledArtifact[]) {
      const own: OpRequest = { ...request, name };
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_snapshot") return snapshotWith([brewInstance(), npm, ollama, pipx], artifacts);
        if (cmd === "plan_operation") return issuedPlanFor({ request: own, ...plan });
        return undefined;
      });
      renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={own} displayName={name} />);
    }

    it("lists npm and its tools under what uses it, offers no Uninstall, and says to uninstall the tools first", async () => {
      open(
        "node@22",
        { warnings: [{ NeededBySource: { instance_id: "npm:/opt/homebrew", program: true, tools: 4 } }] },
        [formula("node@22")],
      );

      expect(await screen.findByText("npm with its 4 tools")).toBeInTheDocument();
      within(group("Notes")).getByText("Software that uses it");
      expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
      expect(screen.getByText("Uninstall the 4 tools installed with npm first to remove node@22.")).toBeInTheDocument();
      expect(screen.queryByText("Uninstall these first to remove node@22.")).toBeNull();
      // The command is still there to read.
      await showCommand();
    });

    it("names Homebrew's dependents first, then the tools whose environment it is", async () => {
      open(
        "python@3.13",
        {
          affected: ["pipx"],
          warnings: [
            { WouldBreak: { names: ["pipx"] } },
            { NeededBySource: { instance_id: "pipx", program: false, tools: 2 } },
          ],
        },
        [formula("python@3.13"), formula("pipx")],
      );

      expect(await screen.findByText("2 tools installed with pipx")).toBeInTheDocument();
      const list = within(group("Notes"))
        .getAllByRole("listitem")
        .map((item) => item.textContent);
      expect(list).toEqual(["pipx", "2 tools installed with pipx"]);
      expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
      expect(
        screen.getByText(
          "Uninstall the Homebrew software above and the 2 tools installed with pipx first to remove python@3.13.",
        ),
      ).toBeInTheDocument();
    });

    it("says nothing of another source's tools where the preview names none, and offers Uninstall", async () => {
      // Homebrew's `pipx` with two pipx tools, as the old caution read it
      // from the command check: the preview's own look found nothing runs
      // on it, so there is nothing to keep it for and nothing to warn of.
      open("pipx", {}, [formula("pipx"), tool("pipx", "aider-chat"), tool("pipx", "httpie")]);

      await showCommand();
      expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
      expect(screen.queryByText("Software that uses it")).toBeNull();
      expect(screen.queryByText(/installed with pipx/)).toBeNull();
    });

    it("says so in Chinese, with the sidebar's name for the source", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        open(
          "ollama",
          { warnings: [{ NeededBySource: { instance_id: ollama.id, program: true, tools: 2 } }] },
          [formula("ollama")],
        );
        expect(await screen.findByText("Ollama及其2个模型")).toBeInTheDocument();
        expect(screen.getByText(zhCN.uninstall.affectedTitle)).toBeInTheDocument();
        expect(screen.getByText("要卸载“ollama”，请先卸载Ollama的2个模型。")).toBeInTheDocument();
      } finally {
        await i18n.changeLanguage("en");
      }
    });

    it("words the refusal of a preview that named one, which only a page that asks for it can reach", async () => {
      // `Session::submit` refuses such a preview whatever the page sends;
      // the dialog checks again, and says why while it does.
      let planCalls = 0;
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "plan_operation") {
          planCalls += 1;
          return planCalls === 1 ? issuedPlanFor() : new Promise<IssuedPlan>(() => {});
        }
        if (cmd === "submit_operation") throw JSON.stringify({ kind: "uninstall_blocked", reason: "NeededBySource" });
        return undefined;
      });
      renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);
      const confirm = await screen.findByRole("button", { name: "Uninstall" });
      await waitFor(() => expect(confirm).toBeEnabled());
      fireEvent.click(confirm);
      const alert = await screen.findByRole("alert");
      await waitFor(() => expect(alert).toHaveTextContent("Couldn't uninstall it because another source runs on it."));
      expect(alert.textContent).not.toMatch(/uninstall_blocked|NeededBySource/);
    });
  });

  it("disables confirm and explains what would break when something depends on it", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ affected: ["jq-cli-wrapper"] }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await waitFor(() => expect(screen.getByText("jq-cli-wrapper")).toBeInTheDocument());
    const confirmButton = screen.getByRole("button", { name: "Uninstall" });
    expect(confirmButton).toBeDisabled();
    within(group("Notes")).getByText("Software that uses it");
    await showCommand();
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    // The reason Confirm is disabled, and what to do about it, is plain text
    // in the dialog body -- not a `title` on a disabled button, which a
    // disabled button never actually shows: it takes no pointer events (no
    // hover) and drops out of the tab order (no keyboard/VoiceOver focus).
    expect(confirmButton).not.toHaveAttribute("title");
    expect(screen.getByText("Uninstall these first to remove jq.")).toBeInTheDocument();
  });

  it("marks a caution with a ⚠︎ before its words, in the label colour at 11, and nothing else", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ warnings: ["DependentsUnknown"], needs_password: true }));

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    await screen.findByRole("region", { name: "Notes" });
    const [caution, password] = within(group("Notes")).getAllByRole("listitem");
    expect(caution).toHaveAttribute("data-caution");
    expect(caution.querySelector("svg")).toHaveClass("text-warning");
    expect(password).not.toHaveAttribute("data-caution");
    expect(password.querySelector("svg")).toBeNull();
    for (const line of [caution, password]) {
      expect(line).toHaveClass("text-foreground", "text-small");
      expect(line.className).not.toMatch(/text-muted/);
    }
    // No warning sign by the question, and none over the notes.
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByRole("heading", { name: "Uninstall “jq”?" }).querySelector("svg")).toBeNull();
  });

  it("names what still needs it once, not again as a warning", async () => {
    // Homebrew's preview fills `WouldBreak` and `affected` from the same
    // `brew uses` (crates/banager-core/src/adapters/brew/mod.rs); the list
    // says it, and the sentence would say it a second time.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ affected: ["wget", "git"], warnings: [{ WouldBreak: { names: ["wget", "git"] } }] }),
    );

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="gettext" />);

    await screen.findByText("wget");
    expect(screen.getAllByText("wget")).toHaveLength(1);
    expect(screen.queryByText(/still need this/)).not.toBeInTheDocument();
    expect(linesOf("Notes")).toEqual(["wget", "git"]);
  });

  it("localises the plan's warnings instead of showing the Rust side's English", async () => {
    // Spec §6: this is the app's only destructive confirmation screen, and
    // a Chinese user was being asked to read an English risk warning right
    // above the button that acts on it.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: ["DependentsUnknown", { WouldBreak: { names: ["python@3.13"] } }] }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(
      await screen.findByText(
        "Couldn't check what else needs this. Check yourself before you uninstall.",
      ),
    ).toBeInTheDocument();
    // No `affected` list on this plan, so the sentence is where the name is said.
    expect(await screen.findByText("python@3.13 still needs this.")).toBeInTheDocument();
  });

  it("pluralises WouldBreak's copy and interpolates every name", async () => {
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: [{ WouldBreak: { names: ["a", "b"] } }] }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("2 other tools still need this: a, b.")).toBeInTheDocument();
  });

  it("groups what a path-list uninstall moves and keeps, and says only what is sure of the Trash", async () => {
    // Spec §6.6, the Claude Code dialog, in the copy table's groups (C4):
    // what moves, in the order the paths will be moved, then what stays.
    // The Trash's sentence stands where a command would be, since no
    // command runs -- and it promises dragging back out of the Trash, not
    // Finder's Put Back, which works as often as not (T4).
    const claudeRequest: OpRequest = {
      ...request,
      instance_id: "standalone-claude",
      artifact_kind: "Binary",
      name: "claude",
    };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: claudeRequest,
        action: {
          TrashPaths: {
            paths: [
              "/Users/someone/.local/share/claude",
              "/Users/someone/.claude/downloads",
              "/Users/someone/.local/bin/claude",
            ],
          },
        },
        warnings: [
          { WillTrash: { path: "~/.local/share/claude", what: "Program" } },
          { WillTrash: { path: "~/.claude/downloads", what: "Cache" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
          { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
          { WillKeep: { path: "~/.claude.json", what: "Settings" } },
        ],
        locks: ["standalone-claude"],
        timeout_secs: 120,
      }),
    );

    const { container } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={claudeRequest} displayName="Claude Code" />,
    );

    await screen.findByRole("region", { name: "Moves to the Trash" });
    expect(linesOf("Moves to the Trash")).toEqual([
      "Program files: ~/.local/share/claude",
      "Cache it can download again: ~/.claude/downloads",
      "The command: ~/.local/bin/claude",
    ]);
    expect(
      within(group("Moves to the Trash")).getByText(
        "These 3 items go to the Trash, where you can drag them back out.",
      ),
    ).toBeInTheDocument();
    // What stays, in the one group every uninstall has, with Copy Path.
    expect(keptLines()).toEqual(["Settings, login and history: ~/.claude", "Settings: ~/.claude.json"]);
    expect(
      within(screen.getByRole("region", { name: "Stays after uninstalling" })).getAllByRole("button", {
        name: /^Copy path: /,
      }),
    ).toHaveLength(2);
    expect(screen.queryByRole("region", { name: "Notes" })).toBeNull();
    // Nothing to show behind "Show Command": no command runs.
    expect(screen.queryByRole("button", { name: /^Show Command/ })).toBeNull();
    expect(container.ownerDocument.body.textContent).not.toMatch(/Put Back/);
    // Everything it removes goes to the Trash: no sentence says otherwise.
    expect(container.ownerDocument.body.textContent).not.toMatch(/don't go to the Trash/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("says a program directory an earlier uninstall already moved is already gone", async () => {
    // The launcher-only row's second uninstall (spec §6.3 check 2): the
    // list adds up -- one item to move, one already in the Trash.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        action: { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } },
        warnings: [
          { AlreadyGone: { path: "~/.local/share/claude" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
        ],
      }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="Claude Code" />,
    );

    await screen.findByText("Already gone: ~/.local/share/claude");
    expect(linesOf("Moves to the Trash")).toEqual([
      "Already gone: ~/.local/share/claude",
      "The command: ~/.local/bin/claude",
    ]);
    expect(
      screen.getByText("This item goes to the Trash, where you can drag it back out."),
    ).toBeInTheDocument();
  });

  it("keeps each line's longer why behind its ⓘ, and says rustup's permanent deletions out loud", async () => {
    // The copy table's `<key>Detail`s: the line keeps what decides whether
    // to go on -- "permanently deletes", the path, what goes with it --
    // and the ⓘ has the rest. The Cargo folder's line keeps all of it:
    // everything in the folder goes, and none of it to the Trash.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        action: { Command: { program: "/Users/someone/.cargo/bin/rustup", args: ["self", "uninstall", "-y"], env: [] } },
        cancel_policy: "NoCancel",
        warnings: [
          { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
          { DeletesCargoHome: { path: "~/.cargo" } },
          { RemovesCargoInstalled: { names: ["tokei"] } },
          "HomebrewRustupLosesToolchains",
          { LeavesShellConfigLine: { path: "~/.zprofile", certain: false } },
          { WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } },
        ],
      }),
    );

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" />);

    await screen.findByRole("region", { name: "Notes" });
    expect(linesOf("Notes")).toEqual([
      "Permanently deletes ~/.rustup: toolchains stable-aarch64-apple-darwin and everything rustup downloaded.",
      "Permanently deletes ~/.cargo and everything in it, including Cargo's cache, settings and saved login. Nothing goes to the Trash.",
      "Also permanently deletes tokei from the Cargo folder.",
      "Homebrew's rustup shares these folders, so its toolchains go too.",
      "~/.zprofile has a line that mentions Cargo, which rustup won't remove.",
      "This can't be cancelled once it starts. Don't quit Banager or shut down your Mac until it finishes.",
    ]);
    expect(keptLines()).toEqual(["Conversations and history: ~/.gemini/antigravity-cli"]);

    const whys: Array<[string, string]> = [
      [
        "Permanently deletes ~/.rustup: toolchains stable-aarch64-apple-darwin and everything rustup downloaded.",
        "Nothing goes to the Trash. Projects that need Rust won't build until you reinstall it.",
      ],
      [
        "Also permanently deletes tokei from the Cargo folder.",
        "After you reinstall Rust, cargo install can put back the ones it installed.",
      ],
      [
        "~/.zprofile has a line that mentions Cargo, which rustup won't remove.",
        "Check that line after uninstalling; if your terminal shows an error when it starts, delete it.",
      ],
      [
        "This can't be cancelled once it starts. Don't quit Banager or shut down your Mac until it finishes.",
        "Wait until the bottom of the window shows the result before you quit or shut down.",
      ],
      // Its kept line's ⓘ is named by its path (`KeptDataGroup`).
      ["~/.gemini/antigravity-cli", "Keeps the whole folder, including the program files in it."],
    ];
    for (const [line, why] of whys) {
      expect(screen.queryByText(why)).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: `Details: ${line}` }));
      expect(screen.getByText(why)).toBeInTheDocument();
    }
    // A line that says it all has no ⓘ.
    for (const line of [
      "Permanently deletes ~/.cargo and everything in it, including Cargo's cache, settings and saved login. Nothing goes to the Trash.",
      "Homebrew's rustup shares these folders, so its toolchains go too.",
    ]) {
      expect(screen.queryByRole("button", { name: `Details: ${line}` })).toBeNull();
    }
  });

  it("says as its text, under the question and not behind an ⓘ, what the uninstall removes and what it leaves", async () => {
    // `Warning::UninstallScope`: one sentence per source, true for the
    // exact command the plan runs (crates/banager-core/src/model.rs).
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: [{ UninstallScope: { what: "HomebrewFormulaOnly" } }] }),
    );

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    // `brew uninstall` deletes the keg in place: the paragraph ends saying
    // none of it is in the Trash afterwards.
    const sentence = await screen.findByText(
      "Removes only the Homebrew version of jq and the links to it. Settings and data stored elsewhere are kept. Removed files don't go to the Trash.",
    );
    // The alert's own text, in the label colour, under the question that
    // names the tool.
    expect(sentence).toHaveAttribute("data-sheet-text");
    expect(sentence.className).toMatch(/\btext-foreground\b/);
    expect(screen.getByRole("alertdialog", { name: "Uninstall “jq”?" })).toContainElement(sentence);
    // No line says it deletes for good: no "can't undo", and plain Uninstall.
    expect(screen.queryByText(/can't undo/)).toBeNull();
    // Not a note: with nothing else to say there is no "Before you
    // continue", and nothing is behind an ⓘ.
    expect(screen.queryByRole("region", { name: "Notes" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Details:/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("shows running-app notes beside a plain cask's retained-settings scope", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ warnings: [
      { UninstallScope: { what: "HomebrewCaskPlain" } },
      { CaskUninstallStep: { step: "QuitsNamedApps", items: ["Zed"] } },
      { CaskUninstallStep: { step: "SignalsApps", items: ["org.jkiss.dbeaver.core.product"] } },
    ] }));
    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={{ ...request, artifact_kind: "Cask", name: "zed" }} displayName="Zed" />);
    expect(await screen.findByText(/Deletes what Homebrew installed for Zed; its settings and data stay/)).toBeVisible();
    expect(screen.getByText("Also quits Zed if it is running.")).toBeVisible();
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Also force-quits an app if it is running. Unsaved work may be lost.");
    expect(screen.getByText(/Also force-quits an app if it is running/)).toBeVisible();
  });

  it("says each source's sentence with the name the row has, in either language, and where it holds, that nothing goes to the Trash", async () => {
    // Each of these runs the source's own uninstall command, which deletes
    // in place (`skipsTrash` in src/lib/warnings.ts): the paragraph ends
    // saying so, where the sentence can -- not beside a step Banager cannot
    // see into, nor for a record it could not read.
    const EN_SKIPS = " Removed files don't go to the Trash.";
    const ZH_SKIPS = "删除的文件不会进入废纸篓。";
    const cases: [Plan["warnings"][number], string, string, string][] = [
      // With a brew.env that brings the autoremove back, whose own line
      // says what else goes -- deleted the same way.
      [
        { UninstallScope: { what: "HomebrewFormula" } },
        "jq",
        "Removes the Homebrew version of jq and the links to it. Settings and data stored elsewhere are kept." +
          EN_SKIPS,
        "删除Homebrew安装的这一版“jq”和指向它的链接；别处的配置和数据不删除。" + ZH_SKIPS,
      ],
      // `brew uninstall --cask` copies the app back into the Caskroom and
      // deletes both: not to the Trash either.
      [
        { UninstallScope: { what: "HomebrewCaskPlain" } },
        "Claudebar",
        "Deletes what Homebrew installed for Claudebar; its settings and data stay." + EN_SKIPS,
        "删除Homebrew为“Claudebar”安装的文件；它的设置和数据保留不动。" + ZH_SKIPS,
      ],
      // Placed files and recorded steps: not every file an installer put
      // down, and nothing else while Homebrew's autoremove is off...
      [
        { UninstallScope: { what: "HomebrewCaskSteps" } },
        "Charles",
        "Deletes the files Homebrew placed for Charles and runs the uninstall steps it recorded; nothing else is deleted." +
          EN_SKIPS,
        "删除Homebrew为“Charles”放置的文件，并执行它记下的卸载步骤；其他文件不删除。" + ZH_SKIPS,
      ],
      // ...and, with a brew.env that brings the autoremove back, whose own
      // line says what else goes, only the cask's other files stay.
      [
        { UninstallScope: { what: "HomebrewCaskStepsAutoremoves" } },
        "Charles",
        "Deletes the files Homebrew placed for Charles and runs the uninstall steps it recorded; Charles's other files stay." +
          EN_SKIPS,
        "删除Homebrew为“Charles”放置的文件，并执行它记下的卸载步骤；“Charles”的其他文件不删除。" + ZH_SKIPS,
      ],
      // A step that runs a program or code -- Ruby around the uninstall,
      // here -- whose deletions Banager cannot see: nothing is said to
      // stay, and nothing about the Trash, where that program may put
      // something for all Banager knows.
      [
        { UninstallScope: { what: "HomebrewCaskStepsUnseen" } },
        "Uninstall Flight Block",
        "Deletes the files Homebrew placed for Uninstall Flight Block and runs the uninstall steps it recorded; what else some of those steps delete can't be seen in advance.",
        "删除Homebrew为“Uninstall Flight Block”放置的文件，并执行它记下的卸载步骤；其中部分步骤还会删除什么，无法事先得知。",
      ],
      // A cask whose record lists nothing Homebrew put down: an installer
      // put it on the Mac, and only the recorded steps take any of it away...
      [
        { UninstallScope: { what: "HomebrewCaskStepsOnly" } },
        "Little Snitch",
        "Runs the uninstall steps Homebrew recorded for Little Snitch; other files its installer put on this Mac stay." +
          EN_SKIPS,
        "执行Homebrew为“Little Snitch”记下的卸载步骤；安装器安装的其他文件不删除。" + ZH_SKIPS,
      ],
      // ...unless a step runs a program -- wireshark-chmodbpf's vendor
      // uninstaller -- whose deletions Banager cannot see.
      [
        { UninstallScope: { what: "HomebrewCaskStepsOnlyUnseen" } },
        "Wireshark-ChmodBPF",
        "Runs the uninstall steps Homebrew recorded for Wireshark-ChmodBPF; what else some of those steps delete can't be seen in advance.",
        "执行Homebrew为“Wireshark-ChmodBPF”记下的卸载步骤；其中部分步骤还会删除什么，无法事先得知。",
      ],
      // A record Banager could not read, or one that lists nothing to go
      // by -- an empty list, for one: no deletion claimed, and a `trash:`
      // step may be in it.
      [
        { UninstallScope: { what: "HomebrewCask" } },
        "Docker",
        "Couldn't tell from Homebrew's records what uninstalling Docker deletes.",
        "无法从Homebrew的记录中读出卸载“Docker”会删除什么。",
      ],
      [
        { UninstallScope: { what: "Npm" } },
        "typescript",
        "Deletes typescript's folder from npm's global folder, along with its commands. None of typescript's code is run, so its settings and data outside that folder stay." +
          EN_SKIPS,
        "删除npm全局目录中的“typescript”文件夹和命令，不运行它的代码；它在别处的设置和数据不删除。" + ZH_SKIPS,
      ],
      [
        { UninstallScope: { what: "Pipx" } },
        "httpie",
        "Deletes the Python environment pipx made just for httpie and the commands that point into it; its settings and data outside that environment are not deleted." +
          EN_SKIPS,
        "删除pipx为“httpie”单独建立的Python环境和指向该环境的命令；它在环境以外的设置和数据不删除。" + ZH_SKIPS,
      ],
      [
        { UninstallScope: { what: "Uv" } },
        "ruff",
        "Deletes the Python environment uv made just for ruff and the commands it recorded; its settings and data outside that environment are not deleted." +
          EN_SKIPS,
        "删除uv为“ruff”单独建立的Python环境和uv记下的命令；它在环境以外的设置和数据不删除。" + ZH_SKIPS,
      ],
      [
        { UninstallScope: { what: "Cargo" } },
        "tokei",
        "Deletes the program files cargo installed for tokei; the download cache, Cargo's settings and the program's own settings stay." +
          EN_SKIPS,
        "删除cargo为“tokei”安装的程序文件；下载缓存、Cargo的设置和此程序自己的设置都保留不动。" + ZH_SKIPS,
      ],
      [
        { UninstallScope: { what: "Ollama" } },
        "qwen3:8b",
        "Deletes the model qwen3:8b; data other models still use is kept, and Ollama itself and your other models stay." +
          EN_SKIPS,
        "删除模型“qwen3:8b”；其他模型还在用的数据会保留，Ollama本身和其他模型保留不动。" + ZH_SKIPS,
      ],
    ];
    for (const [warning, name, english, chinese] of cases) {
      vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ warnings: [warning] }));
      const en = renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={request} displayName={name} />,
      );
      // One paragraph, the whole of the alert's text.
      expect((await screen.findByText(english)).closest("[data-sheet-text]")).not.toBeNull();
      expect(document.querySelectorAll("[data-sheet-text]")).toHaveLength(1);
      // Nothing it says is deleted for good.
      expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
      en.unmount();

      await i18n.changeLanguage("zh-CN");
      try {
        const zh = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={request} displayName={name} />,
        );
        expect((await screen.findByText(chinese)).closest("[data-sheet-text]")).not.toBeNull();
        expect(document.querySelectorAll("[data-sheet-text]")).toHaveLength(1);
        zh.unmount();
      } finally {
        await i18n.changeLanguage("en");
      }
    }
  });

  it("says nothing of the Trash beside a cask step that moves paths there, and says it before can't undo where a step deletes for good", async () => {
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "nvs" };
    const planWith = (warnings: Warning[]) => issuedPlanFor({ request: cask, warnings });
    // nvs's `trash:` (cask_receipt.rs's fixture): its own line says what
    // goes there, so no sentence says nothing does.
    vi.mocked(invoke).mockResolvedValue(
      planWith([
        { UninstallScope: { what: "HomebrewCaskSteps" } },
        { CaskUninstallStep: { step: "Trashes", items: ["~/.nvs"] } },
      ]),
    );
    const trashed = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="nvs" />,
    );
    expect(
      await screen.findByText(
        "Deletes the files Homebrew placed for nvs and runs the uninstall steps it recorded; nothing else is deleted.",
      ),
    ).toHaveAttribute("data-sheet-text");
    expect(linesOf("Notes")).toEqual(["Also moves ~/.nvs to the Trash."]);
    expect(screen.queryByText(/don't go to the Trash/)).toBeNull();
    trashed.unmount();

    // A `delete:` step and nothing moved to the Trash: both, in the one
    // paragraph, as Finder's Delete Immediately alert says them.
    vi.mocked(invoke).mockResolvedValue(
      planWith([
        { UninstallScope: { what: "HomebrewCaskSteps" } },
        { CaskUninstallStep: { step: "Deletes", items: ["~/Library/Application Support/nvs"] } },
      ]),
    );
    const en = renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={cask} displayName="nvs" />);
    expect(await screen.findByRole("button", { name: "Uninstall Permanently" })).toBeEnabled();
    expect(
      screen.getByText(
        "Deletes the files Homebrew placed for nvs and runs the uninstall steps it recorded; nothing else is deleted. Removed files don't go to the Trash. You can't undo this action.",
      ),
    ).toHaveAttribute("data-sheet-text");
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={cask} displayName="nvs" />);
      expect(
        await screen.findByText(
          "删除Homebrew为“nvs”放置的文件，并执行它记下的卸载步骤；其他文件不删除。删除的文件不会进入废纸篓。此操作无法撤销。",
        ),
      ).toHaveAttribute("data-sheet-text");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("lists a cask's extra uninstall steps under Before you continue, one line per kind, counting what only an id names", async () => {
    // `Warning::CaskUninstallStep`s from the cask's install receipt
    // (crates/banager-core/src/adapters/brew/cask_receipt.rs), after the
    // sentence under the tool that says there are steps.
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "microsoft-word" };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: cask,
        action: {
          Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--cask", "microsoft-word"], env: [] },
        },
        needs_password: true,
        warnings: [
          // Word installs with a `pkg`, which its record leaves out; beside
          // a step Banager cannot see into, nothing is said to stay.
          { UninstallScope: { what: "HomebrewCaskStepsOnlyUnseen" } },
          {
            CaskUninstallStep: {
              step: "RemovesPackages",
              items: ["com.microsoft.package.Microsoft_Word.app", "com.microsoft.pkg.licensing"],
            },
          },
          { CaskUninstallStep: { step: "RemovesServices", items: ["com.microsoft.office.licensingV2.helper"] } },
          { CaskUninstallStep: { step: "RunsOwnSteps", items: [] } },
          { CaskUninstallStep: { step: "QuitsApps", items: ["com.microsoft.autoupdate2"] } },
        ],
      }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="Microsoft Word" />,
    );

    const sentence = await screen.findByText(
      "Runs the uninstall steps Homebrew recorded for Microsoft Word; what else some of those steps delete can't be seen in advance.",
    );
    expect(sentence.closest("[data-sheet-text]")).not.toBeNull();
    expect(linesOf("Notes")).toEqual([
      "Also deletes every file these installer packages put on this Mac, whether or not other apps use them: com.microsoft.package.Microsoft_Word.app, com.microsoft.pkg.licensing.",
      "Also stops and removes a background service.",
      "Before or after uninstalling, it also runs other steps Homebrew recorded for it.",
      "Also quits an app if it is running.",
      "You can't enter your Mac password here. If an app asks for it at this step, you'll see how to finish in Terminal.",
    ]);
    // What only a reverse-DNS id names is counted, with the id behind the
    // line's ⓘ.
    for (const [line, ids] of [
      ["Also stops and removes a background service.", "As macOS names it: com.microsoft.office.licensingV2.helper"],
      ["Also quits an app if it is running.", "As macOS names it: com.microsoft.autoupdate2"],
    ]) {
      expect(screen.queryByText(ids)).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: `Details: ${line}` }));
      expect(screen.getByText(ids)).toBeInTheDocument();
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    }
    // Nothing it lists is deleted for good in so many words.
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("says nothing stays beside a cask step that runs a program, and still names the program under Before you continue, in either language", async () => {
    // wireshark-chmodbpf's record (a fixture of cask_receipt.rs): its
    // `early_script:` runs the vendor's uninstaller package with
    // `installer`, whose deletions Banager cannot see, then `pkgutil:`.
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "wireshark-chmodbpf" };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: cask,
        needs_password: true,
        warnings: [
          { UninstallScope: { what: "HomebrewCaskStepsOnlyUnseen" } },
          { CaskUninstallStep: { step: "RemovesPackages", items: ["org.wireshark.ChmodBPF.pkg"] } },
          { CaskUninstallStep: { step: "RunsScript", items: ["/usr/sbin/installer"] } },
        ],
      }),
    );

    const en = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="Wireshark-ChmodBPF" />,
    );
    const sentence = await screen.findByText(
      "Runs the uninstall steps Homebrew recorded for Wireshark-ChmodBPF; what else some of those steps delete can't be seen in advance.",
    );
    expect(sentence.closest("[data-sheet-text]")).not.toBeNull();
    expect(linesOf("Notes")).toEqual([
      "Also deletes every file the installer package org.wireshark.ChmodBPF.pkg put on this Mac, whether or not other apps use them.",
      "Also runs /usr/sbin/installer.",
      "You can't enter your Mac password here. If an app asks for it at this step, you'll see how to finish in Terminal.",
    ]);
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="Wireshark-ChmodBPF" />,
      );
      const chinese = await screen.findByText(
        "执行Homebrew为“Wireshark-ChmodBPF”记下的卸载步骤；其中部分步骤还会删除什么，无法事先得知。",
      );
      expect(chinese.closest("[data-sheet-text]")).not.toBeNull();
      expect(linesOf("说明")).toEqual([
        "还会删除下列安装包安装的全部文件，不论其他App是否在用：org.wireshark.ChmodBPF.pkg。",
        "还会运行：/usr/sbin/installer。",
        "部分App在这一步会要求输入Mac密码，这里无法输入；需要时会告诉你在终端里怎么完成。",
      ]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("names the app a cask quits, and counts its background service, in Chinese with no 这些", async () => {
    // Visual Studio Code: 「还会停止并删除这些后台服务：com.microsoft.VSCode.ShipIt。」
    // and 「还会退出正在运行的这些 App：com.microsoft.VSCode。」 named by
    // reverse-DNS ids, with 这些 for one item, in the most destructive
    // sheet an app's row opens. The app is named as Finder names it, found
    // on the Mac (`BrewAdapter::quit_app_names`).
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "visual-studio-code" };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: cask,
        action: {
          Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--cask", "visual-studio-code"], env: [] },
        },
        needs_password: true,
        warnings: [
          { UninstallScope: { what: "HomebrewCaskSteps" } },
          { CaskUninstallStep: { step: "RemovesServices", items: ["com.microsoft.VSCode.ShipIt"] } },
          { CaskUninstallStep: { step: "QuitsNamedApps", items: ["Visual Studio Code"] } },
        ],
      }),
    );
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <UninstallDialog
          open
          onOpenChange={() => {}}
          request={cask}
          displayName="Microsoft Visual Studio Code"
        />,
      );

      await screen.findByRole("button", { name: "卸载" });
      expect(linesOf("说明")).toEqual([
        "还会停止并删除1个后台服务。",
        "还会退出正在运行的Visual Studio Code。",
        "部分App在这一步会要求输入Mac密码，这里无法输入；需要时会告诉你在终端里怎么完成。",
      ]);
      fireEvent.click(screen.getByRole("button", { name: "详情：还会停止并删除1个后台服务。" }));
      expect(screen.getByText("在macOS中的名称：com.microsoft.VSCode.ShipIt")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "详情：还会退出正在运行的Visual Studio Code。" })).toBeNull();
      expect(JSON.stringify(zhCN.warnings.caskStep)).not.toContain("这些");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("gives no number of background services where a pattern matches some, in either language", async () => {
    // adobe-creative-cloud's recorded `launchctl:` (cask_receipt.rs's
    // fixture): six names and `com.adobe.CCXProcess.*`, which Homebrew
    // matches against every running service. 「还会停止并删除 7 个后台服务。」
    // counted the pattern as one service.
    const services = [
      "Adobe_Genuine_Software_Integrity_Service",
      "com.adobe.acc.installer",
      "com.adobe.acc.installer.v2",
      "com.adobe.AdobeCreativeCloud",
      "com.adobe.AdobeDesktopService",
      "com.adobe.ccxprocess",
      "com.adobe.CCXProcess.*",
    ];
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "adobe-creative-cloud" };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: cask,
        action: {
          Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--cask", "adobe-creative-cloud"], env: [] },
        },
        needs_password: true,
        warnings: [
          { UninstallScope: { what: "HomebrewCaskSteps" } },
          { CaskUninstallStep: { step: "RemovesServices", items: services } },
        ],
      }),
    );

    const english = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="Adobe Creative Cloud" />,
    );
    await screen.findByRole("button", { name: "Uninstall" });
    const line = "Also stops and removes background services, including every running one whose name matches a pattern.";
    expect(linesOf("Notes")).toEqual([line, "You can't enter your Mac password here. If an app asks for it at this step, you'll see how to finish in Terminal."]);
    fireEvent.click(screen.getByRole("button", { name: `Details: ${line}` }));
    expect(screen.getByText(`As macOS names them: ${services.join(", ")}`)).toBeInTheDocument();
    english.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="Adobe Creative Cloud" />,
      );
      await screen.findByRole("button", { name: "卸载" });
      const chinese = "还会停止并删除后台服务，包括名称符合某个规则、正在运行的全部服务。";
      expect(linesOf("说明")).toEqual([chinese, "部分App在这一步会要求输入Mac密码，这里无法输入；需要时会告诉你在终端里怎么完成。"]);
      fireEvent.click(screen.getByRole("button", { name: `详情：${chinese}` }));
      expect(screen.getByText(`在macOS中的名称：${services.join("、")}`)).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says Uninstall permanently where a cask's recorded steps delete paths, and says them in Chinese too", async () => {
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "duckietv" };
    const plan = issuedPlanFor({
      request: cask,
      needs_password: true,
      warnings: [
        { UninstallScope: { what: "HomebrewCaskStepsOnly" } },
        {
          CaskUninstallStep: {
            step: "Deletes",
            items: ["/Applications/duckieTV.app", "~/Library/Application Support/DuckieTV-Standalone"],
          },
        },
        { CaskUninstallStep: { step: "Trashes", items: ["~/.nvs"] } },
      ],
    });
    vi.mocked(invoke).mockResolvedValue(plan);

    const en = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="DuckieTV" />,
    );
    expect(await screen.findByRole("button", { name: "Uninstall Permanently" })).toBeEnabled();
    // The text ends saying so, as an alert about an action that can't be
    // undone does (spec §3.6, 4.1-7).
    expect(
      screen.getByText(
        "Runs the uninstall steps Homebrew recorded for DuckieTV; other files its installer put on this Mac stay. You can't undo this action.",
      ),
    ).toHaveAttribute("data-sheet-text");
    expect(linesOf("Notes").slice(0, 2)).toEqual([
      "Also permanently deletes these: /Applications/duckieTV.app, ~/Library/Application Support/DuckieTV-Standalone.",
      "Also moves ~/.nvs to the Trash.",
    ]);
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={cask} displayName="DuckieTV" />);
      expect(await screen.findByRole("button", { name: "永久卸载" })).toBeEnabled();
      expect(
        await screen.findByText("执行Homebrew为“DuckieTV”记下的卸载步骤；安装器安装的其他文件不删除。此操作无法撤销。"),
      ).toHaveAttribute("data-sheet-text");
      expect(linesOf("说明").slice(0, 2)).toEqual([
        "还会永久删除：/Applications/duckieTV.app、~/Library/Application Support/DuckieTV-Standalone。",
        "还会移到废纸篓：~/.nvs。",
      ]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says Uninstall permanently where a cask's recorded steps delete files they find only as they run", async () => {
    // mailtrackerblocker's `remove` of the script in its staged folder,
    // with no check: `CaskStep::DeletesUnnamed`
    // (crates/banager-core/src/adapters/brew/cask_receipt.rs).
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "mailtrackerblocker" };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: cask,
        needs_password: true,
        warnings: [
          { UninstallScope: { what: "HomebrewCaskStepsOnly" } },
          { CaskUninstallStep: { step: "DeletesUnnamed", items: [] } },
        ],
      }),
    );

    const en = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="MailTrackerBlocker" />,
    );
    expect(await screen.findByRole("button", { name: "Uninstall Permanently" })).toBeEnabled();
    expect(linesOf("Notes")[0]).toBe(
      "Also permanently deletes files Homebrew finds only as it runs the uninstall steps.",
    );
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="MailTrackerBlocker" />,
      );
      expect(await screen.findByRole("button", { name: "永久卸载" })).toBeEnabled();
      expect(linesOf("说明")[0]).toBe("还会永久删除Homebrew执行卸载步骤时才找到的文件。");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says the check a cask's remove step makes before it deletes a path, in either language, and Uninstall permanently", async () => {
    // `symlink_target_contains` and `content_contains`
    // (install_steps.rb:1051-1060 in Homebrew 7.0.6-70): the step deletes
    // only the paths that pass, so the line says which -- and the button
    // still says permanently, since one can go for good.
    const cases: [string, string, Warning[], string[], string[]][] = [
      [
        // playdate-simulator: `delete:` with no check, and a `remove` of
        // its links whose target contains `playdate`.
        "playdate-simulator",
        "Playdate Simulator",
        [
          { UninstallScope: { what: "HomebrewCaskStepsOnly" } },
          { CaskUninstallStep: { step: "Deletes", items: ["/usr/local/playdate"] } },
          {
            CaskUninstallStep: {
              step: "Deletes",
              items: ["/usr/local/bin/arm-*"],
              only_if: { LinkTargetContains: "playdate" },
            },
          },
          { CaskUninstallStep: { step: "Trashes", items: ["~/Developer/PlaydateSDK"] } },
        ],
        [
          "Also permanently deletes /usr/local/playdate.",
          "Also permanently deletes /usr/local/bin/arm-*, but only where it is a link whose target contains “playdate”.",
          "Also moves ~/Developer/PlaydateSDK to the Trash.",
        ],
        [
          "还会永久删除：/usr/local/playdate。",
          "还会永久删除下列路径，但只删除其中指向的路径含有“playdate”的链接：/usr/local/bin/arm-*。",
          "还会移到废纸篓：~/Developer/PlaydateSDK。",
        ],
      ],
      [
        // gpg-suite: three links, each only where its target contains
        // `MacGPG2`.
        "gpg-suite",
        "GPG Suite",
        [
          { UninstallScope: { what: "HomebrewCaskStepsOnly" } },
          {
            CaskUninstallStep: {
              step: "Deletes",
              items: ["/usr/local/bin/gpg", "/usr/local/bin/gpg2", "/usr/local/bin/gpg-agent"],
              only_if: { LinkTargetContains: "MacGPG2" },
            },
          },
        ],
        [
          "Also permanently deletes these, but only where they are links whose target contains “MacGPG2”: /usr/local/bin/gpg, /usr/local/bin/gpg2, /usr/local/bin/gpg-agent.",
        ],
        [
          "还会永久删除下列路径，但只删除其中指向的路径含有“MacGPG2”的链接：/usr/local/bin/gpg、/usr/local/bin/gpg2、/usr/local/bin/gpg-agent。",
        ],
      ],
      [
        // pycharm-edu: `charm` in each folder Homebrew looks for commands
        // in, only where it is a file whose contents hold that line.
        "pycharm-edu",
        "PyCharm Edu",
        [
          { UninstallScope: { what: "HomebrewCaskSteps" } },
          {
            CaskUninstallStep: {
              step: "DeletesUnnamed",
              items: [],
              only_if: { ContentContains: "# see com.intellij.idea.SocketLock for the server side of this interface" },
            },
          },
        ],
        [
          "Also permanently deletes files Homebrew finds only as it runs the uninstall steps, but only those whose contents contain “# see com.intellij.idea.SocketLock for the server side of this interface”.",
        ],
        [
          "还会永久删除Homebrew执行卸载步骤时才找到的文件，但只删除内容含有“# see com.intellij.idea.SocketLock for the server side of this interface”的。",
        ],
      ],
    ];
    for (const [name, displayName, warnings, english, chinese] of cases) {
      const cask: OpRequest = { ...request, artifact_kind: "Cask", name };
      vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ request: cask, warnings }));
      const en = renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={cask} displayName={displayName} />,
      );
      expect(await screen.findByRole("button", { name: "Uninstall Permanently" })).toBeEnabled();
      expect(linesOf("Notes")).toEqual(english);
      en.unmount();

      await i18n.changeLanguage("zh-CN");
      try {
        const zh = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={cask} displayName={displayName} />,
        );
        expect(await screen.findByRole("button", { name: "永久卸载" })).toBeEnabled();
        expect(linesOf("说明")).toEqual(chinese);
        zh.unmount();
      } finally {
        await i18n.changeLanguage("en");
      }
    }
  });

  it("says a cask's certificate step deletes every certificate whose name contains the text, in either language", async () => {
    // `security find-certificate -a -c NAME` lists every certificate whose
    // name contains NAME, and Homebrew deletes each (install_steps.rb in
    // Homebrew 7.0.6-70): autofirma's steps take every `127.0.0.1` one.
    const cask: OpRequest = { ...request, artifact_kind: "Cask", name: "autofirma" };
    const planWith = (items: string[]) =>
      issuedPlanFor({
        request: cask,
        warnings: [
          { UninstallScope: { what: "HomebrewCaskSteps" } },
          { CaskUninstallStep: { step: "DeletesCertificates", items } },
        ],
      });
    const cases: [string[], string, string][] = [
      [
        ["Charles"],
        "Also deletes every certificate in the keychain whose name contains Charles.",
        "还会删除钥匙串中名称含有下列任一文字的所有证书：Charles。",
      ],
      [
        ["AutoFirma ROOT", "127.0.0.1"],
        "Also deletes every certificate in the keychain whose name contains any of these: AutoFirma ROOT, 127.0.0.1.",
        "还会删除钥匙串中名称含有下列任一文字的所有证书：AutoFirma ROOT、127.0.0.1。",
      ],
    ];
    for (const [items, english, chinese] of cases) {
      vi.mocked(invoke).mockResolvedValue(planWith(items));
      const en = renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="AutoFirma" />,
      );
      await screen.findByRole("button", { name: "Uninstall" });
      expect(linesOf("Notes")).toEqual([english]);
      en.unmount();

      await i18n.changeLanguage("zh-CN");
      try {
        const zh = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={cask} displayName="AutoFirma" />,
        );
        await screen.findByRole("button", { name: "卸载" });
        expect(linesOf("说明")).toEqual([chinese]);
        zh.unmount();
      } finally {
        await i18n.changeLanguage("en");
      }
    }
  });

  it("says Homebrew will also remove what nothing needs when a brew.env turns autoremove back on, with the why behind its ⓘ", async () => {
    // `Warning::HomebrewAutoremoves`: every brew command runs with
    // HOMEBREW_NO_AUTOREMOVE=1, and a brew.env took it back
    // (crates/banager-core/src/adapters/brew/brew_env.rs), so after this
    // uninstall Homebrew removes more than the command names.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ warnings: ["HomebrewAutoremoves"] }));

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    await screen.findByRole("region", { name: "Notes" });
    const line =
      "Homebrew will also remove other Homebrew packages that were installed only as dependencies and that nothing needs anymore.";
    expect(linesOf("Notes")).toEqual([line]);
    const why =
      "Homebrew is run with HOMEBREW_NO_AUTOREMOVE=1, but your brew.env sets it to a value Homebrew reads as unset, such as 0 or false, and brew.env wins.";
    expect(screen.queryByText(why)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: `Details: ${line}` }));
    expect(screen.getByText(why)).toBeInTheDocument();
    // Said, not blocked: the button stays, and says nothing of the Trash.
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("says Uninstall permanently where a line says something is deleted for good, and Uninstall everywhere else", async () => {
    // rustup's own uninstall deletes its folders outright; a path-list
    // uninstall moves what it lists to the Trash, and a Homebrew one's
    // button says nothing about the Trash either way -- its text does,
    // where its sentence holds it (`skipsTrash`).
    const rustup = issuedPlanFor({
      action: { Command: { program: "/Users/someone/.cargo/bin/rustup", args: ["self", "uninstall", "-y"], env: [] } },
      cancel_policy: "NoCancel",
      warnings: [
        { RemovesToolchains: { path: "~/.rustup", names: [] } },
        { DeletesCargoHome: { path: "~/.cargo" } },
        "EditsShellConfig",
      ],
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return rustup;
      if (cmd === "submit_operation") return 7;
      return undefined;
    });
    const onSubmitted = vi.fn();
    const first = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" onSubmitted={onSubmitted} />,
    );

    const permanent = await screen.findByRole("button", { name: "Uninstall Permanently" });
    expect(screen.queryByRole("button", { name: "Uninstall" })).toBeNull();
    // rustup has no source sentence: its lines say "permanently", and its
    // text only that this can't be undone.
    expect(screen.getByText("You can't undo this action.")).toHaveAttribute("data-sheet-text");
    expect(screen.queryByText(/don't go to the Trash/)).toBeNull();
    fireEvent.click(permanent);
    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    first.unmount();

    for (const plan of [
      issuedPlanFor(),
      issuedPlanFor({
        action: { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } },
        warnings: [{ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } }],
      }),
    ]) {
      vi.mocked(invoke).mockResolvedValue(plan);
      const { unmount } = renderWithProviders(
        <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
      );
      await waitFor(() => expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled());
      expect(screen.queryByRole("button", { name: "Uninstall Permanently" })).toBeNull();
      unmount();
    }
  });

  it("renders a warning variant the mirror lacks as its raw key rather than dropping it", async () => {
    // `warningKey` is exhaustive over `Warning`, so this value cannot be
    // written without the cast: it stands for a Rust variant the
    // TypeScript mirror has not caught up with (`types.test.ts` pins the
    // spellings; this is what happens if that check is ever wrong). At
    // runtime it reaches `warningKey`'s `never` default and comes back as
    // the key itself, which i18next hands back unchanged. Showing that is
    // the honest failure: a warning is about something the command is
    // about to do, and a silently shorter list would hide it.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: ["SomeFutureVariant" as unknown as Plan["warnings"][number]] }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await screen.findByRole("region", { name: "Notes" });
    expect(linesOf("Notes")).toEqual(["SomeFutureVariant"]);
  });

  it("keeps the command one press away, and open from the start with technical details on", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());
    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const disclosure = await screen.findByRole("button", { name: "Show Command" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(command(JQ_COMMAND))).toBeNull();
    fireEvent.click(disclosure);
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    unmount();

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return {
          language: "System",
          show_technical_details: true,
          ignored_updates: [],
          skipped_versions: [],
          include_self_updating: false,
          auto_check: false,
          notify_updates: false,
        };
      }
      if (cmd === "plan_operation") return issuedPlanFor();
      return undefined;
    });
    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    expect(await screen.findByText(command(JQ_COMMAND))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show Command" })).toHaveAttribute("aria-expanded", "true");
  });

  it("submits the plan id and reports the new op id when nothing would break", async () => {
    const issued = issuedPlanFor();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued;
      if (cmd === "submit_operation") return 7;
      return undefined;
    });
    const onSubmitted = vi.fn();
    const onOpenChange = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    // The exact command is a press away, and nothing has been submitted,
    // before the user is allowed to confirm.
    await showCommand();
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    expect(submitCalls()).toHaveLength(0);
    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("submit_operation", { planId: "1" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("submits nothing when the dialog is cancelled", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());
    const onOpenChange = vi.fn();

    renderWithProviders(
      <UninstallDialog open onOpenChange={onOpenChange} request={request} displayName="jq" />,
    );

    await showCommand();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(submitCalls()).toHaveLength(0);
  });

  it("localises an expired plan, re-plans, and submits the fresh id only when confirmed again", async () => {
    // The dialog sat open past the PlanId's 10-minute lifetime: the backend
    // rejects with `{"kind":"expired"}` (see `submit_operation_error` in
    // src-tauri/src/ipc.rs), which `planErrorMessage` turns into
    // `planRefused.expired` rather than showing the JSON or this project's
    // own hardcoded English. The stale preview must not be resubmittable
    // with the same id.
    let planCalls = 0;
    let submitAttempts = 0;
    let releaseReplan: () => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") {
        planCalls += 1;
        const fresh = { ...issuedPlanFor(), id: String(planCalls) };
        if (planCalls === 1) return fresh;
        // The re-plan is held open until the test releases it: the expired
        // message is shown while the dialog re-checks and goes with the
        // fresh preview, and a mocked re-plan that resolved in microtasks
        // would land before React ever rendered it.
        return new Promise<IssuedPlan>((resolve) => {
          releaseReplan = () => resolve(fresh);
        });
      }
      if (cmd === "submit_operation") {
        submitAttempts += 1;
        if (submitAttempts === 1) {
          throw '{"kind":"expired"}';
        }
        return 7;
      }
      return undefined;
    });
    const onSubmitted = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    );
    await waitFor(() => expect(planCalls).toBe(2));
    releaseReplan();
    // Confirm re-enables only once the fresh plan has arrived; its preview is
    // on screen, but the fresh id has not been sent — the dead id is still
    // the only submit so far, and nothing was reported as started.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    await showCommand();
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: "1" }]);
    expect(onSubmitted).not.toHaveBeenCalled();

    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: "1" }, { planId: "2" }]);
  });

  it("submits once when Uninstall is clicked twice before the first click is acknowledged", async () => {
    // Two clicks in one event-loop turn. `submitMutation.isPending`, which is
    // what disables the button, reaches React only through TanStack's
    // setTimeout(0) notify, so the second click still finds an enabled
    // button. The backend's single-use plan id would reject a second submit
    // with `unknown`, which this dialog answers with an error and a re-plan
    // -- for a user who did nothing wrong. So the second click must not
    // reach `mutate()` at all.
    let submitAttempts = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issuedPlanFor();
      if (cmd === "submit_operation") {
        submitAttempts += 1;
        if (submitAttempts > 1) throw '{"kind":"unknown"}';
        return 7;
      }
      return undefined;
    });
    const onSubmitted = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(onSubmitted).toHaveBeenCalledTimes(1);
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: "1" }]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("drops the submit error once the fresh preview arrives, and says why the preview is new", async () => {
    // The re-plan is held open so each state is caught in turn. While the
    // dialog re-checks, the submit error says why; once the fresh preview
    // is on screen that error would read as "still broken" next to an
    // enabled Uninstall, so it goes, and a note beside the preview says the
    // previous confirm started nothing. The note goes when the user acts on
    // the fresh preview.
    let planCalls = 0;
    let releaseReplan: () => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") {
        planCalls += 1;
        const fresh = { ...issuedPlanFor(), id: String(planCalls) };
        if (planCalls === 1) return fresh;
        return new Promise<IssuedPlan>((resolve) => {
          releaseReplan = () => resolve(fresh);
        });
      }
      if (cmd === "submit_operation") throw '{"kind":"expired"}';
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    // The note's status, there empty before there is anything to say.
    const emptyStatuses = screen.getAllByRole("status").filter((status) => status.textContent === "");
    expect(emptyStatuses).toHaveLength(1);
    const noteStatus = emptyStatuses[0];
    fireEvent.click(confirmButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Couldn't start the uninstall: This confirmation is more than 10 minutes old",
    );
    expect(screen.getByText("Checking what this affects…")).toBeInTheDocument();
    // The one status is that it is checking again: nothing yet about a
    // fresh preview (exactly one says anything).
    expect(saying()).toHaveLength(1);
    expect(saying()[0]).toHaveTextContent(/^Checking what this affects…$/);
    await waitFor(() => expect(planCalls).toBe(2));

    releaseReplan();

    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    await showCommand();
    expect(screen.getByText(command(JQ_COMMAND))).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(saying()).toHaveLength(1);
    expect(saying()[0]).toHaveTextContent("That didn't start, so it was checked again. Confirm once more.");
    // The same node as before, its words changed rather than a new status
    // put in with them: what a screen reader reads out.
    expect(saying()[0]).toBe(noteStatus);

    // Acting on the fresh preview retires the note; the (again expired)
    // submit's own error takes its place while the next re-check runs.
    fireEvent.click(confirmButton);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't start the uninstall:");
    expect(screen.queryByText(/Confirm once more/)).not.toBeInTheDocument();
    // What status there is is the re-check under way.
    for (const status of saying()) expect(status).toHaveTextContent(/^Checking what this affects…$/);
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: "1" }, { planId: "2" }]);
  });

  it("does not ask for another confirm when the fresh preview blocks it", async () => {
    // Between the first preview and the re-check another installed package
    // came to depend on this one: the fresh preview disables Uninstall, so
    // the note must not tell the user to confirm again.
    let planCalls = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") {
        planCalls += 1;
        return planCalls === 1
          ? issuedPlanFor()
          : { ...issuedPlanFor({ affected: ["jq-cli-wrapper"] }), id: "2" };
      }
      if (cmd === "submit_operation") throw '{"kind":"expired"}';
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    expect(await screen.findByText("jq-cli-wrapper")).toBeInTheDocument();
    expect(confirmButton).toBeDisabled();
    await screen.findByText(/That didn't start, so it was checked again\./);
    const [note] = saying();
    expect(note).toHaveTextContent("That didn't start, so it was checked again.");
    expect(note).not.toHaveTextContent("Confirm once more");
  });

  it("keeps only the plan error when the re-issued plan fails as well", async () => {
    // Homebrew stopped answering between preview and confirm: submit is
    // refused, the re-plan is refused for the same reason. The re-plan's
    // error is the current truth and the one that explains the missing
    // preview and the disabled button; the submit's is about a state that
    // has passed, and a second red paragraph saying nearly the same thing
    // is noise. No note either: there is no fresh preview to explain.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return snapshotWith([brewInstance({ status: { unavailable: "NotResponding", notes: [] } })]);
      }
      if (cmd === "plan_operation") {
        if (submitCalls().length === 0) return issuedPlanFor();
        throw '{"kind":"not_actionable","read_only":null,"unavailable":"NotResponding"}';
      }
      if (cmd === "submit_operation") {
        throw '{"kind":"not_actionable","read_only":null,"unavailable":"NotResponding"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    await screen.findByText(/Couldn't check what this affects: Homebrew didn't respond/);
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.queryByText(/Couldn't start the uninstall/)).not.toBeInTheDocument();
    expect(saying()).toHaveLength(0);
    expect(confirmButton).toBeDisabled();
  });

  it("localises a stale-snapshot NotActionable refusal instead of showing the backend's JSON", async () => {
    // This is the app's only destructive confirmation screen (spec §6),
    // and `Session::issue_plan`'s actionability gate (spec §2.5) is the one
    // refusal that can reach it verbatim through a stale snapshot or a
    // genuine TOCTOU. `plan_operation_error` (src-tauri/src/ipc.rs) puts
    // it on the wire as JSON, not English -- it must never show up raw.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return snapshotWith([brewInstance({ status: { unavailable: "NotRunning", notes: [] } })]);
      }
      if (cmd === "plan_operation") {
        throw '{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Couldn't check what this affects: Open Homebrew to see what it has and check for updates.",
      ),
    );
    expect(alert.textContent).not.toMatch(/not_actionable/);
  });

  describe("the backend's own words", () => {
    // Settings with "Show technical details" on or off, for `get_settings`.
    const settingsWith = (technical: boolean) => ({
      language: "System",
      show_technical_details: technical,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
    });

    it("says the check failed and what to do, and the backend's words only with technical details on", async () => {
      for (const technical of [false, true]) {
        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
          if (cmd === "get_settings") return settingsWith(technical);
          if (cmd === "plan_operation") throw "brew: command exploded";
          return undefined;
        });
        const { unmount } = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
        );
        const alert = await screen.findByRole("alert");
        if (technical) {
          await waitFor(() =>
            expect(alert).toHaveTextContent("Couldn't check what this affects: brew: command exploded"),
          );
        } else {
          expect(alert).toHaveTextContent("Couldn't check what this affects. Try again later.");
          expect(screen.queryByText(/command exploded/)).toBeNull();
        }
        unmount();
      }
    });

    it("says a tool could not start, and macOS's reason only with technical details on", async () => {
      for (const technical of [false, true]) {
        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
          if (cmd === "get_settings") return settingsWith(technical);
          if (cmd === "plan_operation") throw '{"kind":"spawn_failed","detail":"Operation not permitted (os error 1)"}';
          return undefined;
        });
        const { unmount } = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
        );
        const alert = await screen.findByRole("alert");
        if (technical) {
          await waitFor(() =>
            expect(alert).toHaveTextContent(
              "Couldn't check what this affects: Homebrew couldn't start. macOS said: Operation not permitted (os error 1)",
            ),
          );
        } else {
          expect(alert).toHaveTextContent("Couldn't check what this affects: Homebrew couldn't start.");
          expect(screen.queryByText(/os error 1/)).toBeNull();
        }
        unmount();
      }
    });

    it("says the uninstall did not start while it checks again, and the backend's words only with technical details on", async () => {
      for (const technical of [false, true]) {
        let plans = 0;
        vi.mocked(invoke).mockImplementation(async (cmd: string) => {
          if (cmd === "get_settings") return settingsWith(technical);
          if (cmd === "plan_operation") {
            plans += 1;
            // The re-plan after the failed submit is held, so the submit's
            // refusal stays up.
            return plans === 1 ? issuedPlanFor() : new Promise(() => {});
          }
          if (cmd === "submit_operation") throw "operation queue is closed";
          return undefined;
        });
        const { unmount } = renderWithProviders(
          <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
        );
        const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
        await waitFor(() => expect(confirmButton).not.toBeDisabled());
        fireEvent.click(confirmButton);
        const alert = await screen.findByRole("alert");
        if (technical) {
          expect(alert).toHaveTextContent("Couldn't start the uninstall: operation queue is closed");
        } else {
          // Nothing to do but what the dialog is doing: checking again.
          expect(alert).toHaveTextContent(/^Couldn't start the uninstall\.$/);
          expect(screen.queryByText(/queue is closed/)).toBeNull();
        }
        expect(screen.getByText("Checking what this affects…")).toBeInTheDocument();
        unmount();
      }
    });
  });

  it("says a pinned package was not uninstalled and gives the unpin command as code", async () => {
    // A stale Installed page can still offer Uninstall on a package pinned
    // since the last refresh; `Session::issue_plan` refuses it
    // (`blocked_uninstall` in crates/banager-core/src/session/plans.rs)
    // and `uninstall_blocked_json` in src-tauri/src/ipc.rs sends this.
    // It is not "couldn't check what this affects": Banager did.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return snapshotWith([
          brewInstance({ id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local", version: "7.0.6" }),
        ]);
      }
      if (cmd === "plan_operation") {
        throw '{"kind":"uninstall_blocked","reason":"Pinned"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={{ ...request, instance_id: "brew:/usr/local", artifact_kind: "Cask", name: "onyx" }}
        displayName="OnyX"
      />,
    );

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Couldn't uninstall it because it's pinned in Homebrew. Run /usr/local/bin/brew unpin --cask onyx in Terminal to unpin it first.",
      ),
    );
    // The owning brew's own path, not whichever `brew` Terminal finds.
    expect(screen.getByText("/usr/local/bin/brew unpin --cask onyx").tagName).toBe("CODE");
    expect(alert.textContent).not.toMatch(/uninstall_blocked|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("says why a uv tool was not uninstalled while UV_TOOL_DIR is set, with no command to copy", async () => {
    // A stale Installed page can still offer Uninstall on a uv tool after
    // UV_TOOL_DIR was set; `Session::issue_plan` (or uv's own plan)
    // refuses it and `uninstall_blocked_json` in src-tauri/src/ipc.rs
    // sends this.
    const uv = brewInstance({ id: "uv", adapter_id: "uv", exe_path: "/opt/homebrew/bin/uv", prefix: "/opt/homebrew/bin" });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshotWith([uv]);
      if (cmd === "plan_operation") throw '{"kind":"uninstall_blocked","reason":"UvToolDirSet"}';
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={{ kind: "Uninstall", instance_id: "uv", artifact_kind: "Tool", name: "ruff" }}
        displayName="ruff"
      />,
    );

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Couldn't uninstall because UV_TOOL_DIR is set.",
      ),
    );
    expect(alert.querySelector("code")).toBeNull();
    expect(alert.textContent).not.toMatch(/uninstall_blocked|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("words a refused path-list preview with the path and the reason, never the payload, and keeps the why behind its ⓘ", async () => {
    // One of the checks a path-list uninstall runs at preview time refused
    // a path (`removal::plan_removal` in
    // crates/banager-core/src/adapters/standalone/removal.rs);
    // `plan_operation_error` in src-tauri/src/ipc.rs sends the path and
    // the reason as data, and the dialog words them. Banager did check,
    // so the sentence is shown on its own, not inside "Couldn't check
    // what this affects" -- the same reason the pin above skips it.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return snapshotWith([
          brewInstance({
            id: "standalone-claude",
            adapter_id: "standalone-claude",
            exe_path: "/Users/someone/.local/bin/claude",
            prefix: "/Users/someone/.local/share/claude",
            version: "2.1.281",
          }),
        ]);
      }
      if (cmd === "plan_operation") {
        throw '{"kind":"uninstall_unsafe","path":"~/.local/bin/claude","reason":"not_what_instructions_expect"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={{ ...request, instance_id: "standalone-claude", artifact_kind: "Binary", name: "claude" }}
        displayName="Claude Code"
      />,
    );

    const refusal = "Couldn't move ~/.local/bin/claude because it isn't what was expected.";
    const alert = await screen.findByRole("alert");
    await waitFor(() => expect(alert).toHaveTextContent(refusal));
    expect(alert.textContent).not.toMatch(/uninstall_unsafe|not_what_instructions_expect|Couldn't check/);
    fireEvent.click(within(alert).getByRole("button", { name: `Details: ${refusal}` }));
    expect(
      within(alert).getByText(
        "It, or a folder it's in, links somewhere else, or it's the wrong kind of file or can't be read.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("says Banager's own refusal in one sentence, with no ⓘ reassuring whose problem it was", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") throw '{"kind":"refused"}';
      return undefined;
    });

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    const alert = await screen.findByRole("alert");
    const text =
      "Couldn't check what this affects: Couldn't continue because of an internal error.";
    await waitFor(() => expect(alert).toHaveTextContent(text));
    // The polish-3 copy rules (规则 3): whose fault it was is reassurance,
    // not a next step, so the sentence has no Details behind it.
    expect(within(alert).queryByRole("button", { name: /^Details:/ })).toBeNull();
    expect(alert).not.toHaveTextContent("not on your Mac");
  });

  it("localises the same refusal when it comes back from submit, not from plan", async () => {
    // `Session::submit` re-runs the actionability gate against the
    // snapshot that is current when Confirm is clicked, which is the only
    // way this refusal reaches someone who did nothing wrong: the preview
    // was fine when it was drawn, and the source stopped answering while
    // they were reading it. `submit_operation_error` sends the same JSON
    // `plan_operation_error` does, and this dialog must decode it there
    // too rather than printing braces at them. The re-plan the refusal
    // triggers is held open: the decoded refusal is on screen while the
    // dialog re-checks and is reset when the re-plan settles, so a mocked
    // re-plan that resolved in microtasks would take it down before React
    // rendered it.
    let planCalls = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return snapshotWith([brewInstance({ status: { unavailable: "NotResponding", notes: [] } })]);
      }
      if (cmd === "plan_operation") {
        planCalls += 1;
        if (planCalls === 1) return issuedPlanFor();
        return new Promise<IssuedPlan>(() => {});
      }
      if (cmd === "submit_operation") {
        throw '{"kind":"not_actionable","read_only":null,"unavailable":"NotResponding"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Couldn't start the uninstall: Homebrew didn't respond, so what it has installed can't be shown.",
      ),
    );
    expect(alert.textContent).not.toMatch(/not_actionable/);
  });

  it("ignores a submit that finishes after the dialog was retargeted", async () => {
    // Deviation from the brief, recorded in the task report: the brief's five
    // tests never exercise the dialog-session guard. A reply belonging to the
    // artifact the dialog has *stopped* showing must not report its op id nor
    // close the dialog now standing over a different artifact.
    let resolveSubmit: (opId: number) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "plan_operation") {
        const planned = (args as { request: OpRequest }).request;
        const issued = issuedPlanFor();
        return {
          ...issued,
          plan: {
            ...issued.plan,
            request: planned,
            action: {
              Command: {
                program: "/opt/homebrew/bin/brew",
                args: ["uninstall", "--formula", planned.name],
                env: [],
              },
            },
          },
        };
      }
      if (cmd === "submit_operation") {
        return new Promise<number>((resolve) => {
          resolveSubmit = resolve;
        });
      }
      return undefined;
    });
    const onSubmitted = vi.fn();
    const onOpenChange = vi.fn();

    const { rerender } = renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);
    await waitFor(() => expect(submitCalls()).toHaveLength(1));

    rerender(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={{ ...request, name: "yq" }}
        displayName="yq"
        onSubmitted={onSubmitted}
      />,
    );
    await screen.findByRole("alertdialog", { name: "Uninstall “yq”?" });
    await showCommand();
    await screen.findByText(command("/opt/homebrew/bin/brew uninstall --formula yq"));

    resolveSubmit(7);

    // The settled submit re-enables confirm; that is the observable edge the
    // stale reply would have crossed on its way to `onSubmitted`.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(onSubmitted).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByText(command("/opt/homebrew/bin/brew uninstall --formula yq"))).toBeInTheDocument();
  });

  it("says a NoCancel plan cannot be stopped once it starts, and says nothing of the kind for a cancellable one", async () => {
    // rustup's `self uninstall` (crates/banager-core/src/adapters/
    // standalone/recipes.rs): `OperationBar` will offer no Cancel once it
    // is Running, so the preview says so before the click (spec §五,
    // §6.6's last line), with why behind its ⓘ.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "NoCancel" }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" />,
    );

    const hint = "This can't be cancelled once it starts. Don't quit Banager or shut down your Mac until it finishes.";
    expect(await screen.findByText(noteLine(hint))).toBeInTheDocument();
    expect(linesOf("Notes")).toEqual([hint]);
    fireEvent.click(screen.getByRole("button", { name: `Details: ${hint}` }));
    expect(screen.getByText("Wait until the bottom of the window shows the result before you quit or shut down.")).toBeInTheDocument();

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "KillThenReconcile" }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await showCommand();
    expect(screen.queryByText(/can't cancel this once it starts/)).not.toBeInTheDocument();
  });
});
