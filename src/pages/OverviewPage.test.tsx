import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders, type RenderOptions } from "../test/setup";
import { OverviewPage } from "./OverviewPage";
import { UpdatesPage } from "./UpdatesPage";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { SnapshotStatus } from "../components/SnapshotStatus";
import { refreshIntoCache } from "../lib/events";
import i18n from "../i18n";
import { artifactKeyId, useUiStore } from "../store/ui";
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

function renderOverview(options?: RenderOptions) {
  return renderWithProviders(
    <SnapshotStatus showsFirstCheck>
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
    expect(spinner?.getAttribute("class")).toContain("animate-spin");
    // A spinner alone, for as long as Homebrew's list update and every
    // online lookup took, looked like a window that had frozen.
    expect(heading.nextElementSibling?.textContent).toBe(
      "The first check looks up every tool's newest version online, and sometimes takes a minute or two.",
    );
    // Once the settings are in, as they are long before the first check.
    expect(await findByRole("button", { name: "Check for updates every day Off" })).toBeInTheDocument();
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

  // Each of these has nothing to install and is not up to date: the
  // Overview must say so exactly when the Updates page does, and never
  // "Everything is up to date" over any of them. Under the headline, the
  // line that says what there is instead, or none; and Review updates
  // only where the Updates page lists a row.
  // The headline says nothing to update only of the sources Canager could
  // check where one was not checked in full, as the Updates page does.
  const NOT_CHECKED = "No updates in the sources checked";
  const notUpToDate: Array<[string, () => void, string | null, boolean, string]> = [
    [
      "a source that is not running",
      () => {
        served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
      },
      // "Needs attention" says it.
      null,
      false,
      NOT_CHECKED,
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
      NOT_CHECKED,
    ],
    [
      "only updates Canager cannot install",
      () => {
        served = snapshotWith({
          updates: [candidate(formula("jq"), { blocked: "Pinned" }), candidate(urllib3)],
        });
      },
      "2 can't be updated here",
      true,
      "Nothing to update",
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
      "Nothing to update",
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
      "1 check didn't finish",
      false,
      NOT_CHECKED,
    ],
    [
      "a source whose detection failed this round",
      () => {
        served = snapshotWith({
          stale: true,
          errors: [{ instance_id: "npm", message: "internal error detecting this source" }],
        });
      },
      "1 check didn't finish",
      false,
      NOT_CHECKED,
    ],
  ];

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

    // Waited for rather than found once: over a snapshot with errors the
    // page is drawn again under the "some checks didn't finish" banner,
    // and the heading found first is not the one that stays.
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
              "No updates to handle. The rest are hidden.",
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
      (element) => element.closest("h2, p") === null,
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

  it("counts the checks that did not finish as the banner names them: two Homebrews are one", async () => {
    // An Apple-silicon Mac with Homebrew in /opt/homebrew and /usr/local,
    // offline, nothing listed: the banner named Homebrew once, and the
    // line under the headline said "2 checks didn't finish".
    const intel = instance("brew:/usr/local", "brew");
    served = snapshotWith({
      instances: [brew, intel, pip],
      stale: true,
      errors: [
        { instance_id: brew.id, message: "brew update failed" },
        { instance_id: intel.id, message: "brew update failed" },
      ],
    });
    const { findByText, getByRole } = renderOverview();

    expect(
      await findByText("Homebrew didn't finish checking this time."),
    ).toBeInTheDocument();
    await waitFor(() => expect(getByRole("heading", { level: 2, name: NOT_CHECKED })).toBeInTheDocument());
    expect(getByRole("heading", { level: 2, name: NOT_CHECKED }).nextElementSibling?.textContent).toBe(
      "1 check didn't finish",
    );
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

      // The stopped Ollama was not checked: the headline says so.
      const headline = await findByRole("heading", {
        level: 2,
        name: "No updates in the sources checked",
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

      const headline = await findByRole("heading", { level: 2, name: "已检查的来源中没有可更新的工具" });
      // Nothing instead to say: when the sources were last checked.
      expect(headline.nextElementSibling?.textContent).toMatch(/^上次检查：/);
      expect(queryByRole("heading", { level: 2, name: "没有要更新的工具" })).not.toBeInTheDocument();
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

      const headline = await findByRole("heading", { level: 2, name: "没有要更新的工具" });
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

    const headline = await findByRole("heading", { level: 2, name: "Nothing to update" });
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

    const headline = await findByRole("heading", { level: 2, name: "Nothing to update" });
    const line = headline.nextElementSibling as HTMLElement;
    expect(line.textContent).toBe("1 can't be updated here");
    expect(within(line).queryByRole("button")).toBeNull();
  });

  it("is a column of groups as wide as Settings', at the top, with no source tiles and no 22 headline", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    const { findByRole, queryByRole, container } = renderOverview();

    await findByRole("heading", { level: 2, name: "No updates in the sources checked" });
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

  it("shows whether the daily check is on in a row of its own, which opens Settings", async () => {
    settings.auto_check = true;
    useUiStore.setState({ page: "overview" });
    const { findByRole } = renderOverview();

    const row = await findByRole("button", { name: "Check for updates every day On" });
    // A row of its group, 36 high, the value muted with a chevron after it.
    expect(row.className.split(" ")).toEqual(expect.arrayContaining(["min-h-9", "w-full"]));
    expect(within(row).getByText("On").parentElement?.className).toContain("text-muted");
    expect(row.querySelector("svg")).not.toBeNull();

    fireEvent.click(row);
    expect(useUiStore.getState().page).toBe("settings");
    // Not the hidden updates: Settings opens at its top.
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);
  });

  it("says the daily check is off, in Chinese as System Settings does: 关闭", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByRole } = renderOverview();
      expect(await findByRole("button", { name: "每天自动检查 关闭" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
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
      expect(container.querySelector(".animate-spin, [class*='animate-spin']")).toBeNull();
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
    expect(title.nextElementSibling?.textContent).toBe("Reason: brew update timed out");
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
    const lines = within(list).getAllByRole("listitem");
    expect(lines).toHaveLength(2);
    // Each 46 high: the title, and its explanation under it in small muted
    // text -- in the row, not behind Details.
    for (const line of lines) expect(line.className).toContain("min-h-11.5");
    expect(within(lines[0]).getByText("Homebrew is updating its software list")).toBeInTheDocument();
    expect(within(lines[1]).getByText("Ollama isn't running")).toBeInTheDocument();
    const why = within(lines[1]).getByText("Open Ollama to see what it has and check for updates.");
    // 11, its lines 16 apart when it wraps.
    expect(why.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "leading-4", "text-muted"]));
    for (const line of lines) expect(line.className).toContain("px-2.5");
    expect(within(lines[1]).queryByRole("button", { name: /^Details/ })).toBeNull();
    // Each with a symbol: information, muted; a warning, a filled orange ⚠︎.
    const icons = lines.map((line) => line.querySelector("svg"));
    expect(icons[0]?.getAttribute("class")).toContain("text-muted");
    expect(icons[1]?.getAttribute("class")).toContain("text-warning");
    expect(icons[1]?.querySelector('path[fill="currentColor"]')).not.toBeNull();
    // No title over the group: the status's is the page's one.
    expect(getAllByRole("heading", { level: 2 })).toHaveLength(1);
    // Ollama's own button, a regular grey one on the right, starts it.
    const open = within(lines[1]).getByRole("button", { name: "Open Ollama" });
    expect(open.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "bg-fill"]));
    fireEvent.click(open);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
    expect(within(lines[0]).queryByRole("button")).toBeNull();
    // pip being read-only is what it always is, not something to attend to.
    expect(queryByText("View only")).not.toBeInTheDocument();
    expect(
      getByRole("heading", { level: 2, name: "No updates in the sources checked" }),
    ).toBeInTheDocument();
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
