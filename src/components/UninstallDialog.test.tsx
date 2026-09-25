import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UninstallDialog } from "./UninstallDialog";
import type { IssuedPlan, OpRequest, Plan } from "../lib/types";

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

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

function submitCalls() {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "submit_operation");
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
    expect(await screen.findByText("Checking what this would affect…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("warns before the sudo prompt when the plan needs a password, and stays quiet when it does not", async () => {
    // Every Cask uninstall sets `needs_password` (crates/canager-core/src/
    // adapters/brew/mod.rs), so removing a GUI app pops a system password
    // dialog. Spec §6: an operation that needs a password is marked in the
    // preview — a password is never a surprise.
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: true }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("This will ask for your Mac password.")).toBeInTheDocument();

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ needs_password: false }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula jq");
    expect(screen.queryByText("This will ask for your Mac password.")).not.toBeInTheDocument();
  });

  it("disables confirm and explains what would break when something depends on it", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ affected: ["jq-cli-wrapper"] }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await waitFor(() => expect(screen.getByText("jq-cli-wrapper")).toBeInTheDocument());
    const confirmButton = screen.getByRole("button", { name: "Uninstall" });
    expect(confirmButton).toBeDisabled();
    expect(
      screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq"),
    ).toBeInTheDocument();
    // The reason Confirm is disabled, and what to do about it, is plain text
    // in the dialog body -- not a `title` on a disabled button, which a
    // disabled button never actually shows: it takes no pointer events (no
    // hover) and drops out of the tab order (no keyboard/VoiceOver focus).
    expect(confirmButton).not.toHaveAttribute("title");
    expect(
      screen.getByText(
        "Canager won't remove jq while the items above still need it. Uninstall those first, from the Installed list, if you want them gone too — otherwise leave jq where it is.",
      ),
    ).toBeInTheDocument();
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
        "Canager couldn't check what depends on this, so removing it might break other software.",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText("This will break python@3.13.")).toBeInTheDocument();
  });

  it("pluralises WouldBreak's copy and interpolates every name", async () => {
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: [{ WouldBreak: { names: ["a", "b"] } }] }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("This will break 2 other things: a, b.")).toBeInTheDocument();
  });

  it("lists what a path-list uninstall moves, keeps and finds gone, and says Canager does the moving", async () => {
    // Spec §6.6, the Claude Code dialog: every item is a sentence in the
    // user's language, in the order the paths will be moved, the kept
    // paths after them; the preview below is one sentence with the
    // count, since no command runs.
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

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={claudeRequest} displayName="Claude Code" />,
    );

    expect(await screen.findByText("Before you continue:")).toBeInTheDocument();
    const items = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual([
      "Moves to the Trash: ~/.local/share/claude (the program's files)",
      "Moves to the Trash: ~/.claude/downloads (downloaded files it can re-create)",
      "Moves to the Trash: ~/.local/bin/claude (the command itself)",
      "Keeps: ~/.claude (your settings, login, history and working files — other apps may use it too)",
      "Keeps: ~/.claude.json (your settings)",
    ]);
    expect(
      screen.getByText(
        "Canager moves the 3 items listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
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

    expect(
      await screen.findByText("Already gone: ~/.local/share/claude (nothing left to move)"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Canager moves the 1 item listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag it back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
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

    expect(await screen.findByText("Before you continue:")).toBeInTheDocument();
    expect(screen.getByText("SomeFutureVariant")).toBeInTheDocument();
  });

  it("submits the plan id and reports the new op id when nothing would break", async () => {
    const issued = issuedPlanFor();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued;
      if (cmd === "submit_operation") return 7;
      throw new Error(`unexpected command ${cmd}`);
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
    // The exact command is on screen, and nothing has been submitted, before
    // the user is allowed to confirm.
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
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

    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula jq");
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
      throw new Error(`unexpected command ${cmd}`);
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
      "This preview is more than 10 minutes old, so Canager didn't start it. Look at the preview again, then confirm it once more.",
    );
    await waitFor(() => expect(planCalls).toBe(2));
    releaseReplan();
    // Confirm re-enables only once the fresh plan has arrived; its preview is
    // on screen, but the fresh id has not been sent — the dead id is still
    // the only submit so far, and nothing was reported as started.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
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
      throw new Error(`unexpected command ${cmd}`);
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
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Couldn't start the uninstall: This preview is more than 10 minutes old",
    );
    expect(screen.getByText("Checking what this would affect…")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await waitFor(() => expect(planCalls).toBe(2));

    releaseReplan();

    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Nothing was started when you confirmed, so Canager checked again — this is a fresh preview. Look it over, then confirm once more.",
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
      throw new Error(`unexpected command ${cmd}`);
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
    expect(note).toHaveTextContent(
      "Nothing was started when you confirmed, so Canager checked again — this is a fresh preview.",
    );
    expect(note).not.toHaveTextContent("confirm once more");
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
        return {
          generation: 2,
          detect: "Found",
          instances: [
            {
              id: "brew:/opt/homebrew",
              adapter_id: "brew",
              exe_path: "/opt/homebrew/bin/brew",
              prefix: "/opt/homebrew",
              scope: "User",
              version: "7.0.3",
              status: { unavailable: "NotResponding", notes: [] },
              unverified_version: null,
              read_only_reason: null,
            },
          ],
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
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

    await screen.findByText(/Couldn't check what this would affect: Homebrew is installed but didn't answer/);
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
        return {
          generation: 1,
          detect: "Found",
          instances: [
            {
              id: "brew:/opt/homebrew",
              adapter_id: "brew",
              exe_path: "/opt/homebrew/bin/brew",
              prefix: "/opt/homebrew",
              scope: "User",
              version: "7.0.3",
              status: { unavailable: "NotRunning", notes: [] },
              unverified_version: null,
              read_only_reason: null,
            },
          ],
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
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
        "Couldn't check what this would affect: Start Homebrew and Canager will list",
      ),
    );
    expect(alert.textContent).not.toMatch(/not_actionable/);
  });

  it("says a pinned package was not uninstalled and gives the unpin command as code", async () => {
    // A stale Installed page can still offer Uninstall on a package pinned
    // since the last refresh; `Session::issue_plan` refuses it
    // (`blocked_uninstall` in crates/canager-core/src/session/plans.rs)
    // and `uninstall_blocked_json` in src-tauri/src/ipc.rs sends this.
    // It is not "couldn't check what this would affect": Canager did.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return {
          generation: 1,
          detect: "Found",
          instances: [
            {
              id: "brew:/usr/local",
              adapter_id: "brew",
              exe_path: "/usr/local/bin/brew",
              prefix: "/usr/local",
              scope: "User",
              version: "7.0.6",
              status: { unavailable: null, notes: [] },
              unverified_version: null,
              read_only_reason: null,
            },
          ],
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
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
        "This has been pinned in Homebrew, and Homebrew won't remove a pinned package, so Canager didn't uninstall it. Nothing has been changed. To uninstall it, first run /usr/local/bin/brew unpin --cask onyx in Terminal to release the pin.",
      ),
    );
    // The owning brew's own path, not whichever `brew` Terminal finds.
    expect(screen.getByText("/usr/local/bin/brew unpin --cask onyx").tagName).toBe("CODE");
    expect(alert.textContent).not.toMatch(/uninstall_blocked|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("words a refused path-list preview with the path and the reason, never the payload", async () => {
    // One of the checks a path-list uninstall runs at preview time refused
    // a path (`removal::plan_removal` in
    // crates/canager-core/src/adapters/standalone/removal.rs);
    // `plan_operation_error` in src-tauri/src/ipc.rs sends the path and
    // the reason as data, and the dialog words them. Canager did check,
    // so the sentence is shown on its own, not inside "Couldn't check
    // what this would affect" -- the same reason the pin above skips it.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return {
          generation: 1,
          detect: "Found",
          instances: [
            {
              id: "standalone-claude",
              adapter_id: "standalone-claude",
              exe_path: "/Users/someone/.local/bin/claude",
              prefix: "/Users/someone/.local/share/claude",
              scope: "User",
              version: "2.1.281",
              status: { unavailable: null, notes: [] },
              unverified_version: null,
              read_only_reason: null,
            },
          ],
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
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

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Canager won't remove ~/.local/bin/claude: it couldn't confirm this is what the official instructions describe — it, or a folder it is in, may be a link to somewhere else, or it may be a different kind of file — so removing it could hit the wrong thing. Nothing was changed.",
      ),
    );
    expect(alert.textContent).not.toMatch(/uninstall_unsafe|not_what_instructions_expect|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
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
        return {
          generation: 2,
          detect: "Found",
          instances: [
            {
              id: "brew:/opt/homebrew",
              adapter_id: "brew",
              exe_path: "/opt/homebrew/bin/brew",
              prefix: "/opt/homebrew",
              scope: "User",
              version: "7.0.3",
              status: { unavailable: "NotResponding", notes: [] },
              unverified_version: null,
              read_only_reason: null,
            },
          ],
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
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
        "Couldn't start the uninstall: Homebrew is installed but didn't answer",
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
      throw new Error(`unexpected command ${cmd}`);
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
    // §6.6's last line).
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "NoCancel" }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" />,
    );

    expect(
      await screen.findByText(
        "Don't close Canager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
      ),
    ).toBeInTheDocument();

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "KillThenReconcile" }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula jq");
    expect(screen.queryByText(/can't be cancelled once it starts/)).not.toBeInTheDocument();
  });
});
