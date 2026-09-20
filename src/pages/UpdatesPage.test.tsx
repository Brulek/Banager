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
const myForkKey: ArtifactKey = {
  instance_id: "cargo:/Users/brulek/.cargo",
  kind: "Binary",
  name: "my-fork",
};
const qwenKey: ArtifactKey = {
  instance_id: "ollama:http://127.0.0.1:11434",
  kind: "Model",
  name: "qwen3:8b",
};
const urllib3Key: ArtifactKey = {
  instance_id: "pip:/usr/bin/python3",
  kind: "Package",
  name: "urllib3",
};

const snapshot: Snapshot = {
  generation: 2,
  detect: "Found",
  // A candidate's adapter is only reachable by joining its key's
  // `instance_id` back to the snapshot's instances, so the page needs real
  // ones: pip's candidates are genuinely checkable but can never be acted
  // on.
  instances: [
    {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      healthy: true,
      unverified_version: null,
    },
    {
      id: "pip:/usr/bin/python3",
      adapter_id: "pip",
      exe_path: "/usr/bin/python3",
      prefix: "/usr",
      scope: "User",
      version: "26.2.1",
      healthy: true,
      unverified_version: null,
    },
  ],
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
let planWarnings: Record<string, string[]>;
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
      warnings: planWarnings[request.name] ?? [],
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
    include_self_updating: false,
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
  planWarnings = {};
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

  it("shows a warning carried on the plan, such as cargo's compile-locally notice", async () => {
    planWarnings.glib = ["This will compile locally and can take several minutes."];
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText(
      "This will compile locally and can take several minutes.",
    );
  });

  it("offers no Update button and no checkbox for a candidate the adapter could not check", async () => {
    // The whole point of UpdateCandidate.checkable. A git-sourced cargo
    // crate reports checkable:false because crates.io knows nothing about
    // it -- and "Update" on such a row would run `cargo install --force
    // my-fork` against the crates.io crate of the same name, a different
    // package entirely. The same flag covers an Ollama model whose manifest
    // could not be read and a pipx tool whose PyPI lookup failed.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["installed from git, cannot check crates.io for updates"],
      },
    ];
    const { findByText, queryByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    expect(queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    expect(queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      await findByText("installed from git, cannot check crates.io for updates"),
    ).toBeInTheDocument();
  });

  it("keeps an uncheckable candidate out of Update selected even when it was selected earlier", async () => {
    // Deviation from the brief, recorded in the task report: hiding the
    // checkbox is not enough on its own. A selection lives in the UI store
    // and outlives the row that made it, so a candidate selected while it
    // was checkable stays selected after a refresh flips `checkable` to
    // false (cargo's crates.io lookup failing is enough to do that). Without
    // this, "Update selected" would still plan the row whose Update button
    // was just taken away -- `cargo install --force my-fork` against the
    // same-named crates.io crate, the exact hazard `checkable` exists for.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["installed from git, cannot check crates.io for updates"],
      },
    ];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(myForkKey);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    fireEvent.click(getByRole("button", { name: "Update selected" }));

    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(
      calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name),
    ).toEqual(["glib"]);
  });

  it("offers no Update button and no checkbox for a pip package, and points at pipx or uv instead", async () => {
    // pip is read-only by design: its plan() refuses every operation with
    // "unsupported: pip is read-only in Canager; use pipx or uv to manage
    // {name}". Its candidates are still built with checkable: true, because
    // pip genuinely can check -- so gating the Update button on `checkable`
    // alone offered a button whose only possible outcome is a raw Rust error
    // string in a dialog.
    updates = [
      snapshot.updates[0],
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
      },
    ];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    // glib's button and checkbox, and only glib's.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(await findAllByRole("checkbox")).toHaveLength(1);
    // Counted as one update, not two: the pip row is not one the user can act on.
    await findByText("1 update available");
    expect(
      await findByText(
        "Canager can only show what's installed with pip, not update or uninstall it. Install Python command-line tools with pipx or uv instead to manage them here.",
      ),
    ).toBeInTheDocument();
  });

  it("keeps a read-only source's candidate out of Update selected even when it was selected earlier", async () => {
    // Same hazard as the uncheckable case above: a selection lives in the UI
    // store and outlives the row that made it, so hiding the checkbox is not
    // enough on its own.
    updates = [
      snapshot.updates[0],
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
      },
    ];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(urllib3Key);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    fireEvent.click(getByRole("button", { name: "Update selected" }));

    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(
      calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name),
    ).toEqual(["glib"]);
  });

  it("says a newer build is available for an Ollama model instead of printing two digests", async () => {
    // `current` is the local manifest digest /api/tags reported; `target` is
    // the registry manifest's config digest. They are different hash spaces,
    // not two readings of one identifier -- the adapter's own comment
    // (crates/canager-core/src/adapters/ollama/mod.rs) forbids rendering
    // them as a version jump, and neither is anything to show a person who
    // does not write code. `channel: "Digest"` is the discriminator.
    settings.show_technical_details = true;
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [],
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("qwen3:8b");
    expect(await findByText("A newer build of this model is available")).toBeInTheDocument();
    expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
    expect(queryByText(/sha256:/)).not.toBeInTheDocument();
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
