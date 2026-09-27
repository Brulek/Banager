import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
import { artifactKeyId, useUiStore } from "../store/ui";
import zhCN from "../i18n/zh-CN.json";
import type {
  ArtifactKey,
  InstanceNote,
  OpRequest,
  Settings,
  Snapshot,
  Warning,
} from "../lib/types";

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
// An npm whose global prefix this account cannot write -- Node installed
// from nodejs.org's package. Read-only like pip, for an entirely different
// reason, and the advice that fixes one is nonsense for the other.
const typescriptKey: ArtifactKey = {
  instance_id: "npm:/usr/local",
  kind: "Package",
  name: "typescript",
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
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    },
    {
      id: "pip:/usr/bin/python3",
      adapter_id: "pip",
      exe_path: "/usr/bin/python3",
      prefix: "/usr",
      scope: "User",
      version: "26.2.1",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: "ByDesign",
    },
    {
      id: "npm:/usr/local",
      adapter_id: "npm",
      exe_path: "/usr/local/bin/npm",
      prefix: "/usr/local",
      scope: "User",
      version: "12.0.2",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: "PrefixNotWritable",
    },
    // Writable and answering. Here so the cargo candidates below have a
    // source to sit under: the page is grouped by source, and `refresh`
    // never produces a candidate whose instance is not in this list.
    {
      id: "cargo:/Users/brulek/.cargo",
      adapter_id: "cargo",
      exe_path: "/Users/brulek/.cargo/bin/cargo",
      prefix: "/Users/brulek/.cargo",
      scope: "User",
      version: "1.92.0",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
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
      blocked: null,
    },
    {
      key: onyxKey,
      current: "5.0.2",
      target: "5.1.0",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: null,
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const stoppedOllama: Snapshot["instances"][number] = {
  id: "ollama:http://127.0.0.1:11434",
  adapter_id: "ollama",
  exe_path: "/usr/local/bin/ollama",
  prefix: "/usr/local",
  scope: "User",
  version: "0.34.1",
  status: { unavailable: "NotRunning", notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

let settings: Settings;
let updates: Snapshot["updates"];
let instances: Snapshot["instances"];
// The Updates page reads a package's own blurb out of `artifacts` when
// technical details are off, so a test about what a row says needs them.
let artifacts: Snapshot["artifacts"];
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
let submitFailures: Record<string, string>;
let saveFailure: string | null;
let holdPlans: Set<string>;
let holdSubmits: Set<string>;
let holdSaves: boolean;
// Names whose plan comes back with `needs_password: true`, mirroring the
// brew adapter, which sets it for every Cask upgrade.
let needsPassword: Set<string>;
// Names whose plan comes back `NoCancel`, mirroring the rustup recipe's
// `self update` (crates/canager-core/src/adapters/standalone/recipes.rs).
let noCancel: Set<string>;
let planWarnings: Record<string, Warning[]>;
let releasePlan: Record<string, () => void>;
let releaseSubmit: Record<string, () => void>;
let releaseSave: Array<() => void>;

// `id` stays a number here purely so the tests can order plans ("the first
// issued", "the second issued"); the wire type is a string (a random
// 128-bit token, not a sequential counter -- see PlanId in
// crates/canager-core/src/session/mod.rs), so it is stringified going out.
function issuedPlanFor(request: OpRequest, id: number) {
  return {
    id: String(id),
    plan: {
      request,
      action: {
        Command: {
          program: "/opt/homebrew/bin/brew",
          args: ["upgrade", request.artifact_kind === "Cask" ? "--cask" : "--formula", request.name],
          env: [],
        },
      },
      needs_password: needsPassword.has(request.name),
      locks: ["brew:/opt/homebrew"],
      cancel_policy: noCancel.has(request.name) ? "NoCancel" : "KillThenReconcile",
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

// The height every virtualized row reports back in jsdom.
const ROW_HEIGHT = 56;

// Which slot of the virtualized list `element` was drawn in. The list is
// flat in the DOM -- every heading and every row is a sibling carrying its
// position as `data-index` -- so "under which source's heading" is a
// question about these numbers.
function slotOf(element: HTMLElement): number {
  const slot = element.closest("[data-index]");
  if (slot === null) throw new Error("not inside a list slot");
  return Number(slot.getAttribute("data-index"));
}

// A row description whose whole text, across the `<code>` the Updates page
// sets a command in, is `text`. `getByText` alone matches an element's own
// text nodes, which no longer hold the whole sentence.
function wholeSentence(text: string) {
  return (_content: string, element: Element | null) =>
    element?.tagName === "P" && element.textContent === text;
}

function brewCandidate(name: string): Snapshot["updates"][number] {
  return {
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
  };
}

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
  };
  updates = snapshot.updates;
  instances = snapshot.instances;
  artifacts = snapshot.artifacts;
  nextPlanId = 1;
  planFailures = {};
  submitFailures = {};
  saveFailure = null;
  holdPlans = new Set();
  holdSubmits = new Set();
  holdSaves = false;
  needsPassword = new Set();
  noCancel = new Set();
  planWarnings = {};
  releasePlan = {};
  releaseSubmit = {};
  releaseSave = [];
  mockInvoke.mockReset();
  // @tanstack/react-virtual measures its scroll container and every row
  // through offsetWidth / offsetHeight, which jsdom hardcodes to 0 with no
  // layout engine behind them. Without these the virtualizer sees a
  // zero-height viewport and renders no rows at all. Same stubs as
  // InstalledPage.test.tsx, which virtualized first.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("data-index") === null ? 600 : ROW_HEIGHT;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({ ...snapshot, updates, instances, artifacts });
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
      const { planId } = args as { planId: string };
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

afterEach(() => {
  vi.restoreAllMocks();
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

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }]));
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

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
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
    planWarnings.glib = ["CompilesLocally"];
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
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText, queryByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    expect(queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    expect(queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      await findByText(
        "This wasn't installed from crates.io, so Canager can't check it for updates.",
      ),
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
        warnings: ["NonRegistrySource"],
        blocked: null,
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

  it("offers no Update button and no checkbox for a pinned formula, and says how to release it", async () => {
    // `brew outdated` lists a pinned formula like any other, and `brew
    // upgrade glib` then exits 1 with "Not upgrading 1 pinned package".
    // The row stays -- the newer version is real -- but it offers nothing
    // Homebrew will refuse, and says why and what the user can do.
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    const { findByText, getAllByRole, getByText } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select onyx for update");
    expect(getByText("Pinned")).toBeInTheDocument();
    expect(
      getByText(
        wholeSentence(
          "Homebrew is keeping this at the version it has now, because it has been pinned, so Canager won't update it. To let it update, run /opt/homebrew/bin/brew unpin glib in Terminal; Canager will offer the update the next time it checks, which at the latest is the next time you start Canager.",
        ),
      ),
    ).toBeInTheDocument();
    // Set apart as code, so it is visibly a command and nothing around it
    // gets copied with it.
    expect(getByText("/opt/homebrew/bin/brew unpin glib").tagName).toBe("CODE");
    // Counted with what Canager cannot update, not as an available update.
    await findByText("1 update available");
    await findByText("1 more can't be updated here");
  });

  it("names the cask form of the unpin command for a pinned cask", async () => {
    // `brew unpin <name>` resolves a formula first; `--cask` makes it
    // release the cask even when a formula shares the name.
    updates = [snapshot.updates[0], { ...snapshot.updates[1], blocked: "Pinned" }];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    expect((await findByText("/opt/homebrew/bin/brew unpin --cask onyx")).tagName).toBe("CODE");
  });

  it("does not promise a pinned app that updates itself will stay at its version", async () => {
    // `brew pin` warns that a cask with `auto_updates true` "may update
    // itself outside Homebrew despite being pinned". Such a row reaches
    // the page mostly with include_self_updating (`brew outdated --greedy`).
    settings = { ...settings, include_self_updating: true };
    updates = [snapshot.updates[0], { ...snapshot.updates[1], blocked: "Pinned" }];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: true,
        uninstall_blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText(
      wholeSentence(
        "This has been pinned, so neither Homebrew nor Canager will update it. The app updates itself, though, and may still do so despite the pin. To let Homebrew update it, run /opt/homebrew/bin/brew unpin --cask onyx in Terminal; Canager will offer the update the next time it checks, which at the latest is the next time you start Canager.",
      ),
    );
    expect(queryByText(/keeping this at the version it has now/)).toBeNull();
  });

  it("promises a silent source's pinned row the update only once the source answers", async () => {
    // `updateStateOf` checks `blocked` before `sourceUnavailable`, so a
    // pinned candidate under a Homebrew that did not answer is still
    // badged and described as Pinned, not as unavailable -- but the row
    // gets no Update button either way until Homebrew answers a check
    // again (`isUpdateActionable` needs `isAvailable`). "The next time it
    // checks, which at the latest is the next time you start Canager"
    // does not hold while Homebrew stays silent.
    instances = [
      { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
      ...snapshot.instances.slice(1),
    ];
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    const { findByText, getByText, queryByText, queryAllByRole } = renderWithProviders(
      <UpdatesPage />,
    );

    await findByText("glib");
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
    expect(getByText("Pinned")).toBeInTheDocument();
    expect(
      getByText(
        wholeSentence(
          "Homebrew is keeping this at the version it has now, because it has been pinned, so Canager won't update it. To let it update, run /opt/homebrew/bin/brew unpin glib in Terminal; after that, Canager will offer the update the next time it checks and Homebrew answers.",
        ),
      ),
    ).toBeInTheDocument();
    expect(queryByText(/next time you start Canager/)).toBeNull();
  });

  it("names the brew that owns a pinned package when a Mac has two, not whichever PATH finds", async () => {
    // A Mac migrated from Intel keeps /usr/local beside /opt/homebrew,
    // and Terminal's `brew` is /opt/homebrew/bin/brew. glib pinned in
    // /usr/local has to be released by /usr/local/bin/brew: the other one
    // answers "glib not pinned" and the row would stay Pinned for good.
    const intelBrew: Snapshot["instances"][number] = {
      ...snapshot.instances[0],
      id: "brew:/usr/local",
      exe_path: "/usr/local/bin/brew",
      prefix: "/usr/local",
    };
    instances = [snapshot.instances[0], intelBrew];
    updates = [
      snapshot.updates[1],
      {
        ...snapshot.updates[0],
        key: { ...glibKey, instance_id: "brew:/usr/local" },
        blocked: "Pinned",
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect((await findByText("/usr/local/bin/brew unpin glib")).tagName).toBe("CODE");
    expect(queryByText(/\/opt\/homebrew\/bin\/brew unpin/)).toBeNull();
  });

  it("names pipx and pipx's own unpin command on a pinned pipx tool", async () => {
    // `pipx list --outdated` lists a pinned tool as `cowsay [pinned]: 5.0
    // -> 6.1`, and `pipx upgrade cowsay` then changes nothing and exits 0.
    // The row must say pipx, not Homebrew, and give the command pipx
    // itself names ("Run `pipx unpin cowsay` to unpin it"), from the pipx
    // Canager found.
    const pipx: Snapshot["instances"][number] = {
      id: "pipx",
      adapter_id: "pipx",
      exe_path: "/opt/homebrew/bin/pipx",
      prefix: "/opt/homebrew/bin",
      scope: "User",
      version: "1.17.3",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    };
    instances = [snapshot.instances[0], pipx];
    updates = [
      snapshot.updates[0],
      {
        key: { instance_id: "pipx", kind: "Tool", name: "cowsay" },
        current: "5.0",
        target: "6.1",
        channel: "Native",
        checkable: true,
        warnings: [],
        blocked: "Pinned",
      },
    ];
    const { findByText, getAllByRole, getByText, queryByText } = renderWithProviders(
      <UpdatesPage />,
    );

    await findByText("cowsay");
    // Only glib's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select glib for update");
    expect(
      getByText(
        wholeSentence(
          "pipx is keeping this at the version it has now, because it has been pinned, so Canager won't update it. To let it update, run /opt/homebrew/bin/pipx unpin cowsay in Terminal; Canager will offer the update the next time it checks, which at the latest is the next time you start Canager.",
        ),
      ),
    ).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/pipx unpin cowsay").tagName).toBe("CODE");
    expect(queryByText(/brew unpin/)).toBeNull();
    expect(queryByText(/Homebrew is keeping/)).toBeNull();
  });

  it("keeps a pinned candidate out of Update selected even when it was selected earlier", async () => {
    // Selected while it was not pinned; a refresh since says it is. The
    // selection outlives the row's checkbox, so `isActionable` has to be
    // what filters the batch, or `brew upgrade --formula glib` would be
    // planned anyway (and refused by `Session::issue_plan`).
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(onyxKey);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click(getByRole("button", { name: "Update selected" }));

    await findByRole("dialog");
    expect(
      calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name),
    ).toEqual(["onyx"]);
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
        blocked: null,
      },
    ];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    // glib's button and checkbox, and only glib's.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(await findAllByRole("checkbox")).toHaveLength(1);
    // One update the user can act on, and one they cannot -- both said out
    // loud. Counting only the first left "0 updates available" above six
    // listed rows on a machine whose only outdated packages were pip's.
    await findByText("1 update available");
    await findByText("1 more can't be updated here");
    expect(
      await findByText(
        "Canager can only show what's installed with pip, not update or uninstall it. Install Python command-line tools with pipx or uv instead to manage them here.",
      ),
    ).toBeInTheDocument();
  });

  it("explains a read-only source once, under its own heading, and leaves every row its own description", async () => {
    // Six outdated pip packages used to mean six copies of the same
    // ~200-character paragraph -- roughly twenty lines of screen, because
    // the row is told not to clip an explanation -- and the packages'
    // own blurbs were displaced by it, so the six rows read identically.
    // The Installed page has always said this once, under the group
    // header. The Updates page now does the same.
    const pipPackages = ["urllib3", "requests", "certifi", "idna", "charset-normalizer", "six"];
    updates = pipPackages.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Registry" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    }));
    artifacts = pipPackages.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      display_name: name,
      version: "1.0.0",
      reason: "Requested" as const,
      description: `what ${name} is for`,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    }));
    const { findByText, queryAllByText } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    expect(
      queryAllByText(/Install Python command-line tools with pipx or uv instead/),
    ).toHaveLength(1);
    // And each row can be told from the next again.
    for (const name of pipPackages) {
      expect(await findByText(`what ${name} is for`)).toBeInTheDocument();
    }
  });

  it("says nothing about a read-only source that has no rows on this page", async () => {
    // A source's heading can be drawn with nothing under it (a silent
    // source has something to say even with no rows), so the capability
    // notice has to be held back explicitly. pip being read-only is not
    // news on a page listing two Homebrew updates.
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    expect(queryByText("Read-only: pip packages")).not.toBeInTheDocument();
    expect(queryByText("Read-only: npm packages")).not.toBeInTheDocument();
  });

  it("does not count a row it could not check as an available update", async () => {
    // The headline counts what Canager can act on, and `checkable` is one
    // of the three things that decides that -- but the count only ever
    // looked at the other two. A writable, answering source whose registry
    // lookup failed produced six rows with no buttons under the words "6
    // updates available": the same claim the empty list used to make, just
    // with rows under it. The two numbers now come from exactly the
    // predicate that draws the buttons.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    await findByText("1 update available");
    await findByText("1 more can't be updated here");
    expect(queryByText("2 updates available")).not.toBeInTheDocument();
  });

  it("says why a row can't be checked even when its source is also read-only", async () => {
    // pip is read-only *and* reaches PyPI, so a failed lookup produces
    // rows where both facts are true at once -- and the read-only advice
    // used to win outright, leaving no trace that Canager had not managed
    // to check anything. The row would then read "use pipx or uv" with a
    // "Read-only" badge, exactly as it does on a good day, while behind it
    // the version information was simply missing. Both still have to be
    // said; they are simply said in the two places they belong. The
    // source's advice is the source's notice, once, at the top; the
    // reason a lookup failed is this row's and nowhere else.
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.2.1",
        channel: "Native",
        checkable: false,
        warnings: [{ Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" }],
        blocked: null,
      },
    ];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    const advice = await findByText(
      /Install Python command-line tools with pipx or uv instead/,
    );
    const reason = await findByText(/Canager couldn't check this one for updates/);
    // Two separate elements: the advice is the source's banner, the reason
    // is the row's description.
    expect(advice).not.toBe(reason);
    expect(advice.textContent).not.toMatch(/Canager couldn't check this one/);
    // jsdom applies no CSS, so being in the DOM says nothing about being
    // visible; the row has to be told not to clip the reason to one line,
    // or the detail at the end of it is what the real window hides.
    expect(reason.className).not.toContain("truncate");
  });

  it("keeps the tool's own error text behind Show technical details", async () => {
    // Going offline used to paper every row with the same line of English
    // stderr. Before this branch an outage produced one banner -- npm
    // returned an empty list, pip/uv/pipx returned Err -- so it was one
    // sentence in one place. Now every installed package gets a row, and
    // every row got "pip list --outdated: ERROR: Could not fetch URL
    // https://pypi.org/simple/" as its description. `show_technical_details`
    // already promises to hide "the commands Canager actually runs", which
    // is exactly what that line is (spec §6).
    const names = ["urllib3", "requests", "certifi"];
    updates = names.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Registry" as const,
      checkable: false,
      warnings: [
        { Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" },
      ],
      blocked: null,
    }));
    const { findByText, queryAllByText } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    expect(queryAllByText(/Could not fetch URL/)).toHaveLength(0);
    expect(queryAllByText(/pip list --outdated/)).toHaveLength(0);
    // What a person who does not write code is told instead: each row
    // that it could not be checked, in one short sentence...
    expect(
      queryAllByText("Canager couldn't check this one for updates."),
    ).toHaveLength(3);
    // ...and, once for the page, what that might mean and where to go if
    // they want the rest.
    const summary = queryAllByText(/Canager couldn't check 3 of the items below/);
    expect(summary).toHaveLength(1);
    expect(summary[0].textContent).toContain("Show technical details");
  });

  it("says why rows could not be checked once for the page, not once per row", async () => {
    // Offline, one failed `npm outdated -g` turns every global package into
    // an uncheckable row. The explanation used to be a 180-character
    // paragraph on each of them -- seventy globals, seventy copies -- and
    // these adapters push no SourceError, so no page-wide banner said it
    // either.
    updates = Array.from({ length: 70 }, (_, index) => ({
      key: { instance_id: "npm:/usr/local", kind: "Package" as const, name: `global-${index}` },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Native" as const,
      checkable: false,
      warnings: [{ Message: "npm outdated -g: npm error code ENOTFOUND" }],
      blocked: null,
    }));
    const { findByText, queryAllByText } = renderWithProviders(<UpdatesPage />);

    await findByText("global-0");
    const summary = queryAllByText(/Canager couldn't check 70 of the items below/);
    expect(summary).toHaveLength(1);
    // Only one line of the tool's stderr can tell "offline" from "the index
    // is refusing you", and that line is what this copy stands in for --
    // so it offers being offline as a possibility, not as the diagnosis.
    expect(summary[0].textContent).toContain("one possible reason, but not the only one");
    expect(queryAllByText(/Being offline/)).toHaveLength(1);
    expect(queryAllByText(/ENOTFOUND/)).toHaveLength(0);
  });

  it("does not count a row with its own reason in the page's cannot-check line", async () => {
    // A git-installed crate says its own, different, reason on its row;
    // "being offline is one possible reason" is not about it. With only
    // such rows there is nothing for the page to add.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    expect(queryByText(/of the items below/)).not.toBeInTheDocument();
  });

  it("shows the tool's own error text once Show technical details is on", async () => {
    settings.show_technical_details = true;
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.2.1",
        channel: "Registry",
        checkable: false,
        warnings: [
          { Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" },
        ],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    const reason = await findByText(/Could not fetch URL https:\/\/pypi\.org\/simple\//);
    expect(reason.textContent).toMatch(/^Canager couldn't check this one for updates\./);
    expect(reason.className).not.toContain("truncate");
    // The row already carries the tool's words, so the page's summary --
    // which exists to point at this switch -- has nothing to add.
    expect(queryByText(/of the items below/)).not.toBeInTheDocument();
  });

  it("leaves a warning that was written for this audience unwrapped", async () => {
    // Two kinds of text end up on an uncheckable row. `NonRegistrySource`
    // is written for this audience already and reads as a whole sentence;
    // a `Message` is whatever the tool printed, in whatever language it
    // printed it, and on its own it is a row whose entire description is
    // a line of somebody's stderr.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    expect(
      await findByText(
        "This wasn't installed from crates.io, so Canager can't check it for updates.",
      ),
    ).toBeInTheDocument();
  });

  it("tells an npm user to install Node with Homebrew, not to use pipx or uv", async () => {
    // Both sources are read-only, for different reasons, and the wire now
    // says which. Before `read_only_reason` the page hardcoded pip's
    // advice for every read-only row, so a user whose npm prefix is
    // root-owned was told to install their JavaScript tooling with a
    // Python tool.
    updates = [
      {
        key: typescriptKey,
        current: "5.6.2",
        target: "5.7.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, queryAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("typescript");
    expect(
      await findByText(
        "Canager can list these but can't update or remove them: npm keeps them in a folder your account isn't allowed to change. That usually means Node was installed with the installer from nodejs.org. Installing Node with Homebrew instead lets Canager manage them.",
      ),
    ).toBeInTheDocument();
    // pip's row keeps pip's advice, right next to it.
    expect(
      await findByText(
        "Canager can only show what's installed with pip, not update or uninstall it. Install Python command-line tools with pipx or uv instead to manage them here.",
      ),
    ).toBeInTheDocument();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("says nothing here can be updated, rather than 0 updates, when every row is read-only", async () => {
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
      {
        key: typescriptKey,
        current: "5.6.2",
        target: "5.7.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("urllib3");
    expect(await findByText("Nothing here can be updated by Canager")).toBeInTheDocument();
    expect(queryByText("0 updates available")).not.toBeInTheDocument();
    // The headline switching to "nothing here" does not excuse dropping the
    // number: two listed rows the user cannot act on should still be counted,
    // or the page says a machine with two stuck packages looks like a machine
    // with twenty.
    expect(await findByText("2 listed below")).toBeInTheDocument();
    // But not as "2 more": more than the nothing the line above just said.
    expect(queryByText(/more can't be updated/)).not.toBeInTheDocument();
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
        blocked: null,
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
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
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

  it("localises an expired plan and submits a fresh plan id only after a second Confirm", async () => {
    // The dialog sat open past the PlanId's 10-minute lifetime: the backend
    // rejects with `{"kind":"expired"}` (see `submit_operation_error` in
    // src-tauri/src/ipc.rs), which `planErrorMessage` renders as
    // `planRefused.expired` rather than showing the JSON or this project's
    // own hardcoded English.
    submitFailures["1"] = '{"kind":"expired"}';
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    let dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not start the update: This preview is more than 10 minutes old, so Canager didn't start it. Look at the preview again, then confirm it once more.",
    );
    // The dead id is not retried on its own, and the dialog stays open so
    // the failure can be read rather than blinking away.
    expect(submittedPlanIds()).toEqual([{ planId: "1" }]);
    expect(calls("plan_operation")).toHaveLength(1);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());

    // Asking again plans again: a new id and a new preview, and still no
    // submit until the user confirms that preview.
    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(calls("plan_operation")).toHaveLength(2);
    expect(submittedPlanIds()).toEqual([{ planId: "1" }]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
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
    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }]));
    await within(dialog).findByText("Started");
    expect(useUiStore.getState().selectedUpdates).toEqual(["brew:/opt/homebrew|Formula|glib"]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("localises a stale-snapshot NotActionable refusal instead of showing the backend's JSON", async () => {
    // glib's row is actionable in this snapshot (brew is writable and
    // answering), but a genuine TOCTOU or a stale snapshot can still make
    // `Session::issue_plan`'s gate refuse it between the click and the
    // reply. The backend's rejection is JSON, not English -- see
    // `plan_operation_error` in src-tauri/src/ipc.rs -- and it must never
    // reach the page verbatim. A single item whose only plan fails never
    // gets a dialog (nothing issued to preview): the reason goes straight
    // to the page as an alert.
    planFailures.glib = '{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}';
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);

    const alert = await findByRole("alert");
    expect(alert).toHaveTextContent(
      "Couldn't prepare the update: Start Homebrew and Canager will list",
    );
    expect(alert.textContent).not.toMatch(/not_actionable/);
    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("after one item starts and the next fails, a retry re-plans and submits only the failed one", async () => {
    submitFailures["2"] = '{"kind":"expired"}';
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
      "Could not start the update: This preview is more than 10 minutes old, so Canager didn't start it. Look at the preview again, then confirm it once more.",
    );
    expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]);
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
      expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }, { planId: "3" }]),
    );
  });

  it("locks the dialog while submitting and drops a superseded batch's late reply", async () => {
    holdPlans.add("glib");
    holdSubmits.add("2");
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

    await waitFor(() => expect(releaseSubmit["2"]).toBeDefined());
    releaseSubmit["2"]();
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([{ planId: "2" }]);
    expect(updateSelected).not.toBeDisabled();
  });

  // The saved settings of the one `set_settings` call a test expects.
  function savedSettings(): Settings {
    const saves = calls("set_settings");
    expect(saves).toHaveLength(1);
    return (saves[0][1] as { settings: Settings }).settings;
  }

  it("hides the row when Never remind me is clicked, and saves its package, not a version", async () => {
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click((await findAllByRole("button", { name: "Never remind me" }))[0]);

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    expect(savedSettings().ignored_updates).toEqual([glibKey]);
    expect(savedSettings().skipped_versions).toEqual([]);
    expect(queryByText("onyx")).toBeInTheDocument();
  });

  it("hides the row when Skip this version is clicked, and saves the version it offers", async () => {
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click((await findAllByRole("button", { name: "Skip this version" }))[0]);

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    expect(savedSettings().skipped_versions).toEqual([{ key: glibKey, version: "2.90.0" }]);
    expect(savedSettings().ignored_updates).toEqual([]);
    expect(queryByText("onyx")).toBeInTheDocument();
  });

  it("replaces a package's earlier skip when its next version is skipped", async () => {
    // glib 2.89.0 was skipped; the source now offers 2.90.0, so the row is
    // back. Skipping it again records 2.90.0 in place of 2.89.0, which
    // could never hide anything again; onyx's skip is left as it was.
    settings.skipped_versions = [
      { key: glibKey, version: "2.89.0" },
      { key: onyxKey, version: "5.0.9" },
    ];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click((await findAllByRole("button", { name: "Skip this version" }))[0]);

    await waitFor(() => expect(calls("set_settings")).toHaveLength(1));
    expect(savedSettings().skipped_versions).toEqual([
      { key: onyxKey, version: "5.0.9" },
      { key: glibKey, version: "2.90.0" },
    ]);
  });

  it("lists a skipped package again once its source offers another version", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.89.0" }];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    await findByText("2 updates available");
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("leaves a skipped row out of the headline, Select all and Update selected, even one selected before", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.90.0" }];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
    });
    const { findByText, queryByText, getByRole, findByRole } = renderWithProviders(
      <UpdatesPage />,
    );

    await findByText("onyx");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(await findByText("1 update available")).toBeInTheDocument();
    // glib's selection outlived its row, and counts for nothing.
    expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

    fireEvent.click(getByRole("button", { name: "Select all items that can be updated here" }));
    fireEvent.click(getByRole("button", { name: "Update selected" }));
    await findByRole("dialog");
    expect(
      calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name),
    ).toEqual(["onyx"]);
  });

  it("offers Never remind me but no Skip this version on a row Canager could not check", async () => {
    // An uncheckable row's `target` is its installed version, not one the
    // source offered, so there is no version to skip.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

    const myFork = (await findByText("my-fork")).parentElement?.parentElement as HTMLElement;
    const glib = (await findByText("glib")).parentElement?.parentElement as HTMLElement;
    expect(within(myFork).getByRole("button", { name: "Never remind me" })).toBeInTheDocument();
    expect(within(myFork).queryByRole("button", { name: "Skip this version" })).toBeNull();
    expect(within(glib).getByRole("button", { name: "Skip this version" })).toBeInTheDocument();
    expect(getAllByRole("button", { name: "Skip this version" })).toHaveLength(1);
  });

  it("skips an Ollama model's newer build by its digest without ever printing the digest", async () => {
    settings.show_technical_details = true;
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    const digest = "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b";
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: digest,
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, getByRole, container } = renderWithProviders(<UpdatesPage />);

    await findByText("qwen3:8b");
    fireEvent.click(getByRole("button", { name: "Skip this version" }));

    await findByText(
      "No pending updates — you've skipped the rest or asked not to be reminded about them.",
    );
    expect(savedSettings().skipped_versions).toEqual([{ key: qwenKey, version: digest }]);
    expect(container.textContent).not.toMatch(/sha256|5642e974/);
  });

  it("says on each button what it will do", async () => {
    const { findAllByRole } = renderWithProviders(<UpdatesPage />);

    const skip = (await findAllByRole("button", { name: "Skip this version" }))[0];
    const never = (await findAllByRole("button", { name: "Never remind me" }))[0];
    expect(skip).toHaveAccessibleDescription(
      "You'll be reminded again when its next version is out.",
    );
    expect(never).toHaveAccessibleDescription(
      "You won't be reminded about any update of this again. You can undo this in Settings.",
    );
  });

  it("calls them 跳过这个版本 and 不再提醒 in Chinese, and says what each does", () => {
    expect(zhCN.updates.skipVersion).toBe("跳过这个版本");
    expect(zhCN.updates.skipVersionHint).toBe("你会在它出下一个版本时再看到提醒。");
    expect(zhCN.updates.neverRemind).toBe("不再提醒");
    expect(zhCN.updates.neverRemindHint).toBe("以后不再提醒这个软件的任何更新，可在设置里撤销。");
  });

  it("disables both buttons on every row while a save is pending so a second click cannot overwrite the first", async () => {
    holdSaves = true;
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    const skipButtons = await findAllByRole("button", { name: "Skip this version" });
    const neverButtons = await findAllByRole("button", { name: "Never remind me" });
    fireEvent.click(skipButtons[0]);

    // Every button locks until the first save settles. A second click now
    // would build its settings from the same stale base, and the later save
    // would drop the earlier one.
    await waitFor(() => expect(skipButtons[1]).toBeDisabled());
    for (const button of [...skipButtons, ...neverButtons]) expect(button).toBeDisabled();
    fireEvent.click(skipButtons[1]);
    fireEvent.click(neverButtons[1]);
    expect(calls("set_settings")).toHaveLength(1);

    releaseSave[0]();
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    await findByText("onyx");
    await waitFor(() => expect(skipButtons[1]).not.toBeDisabled());
    expect(neverButtons[1]).not.toBeDisabled();
    expect(calls("set_settings")).toHaveLength(1);
  });

  it("shows the backend's message when saving the choice fails", async () => {
    saveFailure = "settings.json is read-only";
    const { findByText, findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("glib");
    fireEvent.click((await findAllByRole("button", { name: "Skip this version" }))[0]);

    expect(await findByRole("alert")).toHaveTextContent(
      "Couldn't save that choice: settings.json is read-only",
    );
    // Nothing was saved, so nothing disappears.
    expect(await findByText("glib")).toBeInTheDocument();
  });

  it("says every update is hidden — not that everything is up to date — once each is skipped or never reminded about", async () => {
    settings.ignored_updates = [glibKey];
    settings.skipped_versions = [{ key: onyxKey, version: "5.1.0" }];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText(
      "No pending updates — you've skipped the rest or asked not to be reminded about them.",
    );
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("says everything is up to date only when the backend reports no updates at all", async () => {
    updates = [];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Everything is up to date");
  });
  it("does not say everything is up to date when a source never answered", async () => {
    // The lie this page used to tell. No candidates is exactly what an
    // unreachable source produces, and the early return read that silence
    // as good news: a Mac with Ollama stopped was told, in so many words,
    // that everything was up to date -- about a source Canager had not
    // managed to ask.
    updates = [];
    instances = [...snapshot.instances, stoppedOllama];
    const { findByText, queryByText, getByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("Ollama isn't running");
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    // And the notice is the working one, not a copy of its words: the
    // button that starts the daemon comes with it.
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });

  it("warns that Homebrew's catalogue may be behind, with a way to retry", async () => {
    // A note, not an unavailability: brew answered, and what it said may
    // simply be out of date. "Everything is up to date" is the one
    // sentence that must not appear over it.
    updates = [];
    instances = [
      { ...snapshot.instances[0], status: { unavailable: null, notes: ["IndexMayBeStale"] } },
      ...snapshot.instances.slice(1),
    ];
    const { findByText, queryByText, getByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("“Up to date” may not be accurate for Homebrew");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
  });

  it("does not say everything is up to date while Homebrew is still downloading its list of software", async () => {
    // Nothing has failed, so the notice is information rather than a
    // warning -- but this refresh did not check Homebrew for updates: its
    // candidates are the previous refresh's (`InstanceNote::IndexUpdating`
    // in crates/canager-core/src/model.rs), so none from it is not news
    // that there are none.
    updates = [];
    instances = [
      { ...snapshot.instances[0], status: { unavailable: null, notes: ["IndexUpdating"] } },
      ...snapshot.instances.slice(1),
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Homebrew is still downloading its latest list of software");
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("shows a silent source's notice under its own heading when there are updates as well", async () => {
    instances = [...snapshot.instances, stoppedOllama];
    const { findByText, getByText } = renderWithProviders(<UpdatesPage />);

    const glib = await findByText("glib");
    const notice = await findByText("Ollama isn't running");
    // A source with no rows has its heading first, so its Open Ollama
    // button is not scrolled out of sight under Homebrew's list.
    expect(slotOf(notice)).toBe(0);
    expect(slotOf(getByText("Ollama"))).toBe(0);
    expect(slotOf(glib)).toBeGreaterThan(slotOf(getByText("Homebrew")));
  });

  it("puts a silent source's notice over its own rows, not over another source's", async () => {
    // The critical case. Homebrew answered with three updates; Ollama did
    // not answer and its two candidates were carried forward from last
    // time. The rows carry no source name, so a notice saying "what's
    // listed here is what Canager saw the last time it did" can only be
    // read as being about the rows directly under it -- and when every
    // notice sat at the top of one flat list, that was all five, three of
    // them this minute's Homebrew data.
    const llamaKey: ArtifactKey = { ...qwenKey, name: "llama3.2:3b" };
    instances = [
      ...snapshot.instances,
      { ...stoppedOllama, status: { unavailable: "NotResponding", notes: [] } },
    ];
    updates = [
      ...snapshot.updates,
      brewCandidate("jq"),
      ...[qwenKey, llamaKey].map((key) => ({
        key,
        current: "5642e97495e1",
        target: "a1b2c3d4e5f6",
        channel: "Digest" as const,
        checkable: true,
        warnings: [],
        blocked: null,
      })),
    ];
    const { findByText, getByText } = renderWithProviders(<UpdatesPage />);

    const notice = await findByText(
      "Ollama is installed but didn't answer. What's listed here is what Canager saw the last time it did, so anything added or removed since then is missing.",
    );
    const noticeSlot = slotOf(notice);
    // The heading that names the source is in the same slot as its notice.
    expect(slotOf(getByText("Ollama"))).toBe(noticeSlot);
    // Every Homebrew row is under Homebrew's heading, above Ollama's.
    const brewSlot = slotOf(getByText("Homebrew"));
    for (const name of ["glib", "onyx", "jq"]) {
      const slot = slotOf(getByText(name));
      expect(slot).toBeGreaterThan(brewSlot);
      expect(slot).toBeLessThan(noticeSlot);
    }
    // And Ollama's two rows are the two directly under its notice.
    expect(slotOf(getByText("qwen3:8b"))).toBe(noticeSlot + 1);
    expect(slotOf(getByText("llama3.2:3b"))).toBe(noticeSlot + 2);
  });

  it("puts a read-only source's notice over its own rows, not over another source's", async () => {
    // Same shape for pip: "Canager can only show what's installed with
    // pip" used to sit above nine rows, three of which were Homebrew's and
    // perfectly updatable.
    const pipPackages = ["urllib3", "requests", "certifi", "idna", "charset-normalizer", "six"];
    updates = [
      ...snapshot.updates,
      brewCandidate("jq"),
      ...pipPackages.map((name) => ({
        key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
        current: "1.0.0",
        target: "1.1.0",
        channel: "Registry" as const,
        checkable: true,
        warnings: [],
        blocked: null,
      })),
    ];
    const { findByText, getByText } = renderWithProviders(<UpdatesPage />);

    const noticeSlot = slotOf(await findByText(/Canager can only show what's installed with pip/));
    expect(slotOf(getByText("pip"))).toBe(noticeSlot);
    for (const name of ["glib", "onyx", "jq"]) {
      expect(slotOf(getByText(name))).toBeLessThan(noticeSlot);
    }
    for (const name of pipPackages) {
      expect(slotOf(getByText(name))).toBeGreaterThan(noticeSlot);
    }
  });

  it("tells the truth about carried-forward rows on this page, both ways round", async () => {
    // The notice is above its source's rows, so what it says about them
    // has to match them. Homebrew is silent and its two candidates were
    // carried forward, so they are last time's answer and the user needs
    // telling.
    instances = [
      { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
      ...snapshot.instances.slice(1),
    ];
    const withRows = renderWithProviders(<UpdatesPage />);

    await withRows.findByText("glib");
    expect(
      await withRows.findByText(
        "Homebrew is installed but didn't answer. What's listed here is what Canager saw the last time it did, so anything added or removed since then is missing.",
      ),
    ).toBeInTheDocument();
    withRows.unmount();

    // And the cold start, which is every launch: the snapshot is in
    // memory only, so the first refresh has nothing to carry forward and
    // the same notice sat above an empty group promising rows.
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const coldStart = renderWithProviders(<UpdatesPage />);

    await coldStart.findByText("urllib3");
    expect(
      await coldStart.findByText(
        "Homebrew is installed but didn't answer, so Canager doesn't know what's in it right now.",
      ),
    ).toBeInTheDocument();
    expect(coldStart.queryByText(/What's listed here/)).not.toBeInTheDocument();
  });

  it("offers no Update button for a row carried forward from a source that isn't answering", async () => {
    // `refresh` keeps an unavailable source's last known candidates rather
    // than dropping them, so this row is on screen -- and `ollama pull`
    // against a daemon that is not listening cannot succeed. Offering the
    // button and then refusing the click is the pattern this phase exists
    // to remove; the count says what is really on offer instead.
    instances = [...snapshot.instances, stoppedOllama];
    updates = [
      ...snapshot.updates,
      {
        key: qwenKey,
        current: "5642e97495e1",
        target: "a1b2c3d4e5f6",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    const row = (await findByText("qwen3:8b")).closest("li, div") as HTMLElement;
    expect(within(row).queryByRole("button", { name: "Update" })).toBeNull();
    expect(within(row).queryByRole("checkbox")).toBeNull();
    expect(await findByText("2 updates available")).toBeInTheDocument();
    expect(await findByText("1 more can't be updated here")).toBeInTheDocument();
    // The two brew rows still have their buttons: one silent source does
    // not disarm the page.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });
  it("draws only the rows on screen when a failed lookup turns every package into a row", async () => {
    // Offline, a source cannot establish any remote version, so it reports
    // one `checkable: false` candidate per installed package instead of
    // none -- the page that already has to explain "we could not check"
    // is also the page asked to draw several hundred rows. The old
    // deferral ("Homebrew only, a few dozen rows") died with phase 3.
    updates = Array.from({ length: 400 }, (_, index) => ({
      key: {
        instance_id: "pip:/usr/bin/python3",
        kind: "Package" as const,
        name: `pkg-${String(index).padStart(3, "0")}`,
      },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Registry" as const,
      checkable: false,
      warnings: [{ Message: "Could not reach pypi.org" }],
      blocked: null,
    }));

    const { findByText, container } = renderWithProviders(<UpdatesPage />);

    await findByText("pkg-000");
    const drawn = container.querySelectorAll("[data-index]").length;
    expect(drawn).toBeGreaterThan(0);
    // 600px of viewport over 56px rows is about eleven rows plus the
    // virtualizer's overscan; anything near 400 means the whole list is in
    // the DOM.
    expect(drawn).toBeLessThan(40);
    expect(container.textContent).not.toContain("pkg-399");
    // The list still knows how long it is, so the scrollbar is honest and
    // every row is reachable.
    expect(await findByText("400 listed below")).toBeInTheDocument();
  });
  it("says which version you are moving to, with technical details off", async () => {
    // Spec §6: the one screen whose job is "look before you act". It named
    // the package, the command and the warnings, and never the version --
    // unless the technical-details switch happened to be on, which for this
    // audience it is not. A confirmation that hides what changes is not a
    // confirmation.
    expect(settings.show_technical_details).toBe(false);

    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");

    expect(await within(dialog).findByText("2.88.3 → 2.90.0")).toBeInTheDocument();
  });

  it("never shows an Ollama model's two digests as a version jump in the confirmation", async () => {
    // `current` is the local manifest digest and `target` is the registry
    // manifest's config digest: different hash spaces, unequal even after a
    // successful pull, and not something to put in front of this audience
    // either way. The row already knows this; the dialog has to as well.
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];

    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");

    expect(
      await within(dialog).findByText("A newer build of this model is available"),
    ).toBeInTheDocument();
    expect(within(dialog).queryByText(/sha256:/)).toBeNull();
    expect(within(dialog).queryByText(/→/)).toBeNull();
  });

  const claudeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
  const claudeInstance: Snapshot["instances"][number] = {
    id: "standalone-claude",
    adapter_id: "standalone-claude",
    exe_path: "/Users/someone/.local/bin/claude",
    prefix: "/Users/someone/.local/share/claude",
    scope: "User",
    version: "2.1.281",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
  const claudeArtifact: Snapshot["artifacts"][number] = {
    key: claudeKey,
    display_name: "Claude Code",
    version: "2.1.281",
    reason: "Requested",
    description: null,
    homepage: "https://code.claude.com/docs/en/setup",
    size_bytes: null,
    installed_at: null,
    path: "/Users/someone/.local/share/claude/versions/2.1.281",
    auto_updates: true,
    uninstall_blocked: "NoSafeMethod",
  };
  const claudeUpdate: Snapshot["updates"][number] = {
    key: claudeKey,
    current: "2.1.281",
    target: "2.1.290",
    channel: "Registry",
    checkable: true,
    warnings: [],
    blocked: null,
  };

  it("says a self-updating standalone tool will probably update itself, and still offers the button", async () => {
    // Spec D5: the badge is real (read from the launcher's live version),
    // so the row keeps its Update button; the honest sentence says the
    // tool usually does this itself. `auto_updates`'s first reader beyond
    // the pinned copy.
    instances = [...snapshot.instances, claudeInstance];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    const { findByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

    const hint = await findByText(
      "This copy is behind (2.1.281 → 2.1.290). Claude Code usually updates itself the next time you run it; you can update it now with Canager, or just run it.",
    );
    expect(hint).toBeInTheDocument();
    // An explanation, not a blurb: its second half is what the user can
    // do, so the row must not clip it to one line. jsdom applies no CSS,
    // so the class is what says the real window shows all of it.
    expect(hint.className).not.toContain("truncate");
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
  });

  it("keeps a self-updating Homebrew cask's own blurb: the hint is for tools that update themselves, not for --greedy", async () => {
    // A cask listed through `include_self_updating` carries
    // `auto_updates: true` too, but Homebrew, not the app, is what the
    // button drives; its row keeps its description.
    settings = { ...settings, include_self_updating: true };
    updates = [snapshot.updates[1]];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: true,
        uninstall_blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Verify system files structure")).toBeInTheDocument();
    expect(queryByText(/usually updates itself/)).toBeNull();
  });

  it("gives a standalone tool's row the summary its Installed row shows, not 'No description available'", async () => {
    // A standalone artifact's `description` is `null` on the wire (a bare
    // string cannot be localised), so its sentence is looked up by adapter
    // id, on this page as on the Installed page (`artifactBlurb`). Grok
    // Build is not called self-updating (`auto_updates: false`: whether it
    // installs updates on its own is unverified), so its actionable row
    // gets no self-updating hint and shows that sentence.
    const grokKey: ArtifactKey = { instance_id: "standalone-grok", kind: "Binary", name: "grok" };
    instances = [
      ...snapshot.instances,
      {
        id: "standalone-grok",
        adapter_id: "standalone-grok",
        exe_path: "/Users/someone/.grok/bin/grok",
        prefix: "/Users/someone/.grok",
        scope: "User",
        version: "1.0.41",
        status: { unavailable: null, notes: [] },
        unverified_version: null,
        read_only_reason: null,
      },
    ];
    updates = [
      {
        key: grokKey,
        current: "1.0.41",
        target: "1.0.42",
        channel: "Native",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    artifacts = [
      {
        key: grokKey,
        display_name: "Grok Build",
        version: "1.0.41",
        reason: "Requested",
        description: null,
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: "/Users/someone/.grok/downloads/grok-1.0.41-macos-aarch64",
        auto_updates: false,
        uninstall_blocked: null,
      },
    ];
    const { findByText, queryByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

    expect(
      await findByText(
        "xAI's Grok coding assistant for the terminal. Installed with its own installer.",
      ),
    ).toBeInTheDocument();
    expect(queryByText("No description available")).toBeNull();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
  });

  it("gives a standalone row that cannot be checked its reason, not the self-updating hint", async () => {
    instances = [...snapshot.instances, claudeInstance];
    updates = [
      {
        ...claudeUpdate,
        target: "2.1.281",
        checkable: false,
        warnings: [{ Message: "downloads.claude.ai request failed: network error: offline" }],
      },
    ];
    artifacts = [claudeArtifact];
    const { findByText, queryByText, queryAllByRole } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Canager couldn't check this one for updates.")).toBeInTheDocument();
    expect(queryByText(/usually updates itself/)).toBeNull();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("gives no self-updating hint to a standalone row whose source did not answer: it has no button to offer", async () => {
    // A candidate carried forward from a Claude Code that did not answer
    // the last refresh has no Update button (`isUpdateActionable` needs
    // `isAvailable`), so "you can update it now with Canager" would point
    // at a button that is not there.
    instances = [
      ...snapshot.instances,
      { ...claudeInstance, status: { unavailable: "NotResponding", notes: [] } },
    ];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    const { findByText, queryByText, queryAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("claude");
    expect(queryByText(/usually updates itself/)).toBeNull();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("offers no Update button for a tool that updates itself, and says to open it once", async () => {
    // Spec §4.4, D5 item 4: the newer version is real (read from the
    // launcher's live version), so the row stays and is counted with what
    // Canager cannot update; the tool has no update command Canager could
    // run, so there is no button and no checkbox, and the sentence says
    // what does work while its automatic updates are on -- opening the
    // tool, which checks at most every 15 minutes -- with the launcher set
    // apart as code. The claude fixtures
    // stand in for agy here: the copy record is per reason, not per tool.
    instances = [...snapshot.instances, claudeInstance];
    updates = [{ ...claudeUpdate, blocked: "SelfUpdatesOnly" }, snapshot.updates[1]];
    artifacts = [...snapshot.artifacts, claudeArtifact];
    const { findByText, getAllByRole, getByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("claude");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select onyx for update");
    expect(getByText("Updates itself")).toBeInTheDocument();
    expect(
      getByText(
        wholeSentence(
          "A newer version of Claude Code is out (2.1.281 → 2.1.290), and Claude Code installs updates itself in the background — Canager doesn't have a safe way to do it for you. Open it once (run /Users/someone/.local/bin/claude in Terminal, then quit it): unless its automatic updates have been turned off, it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background.",
        ),
      ),
    ).toBeInTheDocument();
    expect(getByText("/Users/someone/.local/bin/claude").tagName).toBe("CODE");
    // Not the actionable row's hint: this row has no button to point at.
    expect(queryByText(/usually updates itself/)).toBeNull();
    await findByText("1 update available");
    await findByText("1 more can't be updated here");
  });

  // The four PATH notes (spec §七), each with the title of the notice it
  // puts under the source's heading.
  const pathNotes: [InstanceNote, string][] = [
    ["NotOnPath", "Claude Code isn't in your PATH"],
    ["ShadowedByHomebrew", "Another program named claude runs when you type claude"],
    ["ShadowedByNpm", "Another program named claude runs when you type claude"],
    ["ShadowedByOther", "Another program named claude runs when you type claude"],
  ];

  it.each(pathNotes)(
    "tells a self-updating standalone copy under a %s notice that it is behind, not to just run it",
    async (note, noticeTitle) => {
      // Typing `claude` in Terminal probably does not run this copy: it
      // is not on PATH, so nothing or another program named `claude` runs
      // (`NotOnPath`), or another program with that name is found on PATH
      // before it (`ShadowedBy*`) -- the notice under this heading says
      // which. This copy updates itself only when it runs (spec §4.4), so
      // "or just run it" would leave it behind with its badge up. The row
      // keeps its button and says only that this copy is behind and that
      // Canager can update it.
      instances = [
        ...snapshot.instances,
        { ...claudeInstance, status: { unavailable: null, notes: [note] } },
      ];
      updates = [claudeUpdate];
      artifacts = [claudeArtifact];
      const { findByText, getByText, queryByText, getAllByRole } = renderWithProviders(
        <UpdatesPage />,
      );

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      expect(queryByText(/just run it/)).toBeNull();
      expect(queryByText(/usually updates itself/)).toBeNull();
      const hint = getByText(
        "This copy is behind (2.1.281 → 2.1.290). You can update it now with Canager.",
      );
      expect(hint.className).not.toContain("truncate");
      expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    },
  );

  it.each(pathNotes)(
    "says everything is up to date under a %s notice: what typing the name runs is not whether Canager could check it",
    async (note, noticeTitle) => {
      // One source, Claude Code, which answered: with no updates listed,
      // Canager read this copy's version and the published one, and the
      // published one is not newer. The note is only about what typing
      // `claude` in Terminal runs, so its notice goes above the sentence
      // and leaves the sentence alone. "No updates in the sources Canager
      // could check" would say that some source could not be checked, and
      // none here went unchecked.
      instances = [{ ...claudeInstance, status: { unavailable: null, notes: [note] } }];
      updates = [];
      artifacts = [claudeArtifact];
      const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      expect(await findByText("Everything is up to date")).toBeInTheDocument();
      expect(queryByText("No updates in the sources Canager could check")).toBeNull();
    },
  );

  it("does not say everything is up to date when Claude Code's launcher is left without its program: there was no installed version to check", async () => {
    // Its program files are gone, so `StandaloneAdapter::check_updates`
    // returns before reading either version: there is no installed one to
    // compare with the published one. No updates from it means it was not
    // checked.
    instances = [
      { ...claudeInstance, version: null, status: { unavailable: null, notes: ["LauncherOnly"] } },
    ];
    updates = [];
    artifacts = [{ ...claudeArtifact, version: "", path: null }];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("The claude link is still there, but its program is gone")).toBeInTheDocument();
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).toBeNull();
  });

  it("has the behind-only sentence in Chinese too, with no 'just run it'", () => {
    expect(zhCN.updates.selfUpdatingHintNotRunByName).toBe(
      "这份落后了（{{current}} → {{target}}）。可以现在用 Canager 更新。",
    );
  });

  it("says per item, under its command, that a NoCancel update cannot be stopped once it starts", async () => {
    // A batch can mix a rustup self update (NoCancel) with a Homebrew
    // upgrade (cancellable); the sentence belongs next to the command it
    // is true of. This page is one of the two readers spec §五 gives
    // operations.noCancelHint; UninstallDialog is the other.
    noCancel.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: "Update selected" }));
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const hints = within(dialog).getAllByText(
      "Don't close Canager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
    );
    expect(hints).toHaveLength(1);
    expect(hints[0].closest("div")?.textContent).toContain("onyx");
    expect(hints[0].closest("div")?.textContent).not.toContain("glib");
  });

  describe("Select all and Invert selection", () => {
    // Each button's accessible name starts with the words on it ("Select
    // all", "Invert selection") and goes on to say which rows it acts on.
    const SELECT_ALL = "Select all items that can be updated here";
    const INVERT = "Invert selection among the items that can be updated here";

    const jq = brewCandidate("jq");
    const wget: Snapshot["updates"][number] = { ...brewCandidate("wget"), blocked: "Pinned" };
    const tree = brewCandidate("tree");
    const curl = brewCandidate("curl");

    // A row in each of the five `UpdateState`s, and two rows the page does
    // not list. Only glib, onyx and jq have a checkbox: wget is pinned,
    // my-fork could not be checked, urllib3 is pip's (read-only), qwen3:8b's
    // Ollama is not running, tree is never reminded about, and the version
    // curl offers was skipped.
    function listEveryKindOfRow() {
      instances = [...snapshot.instances, stoppedOllama];
      updates = [
        ...snapshot.updates,
        jq,
        wget,
        tree,
        curl,
        {
          key: myForkKey,
          current: "0.1.0",
          target: "0.1.0",
          channel: "Registry",
          checkable: false,
          warnings: ["NonRegistrySource"],
          blocked: null,
        },
        {
          key: urllib3Key,
          current: "2.2.1",
          target: "2.3.0",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
        {
          key: qwenKey,
          current: "5642e97495e1",
          target: "a1b2c3d4e5f6",
          channel: "Digest",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      settings.ignored_updates = [tree.key];
      settings.skipped_versions = [{ key: curl.key, version: curl.target }];
    }

    // The selection as a sorted list of ids: these tests are about which
    // rows are ticked, not the order the store keeps them in.
    function selectedIds(): string[] {
      return [...useUiStore.getState().selectedUpdates].sort();
    }

    function ids(...keys: ArtifactKey[]): string[] {
      return keys.map(artifactKeyId).sort();
    }

    function plannedNames(): string[] {
      return calls("plan_operation")
        .map(([, args]) => (args as { request: OpRequest }).request.name)
        .sort();
    }

    it("Select all ticks every row that has a checkbox and nothing else, and Update selected plans exactly those", async () => {
      listEveryKindOfRow();
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      const checkboxes = await findAllByRole("checkbox");
      expect(checkboxes).toHaveLength(3);
      const selectAll = getByRole("button", { name: SELECT_ALL });
      expect(selectAll.textContent).toBe("Select all");
      expect(getByRole("button", { name: INVERT }).textContent).toBe("Invert selection");
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

      fireEvent.click(selectAll);

      for (const checkbox of checkboxes) expect(checkbox).toBeChecked();
      expect(selectedIds()).toEqual(ids(glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: "Update selected" }));
      await findByRole("dialog");
      expect(plannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("Invert selection unticks the ticked rows that have a checkbox and ticks the unticked ones", async () => {
      listEveryKindOfRow();
      const { findByRole, getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findByRole("checkbox", { name: "Select glib for update" });
      const onyx = getByRole("checkbox", { name: "Select onyx for update" });
      const jqBox = getByRole("checkbox", { name: "Select jq for update" });
      fireEvent.click(glib);

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(glib).not.toBeChecked();
      expect(onyx).toBeChecked();
      expect(jqBox).toBeChecked();
      expect(selectedIds()).toEqual(ids(onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(glib).toBeChecked();
      expect(onyx).not.toBeChecked();
      expect(jqBox).not.toBeChecked();
      expect(selectedIds()).toEqual(ids(glibKey));

      // Inverting a full selection empties it, and Update selected follows.
      fireEvent.click(getByRole("button", { name: SELECT_ALL }));
      expect(getByRole("button", { name: "Update selected" })).toBeEnabled();
      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual([]);
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();
    });

    it("never selects a row the page does not list, and leaves a pinned row's earlier selection as it was", async () => {
      // wget was selected while it could still be updated; a refresh since
      // says it is pinned, so it has no checkbox. Neither button may take it
      // out of the selection or put tree (never reminded about) or curl
      // (skipped) into it: they act on the rows with a checkbox and nothing
      // else. Update selected still leaves wget out of the batch
      // (`isActionable`).
      listEveryKindOfRow();
      act(() => {
        useUiStore.getState().toggleUpdate(wget.key);
      });
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);
      await findAllByRole("checkbox");

      fireEvent.click(getByRole("button", { name: SELECT_ALL }));
      expect(selectedIds()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual(ids(wget.key));
      // A row with no checkbox is all that is left selected: nothing to update.
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: "Update selected" }));
      await findByRole("dialog");
      expect(plannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("disables both when no listed row has a checkbox", async () => {
      // tree and curl could be updated but are hidden, so they are not
      // listed; every row that is listed is one Canager cannot update.
      listEveryKindOfRow();
      const withCheckbox = ids(glibKey, onyxKey, jq.key);
      updates = updates.filter((u) => !withCheckbox.includes(artifactKeyId(u.key)));
      const { findByText, getByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("Nothing here can be updated by Canager");
      expect(queryByRole("checkbox")).toBeNull();
      const selectAll = getByRole("button", { name: SELECT_ALL });
      const invert = getByRole("button", { name: INVERT });
      expect(selectAll).toBeDisabled();
      expect(invert).toBeDisabled();

      fireEvent.click(selectAll);
      fireEvent.click(invert);
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    });

    it("calls them 全选 and 反选 in Chinese, as they were asked for", () => {
      expect(zhCN.updates.selectAll).toBe("全选");
      expect(zhCN.updates.invertSelection).toBe("反选");
    });
  });
});
