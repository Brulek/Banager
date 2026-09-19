import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
import { useUiStore } from "../store/ui";
import type { ArtifactKey, OpRequest, Settings, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const glibKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" };
const onyxKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" };

const snapshot: Snapshot = {
  generation: 2,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [
    {
      key: glibKey,
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
    {
      key: onyxKey,
      current: "5.0.2",
      target: "5.1.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

let settings: Settings;
let updates: Snapshot["updates"];
// Every plan_operation answer carries a fresh server-issued id: a PlanId is
// single-use, so the multi-select tests below must prove that each submit
// sent a *different* id, not the same one twice.
let nextPlanId: number;
// Per-test knobs for the mocked backend. A name in `planFailures` makes its
// plan_operation reject with that text; a plan id in `submitFailures` makes
// its submit_operation reject; `saveFailure` makes set_settings reject. A
// name in `holdPlans`, an id in `holdSubmits`, or `holdSaves` keeps the
// reply pending until the test calls the matching `release*` function.
// Rejections are bare strings, exactly as a `Result<_, String>` command
// rejects; Task 10's `call()` turns them into Errors.
let planFailures: Record<string, string>;
let submitFailures: Record<number, string>;
let saveFailure: string | null;
let holdPlans: Set<string>;
let holdSubmits: Set<number>;
let holdSaves: boolean;
// Names whose plan comes back with `needs_password: true`, mirroring the
// brew adapter, which sets it for every Cask upgrade.
let needsPassword: Set<string>;
let releasePlan: Record<string, () => void>;
let releaseSubmit: Record<number, () => void>;
let releaseSave: Array<() => void>;

function issuedPlanFor(request: OpRequest, id: number) {
  return {
    id,
    plan: {
      request,
      program: "/opt/homebrew/bin/brew",
      args: ["upgrade", request.artifact_kind === "Cask" ? "--cask" : "--formula", request.name],
      env: [],
      needs_password: needsPassword.has(request.name),
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1758000000,
  };
}

function calls(cmd: string) {
  return mockInvoke.mock.calls.filter(([name]) => name === cmd);
}

function submittedPlanIds() {
  return calls("submit_operation").map(([, args]) => args);
}

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
  };
  updates = snapshot.updates;
  nextPlanId = 1;
  planFailures = {};
  submitFailures = {};
  saveFailure = null;
  holdPlans = new Set();
  holdSubmits = new Set();
  holdSaves = false;
  needsPassword = new Set();
  releasePlan = {};
  releaseSubmit = {};
  releaseSave = [];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return Promise.resolve({ ...snapshot, updates });
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "set_settings") {
      if (saveFailure !== null) return Promise.reject(saveFailure);
      const next = (args as { settings: Settings }).settings;
      if (holdSaves) {
        return new Promise<void>((resolve) => {
          releaseSave.push(() => {
            settings = next;
            resolve();
          });
        });
      }
      settings = next;
      return Promise.resolve(undefined);
    }
    if (cmd === "plan_operation") {
      const request = (args as { request: OpRequest }).request;
      const failure = planFailures[request.name];
      if (failure !== undefined) return Promise.reject(failure);
      const issued = issuedPlanFor(request, nextPlanId);
      nextPlanId += 1;
      if (holdPlans.has(request.name)) {
        return new Promise((resolve) => {
          releasePlan[request.name] = () => resolve(issued);
        });
      }
      return Promise.resolve(issued);
    }
    if (cmd === "submit_operation") {
      const { planId } = args as { planId: number };
      const failure = submitFailures[planId];
      if (failure !== undefined) return Promise.reject(failure);
      if (holdSubmits.has(planId)) {
        return new Promise((resolve) => {
          releaseSubmit[planId] = () => resolve(7);
        });
      }
      return Promise.resolve(7);
    }
    return Promise.resolve(undefined);
  });
});

