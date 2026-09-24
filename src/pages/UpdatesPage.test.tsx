import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
import { useUiStore } from "../store/ui";
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
    const reason = await findByText(/Canager couldn't check this one for updates just now/);
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
      queryAllByText("Canager couldn't check this one for updates just now."),
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
    expect(reason.textContent).toMatch(/^Canager couldn't check this one for updates just now\./);
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

    expect(await findByText("Canager couldn't check this one for updates just now.")).toBeInTheDocument();
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

  // The four PATH notes (spec §七), each with the title of the notice it
  // puts under the source's heading.
  const pathNotes: [InstanceNote, string][] = [
    ["NotOnPath", "Claude Code isn't in your PATH"],
    ["ShadowedByHomebrew", "Another copy runs when you type claude"],
    ["ShadowedByNpm", "Another copy runs when you type claude"],
    ["ShadowedByOther", "Another copy runs when you type claude"],
  ];

  it.each(pathNotes)(
    "tells a self-updating standalone copy under a %s notice that it is behind, not to just run it",
    async (note, noticeTitle) => {
      // Typing `claude` in Terminal probably finds nothing, or runs
      // another copy found first on PATH -- the notice under this heading
      // says which. This copy updates itself only when it runs (spec
      // §4.4), so "or just run it" would leave it behind with its badge
      // up. The row keeps its button and says only that this copy is
      // behind and that Canager can update it.
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

  it("has the behind-only sentence in Chinese too, with no 'just run it'", () => {
    expect(zhCN.updates.selfUpdatingHintNotRunByName).toBe(
      "这份落后了（{{current}} → {{target}}）。可以现在用 Canager 更新。",
    );
  });
});
