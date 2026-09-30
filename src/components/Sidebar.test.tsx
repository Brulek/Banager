import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
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

const brew = instance("brew:/opt/homebrew", "brew", { prefix: "/opt/homebrew", exe_path: "/opt/homebrew/bin/brew" });
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

/**
 * The 「来源」 list, once the first snapshot has brought its sources: Other
 * Programs is in it from the start, alone until then.
 */
async function listedSources(
  findByRole: (role: "list", options: { name: string }) => Promise<HTMLElement>,
  name = "Sources",
): Promise<HTMLElement> {
  const list = await findByRole("list", { name });
  await waitFor(() => expect(within(list).getAllByRole("button").length).toBeGreaterThan(1));
  return list;
}

describe("Sidebar", () => {
  it("renders a button for each page, in order, and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole, getAllByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const overviewButton = getByRole("button", { name: "Overview" });
    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const otherButton = getByRole("button", { name: "Other Programs" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(overviewButton).not.toHaveAttribute("aria-current");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(otherButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
    // The Overview first, then the pages about the machine, Updates
    // leading; Settings fourth, in the same list, with no line above it.
    // Other Programs is no page of that list: it is under 「来源」.
    const lists = getAllByRole("list");
    expect(lists).toHaveLength(2);
    expect(within(lists[0]).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Overview",
      "Updates",
      "Installed",
      "Settings",
    ]);
    expect(settingsButton.closest("li")?.parentElement).toBe(lists[0]);
    expect(within(lists[0]).queryByRole("button", { name: "Other Programs" })).toBeNull();
    expect(otherButton.closest("ul")).toBe(getByRole("list", { name: "Sources" }));
  });

  it("draws each entry as a Mac sidebar's row: 32 high, its icon in the accent, nothing under the pointer", async () => {
    const { getByRole, getAllByRole, findByText } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={vi.fn()} />,
    );
    await findByText("5");

    for (const name of ["Overview", "Updates", "Installed", "Settings"]) {
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
    await findByText("2 updates available", { selector: "p" });

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

    getByRole("button", { name: "Other Programs" }).click();
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

    await findByText("2 updates available", { selector: "p" });
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

  it("counts nothing on Other Programs, as on a source's row, once a scan has run too, and never starts one", async () => {
    const { getByRole, findByText, queryClient } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );
    await findByText("5");

    const otherButton = getByRole("button", { name: "Other Programs" });
    const plain = () => {
      expect(otherButton.textContent).toBe("Other Programs");
      expect(otherButton).not.toHaveAttribute("aria-describedby");
      expect(otherButton.querySelector("[data-count], [data-trailing]")).toBeNull();
    };
    plain();
    expect(mockInvoke).not.toHaveBeenCalledWith("scan_unknown");

    // Its page's scan lands in the shared cache: two programs, which its
    // header's subtitle says, not the sidebar.
    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.unknown)).toBe(scan));
    plain();
    expect(within(otherButton).queryByText("2")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith("scan_unknown");
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
      for (const name of ["Updates", "Installed"]) {
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
      for (const name of ["Overview", "Updates", "Installed", "Settings", "Other Programs"]) {
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
    expect(scroller).toHaveAttribute("data-sidebar-scroller");
    expect(scroller.firstElementChild?.firstElementChild).toBe(getAllByRole("list")[0]);
    expect(queryByText("Canager")).toBeNull();
    expect(nav.textContent).not.toContain("Canager");

    // Pressing it drags the window; nothing else in the sidebar does.
    expect(dragsWindow(firstRow)).toBe(true);
    for (const list of getAllByRole("list")) {
      expect(dragsWindow(list)).toBe(false);
    }
    for (const button of getAllByRole("button")) {
      expect(dragsWindow(button)).toBe(false);
    }
  });

  describe("「来源」", () => {
    it("lists every source under its title with no count, and marks a warning with ⚠︎", async () => {
      const { findByRole, getByRole, getByText } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} />,
      );

      const list = await listedSources(findByRole);
      // In the snapshot's order, a source with nothing installed and one
      // that is not running as much as the rest (spec R8); Other Programs
      // after them.
      expect(within(list).getAllByRole("button")).toEqual([
        getByRole("button", { name: "Homebrew" }),
        getByRole("button", { name: "pip" }),
        getByRole("button", { name: "Ollama" }),
        getByRole("button", { name: "Other Programs" }),
      ]);
      // No number by a source: beside 「更新 10」 one read as that
      // source's updates. The page a source opens says how many it has.
      for (const name of ["Homebrew", "pip", "Ollama"]) {
        expect(within(getByRole("button", { name })).queryByText(/^\d+$/)).toBeNull();
        expect(getByRole("button", { name }).querySelector("[data-count]")).toBeNull();
      }
      // The group's title: 11 bold in the secondary colour, 14 in, in a
      // 28-high row whose words sit at its foot.
      const title = getByText("Sources");
      expect(title.className.split(" ")).toEqual(
        expect.arrayContaining(["h-7", "px-3.5", "items-end", "text-small", "font-bold", "text-muted"]),
      );
      expect(list).toHaveAttribute("aria-labelledby", title.id);

      // A source's row is a page's row: 32 high, its mark -- 16, in the
      // pages' 20 box -- where a page's glyph is.
      const homebrew = getByRole("button", { name: "Homebrew" });
      expect(homebrew.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-2.5", "gap-1.5", "text-body"]));
      const markBox = homebrew.firstElementChild as HTMLElement;
      expect(markBox.className.split(" ")).toEqual(expect.arrayContaining(["h-5", "w-5"]));
      expect(markBox.firstElementChild?.className).toContain("h-4 w-4");
      // Nothing more to say to a screen reader than its name: no count is
      // on the row to explain, and the page it opens says how many.
      expect(homebrew).not.toHaveAttribute("aria-describedby");
      expect(within(homebrew).queryByTitle(/./)).toBeNull();

      // Not running: no count, and a filled orange ⚠︎ at 12 with the
      // notice's own words, under the pointer and to a screen reader.
      const stopped = getByRole("button", { name: "Ollama" });
      const warning = within(stopped).getByTitle("Ollama isn't running");
      expect(warning.querySelector("svg")).toHaveAttribute("width", "12");
      expect(warning.querySelector("svg")?.getAttribute("class")).toContain("text-warning");
      expect(stopped).toHaveAccessibleDescription("Ollama isn't running");
    });

    it("puts a source's ⚠︎ where a page's count ends, and none for news that is no problem", async () => {
      served = {
        ...snapshot,
        instances: [
          { ...brew, status: { unavailable: "NotResponding", notes: [] } },
          { ...pip, status: { unavailable: null, notes: ["NotOnPath"] } },
        ],
      };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
      await listedSources(findByRole);

      const homebrew = getByRole("button", { name: "Homebrew" });
      const warning = within(homebrew).getByTitle("Homebrew isn't responding");
      // In the place at the row's right, where the Installed row's number is.
      expect(warning.parentElement).toBe(homebrew.querySelector("[data-trailing]"));
      expect(getByRole("button", { name: "Installed" }).querySelector("[data-trailing] [data-count]")).not.toBeNull();
      // Its words alone, with no count before them.
      expect(homebrew).toHaveAccessibleDescription("Homebrew isn't responding");
      // Which copy runs when its name is typed is information, not a warning.
      expect(within(getByRole("button", { name: "pip" })).queryByTitle(/./)).toBeNull();
    });

    it("tells two Homebrews apart by the Mac each is for, not by a path it would cut short", async () => {
      const intel = instance("brew:/usr/local", "brew", { prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
      served = {
        ...snapshot,
        instances: [brew, intel, pip],
        artifacts: [...snapshot.artifacts, artifact({ instance_id: intel.id, kind: "Formula", name: "wget" }, "Requested")],
      };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);

      const list = await listedSources(findByRole);
      expect(within(list).getAllByRole("button")).toEqual([
        getByRole("button", { name: "Homebrew (Apple silicon)" }),
        getByRole("button", { name: "Homebrew (Intel)" }),
        getByRole("button", { name: "pip" }),
        getByRole("button", { name: "Other Programs" }),
      ]);
      expect(getByRole("button", { name: "Homebrew (Intel)" })).not.toHaveAttribute("aria-describedby");
      // In sight: the name, and under it which one it is, 11 in the
      // secondary colour, on a line of its own that the sidebar's width
      // holds whole -- the row 40 high for it -- and the whole name under
      // the pointer.
      const intelRow = getByRole("button", { name: "Homebrew (Intel)" });
      const place = within(intelRow).getByText("Intel");
      expect(place.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted", "truncate"]));
      expect(place.previousElementSibling).toHaveTextContent(/^Homebrew$/);
      expect(place.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["flex", "flex-col", "min-w-0"]));
      expect(intelRow.className.split(" ")).toContain("h-10");
      expect(intelRow).toHaveAttribute("title", "Homebrew (Intel)");
      // A row with its name alone stays 32.
      expect(getByRole("button", { name: "pip" }).className.split(" ")).toContain("h-8");
      expect(within(getByRole("button", { name: "Homebrew (Apple silicon)" })).getByText("Apple silicon")).toBeInTheDocument();
      expect(list.textContent).not.toContain("/usr/local");
      expect(list.textContent).not.toContain("/opt/homebrew");
      // The only one of its kind: its name alone, no tooltip.
      expect(getByRole("button", { name: "pip" })).not.toHaveAttribute("title");
    });

    it("names the only Homebrew plainly, wherever it is", async () => {
      const intelOnly = instance("brew:/usr/local", "brew", { prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
      served = { ...snapshot, instances: [intelOnly, pip], artifacts: [] };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);

      await listedSources(findByRole);
      const homebrew = getByRole("button", { name: "Homebrew" });
      // Its name alone: no place after it, no tooltip.
      expect(homebrew.querySelector(".truncate")?.textContent).toBe("Homebrew");
      expect(homebrew.querySelector(".text-small.text-muted.truncate")).toBeNull();
      expect(homebrew).not.toHaveAttribute("title");
    });

    it("keeps a place 20 wide at every source row's right, its ⚠︎ in it, so each name has the same room with a ⚠︎ or without", async () => {
      served = {
        ...snapshot,
        instances: [{ ...brew, status: { unavailable: "NotResponding", notes: [] } }, pip, ollama],
      };
      const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
      await listedSources(findByRole);

      const warned = getByRole("button", { name: "Homebrew" });
      const plain = getByRole("button", { name: "pip" });
      for (const row of [warned, plain, getByRole("button", { name: "Ollama" })]) {
        const slot = row.querySelector("[data-trailing]") as HTMLElement;
        // The row's last thing, a column 20 wide at least, what is in it at
        // its right edge -- the row's own right, 20 from the sidebar's,
        // where a page's count ends.
        expect(slot).not.toBeNull();
        // (Only the screen reader's hidden words may follow it.)
        const after = slot.nextElementSibling;
        expect(after === null || (after instanceof HTMLElement && after.hidden)).toBe(true);
        expect(slot.className.split(" ")).toEqual(expect.arrayContaining(["min-w-5", "shrink-0", "justify-end"]));
        expect(slot).toHaveAttribute("aria-hidden", "true");
        // No number in it.
        expect(slot.querySelector("[data-count]")).toBeNull();
      }
      expect(warned.querySelector("[data-trailing] svg")).not.toBeNull();
      // No warning: the place kept, empty.
      expect(plain.querySelector("[data-trailing]")?.childElementCount).toBe(0);
      // A page's row with nothing to count has no such place; one with a
      // count has its number there, in the same column.
      expect(getByRole("button", { name: "Overview" }).querySelector("[data-trailing]")).toBeNull();
      const installed = getByRole("button", { name: "Installed" }).querySelector("[data-trailing]") as HTMLElement;
      expect(installed.className).toBe(warned.querySelector("[data-trailing]")?.className);
      expect(installed.querySelector("[data-count]")).not.toBeNull();
    });

    it("leaves 12 under the last row inside what scrolls", async () => {
      const { findByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
      const list = await listedSources(findByRole);
      const content = list.parentElement as HTMLElement;
      expect(content.className.split(" ")).toContain("pb-3");
      expect(content.parentElement).toHaveAttribute("data-sidebar-scroller");
      expect(content.parentElement?.className).not.toContain("pb-3");
    });

    it("selects one row at a time: a source's while the Installed page shows it alone, Installed's otherwise", async () => {
      const { findByRole, getByRole, rerender } = renderWithProviders(
        <Sidebar page="installed" source={pip.id} onSelectPage={vi.fn()} />,
      );
      await listedSources(findByRole);
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

    describe("as one control for the keyboard (a roving tabindex)", () => {
      const tabbable = () =>
        [...document.querySelectorAll<HTMLElement>("[data-sidebar-row]")]
          .filter((row) => row.tabIndex === 0)
          .map((row) => row.getAttribute("aria-label") ?? row.querySelector(".truncate")?.textContent);

      it("is one Tab stop: the row selected, a page's, a source's or Other Programs'", async () => {
        const { findByRole, getAllByRole, rerender } = renderWithProviders(
          <Sidebar page="updates" onSelectPage={vi.fn()} />,
        );
        await listedSources(findByRole);
        // Four pages, three sources and Other Programs.
        expect(getAllByRole("button")).toHaveLength(8);
        expect(tabbable()).toEqual(["Updates"]);
        for (const row of getAllByRole("button")) {
          if (row.textContent?.startsWith("Updates")) continue;
          expect(row).toHaveAttribute("tabindex", "-1");
        }

        rerender(<Sidebar page="installed" source={pip.id} onSelectPage={vi.fn()} />);
        expect(tabbable()).toEqual(["pip"]);

        rerender(<Sidebar page="unknown" source={pip.id} onSelectPage={vi.fn()} />);
        expect(tabbable()).toEqual(["Other Programs"]);
      });

      it("moves with ↑ and ↓ through the pages and the sources as one list, and Home and End to its ends", async () => {
        const { findByRole, getByRole } = renderWithProviders(<Sidebar page="settings" onSelectPage={vi.fn()} />);
        await listedSources(findByRole);
        const settingsRow = getByRole("button", { name: "Settings" });
        settingsRow.focus();

        expect(fireEvent.keyDown(settingsRow, { key: "ArrowDown" })).toBe(false);
        expect(document.activeElement).toBe(getByRole("button", { name: "Homebrew" }));
        // The row the focus is on is the one Tab stop now.
        expect(tabbable()).toEqual(["Homebrew"]);
        fireEvent.keyDown(document.activeElement as HTMLElement, { key: "ArrowUp" });
        expect(document.activeElement).toBe(settingsRow);
        // The last row is Other Programs, after the last source.
        fireEvent.keyDown(settingsRow, { key: "End" });
        const other = getByRole("button", { name: "Other Programs" });
        expect(document.activeElement).toBe(other);
        expect(tabbable()).toEqual(["Other Programs"]);
        fireEvent.keyDown(other, { key: "ArrowUp" });
        expect(document.activeElement).toBe(getByRole("button", { name: "Ollama" }));
        fireEvent.keyDown(document.activeElement as HTMLElement, { key: "ArrowDown" });
        expect(document.activeElement).toBe(other);
        // No further at either end.
        fireEvent.keyDown(other, { key: "ArrowDown" });
        expect(document.activeElement).toBe(other);
        fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Home" });
        expect(document.activeElement).toBe(getByRole("button", { name: "Overview" }));
        fireEvent.keyDown(document.activeElement as HTMLElement, { key: "ArrowUp" });
        expect(document.activeElement).toBe(getByRole("button", { name: "Overview" }));
      });

      it("comes back in on the row selected once the focus has left it, and opens a row as a click does", async () => {
        const onSelectPage = vi.fn();
        const { findByRole, getByRole } = renderWithProviders(
          <>
            <Sidebar page="installed" onSelectPage={onSelectPage} />
            <button type="button">Beyond</button>
          </>,
        );
        await listedSources(findByRole);
        const installed = getByRole("button", { name: "Installed" });
        installed.focus();
        fireEvent.keyDown(installed, { key: "ArrowUp" });
        const updates = getByRole("button", { name: "Updates" });
        expect(document.activeElement).toBe(updates);
        expect(tabbable()).toEqual(["Updates"]);

        // Out of the sidebar, as Tab takes it to the toolbar: the selected row is the Tab stop again.
        const beyond = getByRole("button", { name: "Beyond" });
        fireEvent.blur(updates, { relatedTarget: beyond });
        beyond.focus();
        expect(tabbable()).toEqual(["Installed"]);

        // A row is a button still: Return and Space press it.
        updates.click();
        expect(onSelectPage).toHaveBeenCalledWith("updates");
      });

      it("leaves a key with ⌘ or ⌥ alone, and any key but those four", async () => {
        const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
        await listedSources(findByRole);
        const overview = getByRole("button", { name: "Overview" });
        overview.focus();
        expect(fireEvent.keyDown(overview, { key: "ArrowDown", metaKey: true })).toBe(true);
        expect(fireEvent.keyDown(overview, { key: "ArrowRight" })).toBe(true);
        expect(document.activeElement).toBe(overview);
      });
    });

    it("opens a source's row with its instance id", async () => {
      const onSelectSource = vi.fn();
      const { findByRole, getByRole } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} onSelectSource={onSelectSource} />,
      );
      await listedSources(findByRole);

      getByRole("button", { name: "Ollama" }).click();
      expect(onSelectSource).toHaveBeenCalledWith(ollama.id);
    });

    describe("Other Programs", () => {
      it("is the last row under 「来源」, after every source, and opens its own page, not the Installed page on a source", async () => {
        const onSelectPage = vi.fn();
        const onSelectSource = vi.fn();
        const { findByRole, getByRole } = renderWithProviders(
          <Sidebar page="overview" onSelectPage={onSelectPage} onSelectSource={onSelectSource} />,
        );
        const list = await listedSources(findByRole);
        const other = getByRole("button", { name: "Other Programs" });
        const rows = within(list).getAllByRole("button");
        expect(rows[rows.length - 1]).toBe(other);
        expect(other.closest("li")).toBe(list.lastElementChild);

        other.click();
        expect(onSelectPage).toHaveBeenCalledTimes(1);
        expect(onSelectPage).toHaveBeenCalledWith("unknown");
        expect(onSelectSource).not.toHaveBeenCalled();
      });

      it("has its programs' tile for a mark, at a source's 16, and no count", async () => {
        const { findByRole, getByRole } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
        await listedSources(findByRole);
        const other = getByRole("button", { name: "Other Programs" });
        // A source's row: 32 high, the mark in the 20 box every glyph has.
        expect(other.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-2.5", "gap-1.5", "text-body"]));
        const box = other.firstElementChild as HTMLElement;
        expect(box.className.split(" ")).toEqual(expect.arrayContaining(["h-5", "w-5"]));
        // The page's program tile (`ProgramAvatar`): a prompt, white on the
        // neutral grey, the size and corners of a source's mark beside it.
        const mark = box.firstElementChild as HTMLElement;
        expect(mark).toHaveAttribute("data-other-programs-mark");
        expect(mark).toHaveAttribute("aria-hidden", "true");
        expect(mark.className.split(" ")).toEqual(
          expect.arrayContaining(["h-4", "w-4", "rounded-[4px]", "bg-neutral-avatar", "text-white"]),
        );
        // In dark mode, the edge a source's logo has, so the grey tile keeps
        // its outline on the selected row's fill.
        expect(mark.className.split(" ")).toEqual(expect.arrayContaining(["dark:inset-ring", "dark:inset-ring-white/12"]));
        const homebrewMark = getByRole("button", { name: "Homebrew" }).firstElementChild?.firstElementChild as HTMLElement;
        expect(homebrewMark.className).toContain("h-4 w-4 rounded-[4px]");
        expect(mark.querySelector("svg path")?.getAttribute("d")).toBe("M5.5 8L9.5 12L5.5 16M12.5 16.5H18.5");
        // Nothing at its right, and nothing more to say than its name.
        expect(other.querySelector("[data-trailing]")).toBeNull();
        expect(other).not.toHaveAttribute("aria-describedby");
        expect(other).not.toHaveAttribute("title");
      });

      it("is the one row selected while its page is open, as a source's is while it is shown", async () => {
        const { findByRole, getByRole, rerender } = renderWithProviders(
          // The Installed page was left on pip: its row is not selected then.
          <Sidebar page="unknown" source={pip.id} onSelectPage={vi.fn()} />,
        );
        await listedSources(findByRole);
        const other = getByRole("button", { name: "Other Programs" });
        expect([...document.querySelectorAll('[aria-current="page"]')]).toEqual([other]);
        expect(other.className).toContain("bg-sidebar-active");
        const selected = other.className;

        // Drawn selected as a source's row is: pip's, while it is shown.
        rerender(<Sidebar page="installed" source={pip.id} onSelectPage={vi.fn()} />);
        expect(getByRole("button", { name: "pip" }).className).toBe(selected);
        expect(other).not.toHaveAttribute("aria-current");
        expect(other.className).not.toContain("bg-sidebar-active");
      });

      it("stays under 「来源」 on a Mac with no source at all, where every program is one of its", async () => {
        served = { ...snapshot, instances: [], artifacts: [], updates: [] };
        const { getByRole, queryClient } = renderWithProviders(<Sidebar page="overview" onSelectPage={vi.fn()} />);
        await waitFor(() => expect(queryClient.getQueryData(queryKeys.snapshot)).toBe(served));
        const list = getByRole("list", { name: "Sources" });
        expect(within(list).getAllByRole("button").map((row) => row.textContent)).toEqual(["Other Programs"]);
      });
    });

    it("scrolls with the pages as one, under the traffic lights' row, and lists Other Programs alone before the first snapshot", async () => {
      mockInvoke.mockImplementation((cmd: string) =>
        cmd === "get_snapshot" ? new Promise(() => {}) : Promise.resolve(undefined),
      );
      const { getAllByRole, getByRole, getByText } = renderWithProviders(
        <Sidebar page="overview" onSelectPage={vi.fn()} />,
      );
      // No source yet; the page of what none installed is there to open.
      const list = getByRole("list", { name: "Sources" });
      expect(getByText("Sources")).toBeInTheDocument();
      expect(within(list).getAllByRole("button").map((row) => row.textContent)).toEqual(["Other Programs"]);

      const scroller = getAllByRole("list")[0].parentElement?.parentElement as HTMLElement;
      expect(scroller).toHaveAttribute("data-sidebar-scroller");
      expect(scroller.className.split(" ")).toEqual(expect.arrayContaining(["min-h-0", "flex-1", "overflow-y-auto"]));
      expect(scroller.previousElementSibling).toHaveAttribute("data-tauri-drag-region");
    });

    it("titles the group 来源 and counts the pages in Chinese, with which Homebrew it is in full-width brackets", async () => {
      const intel = instance("brew:/usr/local", "brew", { prefix: "/usr/local" });
      served = { ...snapshot, instances: [brew, intel] };
      await act(async () => {
        await i18n.changeLanguage("zh-CN");
      });
      try {
        const { findByRole, getByText, getByRole, getAllByRole } = renderWithProviders(
          <Sidebar page="overview" onSelectPage={vi.fn()} />,
        );
        const list = await listedSources(findByRole, "来源");
        expect(getByText("来源")).toBeInTheDocument();
        // The pages' counts, not the sources'.
        expect(getByRole("button", { name: "已安装" })).toHaveAccessibleDescription(/^已安装\d+个$/);
        expect(getByRole("button", { name: "Homebrew（Apple芯片）" })).not.toHaveAttribute("aria-describedby");
        expect(getByRole("button", { name: "Homebrew（Intel）" })).not.toHaveAttribute("aria-describedby");
        // 其他程序, the last under 来源, after both Homebrews; the pages
        // 概览, 更新, 已安装 and 设置 above.
        expect(within(list).getAllByRole("button").map((row) => row.getAttribute("aria-label") ?? row.textContent)).toEqual([
          "Homebrew（Apple芯片）",
          "Homebrew（Intel）",
          "其他程序",
        ]);
        expect(within(getAllByRole("list")[0]).getAllByRole("button").map((row) => row.querySelector(".truncate")?.textContent)).toEqual([
          "概览",
          "更新",
          "已安装",
          "设置",
        ]);
        expect(getByRole("button", { name: "其他程序" })).not.toHaveAttribute("aria-describedby");
      } finally {
        await act(async () => {
          await i18n.changeLanguage("en");
        });
      }
    });
  });
});