describe("UpdatesPage", () => {
  it("lists each update with its version change when technical details are on", async () => {
    settings.show_technical_details = true;
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("2.88.3 → 2.90.0");
    await findByText("5.0.2 → 5.1.0");
  });

  it("hides version numbers under the default settings", async () => {
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    expect(queryByText("2.88.3 → 2.90.0")).not.toBeInTheDocument();
  });

  it("previews the command, submits nothing until Confirm, then submits the single update", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const updateButtons = await findAllByRole("button", { name: "Update" });
    fireEvent.click(updateButtons[0]);

    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Upgrade",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "glib",
      },
    });
    // Seeing the command comes first; opening the dialog submits nothing.
    expect(submittedPlanIds()).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: 1 }]));
    // Every item started, so the dialog closes on its own.
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("previews every selected command, then submits one operation per item, each with its own plan id", async () => {
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: "Update selected" }));
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(submittedPlanIds()).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: 1 }, { planId: 2 }]));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("warns per item, before the sudo prompt, about the one update that needs a password", async () => {
    // A batch can mix Casks and formulae, and the brew adapter only sets
    // `needs_password` for Casks (crates/canager-core/src/adapters/brew/
    // mod.rs). Spec §6: whatever will ask for a password says so in the
    // preview, next to the command it belongs to.
    needsPassword.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: "Update selected" }));
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const notices = within(dialog).getAllByText("This will ask for your Mac password.");
    expect(notices).toHaveLength(1);
    expect(notices[0].closest("div")?.textContent).toContain("onyx");
    expect(notices[0].closest("div")?.textContent).not.toContain("glib");
  });

  it("submits nothing when the confirmation is cancelled", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([]);
  });

  it("shows the backend's rejection verbatim and submits a fresh plan id only after a second Confirm", async () => {
    // The dialog sat open past the PlanId's 10-minute lifetime (or the id
    // was already consumed): the backend rejects with a bare string.
    submitFailures[1] = "this plan is older than 10 minutes; preview it again";
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    let dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not start the update: this plan is older than 10 minutes; preview it again",
    );
    // The dead id is not retried on its own, and the dialog stays open so
    // the failure can be read rather than blinking away.
    expect(submittedPlanIds()).toEqual([{ planId: 1 }]);
    expect(calls("plan_operation")).toHaveLength(1);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());

    // Asking again plans again: a new id and a new preview, and still no
    // submit until the user confirms that preview.
    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(calls("plan_operation")).toHaveLength(2);
    expect(submittedPlanIds()).toEqual([{ planId: 1 }]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: 1 }, { planId: 2 }]));
  });

  it("shows one item's planning failure in the dialog while the other stays submittable", async () => {
    planFailures.glib = "glib is pinned";
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: "Update selected" }));

    const dialog = await findByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Couldn't prepare the update: glib is pinned",
    );
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(
      within(dialog).queryByText("/opt/homebrew/bin/brew upgrade --formula glib"),
    ).not.toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    // onyx holds the only issued plan (glib never received an id). The batch
    // had a failure, so the dialog stays open with the outcome per item, and
    // only the item that started leaves the selection.
    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: 1 }]));
    await within(dialog).findByText("Started");
    expect(useUiStore.getState().selectedUpdates).toEqual(["brew:/opt/homebrew|Formula|glib"]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("after one item starts and the next fails, a retry re-plans and submits only the failed one", async () => {
    submitFailures[2] = "this plan is older than 10 minutes; preview it again";
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: "Update selected" }));

    let dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    // glib started, onyx did not, and the dialog says which is which.
    await within(dialog).findByText("Started");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not start the update: this plan is older than 10 minutes; preview it again",
    );
    expect(submittedPlanIds()).toEqual([{ planId: 1 }, { planId: 2 }]);
    // A started item leaves the selection at once; the failed one stays.
    expect(useUiStore.getState().selectedUpdates).toEqual(["brew:/opt/homebrew|Cask|onyx"]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());

    // Retry: plan_operation is asked exactly once more, for onyx only — a
    // fresh id must never re-queue the item that already started.
    fireEvent.click(getByRole("button", { name: "Update selected" }));
    dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    const secondRound = calls("plan_operation").slice(2);
    expect(secondRound).toHaveLength(1);
    expect(secondRound[0][1]).toEqual({
      request: {
        kind: "Upgrade",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Cask",
        name: "onyx",
      },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() =>
      expect(submittedPlanIds()).toEqual([{ planId: 1 }, { planId: 2 }, { planId: 3 }]),
    );
  });

  it("locks the dialog while submitting and drops a superseded batch's late reply", async () => {
    holdPlans.add("glib");
    holdSubmits.add(2);
    const { findAllByRole, findByRole, getByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    // glib is selected so "Update selected" has something to do: the lock
    // below is what disables it, not an empty selection.
    fireEvent.click((await findAllByRole("checkbox"))[0]);
    const updateSelected = getByRole("button", { name: "Update selected" });
    const updateButtons = await findAllByRole("button", { name: "Update" });

    // Batch 1 (glib) is still planning when batch 2 (onyx) opens the dialog.
    // Planning has no side effect beyond issuing a PlanId that expires on
    // its own, so the newer click supersedes the older batch.
    fireEvent.click(updateButtons[0]);
    fireEvent.click(updateButtons[1]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    await waitFor(() => expect(releasePlan.glib).toBeDefined());

    // Batch 1's plan lands late. It must neither replace nor add to the
    // preview the user is looking at, and must not close it.
    await act(async () => {
      releasePlan.glib();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(queryByRole("dialog")).toBe(dialog);
    expect(
      within(dialog).queryByText("/opt/homebrew/bin/brew upgrade --formula glib"),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    // Submitting: nothing closes the dialog or starts another batch until
    // this one has settled — not Cancel, not Escape, not "Update selected".
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled(),
    );
    expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeDisabled();
    expect(updateSelected).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(queryByRole("dialog")).toBe(dialog);

    await waitFor(() => expect(releaseSubmit[2]).toBeDefined());
    releaseSubmit[2]();
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([{ planId: 2 }]);
    expect(updateSelected).not.toBeDisabled();
  });

  it("removes an item from the list when Ignore is clicked", async () => {
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    const ignoreButtons = await findAllByRole("button", { name: "Ignore" });
    fireEvent.click(ignoreButtons[0]);

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
  });

  it("disables every Ignore while a save is pending so a second click cannot overwrite the first", async () => {
    holdSaves = true;
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    const ignoreButtons = await findAllByRole("button", { name: "Ignore" });
    fireEvent.click(ignoreButtons[0]);

    // Both buttons lock until the first save settles. A second Ignore now
    // would build its settings from the same stale base, and the later save
    // would drop the earlier one.
    await waitFor(() => expect(ignoreButtons[1]).toBeDisabled());
    expect(ignoreButtons[0]).toBeDisabled();
    fireEvent.click(ignoreButtons[1]);
    expect(calls("set_settings")).toHaveLength(1);

    releaseSave[0]();
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    await findByText("onyx");
    await waitFor(() => expect(ignoreButtons[1]).not.toBeDisabled());
    expect(calls("set_settings")).toHaveLength(1);
  });

  it("shows the backend's message when saving the ignore list fails", async () => {
    saveFailure = "settings.json is read-only";
    const { findByText, findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click((await findAllByRole("button", { name: "Ignore" }))[0]);

    expect(await findByRole("alert")).toHaveTextContent(
      "Couldn't save the ignored updates: settings.json is read-only",
    );
    // Nothing was saved, so nothing disappears.
    expect(await findByText("glib")).toBeInTheDocument();
  });

  it("says every update is ignored — not that everything is up to date — once all are ignored", async () => {
    settings.ignored_updates = [glibKey, onyxKey];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("No pending updates — everything else is ignored.");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("says everything is up to date only when the backend reports no updates at all", async () => {
    updates = [];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Everything is up to date");
  });
});
