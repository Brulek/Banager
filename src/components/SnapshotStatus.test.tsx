import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { SnapshotStatus } from "./SnapshotStatus";
import { refreshIntoCache } from "../lib/events";
import { useSnapshot } from "../lib/queries";
import { useUiStore } from "../store/ui";
import type { Snapshot } from "../lib/types";

/**
 * Rendered as a sibling of the component under test, sharing its
 * QueryClient. Before `get_snapshot` resolves, `SnapshotStatus` has no
 * snapshot to judge and passes `children` straight through -- so an
 * assertion that children are visible passes vacuously on the very first
 * render, whatever the branch under test would do with the data. Waiting
 * for this probe's "snapshot loaded" is what makes such an assertion be
 * about the loaded snapshot.
 */
function SnapshotProbe() {
  const { data } = useSnapshot();
  return <p>{data ? "snapshot loaded" : "snapshot pending"}</p>;
}

function baseSnapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [],
    artifacts: [],
    updates: [],
    refreshed_at: 1700000000,
    stale: false,
    errors: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SnapshotStatus", () => {
  it("shows the no-sources empty state and hides children when detect is Missing", async () => {
    // `Missing` now means every source found nothing, not that Homebrew
    // alone is absent.
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "Missing" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    // "Found nothing", never "nothing yet" (T8): a tool with its own
    // installer is looked for only where its installer puts it, so one
    // somewhere else is not found although it is there.
    expect(await screen.findByText("No tools to manage")).toBeInTheDocument();
    expect(screen.getByText("Install Homebrew first.")).toBeInTheDocument();
    expect(screen.queryByText(/None of them are set up|yet/)).not.toBeInTheDocument();
    // What Canager works with, and where it looks, behind Details.
    const details = screen.getByRole("button", { name: "Details: No tools to manage" });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Supports Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and Claude Code, Antigravity CLI, Grok Build and rustup in their default locations.",
    );
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows the first check's spinner and why it takes a while, not the no-sources state, before the first refresh has completed", async () => {
    // Session boots with Snapshot::empty(): generation 0, detect Missing,
    // refreshed_at null. Only a completed refresh ever sets refreshed_at —
    // including a refresh that finds Homebrew genuinely missing.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );

    const { container } = renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    // The Overview's own first-check view, on the Updates and Installed
    // pages too: a bare "Loading…" in a corner, for as long as the first
    // check took, could not be told from a window that had frozen.
    expect(await screen.findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "The first check looks up every tool's newest version online, and sometimes takes a minute or two.",
      ),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-first-check] svg")).toHaveAttribute("width", "32");
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
    expect(screen.queryByText("No tools to manage")).not.toBeInTheDocument();
  });

  it("says in Chinese why the first check takes a while", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <SnapshotStatus>
          <p>installed list</p>
        </SnapshotStatus>,
      );

      expect(await screen.findByRole("heading", { level: 2, name: "正在检查…" })).toBeInTheDocument();
      expect(screen.getByText("首次检查需要联网查询每个工具的新版本，有时要一两分钟。")).toBeInTheDocument();
      expect(screen.queryByText("加载中…")).not.toBeInTheDocument();
      expect(screen.queryByText("正在载入…")).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("shows the stale-data banner, and no separate incomplete-check one, when a source failed", async () => {
    // There used to be a second banner here for `refreshed_at === null &&
    // errors.length > 0` -- "no check has ever finished, and this one
    // didn't either". `refresh()` now stamps `refreshed_at` whenever it
    // ran (spec §2.4-1), so a snapshot with errors always has one and that
    // branch could never fire again; it and its copy are gone. A refresh
    // that failed for a source says so once, here.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
        errors: [{ instance_id: "brew:/opt/homebrew", message: "timed out" }],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some checks didn't finish")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("names one broken source once, not twice, when its inventory and update check both failed", async () => {
    // session/refresh.rs pushes one SourceError from the inventory fetch
    // and a second from check_updates for the very same instance -- that
    // is two failed *calls* against one failed *source*. The banner names
    // sources, each once.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
        errors: [
          { instance_id: "brew:/opt/homebrew", message: "brew list failed" },
          { instance_id: "brew:/opt/homebrew", message: "brew outdated failed" },
        ],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(
      await screen.findByText("Homebrew didn't finish checking this time."),
    ).toBeInTheDocument();
  });

  it("names every source whose check did not finish, never with 更新 for a refresh, in Chinese", async () => {
    // 「2 项没完成，相关内容没有更新。」 said neither which two, and 更新 is
    // this app's word for installing a newer version: it read as "these
    // have no updates". An error against a bare adapter id -- its detect
    // failed -- is named by that adapter.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
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
        ],
        errors: [
          { instance_id: "brew:/opt/homebrew", message: "brew outdated failed" },
          { instance_id: "npm", message: "internal error detecting this source" },
          { instance_id: "uv:/Users/you/.local/share/uv/tools", message: "uv tool list exited 2" },
        ],
      }),
    );
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <SnapshotStatus>
          <p>installed list</p>
        </SnapshotStatus>,
      );

      expect(await screen.findByText("部分检查未完成")).toBeInTheDocument();
      expect(screen.getByText("Homebrew、npm和uv这次未检查完。")).toBeInTheDocument();
      // The header's Check again, right above it, runs the same check.
      expect(screen.queryByRole("button")).toBeNull();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("lets a page that says Checking… itself show through while the first refresh runs", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );

    renderWithProviders(
      <SnapshotStatus showsFirstCheck>
        <p>overview</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("overview")).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("still judges a finished check for a page that says Checking… itself", async () => {
    // Checked, and nothing is there: not the first check any more, so the
    // page gets the same empty state as any other.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: 1700000000 }),
    );

    renderWithProviders(
      <SnapshotStatus showsFirstCheck>
        <p>overview</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("No tools to manage")).toBeInTheDocument();
    expect(screen.queryByText("overview")).not.toBeInTheDocument();
  });

  it("shows the load-failure surface instead of Loading… when the startup refresh has failed", async () => {
    // get_snapshot itself succeeded with the empty startup snapshot, but the
    // startup refresh() IPC call rejected — refreshed_at will never be set.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );
    useUiStore.setState({ startupRefreshError: "brew: command not found" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    expect(screen.getByText(/brew: command not found/)).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("says why loading failed in a person's words where the message says, and no more", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );
    useUiStore.setState({ startupRefreshError: "No space left on device (os error 28)" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    expect(screen.getByText("The disk is full. Free up some space, then try again.")).toBeInTheDocument();
    expect(screen.queryByText(/os error 28/)).toBeNull();
  });

  it("keeps showing the data it has when a later refresh fails on a Mac that has refreshed before", async () => {
    // A Mac that has refreshed 412 times and whose npm is broken. Gating
    // the full-page load-failure surface on the startup error alone let one
    // rejected refresh hide every artifact the other six sources found.
    // `generation > 0` says a refresh has committed data at least once, and
    // data in hand beats a full-page error.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
        errors: [{ instance_id: "npm:/opt/homebrew/lib", message: "npm ls exited 1" }],
      }),
    );
    useUiStore.setState({ startupRefreshError: "refresh timed out" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some checks didn't finish")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("Couldn't load installed tools")).not.toBeInTheDocument();
  });

  it("shows the backend's error verbatim when the snapshot itself cannot be loaded", async () => {
    // get_snapshot rejects with a bare string (Task 10's call() turns it
    // into an Error); without this branch the page would be blank.
    vi.mocked(invoke).mockRejectedValue("brew: command not found" as never);

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    expect(screen.getByText(/brew: command not found/)).toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows a stale banner above the existing data when the last refresh failed, with no button of its own", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        stale: true,
        errors: [{ instance_id: "brew:/opt/homebrew", message: "timed out" }],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some checks didn't finish")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();
    // Its Try again ran the check the header's Check again, right above
    // it, runs: two names for one button.
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("calls the load failure's button what the header calls the same check", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null });
      if (cmd === "refresh") return baseSnapshot();
      throw new Error(`unexpected command ${cmd}`);
    });
    useUiStore.setState({ startupRefreshError: "refresh timed out" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Check Again" }));
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "refresh")).toBe(true),
    );
  });

  it("keeps the load failure's Check again off while its check runs, and runs it once", async () => {
    // Pressed again while the check ran, it queued a second full check
    // behind it, which the header's Check again never does.
    let fail: (reason: string) => void = () => {};
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") {
        return Promise.resolve(baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }));
      }
      if (cmd === "refresh") {
        return new Promise<Snapshot>((_resolve, reject) => {
          fail = reject;
        });
      }
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    });
    useUiStore.setState({ startupRefreshError: "refresh timed out" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Check Again" });
    fireEvent.click(button);
    await waitFor(() => expect(button).toBeDisabled());
    fireEvent.click(button);
    await act(async () => {
      fail("the session is gone");
    });

    // It failed again: its words, and the button back on.
    expect(await screen.findByText(/the session is gone/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check Again" })).toBeEnabled();
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1);
  });

  it("keeps the Check again of a snapshot that could not be read off while a check it did not start runs", async () => {
    let finish: (snapshot: Snapshot) => void = () => {};
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.reject("brew: command not found");
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          finish = resolve;
        });
      }
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    });

    const { queryClient } = renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load installed tools")).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Check Again" });
    expect(button).toBeEnabled();

    // The startup's check, or the one after an operation.
    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = refreshIntoCache(queryClient, "test");
    });
    await waitFor(() => expect(button).toBeDisabled());

    await act(async () => {
      finish(baseSnapshot({ generation: 2 }));
      await run;
    });
    // The check read what the snapshot could not.
    await waitFor(() => expect(screen.queryByText("Couldn't load installed tools")).toBeNull());
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1);
  });

  it("shows the nothing-installed empty state when there are no artifacts", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ artifacts: [] }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    // "Found nothing installed", not "nothing installed yet" (T8).
    expect(await screen.findByText("No installed tools found")).toBeInTheDocument();
    // Not "Once you install something with Homebrew": a Mac with Node and no
    // global packages lands here too.
    expect(
      screen.getByText("Tools you install with Homebrew, npm and the like show up here."),
    ).toBeInTheDocument();
    const details = screen.getByRole("button", { name: "Details: No installed tools found" });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Supports Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and Claude Code, Antigravity CLI, Grok Build and rustup in their default locations.",
    );
  });

  it("says in Chinese that nothing was found, never that nothing is installed yet (T8)", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      vi.mocked(invoke).mockResolvedValue(baseSnapshot({ artifacts: [] }));
      renderWithProviders(
        <SnapshotStatus>
          <p>installed list</p>
        </SnapshotStatus>,
      );

      expect(await screen.findByText("没有找到已安装的工具")).toBeInTheDocument();
      expect(screen.queryByText(/还没有/)).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("renders children, not the nothing-installed state, when a source still has a notice to show", async () => {
    // A Mac with Ollama installed but not running and nothing installed
    // anywhere else. InstalledPage renders that instance's group header and
    // its "Open Ollama" notice with no artifacts under it; swallowing the
    // children here would make that notice -- and with it the whole
    // open_ollama_app affordance -- unreachable in the app.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        artifacts: [],
        instances: [
          {
            id: "ollama:http://127.0.0.1:11434",
            adapter_id: "ollama",
            exe_path: "/usr/local/bin/ollama",
            prefix: "/usr/local",
            scope: "User",
            version: null,
            unverified_version: null,
            read_only_reason: null,
            status: { unavailable: "NotRunning", notes: [] },
          },
        ],
      }),
    );

    renderWithProviders(
      <>
        <SnapshotProbe />
        <SnapshotStatus>
          <p>installed list</p>
        </SnapshotStatus>
      </>,
    );

    await screen.findByText("snapshot loaded");
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("No installed tools found")).not.toBeInTheDocument();
    // And no page-wide banner either. An unavailable source is not a
    // failed refresh (`refresh()` leaves `stale` false for it), and it
    // already says so itself, in its own words and with its own button,
    // through the notice InstalledPage renders for it. A second, vaguer
    // "some checks didn't finish" over the top would say the same
    // thing worse.
    expect(screen.queryByText("Some checks didn't finish")).not.toBeInTheDocument();
  });

  it("renders children unchanged once something is installed", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        artifacts: [
          {
            key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
            display_name: "jq",
            version: "1.7",
            reason: "Requested",
            description: null,
            homepage: null,
            size_bytes: null,
            installed_at: null,
            path: null,
            auto_updates: false,
            uninstall_blocked: null,
          },
        ],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("installed list")).toBeInTheDocument();
  });
});
