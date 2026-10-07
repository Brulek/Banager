import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders, type RenderOptions } from "../test/setup";
import { OverviewPage } from "./OverviewPage";
import { UpdatesPage } from "./UpdatesPage";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { SnapshotStatus } from "../components/SnapshotStatus";
import { BUTTON } from "../components/ui/controls";
import { refreshIntoCache } from "../lib/events";
import i18n from "../i18n";
import { artifactKeyId, useUiStore } from "../store/ui";
import { useToolSetupSheet } from "../lib/toolSetupCheck";
import type {
  ArtifactKey,
  InstalledArtifact,
  InstanceNote,
  ManagerInstance,
  OpSummary,
  Settings,
  Snapshot,
  UpdateCandidate,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { failureCause } from "../lib/failureCause";

const mockInvoke = vi.mocked(invoke);

function instance(
  id: string,
  adapterId: string,
  overrides: Partial<ManagerInstance> = {},
): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/${adapterId}/bin/${adapterId}`,
    prefix: `/opt/${adapterId}`,
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    ...overrides,
  };
}

const brew = instance("brew:/opt/homebrew", "brew");
const pip = instance("pip:/usr/bin/python3", "pip", { read_only_reason: "ByDesign" });
const stoppedOllama = instance("ollama:http://127.0.0.1:11434", "ollama", {
  status: { unavailable: "NotRunning", notes: [] },
});

function formula(name: string): ArtifactKey {
  return { instance_id: brew.id, kind: "Formula", name };
}

const urllib3: ArtifactKey = { instance_id: pip.id, kind: "Package", name: "urllib3" };

function candidate(key: ArtifactKey, overrides: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key,
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...overrides,
  };
}

function artifact(key: ArtifactKey): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0.0",
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
}

function snapshotWith(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 7,
    round: 7,
    detect: "Found",
    instances: [brew, pip],
    artifacts: [
      artifact(formula("glib")),
      artifact(formula("wget")),
      artifact(formula("jq")),
      artifact(urllib3),
    ],
    updates: [],
    refreshed_at: 1790586000,
    stale: false,
    errors: [],
    ...overrides,
  };
}

// The placeholder the backend starts from, before the first check.
const startupSnapshot: Snapshot = {
  generation: 0,
  round: 0,
  detect: "Missing",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

let served: Snapshot;
let settings: Settings;
// What `list_operations` answers, newest first.
let operations: OpSummary[];

beforeEach(() => {
  served = snapshotWith();
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  operations = [];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve(operations);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  useUiStore.setState({ startupRefreshError: null });
  vi.useRealTimers();
});

/** The 48 symbol at the left of the status row. */
function symbolOf(container: HTMLElement): HTMLElement {
  const symbol = container.querySelector<HTMLElement>("[data-symbol]");
  if (symbol === null) throw new Error("no symbol");
  return symbol;
}

/** The status row: its symbol, its title and the line under it, and its button. */
function statusRowOf(container: HTMLElement): HTMLElement {
  const row = container.querySelector<HTMLElement>("[data-status]");
  if (row === null) throw new Error("no status row");
  return row;
}

/** The time the tests' snapshots were checked at, and a clock 30 s later: "Checked just now". */
function clockJustAfterTheCheck() {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime((1790586000 + 30) * 1000);
}

/** As App.tsx renders it. */
function renderOverview(options?: RenderOptions) {
  return renderWithProviders(
    <SnapshotStatus showsFirstCheck showsNothingFound>
      <OverviewPage />
    </SnapshotStatus>,
    options,
  );
}

describe("OverviewPage", () => {
  it("says Checking… while the first check runs, and why it takes a while, in the status row it will answer in", async () => {
    served = startupSnapshot;
    const { findByRole, queryByText, container } = renderOverview();

    const heading = await findByRole("heading", { level: 2, name: "Checking…" });
    // The page's own layout, not a block of its own in the middle: the
    // column, the status row -- a spinner in its symbol's slot, the title,
    // why it takes a while as its line -- and the daily check's row, so
    // that nothing moves when the answer comes.
    const column = container.firstElementChild as HTMLElement;
    expect(column.className).toContain("w-[min(560px,calc(100%-40px))]");
    expect(column.className).not.toContain("justify-center");
    expect(statusRowOf(container)).toContainElement(heading);
    expect(heading.className).toContain("text-title");
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("busy");
    const spinner = symbolOf(container).querySelector("svg");
    expect(spinner).toHaveAttribute("width", "32");
    expect(spinner?.getAttribute("class")).toContain("motion-safe:animate-spinner");
    // A spinner alone, for as long as Homebrew's list update and every
    // online lookup took, looked like a window that had frozen.
    expect(heading.nextElementSibling?.textContent).toBe(
      "The first check looks up every tool's newest version online, and sometimes takes a minute or two.",
    );
    // Once the settings are in, as they are long before the first check.
    expect(await findByRole("button", { name: "Check for updates: Manually" })).toBeInTheDocument();
    expect(heading).toHaveTextContent("Checking…");
    // Nothing to press in the row yet, no number, no ring.
    expect(within(statusRowOf(container)).queryByRole("button")).toBeNull();
    expect(container.querySelector("[data-ring]")).toBeNull();
    expect(container.textContent).not.toMatch(/\d/);
    // Not "Loading…", and not "No tools to manage": the
    // placeholder is not an answer.
    expect(queryByText("Loading…")).not.toBeInTheDocument();
    expect(queryByText("No tools to manage")).not.toBeInTheDocument();
  });

  // A check that found nothing to show: in the status row, as every other
  // state, not a view of its own in the middle of the page.
  const nothingFoundStates: Array<[string, Partial<Snapshot>, string, string]> = [
    [
      "nothing installed",
      { instances: [brew], artifacts: [] },
      "No installed tools found",
      "Tools you install with Homebrew, npm and the like show up here.",
    ],
    ["no source at all", { detect: "Missing", instances: [], artifacts: [] }, "No tools to manage", "Install Homebrew first."],
  ];

  it.each(nothingFoundStates)("says %s in the status row, with Details and a grey Check Again, over the daily check", async (_name, over, title, sentence) => {
    served = snapshotWith(over);
    const { findByRole, getByRole, container } = renderOverview();

    const heading = await findByRole("heading", { level: 2, name: title });
    expect(statusRowOf(container)).toContainElement(heading);
    expect(heading.className).toContain("text-title");
    // Not the list's empty state, centred in the page.
    expect(container.querySelector("[data-empty-state]")).toBeNull();
    const column = container.firstElementChild as HTMLElement;
    expect(column.className).toContain("w-[min(560px,calc(100%-40px))]");
    // A 48 ⓘ in a circle, in outline and the muted colour, as the quiet check is.
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("info");
    const symbol = symbolOf(container).querySelector("svg");
    expect(symbol).toHaveAttribute("width", "48");
    expect(symbol?.getAttribute("class")).toContain("text-muted");
    expect(symbol?.querySelector('[fill="currentColor"]')).toBeNull();
    expect(symbol?.querySelector('g[stroke="currentColor"] circle')).not.toBeNull();
    // Its sentence under it, 11 muted, and Details, a link, on what
    // Banager works with and where it looks.
    const line = heading.nextElementSibling as HTMLElement;
    expect(line.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    expect(line).toHaveTextContent(sentence);
    const details = within(line).getByRole("button", { name: `Details: ${title}` });
    expect(details).toHaveTextContent("Details");
    expect(details.className).toContain("text-accent-text");
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Supports Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and Claude Code, Antigravity CLI, Grok Build and rustup in their default locations.",
    );
    // On the right, the row's one button: a grey Check Again.
    const [button, ...more] = within(statusRowOf(container)).getAllByRole("button").filter(
      (element) => element.closest("h2, [data-status-line]") === null,
    );
    expect(more).toEqual([]);
    expect(button).toHaveAccessibleName("Check Again");
    expect(button.className).toBe(BUTTON.regular.grey);
    // The daily check under it, as in every other state.
    expect(getByRole("button", { name: "Check for updates: Manually" })).toBeInTheDocument();
    fireEvent.click(button);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
  });

  it("says in Chinese that nothing was found, in the status row", async () => {
    served = snapshotWith({ instances: [brew], artifacts: [] });
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByRole, container } = renderOverview();
      const heading = await findByRole("heading", { level: 2, name: "没有找到已安装的工具" });
      const line = heading.nextElementSibling as HTMLElement;
      expect(line).toHaveTextContent("用Homebrew、npm等安装的工具会显示在这里。");
      expect(within(line).getByRole("button", { name: "详情：没有找到已安装的工具" })).toHaveTextContent("详情");
      expect(within(statusRowOf(container)).getByRole("button", { name: "重新检查" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("counts the updates the Updates page offers, and Review updates ticks exactly those and opens it", async () => {
    // glib and wget can be updated; jq is pinned, urllib3's source is
    // read-only, ffmpeg is never to be reminded about.
    served = snapshotWith({
      updates: [
        candidate(formula("glib")),
        candidate(formula("jq"), { blocked: "Pinned" }),
        candidate(urllib3),
        candidate(formula("ffmpeg")),
        candidate(formula("wget")),
      ],
    });
    settings.ignored_updates = [formula("ffmpeg")];
    useUiStore.setState({ page: "overview" });
    clockJustAfterTheCheck();
    const { findByRole, container } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "2 tools can be updated" });
    // 13/16 bold, the one title of the row; under it, when the sources
    // were last checked.
    expect(headline.className).toContain("text-title");
    expect(headline.nextElementSibling?.textContent).toBe("Checked just now");
    expect(headline.nextElementSibling?.className).toContain("text-small");
    expect(headline.nextElementSibling?.className).toContain("text-muted");
    // The accent's arrow down, in a disc as wide as the 48 slot, which
    // starts at the group's inset, 10 in: the disc's edge is the row's.
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("updates");
    const disc = symbolOf(container).querySelector("svg");
    expect(disc?.getAttribute("class")).toContain("text-accent");
    expect(disc).toHaveAttribute("width", "48");
    expect(disc).toHaveAttribute("viewBox", "2 2 20 20");
    expect(disc?.querySelector("circle")).toHaveAttribute("r", "10");
    expect(statusRowOf(container).className).toContain("px-2.5");
    // The number is said once on the page, in the title.
    expect(container.textContent?.match(/2/g)).toHaveLength(1);
    // The row's one button, the default one, of the regular size.
    const review = await findByRole("button", { name: "Review Updates" });
    expect(within(statusRowOf(container)).getByRole("button")).toBe(review);
    expect(review.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "bg-accent"]));

    fireEvent.click(review);

    const state = useUiStore.getState();
    expect(state.page).toBe("updates");
    expect(state.selectedUpdates).toEqual([
      artifactKeyId(formula("glib")),
      artifactKeyId(formula("wget")),
    ]);
  });

  it("leaves out an update under way or just done, and Review updates does not tick it", async () => {
    served = snapshotWith({
      updates: [candidate(formula("glib")), candidate(formula("jq")), candidate(formula("wget"))],
    });
    const upgrade = (id: number, name: string, fields: Partial<OpSummary>): OpSummary => ({
      id,
      kind: "Upgrade",
      instance_id: brew.id,
      artifact_kind: "Formula",
      name,
      status: "Running",
      outcome: null,
      argv_preview: [],
      cancel_policy: "KillThenReconcile",
      ...fields,
    });
    operations = [upgrade(9, "jq", { status: "Done", outcome: "Succeeded" }), upgrade(8, "glib", {})];
    useUiStore.setState({ page: "overview", selectedUpdates: [], updateTargets: { 9: "1.1.0" } });
    const { findByRole } = renderOverview();

    expect(await findByRole("heading", { level: 2, name: "1 tool can be updated" })).toBeInTheDocument();
    fireEvent.click(await findByRole("button", { name: "Review Updates" }));
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(formula("wget"))]);
  });

  it("says it is updating while every update it offered is under way, and See progress opens the Updates page", async () => {
    served = snapshotWith({
      updates: [candidate(formula("glib")), candidate(formula("jq"))],
    });
    const upgrade = (id: number, name: string, fields: Partial<OpSummary>): OpSummary => ({
      id,
      kind: "Upgrade",
      instance_id: brew.id,
      artifact_kind: "Formula",
      name,
      status: "Running",
      outcome: null,
      argv_preview: [],
      cancel_policy: "KillThenReconcile",
      ...fields,
    });
    // jq is done and waits for the refresh; glib is still going.
    operations = [upgrade(9, "jq", { status: "Done", outcome: "Succeeded" }), upgrade(8, "glib", {})];
    useUiStore.setState({ page: "overview", selectedUpdates: [], updateTargets: { 9: "1.1.0" } });
    const { findByRole, queryByRole, container } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "Updating 1 tool" });
    // A spinner, and no line: when the sources were checked is not news
    // while they install.
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("busy");
    expect(symbolOf(container).querySelector("svg")).toHaveAttribute("width", "32");
    expect(headline.nextElementSibling).toBeNull();
    expect(queryByRole("button", { name: "Review Updates" })).toBeNull();
    const progress = await findByRole("button", { name: "See Progress" });
    // Grey: nothing on this page to start.
    expect(progress.className).toContain("bg-fill");
    expect(progress.className).not.toContain("bg-accent");
    fireEvent.click(progress);
    expect(useUiStore.getState().page).toBe("updates");
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  // walk-4 W4-1, on the Overview: after Update All, updates stopped where
  // sudo wanted the Mac's password. The Updates page and the operation
  // bar say 「2个需要输入密码」; the Overview said "Nothing to update".
  describe("updates that stopped for the password", () => {
    const SUDO_NO_TERMINAL =
      "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper\nsudo: a password is required";
    const stopped = (id: number, name: string): OpSummary => ({
      id,
      kind: "Upgrade",
      instance_id: brew.id,
      artifact_kind: "Formula",
      name,
      status: "Done",
      outcome: { Failed: { exit_code: 1, summary: SUDO_NO_TERMINAL, cause: failureCause(SUDO_NO_TERMINAL) } },
      argv_preview: [],
      cancel_policy: "KillThenReconcile",
    });
    beforeEach(() => {
      operations = [stopped(10, "jq"), stopped(9, "glib")];
      useUiStore.setState({ page: "overview", selectedUpdates: [], updateTargets: { 9: "1.1.0", 10: "1.1.0" } });
    });

    it("says how many need the password, with a warning and Review Updates, never Nothing to update", async () => {
      served = snapshotWith({ updates: [candidate(formula("glib")), candidate(formula("jq"))] });
      const { findByRole, queryByRole, container } = renderOverview();

      const headline = await findByRole("heading", { level: 2, name: "2 updates need your password" });
      expect(queryByRole("heading", { level: 2, name: "Nothing to update" })).toBeNull();
      expect(symbolOf(container).getAttribute("data-symbol")).toBe("failed");
      expect(statusRowOf(container)).toContainElement(headline);
      // The one thing to do: the Updates page, where each row has the steps.
      const review = await findByRole("button", { name: "Review Updates" });
      expect(review.className).toContain("bg-accent");
      fireEvent.click(review);
      expect(useUiStore.getState().page).toBe("updates");
      // Nothing to select: Terminal finishes them.
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    });

    it("says them under the headline beside an update still to review", async () => {
      served = snapshotWith({
        updates: [candidate(formula("glib")), candidate(formula("jq")), candidate(formula("wget"))],
      });
      const { findByRole } = renderOverview();

      const headline = await findByRole("heading", { level: 2, name: "1 tool can be updated" });
      expect(headline.nextElementSibling).toHaveTextContent("2 need your password");
    });

    it.each([
      ["zh-CN", "2个更新需要输入密码", "没有要更新的工具"],
      ["zh-Hant", "2個更新需要輸入密碼", "沒有要更新的工具"],
    ])("says it in %s too", async (language, title, nothing) => {
      await i18n.changeLanguage(language);
      try {
        served = snapshotWith({ updates: [candidate(formula("glib")), candidate(formula("jq"))] });
        const { findByRole, queryByRole } = renderOverview();

        expect(await findByRole("heading", { level: 2, name: title })).toBeInTheDocument();
        expect(queryByRole("heading", { level: 2, name: nothing })).toBeNull();
      } finally {
        await i18n.changeLanguage("en");
      }
    });
  });

  it("keeps a row the user had already selected when Review updates adds the rest", async () => {
    served = snapshotWith({
      updates: [candidate(formula("glib")), candidate(formula("wget"))],
    });
    useUiStore.setState({ selectedUpdates: [artifactKeyId(formula("wget"))] });
    const { findByRole } = renderOverview();

    fireEvent.click(await findByRole("button", { name: "Review Updates" }));

    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(formula("wget")),
      artifactKeyId(formula("glib")),
    ]);
  });

  it("says everything is up to date with a green check when every source answered and nothing needs updating", async () => {
    clockJustAfterTheCheck();
    const { findByRole, queryByRole, container } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "Everything is up to date" });
    expect(headline.nextElementSibling?.textContent).toBe("Checked just now");
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("upToDate");
    expect(symbolOf(container).querySelector("svg.text-success")).not.toBeNull();
    // The row's one button: a grey Check Again, as macOS's empty states
    // have, which checks again.
    const again = within(statusRowOf(container)).getByRole("button");
    expect(again).toHaveAccessibleName("Check Again");
    expect(again.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "bg-fill"]));
    expect(again.className).not.toContain("bg-accent");
    expect(queryByRole("button", { name: "Review Updates" })).not.toBeInTheDocument();
    fireEvent.click(again);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    // No problem to list: the status and the daily check, nothing else.
    expect(queryByRole("list", { name: "Needs attention" })).toBeNull();
  });

  // Each of these has nothing to install and is not up to date: a source
  // was not checked this time, and the Overview names it (decision I22),
  // never "Everything is up to date" over any of them. Under the headline,
  // the line that says what there is instead, or none; and Review updates
  // only where the Updates page lists a row.
  const notUpToDate: Array<[string, () => void, string | null, boolean, string]> = [
    [
      "a source that is not running",
      () => {
        served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
      },
      // "Needs attention" says it.
      null,
      false,
      "Ollama wasn't checked this time; everything else is up to date",
    ],
    [
      "Homebrew still updating its list of software",
      () => {
        const note: InstanceNote = "IndexUpdating";
        served = snapshotWith({
          instances: [{ ...brew, status: { unavailable: null, notes: [note] } }, pip],
        });
      },
      null,
      false,
      "Homebrew wasn't checked this time; everything else is up to date",
    ],
    // Homebrew still reads as answering, with no note: `refresh` keeps a
    // source whose update check failed as it was, and carries its last
    // candidates forward -- none, here -- with a `SourceError`.
    [
      "a check that failed this round",
      () => {
        served = snapshotWith({
          stale: true,
          errors: [{ instance_id: brew.id, message: "brew outdated exited with code 1" }],
        });
      },
      // "Needs attention" says it, as the lists' first line does.
      null,
      false,
      "Homebrew wasn't fully checked this time; everything else is up to date",
    ],
    [
      "a source whose detection failed this round",
      () => {
        served = snapshotWith({
          stale: true,
          errors: [{ instance_id: "npm", message: "internal error detecting this source" }],
        });
      },
      null,
      false,
      "npm wasn't checked this time; everything else is up to date",
    ],
  ];

  // Every source checked this time, nothing to install, and the Updates
  // page lists something besides: the all good, its green check, and what
  // is listed besides under it (decision I22). The Updates page, which
  // lists those rows, still does not say "Everything is up to date".
  const allGoodBesides: Array<[string, () => void, string, boolean]> = [
    [
      "only updates Banager cannot install",
      () => {
        served = snapshotWith({
          updates: [candidate(formula("jq"), { blocked: "Pinned" }), candidate(urllib3)],
        });
      },
      "2 can't be updated here",
      true,
    ],
    [
      "only updates the user hid",
      () => {
        served = snapshotWith({ updates: [candidate(formula("glib"))] });
        settings.ignored_updates = [formula("glib")];
      },
      // The Updates page lists no row of it: nothing there to review.
      "1 hidden",
      false,
    ],
  ];

  it.each(allGoodBesides)("says what can be updated here is up to date, with %s", async (_name, arrange, line, review) => {
    arrange();
    const { getByRole, queryByRole, queryByText, container } = renderWithProviders(
      <>
        <SnapshotStatus showsFirstCheck>
          <OverviewPage />
        </SnapshotStatus>
        <UpdatesToolbar>
          <UpdatesPage />
        </UpdatesToolbar>
      </>,
    );
    const title = "Everything you can update here is up to date";
    await waitFor(() => expect(getByRole("heading", { level: 2, name: title })).toBeInTheDocument());
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("upToDate");
    expect(getByRole("heading", { level: 2, name: title }).nextElementSibling?.textContent).toBe(line);
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    const [button, ...more] = within(statusRowOf(container)).getAllByRole("button").filter(
      (element) => element.closest("h2, [data-status-line]") === null,
    );
    expect(more).toEqual([]);
    expect(button.className).toContain("bg-fill");
    expect(button).toHaveAccessibleName(review ? "Review Updates" : "Check Again");
    if (!review) expect(queryByRole("button", { name: "Review Updates" })).not.toBeInTheDocument();
  });

  it.each(notUpToDate)("says nothing to update, not up to date, with %s", async (_name, arrange, line, review, title) => {
    arrange();
    const { getByRole, getByText, queryByRole, queryByText, container } = renderWithProviders(
      <>
        <SnapshotStatus showsFirstCheck>
          <OverviewPage />
        </SnapshotStatus>
        {/* With the toolbar's subtitle, where the page says how many it can update. */}
        <UpdatesToolbar>
          <UpdatesPage />
        </UpdatesToolbar>
      </>,
    );

    await waitFor(() => expect(getByRole("heading", { level: 2, name: title })).toBeInTheDocument());
    expect(queryByRole("heading", { name: "Everything is up to date" })).not.toBeInTheDocument();
    // Not the green check: a check in a circle, in outline and the muted
    // colour -- a mark, not news, and not a disabled-looking grey disc.
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("quiet");
    expect(symbolOf(container).querySelector(".text-success")).toBeNull();
    expect(symbolOf(container).querySelector("svg")?.getAttribute("class")).toContain("text-muted");
    expect(symbolOf(container).querySelector('[fill="currentColor"]')).toBeNull();
    expect(symbolOf(container).querySelector('g[stroke="currentColor"] circle')).not.toBeNull();
    // The Updates page, on the same snapshot, does not say it either.
    await waitFor(() =>
      expect(
        getByText(
          (_content, element) =>
            element?.tagName === "P" &&
            [
              "No updates in the sources checked",
              "Nothing to update here",
              "No updates to install",
            ].includes(element.textContent ?? ""),
        ),
      ).toBeInTheDocument(),
    );
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    const headline = getByRole("heading", { level: 2, name: title });
    if (line === null) {
      // Nothing instead to say: when the sources were last checked.
      expect(headline.nextElementSibling?.textContent).toMatch(/^Checked /);
    } else {
      expect(headline.nextElementSibling?.textContent).toBe(line);
    }
    // One button in the row, whatever the state: Review Updates where the
    // Updates page lists rows, else Check Again, both grey.
    const [button, ...more] = within(statusRowOf(container)).getAllByRole("button").filter(
      (element) => element.closest("h2, [data-status-line]") === null,
    );
    expect(more).toEqual([]);
    expect(button.className).toContain("bg-fill");
    if (review) {
      expect(button).toHaveAccessibleName("Review Updates");
    } else {
      expect(button).toHaveAccessibleName("Check Again");
      expect(queryByRole("button", { name: "Review Updates" })).not.toBeInTheDocument();
    }
  });

  it("says when a source that did not answer last did, in the lists' own sentence (R12)", async () => {
    // A clock of local times, so the day does not hang on the machine's time zone.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 8, 28, 10, 0));
    const at = new Date(2026, 8, 28, 9, 12);
    served = snapshotWith({
      instances: [
        { ...brew, answered_at: at.getTime() / 1000, status: { unavailable: "NotResponding", notes: [] } },
        pip,
      ],
    });
    const nine = new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(at);
    const first = renderOverview();
    const row = within(await first.findByRole("list", { name: "Needs attention" })).getAllByRole("listitem")[0];
    expect(row).toHaveTextContent(
      `3 tools were installed with Homebrew. It didn't respond this time, so they're shown as they were when it last responded at ${nine} today. Check again later.`,
    );
    first.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      const { findByRole } = renderOverview();
      const zhRow = within(await findByRole("list", { name: "需要查看" })).getAllByRole("listitem")[0];
      expect(zhRow).toHaveTextContent(
        "有3个工具是用Homebrew安装的。Homebrew这次没有响应，显示的是它今天09:12响应时的结果，请稍后重新检查。",
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says the checks that did not finish once, as the problems group's first row, with Check Again", async () => {
    // It was a band over the page -- 「部分检查未完成」, Homebrew's name --
    // and a count under the headline, "2 checks didn't finish", for an
    // Apple-silicon Mac with Homebrew in /opt/homebrew and /usr/local,
    // offline: the same fact twice, and a count of neither. Now it is the
    // row the lists' first line is, naming Homebrew once, and the line
    // under the headline says when the check was.
    const intel = instance("brew:/usr/local", "brew");
    served = snapshotWith({
      instances: [brew, intel, pip, stoppedOllama],
      stale: true,
      errors: [
        { instance_id: brew.id, message: "brew update failed" },
        { instance_id: intel.id, message: "brew update failed" },
      ],
    });
    const { findByRole, getByRole, queryByText, container } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    const rows = within(list).getAllByRole("listitem");
    // First, before the sources' own warnings.
    expect(within(rows[0]).getByText("Some checks didn't finish")).toBeInTheDocument();
    expect(within(rows[0]).getByText("Homebrew didn't finish checking this time; some updates may not be listed yet.")).toBeInTheDocument();
    expect(rows[0].querySelector("svg")?.getAttribute("class")).toContain("text-warning");
    expect(within(rows[1]).getByText("Ollama isn't running")).toBeInTheDocument();
    // Said once: no band over the page, no count under the headline.
    expect(container.querySelector('[role="status"]')).toBeNull();
    expect(queryByText(/^\d+ checks? didn't finish$/)).toBeNull();
    expect(within(container).getAllByText("Some checks didn't finish")).toHaveLength(1);
    // The headline names them (I22): both Homebrews as one, as the row does.
    const headline = getByRole("heading", {
      level: 2,
      name: "Homebrew and Ollama weren't fully checked this time; everything else is up to date",
    });
    expect(headline.nextElementSibling?.textContent).toMatch(/^Checked /);
    // Its button checks again, as the toolbar's does.
    const again = within(rows[0]).getByRole("button", { name: "Check Again" });
    // A regular grey one, centred on the title's line (p1 polish).
    expect(again.className).toBe(`-mt-1 ${BUTTON.regular.grey}`);
    fireEvent.click(again);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
  });

  // Opus review finding 2: a login shell too slow to read left PATH as
  // Finder's four folders, and only Check Tool Setup said so.
  it("says first among the problems that Terminal's settings couldn't be read, with Check Again", async () => {
    let facts = { login_path: false };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") return Promise.resolve(operations);
      if (cmd === "get_system_facts") return Promise.resolve(facts);
      if (cmd === "refresh") return Promise.resolve(served);
      return Promise.resolve(undefined);
    });
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    const rows = within(list).getAllByRole("listitem");
    expect(within(rows[0]).getByText("Couldn't read Terminal's settings")).toBeInTheDocument();
    expect(
      within(rows[0]).getByText(
        "Tools that Terminal finds may be missing here, such as those installed with npm, pipx, uv or Cargo.",
      ),
    ).toBeInTheDocument();
    expect(rows[0].querySelector("svg")?.getAttribute("class")).toContain("text-warning");
    expect(within(rows[1]).getByText("Ollama isn't running")).toBeInTheDocument();
    // Check Again reads the shell once more (`AppState::read_login_path`);
    // this time it is read, and the row goes with the round's new facts.
    facts = { login_path: true };
    served = snapshotWith({ instances: [brew, pip, stoppedOllama], generation: 8, round: 8 });
    fireEvent.click(within(rows[0]).getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    await waitFor(() => expect(within(list).queryByText("Couldn't read Terminal's settings")).toBeNull());
  });

  it("says how many tools could not be checked in place of when the check was, and why in a row with Check Again", async () => {
    // Walk-2 W2-1: offline, the line under 「3个工具可以更新」 said
    // 「上次检查：刚才」 while 21 tools could not be checked, as if the
    // check had worked, and why was folded away on the Updates page. The
    // headline's number, what can be updated, is what it was. The count is
    // said once, in the line; the row says what it means, why and what to
    // do (walk-2 review 1.4).
    clockJustAfterTheCheck();
    const offline: UpdateCandidate["warnings"] = [
      { Message: "pip list --outdated: Failed to establish a new connection: [Errno 8] nodename nor servname provided" },
      "TransientLookupFailure",
    ];
    const certifi: ArtifactKey = { instance_id: pip.id, kind: "Package", name: "certifi" };
    const idna: ArtifactKey = { instance_id: pip.id, kind: "Package", name: "idna" };
    served = snapshotWith({
      updates: [
        candidate(formula("glib")),
        candidate(urllib3, { checkable: false, target: "1.0.0", warnings: offline }),
        candidate(certifi, { checkable: false, target: "1.0.0", warnings: offline }),
        // No check will find a crate not from crates.io, nor mend a 404:
        // not counted (walk-2 review 1.1).
        candidate(formula("jq"), { checkable: false, target: "1.0.0", warnings: ["NonRegistrySource"] }),
        candidate(idna, { checkable: false, target: "1.0.0", warnings: [{ Message: "PyPI returned status 404" }] }),
      ],
    });
    const { findByRole, getByRole, container } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "1 tool can be updated" });
    expect(headline.nextElementSibling?.textContent).toBe("2 tools couldn't be checked");
    expect(statusRowOf(container).textContent).not.toMatch(/Checked/);
    expect(getByRole("button", { name: "Review Updates" }).className).toContain("bg-accent");

    const rows = within(getByRole("list", { name: "Needs attention" })).getAllByRole("listitem");
    expect(within(rows[0]).getByText("Some updates may not be listed")).toBeInTheDocument();
    expect(
      within(rows[0]).getByText("The connection failed. Check your internet connection, then try again."),
    ).toBeInTheDocument();
    expect(rows[0].textContent).not.toMatch(/couldn't be checked/);
    expect(rows[0].querySelector("svg")?.getAttribute("class")).toContain("text-warning");
    fireEvent.click(within(rows[0]).getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
  });

  it("says nothing of a check that failed in a way no later check will mend", async () => {
    // A model Ollama's registry answers 404 for, Antigravity CLI on an
    // Intel Mac: without `TransientLookupFailure` the warning and its
    // Check Again would stay for good (walk-2 review 1.1).
    clockJustAfterTheCheck();
    served = snapshotWith({
      updates: [
        candidate(formula("glib")),
        candidate(urllib3, { checkable: false, target: "1.0.0", warnings: [{ Message: "PyPI returned status 404" }] }),
      ],
    });
    const { findByRole, queryByRole } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "1 tool can be updated" });
    expect(headline.nextElementSibling?.textContent).toMatch(/^Checked /);
    expect(queryByRole("list", { name: "Needs attention" })).toBeNull();
  });

  it("says nothing to update only of what was checked where tools could not be, and counts them, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      served = snapshotWith({
        updates: [
          candidate(urllib3, {
            checkable: false,
            target: "1.0.0",
            warnings: [
              { Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" },
              "TransientLookupFailure",
            ],
          }),
        ],
      });
      const { findByRole, getByRole } = renderOverview();

      const headline = await findByRole("heading", { level: 2, name: "已检查的来源中没有可更新的工具" });
      expect(headline.nextElementSibling?.textContent).toBe("1个无法在这里更新，其中1个没有检查成功");
      const rows = within(getByRole("list", { name: "需要查看" })).getAllByRole("listitem");
      // pip's words name no cause a person knows: none is claimed.
      expect(within(rows[0]).getByText("可能还有更新没有列出")).toBeInTheDocument();
      expect(within(rows[0]).getByText("可以稍后点按“重新检查”再试。")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("claims no source was checked when every installed tool could not be", async () => {
    // Walk-2 review 1.2: "No updates in the sources checked" over a check
    // in which nothing was checked.
    const offline: UpdateCandidate["warnings"] = [{ Message: "npm error code ENOTFOUND" }, "TransientLookupFailure"];
    const certifi: ArtifactKey = { instance_id: pip.id, kind: "Package", name: "certifi" };
    served = snapshotWith({
      artifacts: [artifact(urllib3), artifact(certifi)],
      updates: [
        candidate(urllib3, { checkable: false, target: "1.0.0", warnings: offline }),
        candidate(certifi, { checkable: false, target: "1.0.0", warnings: [{ Message: "PyPI returned status 404" }] }),
      ],
    });
    const { findByRole, queryByRole } = renderOverview();

    expect(await findByRole("heading", { level: 2, name: "No tools could be checked" })).toBeInTheDocument();
    expect(queryByRole("heading", { level: 2, name: "No updates in the sources checked" })).toBeNull();
    await i18n.changeLanguage("zh-CN");
    try {
      expect(await findByRole("heading", { level: 2, name: "所有工具都没有检查成功" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says under Nothing to update what the Updates page has instead, in its numbers, and Review updates opens it", async () => {
    // jq is pinned and urllib3's source is read-only: listed, under "Can't
    // update here". glib is never to be reminded about and gh's version is
    // skipped: not listed. wget's skip hides no update this check found,
    // and is not counted; nor is the stopped Ollama, which "Needs
    // attention" names.
    served = snapshotWith({
      instances: [brew, pip, stoppedOllama],
      updates: [
        candidate(formula("jq"), { blocked: "Pinned" }),
        candidate(urllib3),
        candidate(formula("glib")),
        candidate(formula("gh"), { target: "2.102.0" }),
      ],
    });
    settings.ignored_updates = [formula("glib")];
    settings.skipped_versions = [
      { key: formula("gh"), version: "2.102.0" },
      { key: formula("wget"), version: "1.24.0" },
    ];
    useUiStore.setState({ page: "overview" });
    // The Updates page's list is virtualized, and measures its box and
    // its slots through these, which jsdom leaves at 0 (as in
    // UpdatesPage.test.tsx): without them it draws no slot at all.
    const height = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("data-index") === null ? 600 : 56;
      });
    const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    try {
      const { findByRole, getByRole, container } = renderWithProviders(
        <>
          <SnapshotStatus showsFirstCheck>
            <OverviewPage />
          </SnapshotStatus>
          <UpdatesPage />
        </>,
      );

      // The stopped Ollama was not checked: the headline names it (I22),
      // and of the rest, which lists rows, says only what can be updated here.
      const headline = await findByRole("heading", {
        level: 2,
        name: "Ollama wasn't checked this time; everything else you can update here is up to date",
      });
      expect(headline.nextElementSibling?.textContent).toBe("2 hidden, 2 can't be updated here");
      // The same number the Updates page gives its folded rows.
      expect(await findByRole("button", { name: "2 more can't be updated here" })).toBeInTheDocument();
      // The quiet check, and a grey button: nothing to select there.
      expect(symbolOf(container).getAttribute("data-symbol")).toBe("quiet");
      const review = getByRole("button", { name: "Review Updates" });
      expect(review.className).toContain("bg-fill");
      expect(review.className).not.toContain("bg-accent");

      fireEvent.click(review);
      expect(useUiStore.getState().page).toBe("updates");
      // Nothing on that page has a checkbox: nothing is selected.
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    } finally {
      height.mockRestore();
      width.mockRestore();
    }
  });

  it("says in Chinese, on a first launch whose Homebrew list is still downloading, only what it checked", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const note: InstanceNote = "IndexUpdating";
      served = snapshotWith({ instances: [{ ...brew, status: { unavailable: null, notes: [note] } }] });
      const { findByRole, queryByRole } = renderOverview();

      // Homebrew is the only source, and was not checked: named, and no 「其余」 (I22).
      const headline = await findByRole("heading", { level: 2, name: "Homebrew这次没检查" });
      // Nothing instead to say: when the sources were last checked.
      expect(headline.nextElementSibling?.textContent).toMatch(/^上次检查：/);
      expect(queryByRole("heading", { level: 2, name: "所有工具都是最新的" })).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says what there is instead in Chinese, one line", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      served = snapshotWith({
        updates: [
          candidate(formula("jq"), { blocked: "Pinned" }),
          candidate(formula("glib")),
          candidate(formula("wget")),
        ],
      });
      settings.ignored_updates = [formula("glib"), formula("wget")];
      const { findByRole } = renderOverview();

      // Every source checked: the all good, and the rest under it (I22).
      const headline = await findByRole("heading", { level: 2, name: "能在这里更新的都已是最新" });
      expect(headline.nextElementSibling?.textContent).toBe("2个已隐藏，1个无法在这里更新");
      expect(await findByRole("button", { name: "查看更新" })).toBeInTheDocument();
      expect(within(headline.nextElementSibling as HTMLElement).getByRole("button")).toHaveAccessibleName(
        "2个已隐藏",
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("makes the count of hidden updates the way to them in Settings, and the rest of its line text", async () => {
    // The Updates page lists no hidden update: Settings lists them all.
    served = snapshotWith({
      updates: [
        candidate(formula("jq"), { blocked: "Pinned" }),
        candidate(formula("glib")),
        candidate(formula("wget")),
      ],
    });
    settings.ignored_updates = [formula("glib"), formula("wget")];
    useUiStore.setState({ page: "overview" });
    const { findByRole } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "Everything you can update here is up to date" });
    const line = headline.nextElementSibling as HTMLElement;
    expect(line.textContent).toBe("2 hidden, 1 can't be updated here");
    // One control in the line, and it is the count of hidden ones.
    const [hidden, ...others] = within(line).getAllByRole("button");
    expect(others).toEqual([]);
    expect(hidden).toHaveAccessibleName("2 hidden");

    fireEvent.click(hidden);
    expect(useUiStore.getState().page).toBe("settings");
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(true);
  });

  it("gives a line with nothing hidden no control at all", async () => {
    served = snapshotWith({ updates: [candidate(formula("jq"), { blocked: "Pinned" })] });
    const { findByRole } = renderOverview();

    const headline = await findByRole("heading", { level: 2, name: "Everything you can update here is up to date" });
    const line = headline.nextElementSibling as HTMLElement;
    expect(line.textContent).toBe("1 can't be updated here");
    expect(within(line).queryByRole("button")).toBeNull();
  });

  it("is a column of groups as wide as Settings', at the top, with no source tiles and no 22 headline", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    const { findByRole, queryByRole, container } = renderOverview();

    await findByRole("heading", { level: 2, name: "Ollama wasn't checked this time; everything else is up to date" });
    const column = container.firstElementChild as HTMLElement;
    // Settings' column: min(560, the page less 40), centred, 20 under the
    // toolbar -- not centred in the window's height.
    expect(column.className.split(" ")).toEqual(
      expect.arrayContaining(["mx-auto", "w-[min(560px,calc(100%-40px))]", "pt-5"]),
    );
    expect(column.className).not.toContain("justify-center");
    // Three groups: the status, the daily check, the problems.
    const groups = [...column.children] as HTMLElement[];
    expect(groups).toHaveLength(3);
    for (const group of groups) {
      expect(group.className.split(" ")).toEqual(expect.arrayContaining(["bg-group", "rounded-group"]));
      expect(group.className).not.toContain("border");
    }
    expect(groups[0]).toContainElement(statusRowOf(container));
    // The sources are in the sidebar: no tiles here, no "Sources" panel.
    expect(queryByRole("heading", { name: "Sources" })).toBeNull();
    expect(queryByRole("button", { name: /^Homebrew/ })).toBeNull();
    expect(queryByRole("button", { name: /^pip/ })).toBeNull();
    // One title on the page, the status's, in the 13 bold style.
    expect(container.querySelectorAll("h2")).toHaveLength(1);
    expect(container.querySelector(".text-headline")).toBeNull();
  });

  it("shows how often the automatic check runs in a row of its own, which opens Settings", async () => {
    settings.auto_check = true;
    useUiStore.setState({ page: "overview" });
    const { findByRole } = renderOverview();

    const row = await findByRole("button", { name: "Check for updates: Daily" });
    // A button to Settings, not a switch: what pressing it does, said.
    expect(row).toHaveAccessibleDescription("Opens Settings");
    expect(row).not.toHaveAttribute("role");
    // A row of its group, 36 high, the value muted with a chevron after it.
    expect(row.className.split(" ")).toEqual(expect.arrayContaining(["min-h-9", "w-full"]));
    expect(within(row).getByText("Daily").parentElement?.className).toContain("text-muted");
    expect(row.querySelector("svg")).not.toBeNull();

    fireEvent.click(row);
    expect(useUiStore.getState().page).toBe("settings");
    // Not the hidden updates: Settings opens at its top.
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);
  });

  it("says the automatic check is off, in Chinese as Settings' popup does: 不自动检查", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByRole } = renderOverview();
      const row = await findByRole("button", { name: "检查更新的频率：不自动检查" });
      expect(row).toHaveAccessibleDescription("在“设置”中更改");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("offers Check Tool Setup under the automatic check, as Settings' 诊断 does", async () => {
    // Terminal's settings read: nothing in the sheet to look at, the plain row (I4).
    const answer = mockInvoke.getMockImplementation()!;
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "get_system_facts"
        ? Promise.resolve({ macos_version: "27.0", chip: "Apple M2", arch: "aarch64", login_path: true, path_dirs: [], sources: [] })
        : answer(cmd, args),
    );
    const { findByRole } = renderOverview();
    const open = await findByRole("button", { name: "Check Tool Setup…" });
    expect(open).toHaveTextContent(/^Check Tool Setup…$/);
    const row = open.closest("[data-overview-tool-setup]") as HTMLElement;
    expect(row).toHaveTextContent("Tool setup");
    await waitFor(() => expect(row).toHaveTextContent("Whether Terminal finds your tools, and how each source is doing."));
    // In the automatic check's group.
    expect(row.parentElement).toContainElement(await findByRole("button", { name: /^Check for updates: / }));
    expect(useToolSetupSheet.getState().open).toBe(false);
    fireEvent.click(open);
    expect(useToolSetupSheet.getState().open).toBe(true);
    useToolSetupSheet.setState({ open: false });
  });

  it("says a weekly check is weekly", async () => {
    settings.auto_check = true;
    settings.auto_check_every = "Week";
    const { findByRole } = renderOverview();
    expect(await findByRole("button", { name: "Check for updates: Weekly" })).toBeInTheDocument();
  });

  it("turns the symbol and says Checking… under the verdict while a check runs", async () => {
    served = snapshotWith({ updates: [candidate(formula("glib"))] });
    let answer: (snapshot: Snapshot) => void = () => {};
    const { findByRole, queryClient, container } = renderOverview();
    const headline = await findByRole("heading", { level: 2, name: "1 tool can be updated" });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return new Promise<Snapshot>((resolve) => (answer = resolve));
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") return Promise.resolve(operations);
      return Promise.resolve(undefined);
    });

    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = refreshIntoCache(queryClient, "test");
    });
    try {
      // The verdict stays, its symbol and its button too; only the line
      // says so (the toolbar's ⟳ is turning already): no second spinner.
      await waitFor(() => expect(headline.nextElementSibling?.textContent).toBe("Checking…"));
      expect(headline).toHaveTextContent("1 tool can be updated");
      expect(symbolOf(container).getAttribute("data-symbol")).toBe("updates");
      expect(container.querySelector("[class*='animate-spinner']")).toBeNull();
      expect(within(statusRowOf(container)).getByRole("button", { name: "Review Updates" })).toBeInTheDocument();
    } finally {
      await act(async () => {
        answer(served);
        await run;
      });
    }
    await waitFor(() => expect(headline.nextElementSibling?.textContent).toMatch(/^Checked /));
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("updates");
  });

  it("offers Check Again off while a check runs, where it is the row's button", async () => {
    let answer: (snapshot: Snapshot) => void = () => {};
    const { findByRole, queryClient, container } = renderOverview();
    await findByRole("heading", { level: 2, name: "Everything is up to date" });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return new Promise<Snapshot>((resolve) => (answer = resolve));
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") return Promise.resolve(operations);
      return Promise.resolve(undefined);
    });
    const again = within(statusRowOf(container)).getByRole("button", { name: "Check Again" });
    expect(again).toBeEnabled();

    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = refreshIntoCache(queryClient, "test");
    });
    try {
      await waitFor(() => expect(again).toBeDisabled());
      expect(symbolOf(container).getAttribute("data-symbol")).toBe("upToDate");
    } finally {
      await act(async () => {
        answer(served);
        await run;
      });
    }
    await waitFor(() => expect(again).toBeEnabled());
  });

  it("says, as an alert, that the last check failed and why, over what the check before it found", async () => {
    served = snapshotWith({ updates: [candidate(formula("glib"))] });
    useUiStore.setState({ startupRefreshError: "brew update timed out" });
    const { findByRole, queryByRole, container } = renderOverview();

    const alert = await findByRole("alert");
    const title = within(alert).getByRole("heading", { level: 2, name: "Couldn't check" });
    // Said once, not shouted: the title in the label colour, the ⚠︎ at 32
    // in the middle of the 48 slot, and its reason under it.
    expect(title.className).toContain("text-foreground");
    expect(title.className).not.toContain("text-danger");
    // In a person's words: Homebrew was updating its list, so try later.
    expect(title.nextElementSibling?.textContent).toBe("Homebrew is checking online for new versions. Try again later.");
    expect(title.nextElementSibling?.className).toContain("text-muted");
    expect(symbolOf(container).getAttribute("data-symbol")).toBe("failed");
    const warning = symbolOf(container).querySelector("svg");
    expect(warning?.getAttribute("class")).toContain("text-warning");
    expect(warning).toHaveAttribute("width", "32");
    expect(symbolOf(container).className).toContain("size-12");
    // The one thing to do now: check again, the row's default button.
    const again = within(statusRowOf(container)).getByRole("button");
    expect(again).toHaveAccessibleName("Check Again");
    expect(again.className).toContain("bg-accent");
    expect(queryByRole("heading", { name: "1 tool can be updated" })).toBeNull();
    expect(queryByRole("button", { name: "Review Updates" })).toBeNull();
    fireEvent.click(again);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));

    // A check that works clears it.
    act(() => useUiStore.setState({ startupRefreshError: null }));
    expect(await findByRole("heading", { level: 2, name: "1 tool can be updated" })).toBeInTheDocument();
    expect(queryByRole("alert")).toBeNull();
  });

  it.each([
    [
      "a network failure, as that and the next step",
      "error sending request for url (https://formulae.brew.sh/api/formula.jws.json): dns error",
      false,
      "The connection failed. Check your internet connection, then try again.",
    ],
    ["a full disk, as that and the next step", "No space left on device (os error 28)", false, "The disk is full. Free up some space, then try again."],
    ["anything else, without its own words", "command refresh failed: the backend is not responding", false, "Try again later."],
    [
      "anything else, in its own words with technical details on",
      "command refresh failed: the backend is not responding",
      true,
      "Reason: command refresh failed: the backend is not responding",
    ],
  ])("says why the last check failed: %s", async (_what, message, technical, line) => {
    settings = { ...settings, show_technical_details: technical };
    served = snapshotWith({ updates: [candidate(formula("glib"))] });
    useUiStore.setState({ startupRefreshError: message });
    const { findByRole } = renderOverview();

    const alert = await findByRole("alert");
    const title = within(alert).getByRole("heading", { level: 2, name: "Couldn't check" });
    expect(title.nextElementSibling?.textContent).toBe(line);
  });

  it("gives each source with a problem one row in a group of its own, with its words and its own button, as the lists do", async () => {
    served = snapshotWith({
      instances: [
        { ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } },
        pip,
        stoppedOllama,
      ],
    });
    const { findByRole, getByRole, getAllByRole, queryByText } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    expect(list.className.split(" ")).toEqual(expect.arrayContaining(["bg-group", "rounded-group"]));
    // Everything 10 in; the hairlines between the rows start where the
    // words do (10 + the 16 symbol + 8), not at the symbol.
    expect(list.className).toContain("[&>*+*]:before:left-8.5");
    expect(list.className).not.toContain("[&>*+*]:before:left-2.5");
    // The warning first, as a row; Homebrew's news folded into the last.
    const lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(2);
    expect(lines[0].className).toContain("min-h-11.5");
    expect(lines[0].className).toContain("px-2.5");
    expect(within(lines[0]).getByText("Ollama isn't running")).toBeInTheDocument();
    // The title, and its explanation under it in small muted text -- in
    // the row, not behind Details. 11, its lines 16 apart when it wraps.
    const why = within(lines[0]).getByText("Open Ollama to see what it has and check for updates.");
    expect(why.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "leading-4", "text-muted"]));
    expect(within(lines[0]).queryByRole("button", { name: /^Details/ })).toBeNull();
    // A warning's symbol: a filled orange ⚠︎.
    const warningIcon = lines[0].querySelector("svg");
    expect(warningIcon?.getAttribute("class")).toContain("text-warning");
    expect(warningIcon?.querySelector('path[fill="currentColor"]')).not.toBeNull();
    expect(queryByText("Homebrew is checking online for new versions")).toBeNull();
    // No title over the group: the status's is the page's one.
    expect(getAllByRole("heading", { level: 2 })).toHaveLength(1);
    // Ollama's own button, a regular grey one on the right, starts it.
    const open = within(lines[0]).getByRole("button", { name: "Open Ollama" });
    expect(open.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "bg-fill"]));
    fireEvent.click(open);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
    // pip being read-only is what it always is, not something to attend to.
    expect(queryByText("View only")).not.toBeInTheDocument();
    // The headline names both sources not checked (I22).
    expect(
      getByRole("heading", { level: 2, name: "Homebrew and Ollama weren't checked this time; everything else is up to date" }),
    ).toBeInTheDocument();
  });

  it("folds the notes, which ask nothing of the user, into one last row that shows them in place", async () => {
    const claude = instance("standalone-claude", "standalone-claude", {
      exe_path: "/Users/someone/.local/bin/claude",
      prefix: "/Users/someone/.local/share/claude",
      status: { unavailable: null, notes: ["ShadowedByNpm"] },
    });
    served = snapshotWith({
      instances: [
        { ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } },
        pip,
        stoppedOllama,
        claude,
      ],
    });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    let lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(2);
    expect(within(lines[0]).getByText("Ollama isn't running")).toBeInTheDocument();
    // The last row: a disclosure, 13 muted, a 10pt triangle pointing right.
    const more = within(lines[1]).getByRole("button", { name: "2 more notes" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(more.className.split(" ")).toEqual(expect.arrayContaining(["text-body", "text-muted"]));
    expect(more.className).not.toContain("bg-fill");
    const triangle = more.querySelector("svg");
    expect(triangle).toHaveAttribute("width", "10");
    expect(triangle?.getAttribute("class")).not.toContain("rotate-90");
    expect(list.textContent).not.toContain("Homebrew is checking online for new versions");

    // Pressed: the row stays where it was, after the warnings, its
    // triangle down, and the notes show under it in their sources' order,
    // each with a muted ⓘ.
    fireEvent.click(more);
    lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(4);
    const hide = within(lines[1]).getByRole("button", { name: "Hide 2 notes" });
    expect(hide).toBe(more);
    expect(hide).toHaveAttribute("aria-expanded", "true");
    expect(hide.querySelector("svg")?.getAttribute("class")).toContain("rotate-90");
    expect(within(lines[2]).getByText("Homebrew is checking online for new versions")).toBeInTheDocument();
    expect(within(lines[3]).getByText("Typing claude in Terminal runs a program with that name from npm")).toBeInTheDocument();
    for (const line of lines.slice(2)) {
      expect(line.className).toContain("min-h-11.5");
      expect(line.querySelector("svg")?.getAttribute("class")).toContain("text-muted");
    }

    // Pressed again, they fold.
    fireEvent.click(hide);
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(list).getByRole("button", { name: "2 more notes" })).toHaveAttribute("aria-expanded", "false");
  });

  it("gives the note that another program answers to a tool's command a Show, which searches the Installed page for it", async () => {
    // W2-9: the note said 「可在“已安装”的npm中查看」 with no button, while
    // the warnings beside it had one. Its Show lists the rows it is about.
    const claude = instance("standalone-claude", "standalone-claude", {
      exe_path: "/Users/someone/.local/bin/claude",
      prefix: "/Users/someone/.local/share/claude",
      status: { unavailable: null, notes: ["ShadowedByNpm"] },
    });
    // Grok Build's launcher left without its program: a warning with a
    // Show of its own beside this one.
    const grok = instance("standalone-grok", "standalone-grok", {
      exe_path: "/Users/someone/.grok/bin/grok",
      prefix: "/Users/someone/.grok",
      status: { unavailable: null, notes: ["LauncherOnly"] },
    });
    served = snapshotWith({ instances: [brew, claude, grok] });
    useUiStore.setState({ page: "overview" });
    const { findByRole } = renderOverview();

    // A note is folded behind the warnings: unfolded, it shows.
    const list = await findByRole("list", { name: "Needs attention" });
    fireEvent.click(within(list).getByRole("button", { name: "1 more note" }));
    // Two Show buttons, each named for what it shows (walk-3 W3-5) and
    // described by its row's title, so a screen reader's list of buttons
    // tells them apart.
    expect(
      within(list)
        .getAllByRole("button", { name: /^Show / })
        .map((button) => document.getElementById(button.getAttribute("aria-describedby") ?? "")?.textContent),
    ).toEqual([
      "Grok Build's program files are missing",
      "Typing claude in Terminal runs a program with that name from npm",
    ]);
    const lines = within(list).getAllByRole("listitem");
    const line = lines[lines.length - 1];
    expect(within(line).getByRole("button", { name: "Show “claude”" })).toHaveAccessibleDescription(
      "Typing claude in Terminal runs a program with that name from npm",
    );
    expect(within(line).getByText("Typing claude in Terminal runs a program with that name from npm")).toBeInTheDocument();
    expect(
      within(line).getByText(
        "Terminal finds the one from npm first, not the one Claude Code's own installer installed. Couldn't confirm whether it's another copy of Claude Code.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(line).getByRole("button", { name: "Show “claude”" }));

    const state = useUiStore.getState();
    expect(state.page).toBe("installed");
    expect(state.installedFilter).toBeNull();
    expect(state.query).toBe("claude");
  });

  it("makes the group that one folded row when every problem is a note, in Chinese too", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      served = snapshotWith({
        instances: [{ ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } }, pip],
      });
      const { findByRole } = renderOverview();

      const list = await findByRole("list", { name: "需要查看" });
      const lines = within(list).getAllByRole("listitem");
      expect(lines).toHaveLength(1);
      const more = within(lines[0]).getByRole("button", { name: "另有1条提示" });
      fireEvent.click(more);
      expect(within(list).getByRole("button", { name: "收起1条提示" })).toBe(more);
      expect(within(list).getByText("Homebrew正在联网查找新版本")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("gives a source its warning rather than a note that comes before it, so that the warning does not fold away", async () => {
    served = snapshotWith({
      instances: [{ ...brew, status: { unavailable: null, notes: ["IndexUpdating", "IndexMayBeStale"] } }, pip],
    });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    const lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(1);
    expect(within(lines[0]).getByText("Couldn't reach Homebrew, so updates for its tools weren't fully checked")).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: /more note/ })).toBeNull();
  });

  it("gives each problem whose next step is checking again a Check Again of its own, and a launcher left without its program a Show", async () => {
    const intel = instance("brew:/usr/local", "brew", {
      prefix: "/usr/local",
      exe_path: "/usr/local/bin/brew",
      status: { unavailable: null, notes: ["IndexMayBeStale"] },
    });
    const claude = instance("standalone-claude", "standalone-claude", {
      exe_path: "/Users/someone/.local/bin/claude",
      prefix: "/Users/someone/.local/share/claude",
      status: { unavailable: null, notes: ["LauncherOnly"] },
    });
    const claudeCode = { ...artifact({ instance_id: claude.id, kind: "Binary", name: "claude" }), display_name: "Claude Code" };
    served = snapshotWith({
      instances: [
        { ...brew, prefix: "/opt/homebrew", status: { unavailable: "NotResponding", notes: [] } },
        intel,
        claude,
        pip,
        stoppedOllama,
      ],
      artifacts: [...snapshotWith().artifacts, claudeCode],
    });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    const lines = within(list).getAllByRole("listitem");
    expect(lines.map((line) => line.querySelector("p")?.textContent)).toEqual([
      "Homebrew (Apple silicon) isn't responding",
      "Couldn't reach Homebrew, so updates for its tools weren't fully checked",
      "Claude Code's program files are missing",
      "Ollama isn't running",
    ]);
    // Reason and next step, the button beside them doing it.
    expect(lines[0]).toHaveTextContent(
      "3 tools were installed with Homebrew (Apple silicon). It didn't respond this time, so they're shown as they were when it last responded. Check again later.",
    );
    expect(lines[1]).toHaveTextContent("This check used what Homebrew knew the last time it could be reached. Check your internet connection, then check again.");
    for (const line of lines.slice(0, 2)) {
      const again = within(line).getByRole("button", { name: "Check Again" });
      expect(again.className).toContain(BUTTON.regular.grey);
      expect(line.textContent).not.toMatch(/click Check Again/i);
    }
    // Claude Code's program files are gone: its way out in the app is its
    // Uninstall…, so its button shows it on the Installed page. Checking
    // again after a reinstall is the toolbar's ⟳.
    const show = within(lines[2]).getByRole("button", { name: "Show Tool" });
    expect(show.className).toContain(BUTTON.regular.grey);
    expect(within(lines[2]).queryByRole("button", { name: "Check Again" })).toBeNull();
    // Ollama's keeps its own.
    expect(within(lines[3]).getByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
    expect(within(lines[3]).queryByRole("button", { name: "Check Again" })).toBeNull();

    // Pressed, it starts a check.
    mockInvoke.mockClear();
    fireEvent.click(within(lines[0]).getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));

    // Show: the Installed page, asked to select Claude Code.
    fireEvent.click(show);
    expect(useUiStore.getState().page).toBe("installed");
    expect(useUiStore.getState().inspectRequested).toBe(artifactKeyId(claudeCode.key));
  });

  it("names which Homebrew a row is about where this Mac has two, as the sidebar does", async () => {
    const appleSilicon = { ...brew, prefix: "/opt/homebrew" };
    const intel = instance("brew:/usr/local", "brew", {
      prefix: "/usr/local",
      exe_path: "/usr/local/bin/brew",
      status: { unavailable: "NotResponding", notes: [] },
    });
    served = snapshotWith({ instances: [appleSilicon, intel, pip] });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    const lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(1);
    expect(within(lines[0]).getByText("Homebrew (Intel) isn't responding")).toBeInTheDocument();
    expect(list.textContent).not.toContain("/usr/local");
  });

  it("names the only Homebrew plainly", async () => {
    const intelOnly = instance("brew:/usr/local", "brew", {
      prefix: "/usr/local",
      status: { unavailable: "NotResponding", notes: [] },
    });
    served = snapshotWith({ instances: [intelOnly, pip] });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    expect(within(list).getByText("Homebrew isn't responding")).toBeInTheDocument();
  });

  it("says under the row when Open Ollama did not work", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") return Promise.resolve(operations);
      if (cmd === "open_ollama_app") return Promise.reject(new Error("open exited with status 1"));
      return Promise.resolve(undefined);
    });
    const { findByRole } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    fireEvent.click(within(list).getByRole("button", { name: "Open Ollama" }));

    const alert = await within(list).findByRole("alert");
    expect(alert.className).toContain("text-danger-text");
    expect(alert.textContent).not.toBe("");
  });

  it("says it in Chinese with the words the author asked for", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      served = snapshotWith({ updates: [candidate(formula("glib")), candidate(formula("wget"))] });
      clockJustAfterTheCheck();
      const { findByRole, queryByRole, container } = renderOverview();

      const headline = await findByRole("heading", { level: 2, name: "2个工具可以更新" });
      expect(headline.nextElementSibling?.textContent).toBe("上次检查：刚才");
      expect(within(statusRowOf(container)).getByRole("button", { name: "查看更新" })).toBeInTheDocument();
      expect(queryByRole("heading", { level: 2, name: "来源" })).toBeNull();
      // 「2」 once, in the title.
      expect(container.textContent?.match(/2/g)).toHaveLength(1);
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

it("counts a persisted password stop as needing steps after restart", async () => {
  served = snapshotWith({ updates: [candidate(formula("glib"))] });
  const key = served.updates[0].key;
  const answer = mockInvoke.getMockImplementation()!;
  mockInvoke.mockImplementation((cmd, args) => cmd === "get_history" ? Promise.resolve({
    run: "current", cleared_before: null, records: [{
      run: "previous", op_id: 1, finished_at: Date.now() - 1000, key, display_name: "glib", adapter_id: "brew",
      kind: "Update", from_version: "1.0.0", to_version: null, result: { Failed: { cause: "needsPassword" } }, verified: false,
    }],
  }) : answer(cmd, args));
  const view = renderOverview();
  expect(await view.findByRole("heading", { name: "1 update needs your password" })).toBeInTheDocument();
  fireEvent.click(view.getByRole("button", { name: "Review Updates" }));
  expect(useUiStore.getState().selectedUpdates).toEqual([]);
});

describe("a problem row's sign and button (p1 polish)", () => {
  it("sit beside the title, at the top, as a Mac's list puts them, however tall the startup diagnostic makes the row", async () => {
    await i18n.changeLanguage("en");
    const npm = instance("npm:/opt/homebrew", "npm", {
      version: null,
      status: {
        unavailable: "NotResponding",
        notes: [],
        no_answer: {
          kind: "ExitedWithError",
          missing_program: null,
          link_fixes: [],
          cause: null,
          diagnostic: "npm error config Invalid npmrc\nnpm error Invalid proxy URL https://****@proxy.example.test",
        },
      },
    });
    served = snapshotWith({ instances: [brew, pip, npm, stoppedOllama] });
    const { findByRole, getByRole } = renderOverview();
    const list = await findByRole("list", { name: "Needs attention" });
    // Each source's row: the notice with the diagnostic folded and the one without.
    const rows = within(list)
      .getAllByRole("listitem")
      .filter((row) => row.querySelector("p[id]") !== null);
    expect(rows.length).toBeGreaterThanOrEqual(2);
    const check = (row: HTMLElement) => {
      // Neither the sign nor the button is centred on the whole row.
      expect(row.className.split(" ")).toContain("items-start");
      expect(row.className.split(" ")).not.toContain("items-center");
      const title = row.querySelector("p")!;
      const sign = row.querySelector("svg")!;
      const signRow = sign.parentElement!;
      expect(signRow.contains(title)).toBe(true);
      expect(signRow.className.split(" ")).toContain("items-start");
      expect(signRow.className.split(" ")).not.toContain("items-center");
      // The 24 button centred on the title's 16 line: 4 above its top.
      const button = [...row.querySelectorAll("button")].find((b) => b.getAttribute("aria-describedby") === title.id)!;
      expect(button.className.split(" ")).toContain("-mt-1");
    };
    rows.forEach(check);
    // Unfolded, the diagnostic makes the row taller; the sign and the button stay by the title.
    const disclosure = getByRole("button", { name: "Startup Diagnostic" });
    fireEvent.click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    check(disclosure.closest("li")!);
  });
});
