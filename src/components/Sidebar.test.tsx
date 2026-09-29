import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { dragsWindow } from "../test/dragRegion";
import { Sidebar } from "./Sidebar";
import { UpdatesPage } from "../pages/UpdatesPage";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { queryKeys } from "../lib/queries";
import i18n from "../i18n";
import type {
  ArtifactKey,
  InstalledArtifact,
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
const ollama = instance("ollama:http://127.0.0.1:11434", "ollama", {
  status: { unavailable: "NotRunning", notes: [] },
});

function candidate(
  key: ArtifactKey,
  overrides: Partial<UpdateCandidate> = {},
): UpdateCandidate {
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

function formula(name: string): ArtifactKey {
  return { instance_id: brew.id, kind: "Formula", name };
}

function artifact(key: ArtifactKey, reason: InstalledArtifact["reason"]): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0.0",
    reason,
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
  };
}

// Every kind of row the Updates page lists without offering it, beside the
// two it offers: pinned, could not be checked, from a read-only source, from
// a source that is not answering, and two the user hid -- one never to be
// reminded about, one whose version was skipped.
const snapshot: Snapshot = {
  generation: 3,
  round: 3,
  detect: "Found",
  instances: [brew, pip, ollama],
  artifacts: [
    artifact(formula("glib"), "Requested"),
    artifact(formula("wget"), "Requested"),
    artifact(formula("jq"), "Requested"),
    artifact(formula("pcre2"), "Dependency"),
    artifact({ instance_id: pip.id, kind: "Package", name: "urllib3" }, "Unknown"),
  ],
  updates: [
    candidate(formula("glib")),
    candidate(formula("wget")),
    candidate(formula("jq"), { blocked: "Pinned" }),
    candidate(formula("pcre2"), { checkable: false, target: "1.0.0" }),
    candidate(formula("ffmpeg")),
    candidate(formula("gh"), { target: "2.102.0" }),
    candidate({ instance_id: pip.id, kind: "Package", name: "urllib3" }),
    candidate({ instance_id: ollama.id, kind: "Model", name: "qwen3:8b" }, { channel: "Digest" }),
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [formula("ffmpeg")],
  skipped_versions: [{ key: formula("gh"), version: "2.102.0" }],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

const scan: UnknownScan = {
  scanned: [{ path: "~/.local/bin", entries: 4 }],
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
      kind: "BrokenSymlink",
      resolved: null,
      link_target: "/Applications/Gone.app/tool-b",
      size_bytes: null,
      modified_at: null,
      owned_by_me: true,
      app_bundle: "Gone",
    },
  ],
  attributed: 2,
  stopped: null,
};

let served: Snapshot;

beforeEach(() => {
  served = snapshot;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "scan_unknown") return Promise.resolve(scan);
    return Promise.resolve(undefined);
  });
});

