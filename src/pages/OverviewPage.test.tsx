import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OverviewPage } from "./OverviewPage";
import { UpdatesPage } from "./UpdatesPage";
import { SnapshotStatus } from "../components/SnapshotStatus";
import { queryKeys } from "../lib/queries";
import i18n from "../i18n";
import { artifactKeyId, useUiStore } from "../store/ui";
import type {
  ArtifactKey,
  InstalledArtifact,
  InstanceNote,
  ManagerInstance,
  Settings,
  Snapshot,
  UnknownScan,
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

beforeEach(() => {
  served = snapshotWith();
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
  };
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    return Promise.resolve(undefined);
  });
});

/** The large ring over the headline. */
function ringOf(container: HTMLElement): HTMLElement {
  const ring = container.querySelector<HTMLElement>("[data-ring]");
  if (ring === null) throw new Error("no ring");
  return ring;
}

function renderOverview() {
  return renderWithProviders(
    <SnapshotStatus showsFirstCheck>
      <OverviewPage />
    </SnapshotStatus>,
  );
}

describe("OverviewPage", () => {
  it("says Checking… while the first check runs, with nothing to press", async () => {
    served = startupSnapshot;
    const { findByRole, queryByRole, queryByText, container } = renderOverview();

    expect(await findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    // The ring turns while it waits, and says no number.
    expect(ringOf(container).getAttribute("data-ring")).toBe("checking");
    expect(ringOf(container).textContent).toBe("");
    // Not "Loading…", and not "Canager found nothing it can manage": the
    // placeholder is not an answer.
    expect(queryByText("Loading…")).not.toBeInTheDocument();
    expect(queryByText("Canager found nothing it can manage")).not.toBeInTheDocument();
    expect(queryByRole("button", { name: "Review updates" })).not.toBeInTheDocument();
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
    const { findByRole, container } = renderOverview();

    expect(
      await findByRole("heading", { level: 2, name: "2 tools can be updated" }),
    ).toBeInTheDocument();
    // The same number, large, in the ring over it.
    expect(ringOf(container).getAttribute("data-ring")).toBe("updates");
    expect(ringOf(container).textContent).toBe("2updates");

    fireEvent.click(await findByRole("button", { name: "Review updates" }));

    const state = useUiStore.getState();
    expect(state.page).toBe("updates");
    expect(state.selectedUpdates).toEqual([
      artifactKeyId(formula("glib")),
      artifactKeyId(formula("wget")),
    ]);
  });

  it("keeps a row the user had already selected when Review updates adds the rest", async () => {
    served = snapshotWith({
      updates: [candidate(formula("glib")), candidate(formula("wget"))],
    });
    useUiStore.setState({ selectedUpdates: [artifactKeyId(formula("wget"))] });
    const { findByRole } = renderOverview();

    fireEvent.click(await findByRole("button", { name: "Review updates" }));

    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(formula("wget")),
      artifactKeyId(formula("glib")),
    ]);
  });

  it("says everything is up to date with a check mark when every source answered and nothing needs updating", async () => {
    const { findByRole, queryByRole, container } = renderOverview();

    expect(
      await findByRole("heading", { level: 2, name: "Everything is up to date" }),
    ).toBeInTheDocument();
    expect(ringOf(container).getAttribute("data-ring")).toBe("upToDate");
    expect(ringOf(container).querySelector("svg.text-success")).not.toBeNull();
    expect(queryByRole("button", { name: "Review updates" })).not.toBeInTheDocument();
  });

  // Each of these has nothing to install and is not up to date: the
  // Overview must say so exactly when the Updates page does, and never
  // "Everything is up to date" over any of them. Under the headline, the
  // line that says what there is instead, or none; and Review updates
  // only where the Updates page lists a row.
  const notUpToDate: Array<[string, () => void, string | null, boolean]> = [
    [
      "a source that is not running",
      () => {
        served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
      },
      // "Needs attention" says it.
      null,
      false,
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
    ],
  ];

  it.each(notUpToDate)("says nothing to update, not up to date, with %s", async (_name, arrange, line, review) => {
    arrange();
    const { getByRole, getByText, queryByRole, queryByText, container } = renderWithProviders(
      <>
        <SnapshotStatus showsFirstCheck>
          <OverviewPage />
        </SnapshotStatus>
        <UpdatesPage />
      </>,
    );

    // Waited for rather than found once: over a snapshot with errors the
    // page is drawn again under the "some checks didn't finish" banner,
    // and the heading found first is not the one that stays.
    await waitFor(() =>
      expect(getByRole("heading", { level: 2, name: "Nothing to update" })).toBeInTheDocument(),
    );
    expect(queryByRole("heading", { name: "Everything is up to date" })).not.toBeInTheDocument();
    // Neither the green ring nor its check: a grey ring with a dash.
    expect(ringOf(container).getAttribute("data-ring")).toBe("nothingToUpdate");
    expect(ringOf(container).querySelector(".text-success, .stroke-success")).toBeNull();
    // The Updates page, on the same snapshot, does not say it either.
    await waitFor(() =>
      expect(
        getByText(
          (_content, element) =>
            element?.tagName === "P" &&
            [
              "No updates in the sources Canager could check",
              "Nothing to update here",
              "No pending updates — you've skipped the rest or asked not to be reminded about them.",
            ].includes(element.textContent ?? ""),
        ),
      ).toBeInTheDocument(),
    );
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    const headline = getByRole("heading", { level: 2, name: "Nothing to update" });
    if (line === null) {
      expect(headline.nextElementSibling).toBeNull();
    } else {
      expect(headline.nextElementSibling?.textContent).toBe(line);
    }
    if (review) {
      expect(getByRole("button", { name: "Review updates" })).toBeInTheDocument();
    } else {
      expect(queryByRole("button", { name: "Review updates" })).not.toBeInTheDocument();
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

      const headline = await findByRole("heading", { level: 2, name: "Nothing to update" });
      expect(headline.nextElementSibling?.textContent).toBe("2 hidden, 2 can't be updated here");
      // The same number the Updates page gives its folded rows.
      expect(await findByRole("button", { name: "Can't update here (2)" })).toBeInTheDocument();
      // The ring is as it was: grey, with a dash.
      expect(ringOf(container).getAttribute("data-ring")).toBe("nothingToUpdate");

      fireEvent.click(getByRole("button", { name: "Review updates" }));
      expect(useUiStore.getState().page).toBe("updates");
      // Nothing on that page has a checkbox: nothing is selected.
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    } finally {
      height.mockRestore();
      width.mockRestore();
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
      expect(headline.nextElementSibling?.textContent).toBe("2 个已隐藏，1 个不能在这里更新");
      expect(await findByRole("button", { name: "查看更新" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("shows each source with something installed and how much, and a tile opens Installed on that source", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    useUiStore.setState({ page: "overview" });
    const { findByRole, getByRole, queryByRole } = renderOverview();

    const homebrew = await findByRole("button", { name: "Homebrew 3 items" });
    expect(getByRole("button", { name: "pip 1 item" })).toBeInTheDocument();
    // Nothing installed from the stopped Ollama: no tile for it.
    expect(queryByRole("button", { name: /^Ollama/ })).not.toBeInTheDocument();

    fireEvent.click(homebrew);
    expect(useUiStore.getState().page).toBe("installed");
    // The three its count counted.
    expect(useUiStore.getState().installedFilter).toBe(brew.id);
  });

  it("gives the programs the last scan could not place a tile of their own, which opens the Unknown page", async () => {
    const scan: UnknownScan = {
      scanned: [{ path: "~/.local/bin", entries: 2 }],
      entries: [
        {
          path: "~/.local/bin/tool-a",
          kind: "File",
          resolved: "/Users/you/.local/bin/tool-a",
          link_target: null,
          size_bytes: 10,
          modified_at: 1789000000,
          owned_by_me: true,
          app_bundle: null,
        },
        {
          path: "~/.local/bin/tool-b",
          kind: "File",
          resolved: "/Users/you/.local/bin/tool-b",
          link_target: null,
          size_bytes: 10,
          modified_at: 1789000000,
          owned_by_me: true,
          app_bundle: null,
        },
      ],
      attributed: 0,
      stopped: null,
    };
    const { findByRole, queryByRole, queryClient } = renderOverview();
    await findByRole("heading", { level: 2, name: "Everything is up to date" });
    // No scan yet, and the Overview does not start one.
    expect(queryByRole("button", { name: /^Unknown/ })).not.toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalledWith("scan_unknown");

    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });

    const tile = await findByRole("button", { name: "Unknown 2 items" });
    // Among the tools, after the sources.
    const tools = within(await findByRole("list", { name: "Your tools" })).getAllByRole("button");
    expect(tools[tools.length - 1]).toBe(tile);
    fireEvent.click(tile);
    expect(useUiStore.getState().page).toBe("unknown");
  });

  it("gives each source that needs attention one line, its title and nothing more", async () => {
    served = snapshotWith({
      instances: [
        { ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } },
        pip,
        stoppedOllama,
      ],
    });
    const { findByRole, getByRole, queryByText } = renderOverview();

    const list = await findByRole("list", { name: "Needs attention" });
    expect(
      [...list.querySelectorAll("li")].map((line) => line.textContent),
    ).toEqual(["Homebrew is updating its software list", "Ollama isn't running"]);
    // Each with an icon: information, and a warning.
    const icons = [...list.querySelectorAll("li")].map((line) => line.querySelector("svg"));
    expect(icons[0]?.getAttribute("class")).toContain("text-muted");
    expect(icons[1]?.getAttribute("class")).toContain("text-warning");
    // In a panel of its own, apart from the tools.
    expect(getByRole("heading", { level: 2, name: "Needs attention" })).toBeInTheDocument();
    // Titles only: the explanations stay on the Installed and Updates pages.
    expect(queryByText("Open Ollama to see what it has and check for updates.")).not.toBeInTheDocument();
    // pip being read-only is what it always is, not something to attend to.
    expect(queryByText("View only")).not.toBeInTheDocument();
    expect(getByRole("heading", { level: 2, name: "Nothing to update" })).toBeInTheDocument();
  });

  it("shows the tiles under Your tools, in a grid, the source's avatar and how many", async () => {
    const { findByRole } = renderOverview();

    const heading = await findByRole("heading", { level: 2, name: "Your tools" });
    const list = await findByRole("list", { name: "Your tools" });
    expect(heading.closest("section")).toBe(list.closest("section"));
    expect(list.className).toContain("grid-cols-3");
    const homebrew = within(list).getByRole("button", { name: "Homebrew 3 items" });
    const avatar = homebrew.querySelector('[aria-hidden="true"]');
    expect(avatar).toHaveTextContent("H");
    // The 32px avatar, the one a tool's row has.
    expect(avatar?.className).toContain("h-8");
  });

  it("names a tool with its own installer by its product name alone on its tile", async () => {
    // "Grok Build", not "Grok Build (grok)": the redesign has no
    // parenthetical asides, and the command is said where typing it
    // matters (the PATH notices' {{command}}).
    const grok = instance("standalone-grok", "standalone-grok");
    const agy = instance("standalone-agy", "standalone-agy");
    served = snapshotWith({
      instances: [brew, grok, agy],
      artifacts: [
        artifact(formula("glib")),
        artifact({ instance_id: grok.id, kind: "Binary", name: "grok" }),
        artifact({ instance_id: agy.id, kind: "Binary", name: "agy" }),
      ],
    });
    const { findByRole, container } = renderOverview();

    expect(await findByRole("button", { name: "Grok Build 1 item" })).toBeInTheDocument();
    expect(await findByRole("button", { name: "Antigravity CLI 1 item" })).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/\(grok\)|\(agy\)|（grok）|（agy）/);
  });

  it("draws the ring in Chinese with the words the author asked for", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      served = snapshotWith({ updates: [candidate(formula("glib")), candidate(formula("wget"))] });
      const { findByRole, container } = renderOverview();

      expect(await findByRole("heading", { level: 2, name: "有 2 个工具可以更新" })).toBeInTheDocument();
      expect(ringOf(container).textContent).toBe("2个可更新");
      expect(await findByRole("heading", { level: 2, name: "你的工具" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
