import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UninstallDialog } from "./UninstallDialog";
import type { InstalledArtifact, IssuedPlan, ManagerInstance, OpRequest, Plan, Snapshot } from "../lib/types";

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
    unverified_version: null,
    read_only_reason: null,
    ...over,
  };
}

function snapshotWith(instances: ManagerInstance[], artifacts: InstalledArtifact[] = []): Snapshot {
  return {
    generation: 1,
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
  const disclosure = await screen.findByRole("button", { name: "Show the command" });
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
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("asks the question as its title and names the tool under it, with its source and the version it has", async () => {
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
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshotWith([brewInstance()], [jq]);
      if (cmd === "plan_operation") return issuedPlanFor();
      return undefined;
    });

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    const dialog = await screen.findByRole("dialog", { name: "Uninstall jq?" });
    const tool = await within(dialog).findByText("1.8.1");
    const item = tool.closest("[data-sheet-tool]");
    expect(item).not.toBeNull();
    expect(within(item as HTMLElement).getByText("jq")).toBeInTheDocument();
    expect(within(item as HTMLElement).getByText("Homebrew")).toBeInTheDocument();
    // The avatar a row has: the source's initial.
    expect(within(item as HTMLElement).getByText("H")).toHaveAttribute("aria-hidden", "true");
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

    const dialog = await screen.findByRole("dialog", { name: "Uninstall iTerm2?" });
    const item = within(dialog).getByText("iTerm2", { selector: "p" }).closest("[data-sheet-tool]") as HTMLElement;
    await waitFor(() => expect(item.querySelector("img[data-app-icon]")).toHaveAttribute("src", icon));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("artifact_icon", {
      key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" },
    });
    // In place of the source's initial, not beside it.
    expect(within(item).queryByText("H")).toBeNull();
  });

  it("puts the focus on Cancel as it opens, and gives Uninstall the danger colour", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    const cancel = screen.getByRole("button", { name: "Cancel" });
    await waitFor(() => expect(document.activeElement).toBe(cancel));
    const uninstall = screen.getByRole("button", { name: "Uninstall" });
    expect(uninstall.className).toMatch(/\bbg-danger\b/);
    expect(cancel.className).not.toMatch(/\bbg-danger\b/);
  });

  it("says some apps ask for the Mac's password when the plan may need it, and stays quiet when it does not", async () => {
    // Every Cask uninstall sets `needs_password` (crates/canager-core/src/
    // adapters/brew/mod.rs), though not every app then asks for it: T3 of
    // the copy table. Spec §6: an operation that needs a password is
    // marked in the preview -- a password is never a surprise.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: true }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("Some apps ask for your Mac password at this step.")).toBeInTheDocument();
    expect(linesOf("Before you continue")).toEqual(["Some apps ask for your Mac password at this step."]);

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: false }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await showCommand();
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
    expect(screen.queryByText(/password/)).not.toBeInTheDocument();
  });

  it("disables confirm and explains what would break when something depends on it", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ affected: ["jq-cli-wrapper"] }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await waitFor(() => expect(screen.getByText("jq-cli-wrapper")).toBeInTheDocument());
    const confirmButton = screen.getByRole("button", { name: "Uninstall" });
    expect(confirmButton).toBeDisabled();
    within(group("Before you continue")).getByText("These still need it");
    await showCommand();
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
    // The reason Confirm is disabled, and what to do about it, is plain text
    // in the dialog body -- not a `title` on a disabled button, which a
    // disabled button never actually shows: it takes no pointer events (no
    // hover) and drops out of the tab order (no keyboard/VoiceOver focus).
    expect(confirmButton).not.toHaveAttribute("title");
    expect(screen.getByText("Uninstall these first to remove jq.")).toBeInTheDocument();
  });

  it("names what still needs it once, not again as a warning", async () => {
    // Homebrew's preview fills `WouldBreak` and `affected` from the same
    // `brew uses` (crates/canager-core/src/adapters/brew/mod.rs); the list
    // says it, and the sentence would say it a second time.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ affected: ["wget", "git"], warnings: [{ WouldBreak: { names: ["wget", "git"] } }] }),
    );

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="gettext" />);

    await screen.findByText("wget");
    expect(screen.getAllByText("wget")).toHaveLength(1);
    expect(screen.queryByText(/still need this/)).not.toBeInTheDocument();
    expect(linesOf("Before you continue")).toEqual(["wget", "git"]);
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
        "Couldn't check what else needs this. Make sure nothing does before you uninstall.",
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

    expect(await screen.findByText("2 others still need this: a, b.")).toBeInTheDocument();
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
    expect(linesOf("Stays where it is")).toEqual([
      "Settings, login and history: ~/.claude",
      "Your settings: ~/.claude.json",
    ]);
    expect(screen.queryByRole("region", { name: "Before you continue" })).toBeNull();
    // Nothing to show behind "Show the command": no command runs.
    expect(screen.queryByRole("button", { name: /^Show the command/ })).toBeNull();
    expect(container.ownerDocument.body.textContent).not.toMatch(/Put Back/);
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

    await screen.findByRole("region", { name: "Before you continue" });
    expect(linesOf("Before you continue")).toEqual([
      "Permanently deletes ~/.rustup: toolchains stable-aarch64-apple-darwin and everything rustup downloaded.",
      "Permanently deletes ~/.cargo and everything in it, including Cargo's cache, settings and saved login. Nothing goes to the Trash.",
      "Also permanently deletes tokei from the Cargo folder.",
      "Homebrew's rustup shares these folders, so its toolchains go too.",
      "~/.zprofile has a line that mentions Cargo, which rustup won't remove.",
      "You can't cancel this once it starts. Keep Canager and your Mac on until it finishes.",
    ]);
    expect(linesOf("Stays where it is")).toEqual(["Conversations and history: ~/.gemini/antigravity-cli"]);

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
        "You can't cancel this once it starts. Keep Canager and your Mac on until it finishes.",
        "Wait until the bottom of the window shows how it went before you quit Canager or turn off your Mac.",
      ],
      [
        "Conversations and history: ~/.gemini/antigravity-cli",
        "Some program files are in there too, so Canager keeps it whole.",
      ],
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

  it("says Uninstall permanently where a line says something is deleted for good, and Uninstall everywhere else", async () => {
    // rustup's own uninstall deletes its folders outright; a path-list
    // uninstall moves what it lists to the Trash, and a Homebrew one says
    // nothing about the Trash either way.
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

    const permanent = await screen.findByRole("button", { name: "Uninstall permanently" });
    expect(screen.queryByRole("button", { name: "Uninstall" })).toBeNull();
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
      expect(screen.queryByRole("button", { name: "Uninstall permanently" })).toBeNull();
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

    await screen.findByRole("region", { name: "Before you continue" });
    expect(linesOf("Before you continue")).toEqual(["SomeFutureVariant"]);
  });

  it("keeps the command one press away, and open from the start with technical details on", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());
    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const disclosure = await screen.findByRole("button", { name: "Show the command" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(JQ_COMMAND)).toBeNull();
    fireEvent.click(disclosure);
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
    unmount();

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return {
          language: "System",
          show_technical_details: true,
          ignored_updates: [],
          skipped_versions: [],
          include_self_updating: false,
        };
      }
      if (cmd === "plan_operation") return issuedPlanFor();
      return undefined;
    });
    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    expect(await screen.findByText(JQ_COMMAND)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show the command" })).toHaveAttribute("aria-expanded", "true");
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
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
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
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
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
    fireEvent.click(confirmButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Couldn't start the uninstall: This confirmation is more than 10 minutes old",
    );
    expect(screen.getByText("Checking what this affects…")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await waitFor(() => expect(planCalls).toBe(2));

    releaseReplan();

    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    await showCommand();
    expect(screen.getByText(JQ_COMMAND)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Nothing started, so Canager checked again. Confirm once more.",
    );

    // Acting on the fresh preview retires the note; the (again expired)
    // submit's own error takes its place while the next re-check runs.
    fireEvent.click(confirmButton);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't start the uninstall:");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
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
    const note = await screen.findByRole("status");
    expect(note).toHaveTextContent("Nothing started, so Canager checked again.");
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
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
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

  it("says a pinned package was not uninstalled and gives the unpin command as code", async () => {
    // A stale Installed page can still offer Uninstall on a package pinned
    // since the last refresh; `Session::issue_plan` refuses it
    // (`blocked_uninstall` in crates/canager-core/src/session/plans.rs)
    // and `uninstall_blocked_json` in src-tauri/src/ipc.rs sends this.
    // It is not "couldn't check what this affects": Canager did.
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
        "It's pinned in Homebrew, so Canager didn't uninstall or change anything. Run /usr/local/bin/brew unpin --cask onyx in Terminal to unpin it first.",
      ),
    );
    // The owning brew's own path, not whichever `brew` Terminal finds.
    expect(screen.getByText("/usr/local/bin/brew unpin --cask onyx").tagName).toBe("CODE");
    expect(alert.textContent).not.toMatch(/uninstall_blocked|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("words a refused path-list preview with the path and the reason, never the payload, and keeps the why behind its ⓘ", async () => {
    // One of the checks a path-list uninstall runs at preview time refused
    // a path (`removal::plan_removal` in
    // crates/canager-core/src/adapters/standalone/removal.rs);
    // `plan_operation_error` in src-tauri/src/ipc.rs sends the path and
    // the reason as data, and the dialog words them. Canager did check,
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

    const refusal = "~/.local/bin/claude isn't what Canager expected, so it won't move it. Nothing changed.";
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

  it("says Canager's own refusal was Canager's problem, behind its ⓘ", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") throw '{"kind":"refused"}';
      return undefined;
    });

    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />);

    const alert = await screen.findByRole("alert");
    const text =
      "Couldn't check what this affects: Something went wrong inside Canager, so it stopped. Nothing changed.";
    await waitFor(() => expect(alert).toHaveTextContent(text));
    fireEvent.click(within(alert).getByRole("button", { name: `Details: ${text}` }));
    expect(within(alert).getByText("This is a problem in Canager, not on your Mac.")).toBeInTheDocument();
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
        "Couldn't start the uninstall: Homebrew didn't respond, so Canager can't show what it has installed.",
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
    await screen.findByRole("dialog", { name: "Uninstall yq?" });
    await showCommand();
    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula yq");

    resolveSubmit(7);

    // The settled submit re-enables confirm; that is the observable edge the
    // stale reply would have crossed on its way to `onSubmitted`.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(onSubmitted).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula yq")).toBeInTheDocument();
  });

  it("says a NoCancel plan cannot be stopped once it starts, and says nothing of the kind for a cancellable one", async () => {
    // rustup's `self uninstall` (crates/canager-core/src/adapters/
    // standalone/recipes.rs): `OperationBar` will offer no Cancel once it
    // is Running, so the preview says so before the click (spec §五,
    // §6.6's last line), with why behind its ⓘ.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "NoCancel" }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" />,
    );

    const hint = "You can't cancel this once it starts. Keep Canager and your Mac on until it finishes.";
    expect(await screen.findByText(hint)).toBeInTheDocument();
    expect(linesOf("Before you continue")).toEqual([hint]);
    fireEvent.click(screen.getByRole("button", { name: `Details: ${hint}` }));
    expect(screen.getByText("Wait until the bottom of the window shows how it went before you quit Canager or turn off your Mac.")).toBeInTheDocument();

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