describe("Sidebar", () => {
  it("renders a button for each page, in order, and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole, getAllByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const overviewButton = getByRole("button", { name: "Overview" });
    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const unknownButton = getByRole("button", { name: "Unknown" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(overviewButton).not.toHaveAttribute("aria-current");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(unknownButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
    // The Overview first, then the pages about the machine, Updates
    // leading; Settings fifth, in the same list, with no line above it.
    expect(getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Overview",
      "Updates",
      "Installed",
      "Unknown",
      "Settings",
    ]);
    const lists = getAllByRole("list");
    expect(lists).toHaveLength(1);
    expect(within(lists[0]).getAllByRole("button")).toHaveLength(5);
    expect(settingsButton.closest("li")?.parentElement).toBe(lists[0]);
  });

  it("draws each entry as a Mac sidebar's row: 32 high, its icon in the accent, nothing under the pointer", async () => {
    const { getByRole, getAllByRole, findByText } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={vi.fn()} />,
    );
    await findByText("5");

    for (const name of ["Overview", "Updates", "Installed", "Unknown", "Settings"]) {
      const button = getByRole("button", { name });
      // 32 high, 10 in from either side of the list's own 10 (so the icon's
      // box starts 20 in and the words 46), in the regular weight.
      expect(button.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-2.5", "gap-1.5", "text-body"]));
      expect(button.className).not.toMatch(/font-(medium|semibold|bold)|hover:/);
      const box = button.querySelector("svg")?.parentElement as HTMLElement;
      expect(box.className.split(" ")).toEqual(expect.arrayContaining(["h-5", "w-5", "text-accent"]));
      expect(button.querySelector("svg")).toHaveAttribute("width", "20");
    }
    expect(getAllByRole("list")[0].className.split(" ")).toEqual(expect.arrayContaining(["px-2.5", "pt-2"]));
    // Selected: the system fill, the words in their own colour.
    const installed = getByRole("button", { name: "Installed" });
    expect(installed.className).toContain("bg-sidebar-active");
    expect(installed.className).not.toMatch(/text-(white|accent)/);
  });

  it("counts in plain small numbers in the secondary colour, Updates' as the others", async () => {
    const { getByRole, findByText } = renderWithProviders(
      <>
        <Sidebar page="installed" onSelectPage={vi.fn()} />
        <UpdatesToolbar>
          <UpdatesPage />
        </UpdatesToolbar>
      </>,
    );
    await findByText("2 can be updated", { selector: "p" });

    for (const [name, count] of [
      ["Updates", "2"],
      ["Installed", "5"],
    ]) {
      const number = within(getByRole("button", { name })).getByText(count);
      expect(number.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted", "tabular-nums"]));
      expect(number.className).not.toMatch(/\bbg-|rounded/);
    }
  });

  it("calls onSelectPage with the clicked page", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    getByRole("button", { name: "Updates" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("updates");

    getByRole("button", { name: "Unknown" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("unknown");

    getByRole("button", { name: "Settings" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("settings");

    getByRole("button", { name: "Overview" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("overview");
  });

  it("counts on Updates exactly the updates the Updates page says it can install", async () => {
    // Both on one snapshot and one QueryClient, as in the app. Of the eight
    // updates, only glib and wget have an Update button: the rest are
    // pinned, not checkable, read-only, from a stopped Ollama, never to be
    // reminded about, or skipped.
    const { getByRole, findByText } = renderWithProviders(
      <>
        <Sidebar page="updates" onSelectPage={vi.fn()} />
        <UpdatesToolbar>
          <UpdatesPage />
        </UpdatesToolbar>
      </>,
    );

    await findByText("2 can be updated", { selector: "p" });
    const updatesButton = getByRole("button", { name: "Updates" });
    expect(within(updatesButton).getByText("2")).toBeInTheDocument();
    expect(updatesButton).toHaveAccessibleDescription("2 can be updated");
  });

  it("leaves an update being installed out of its count, as the Updates page's toolbar does", async () => {
    // glib's update is running: the page says so in words, and counts wget
    // alone among those that can be updated; the sidebar counts the same.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 7,
            kind: "Upgrade",
            instance_id: brew.id,
            artifact_kind: "Formula",
            name: "glib",
            status: "Running",
            outcome: null,
            argv_preview: ["/opt/brew/bin/brew", "upgrade", "glib"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });
    const { getByRole, findByText } = renderWithProviders(
      <>
        <Sidebar page="updates" onSelectPage={vi.fn()} />
        <UpdatesToolbar>
          <UpdatesPage />
        </UpdatesToolbar>
      </>,
    );

    await findByText("Updating 1 tool, 1 more can be updated");
    const updatesButton = getByRole("button", { name: "Updates" });
    expect(within(updatesButton).getByText("1")).toBeInTheDocument();
    expect(updatesButton).toHaveAccessibleDescription("1 can be updated");
  });

  it("counts everything installed, components other software brought in included", async () => {
    const { getByRole, findByText } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );

    const installedButton = getByRole("button", { name: "Installed" });
    await findByText("5");
    expect(within(installedButton).getByText("5")).toBeInTheDocument();
    expect(installedButton).toHaveAccessibleDescription("5 installed");
  });

  it("counts Unknown only once a scan has run, and never starts one itself", async () => {
    const { getByRole, findByText, queryClient } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );
    await findByText("5");

    const unknownButton = getByRole("button", { name: "Unknown" });
    expect(unknownButton.textContent).toBe("Unknown");
    expect(unknownButton).not.toHaveAttribute("aria-describedby");
    expect(mockInvoke).not.toHaveBeenCalledWith("scan_unknown");

    // The Unknown page's scan lands in the shared cache.
    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });

    expect(await within(unknownButton).findByText("2")).toBeInTheDocument();
    expect(unknownButton).toHaveAccessibleDescription("2 found");
  });

  it("shows no count once it drops to zero", async () => {
    const { getByRole, queryClient } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );
    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });
    // Every count showing first, so their going away below is the zero
    // and not data that has not arrived yet.
    await waitFor(() => {
      for (const name of ["Updates", "Installed", "Unknown"]) {
        expect(getByRole("button", { name })).toHaveAttribute("aria-describedby");
      }
    });

    // Everything updated and uninstalled, and a scan that found nothing.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...snapshot,
        generation: snapshot.generation + 1,
        updates: [],
        artifacts: [],
      });
      queryClient.setQueryData(queryKeys.unknown, { ...scan, entries: [] });
    });

    await waitFor(() => {
      for (const name of ["Overview", "Updates", "Installed", "Unknown", "Settings"]) {
        const button = getByRole("button", { name });
        expect(button.textContent).toBe(name);
        expect(button).not.toHaveAttribute("aria-describedby");
      }
    });
  });

  it("keeps its first row empty for the window's traffic lights, as a drag region, with the entries right under it", () => {
    const { getByRole, getAllByRole, queryByText } = renderWithProviders(
      <Sidebar page="overview" onSelectPage={vi.fn()} />,
    );

    const nav = getByRole("navigation", { name: "Navigation" });
    const firstRow = nav.firstElementChild as HTMLElement;
    // Nothing under the lights: no text, no control.
    expect(firstRow.childElementCount).toBe(0);
    expect(firstRow.textContent).toBe("");
    // 52px: the 14px lights with 19px above and below them, their centre
    // 26px from the window's top (src/test/windowChrome.test.ts).
    expect(firstRow.className).toContain("h-13");
    // No name of the app between them and the entries, which start 8
    // below (`pt-2`): 60 from the top, at the top of what scrolls.
    const scroller = firstRow.nextElementSibling as HTMLElement;
    expect(scroller.firstElementChild).toBe(getByRole("list"));
    expect(queryByText("Canager")).toBeNull();
    expect(nav.textContent).not.toContain("Canager");

    // Pressing it drags the window; nothing else in the sidebar does.
    expect(dragsWindow(firstRow)).toBe(true);
    expect(dragsWindow(getByRole("list"))).toBe(false);
    for (const button of getAllByRole("button")) {
      expect(dragsWindow(button)).toBe(false);
    }
  });

  describe("「来源」", () => {
    it("lists every source under its title, counting what each has installed and marking a warning with ⚠︎", async () => {
      const { findByRole, getByRole, getByText } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} />,
      );

      const list = await findByRole("list", { name: "Sources" });
      // In the snapshot's order, a source with nothing installed and one
      // that is not running as much as the rest (spec R8).
      expect(within(list).getAllByRole("button")).toEqual([
        getByRole("button", { name: "Homebrew" }),
        getByRole("button", { name: "pip" }),
        getByRole("button", { name: "Ollama" }),
      ]);
      expect(within(getByRole("button", { name: "pip" })).getByText("1")).toBeInTheDocument();
      expect(within(getByRole("button", { name: "Ollama" })).queryByText(/^\d+$/)).toBeNull();
      // The group's title: 11 bold in the secondary colour, 14 in, in a
      // 28-high row whose words sit at its foot.
      const title = getByText("Sources");
      expect(title.className.split(" ")).toEqual(
        expect.arrayContaining(["h-7", "px-3.5", "items-end", "text-small", "font-bold", "text-muted"]),
      );
      expect(list).toHaveAttribute("aria-labelledby", title.id);

      // A source's row is a page's row: 32 high, its mark -- 16, in the
      // pages' 20 box -- where a page's glyph is, its count plain.
      const homebrew = getByRole("button", { name: "Homebrew" });
      expect(homebrew.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-2.5", "gap-1.5", "text-body"]));
      const markBox = homebrew.firstElementChild as HTMLElement;
      expect(markBox.className.split(" ")).toEqual(expect.arrayContaining(["h-5", "w-5"]));
      expect(markBox.firstElementChild?.className).toContain("h-4 w-4");
      expect(within(homebrew).getByText("4").className.split(" ")).toEqual(
        expect.arrayContaining(["text-small", "text-muted", "tabular-nums"]),
      );
      expect(homebrew).toHaveAccessibleDescription("4 installed");
      expect(within(homebrew).queryByTitle(/./)).toBeNull();

      // Not running: no count, and a filled orange ⚠︎ at 12 with the
      // notice's own words, under the pointer and to a screen reader.
      const stopped = getByRole("button", { name: "Ollama" });
      const warning = within(stopped).getByTitle("Ollama isn't running");
      expect(warning.querySelector("svg")).toHaveAttribute("width", "12");
      expect(warning.querySelector("svg")?.getAttribute("class")).toContain("text-warning");
      expect(stopped).toHaveAccessibleDescription("Ollama isn't running");
    });

    it("puts the ⚠︎ before the count, and none for news that is no problem", async () => {
      served = {
        ...snapshot,
        instances: [
          { ...brew, status: { unavailable: "NotResponding", notes: [] } },
          { ...pip, status: { unavailable: null, notes: ["NotOnPath"] } },
        ],
      };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
      await findByRole("list", { name: "Sources" });

      const homebrew = getByRole("button", { name: "Homebrew" });
      const warning = within(homebrew).getByTitle("Homebrew isn't responding");
      expect(warning.compareDocumentPosition(within(homebrew).getByText("4")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(homebrew).toHaveAccessibleDescription("4 installed, Homebrew isn't responding");
      // Which copy runs when its name is typed is information, not a warning.
      expect(within(getByRole("button", { name: "pip" })).queryByTitle(/./)).toBeNull();
    });

    it("tells two sources of one kind apart by where each is", async () => {
      const intel = instance("brew:/usr/local", "brew", { prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
      served = {
        ...snapshot,
        instances: [brew, intel, pip],
        artifacts: [...snapshot.artifacts, artifact({ instance_id: intel.id, kind: "Formula", name: "wget" }, "Requested")],
      };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);

      const list = await findByRole("list", { name: "Sources" });
      expect(within(list).getAllByRole("button")).toEqual([
        getByRole("button", { name: "Homebrew (/opt/brew)" }),
        getByRole("button", { name: "Homebrew (/usr/local)" }),
        getByRole("button", { name: "pip" }),
      ]);
      expect(getByRole("button", { name: "Homebrew (/usr/local)" })).toHaveAccessibleDescription("1 installed");
      // In sight: the name, then where it is, 11 in the secondary colour --
      // what gives way first in the sidebar's width -- and the whole name
      // under the pointer.
      const intelRow = getByRole("button", { name: "Homebrew (/usr/local)" });
      const place = within(intelRow).getByText("/usr/local");
      expect(place.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted", "truncate", "min-w-0"]));
      expect(place.previousElementSibling).toHaveTextContent(/^Homebrew$/);
      expect(place.previousElementSibling?.className).toContain("shrink-0");
      expect(intelRow).toHaveAttribute("title", "Homebrew (/usr/local)");
      // The only one of its kind: its name alone, no tooltip.
      expect(getByRole("button", { name: "pip" })).not.toHaveAttribute("title");
    });

    it("selects one row at a time: a source's while the Installed page shows it alone, Installed's otherwise", async () => {
      const { findByRole, getByRole, rerender } = renderWithProviders(
        <Sidebar page="installed" source={pip.id} onSelectPage={vi.fn()} />,
      );
      await findByRole("list", { name: "Sources" });
      const current = () =>
        [...document.querySelectorAll('[aria-current="page"]')].map(
          (element) => element.querySelector(".truncate")?.textContent,
        );

      expect(current()).toEqual(["pip"]);
      expect(getByRole("button", { name: "Installed" })).not.toHaveAttribute("aria-current");
      expect(getByRole("button", { name: "pip" }).className).toContain("bg-sidebar-active");

      rerender(<Sidebar page="installed" source={null} onSelectPage={vi.fn()} />);
      expect(current()).toEqual(["Installed"]);

      // Another page: its row, whatever the Installed page was left on.
      rerender(<Sidebar page="updates" source={pip.id} onSelectPage={vi.fn()} />);
      expect(current()).toEqual(["Updates"]);
    });

    it("opens a source's row with its instance id", async () => {
      const onSelectSource = vi.fn();
      const { findByRole, getByRole } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} onSelectSource={onSelectSource} />,
      );
      await findByRole("list", { name: "Sources" });

      getByRole("button", { name: "Ollama" }).click();
      expect(onSelectSource).toHaveBeenCalledWith(ollama.id);
    });

    it("scrolls with the pages as one, under the traffic lights' row, and lists nothing before the first snapshot", async () => {
      mockInvoke.mockImplementation((cmd: string) =>
        cmd === "get_snapshot" ? new Promise(() => {}) : Promise.resolve(undefined),
      );
      const { getByRole, queryByRole, queryByText } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} />,
      );
      expect(queryByRole("list", { name: "Sources" })).toBeNull();
      expect(queryByText("Sources")).toBeNull();

      const scroller = getByRole("list").parentElement as HTMLElement;
      expect(scroller.className.split(" ")).toEqual(expect.arrayContaining(["min-h-0", "flex-1", "overflow-y-auto"]));
      expect(scroller.previousElementSibling).toHaveAttribute("data-tauri-drag-region");
    });

    it("titles the group 来源 and counts in Chinese, with a source's place in full-width brackets", async () => {
      const intel = instance("brew:/usr/local", "brew", { prefix: "/usr/local" });
      served = { ...snapshot, instances: [brew, intel] };
      await act(async () => {
        await i18n.changeLanguage("zh-CN");
      });
      try {
        const { findByRole, getByText, getByRole } = renderWithProviders(
          <Sidebar page="overview" onSelectPage={vi.fn()} />,
        );
        await findByRole("list", { name: "来源" });
        expect(getByText("来源")).toBeInTheDocument();
        expect(getByRole("button", { name: "Homebrew（/opt/brew）" })).toHaveAccessibleDescription("已安装4个");
        expect(getByRole("button", { name: "Homebrew（/usr/local）" })).not.toHaveAttribute("aria-describedby");
      } finally {
        await act(async () => {
          await i18n.changeLanguage("en");
        });
      }
    });
  });
});
